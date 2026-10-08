use crate::ast::{AstNode, Op};
use crate::lexer::{Lexer, Span, Token, TokenKind};
use std::fmt;
use std::num::IntErrorKind;

/// What the parser was looking for when it hit an error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Expected {
    Token(TokenKind),
    Expression,
}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expected::Token(kind) => write!(f, "{kind}"),
            Expected::Expression => write!(f, "an expression"),
        }
    }
}

#[derive(Debug)]
pub enum ParserError {
    /// The lexer found a character that cannot start a token.
    UnknownChar { span: Span },
    /// An integer literal that does not fit in an `i64`.
    InvalidInt { kind: IntErrorKind, span: Span },
    UnexpectedToken {
        expected: Expected,
        actual: TokenKind,
        span: Span,
    },
}

impl ParserError {
    pub fn span(&self) -> Span {
        match self {
            ParserError::UnknownChar { span }
            | ParserError::InvalidInt { span, .. }
            | ParserError::UnexpectedToken { span, .. } => *span,
        }
    }
}

impl std::error::Error for ParserError {}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParserError::UnknownChar { span } => {
                write!(f, "Unexpected char at {span}")
            }
            ParserError::InvalidInt { kind, span } => match kind {
                IntErrorKind::PosOverflow => {
                    write!(f, "Integer literal at {span} is too large for i64")
                }
                _ => write!(f, "Invalid integer literal at {span}"),
            },
            ParserError::UnexpectedToken {
                expected,
                actual,
                span,
            } => write!(
                f,
                "Unexpected token at {span}: expected {expected} but got {actual}"
            ),
        }
    }
}

/// A recursive-descent parser in the style of rustc's: it always holds the
/// current token and moves forward with `bump`. Lookahead is just reading
/// `self.token`, which is `Copy`, so nothing stays borrowed.
pub struct Parser<'src> {
    lexer: Lexer<'src>,
    /// The token being looked at. `Eof` once the input runs out.
    token: Token,
}

impl<'src> Parser<'src> {
    pub fn new(mut lexer: Lexer<'src>) -> Self {
        let token = lexer.next_token();
        Parser { lexer, token }
    }

    fn text(&self, token: Token) -> &'src str {
        &self.lexer.src()[token.span.start..token.span.end]
    }

    fn bump(&mut self) {
        self.token = self.lexer.next_token();
    }

    /// Consume the current token if it is `kind`, or fail.
    fn expect(&mut self, kind: TokenKind) -> Result<Span, ParserError> {
        let span = self.token.span;
        if kind == self.token.kind {
            self.bump();
            Ok(span)
        } else {
            Err(self.unexpected(Expected::Token(kind)))
        }
    }

    /// An error saying the current token is not what we wanted. Bad input
    /// from the lexer is reported as such, since no grammar rule accepts it.
    fn unexpected(&self, expected: Expected) -> ParserError {
        let Token { kind, span } = self.token;
        match kind {
            TokenKind::Unknown => ParserError::UnknownChar { span },
            actual => ParserError::UnexpectedToken {
                expected,
                actual,
                span,
            },
        }
    }

    fn parse_atom(&mut self) -> Result<AstNode, ParserError> {
        match self.token.kind {
            TokenKind::Int => {
                let text = self.text(self.token);
                let n =
                    text.parse::<i64>()
                        .map(AstNode::Int)
                        .map_err(|e| ParserError::InvalidInt {
                            kind: *e.kind(),
                            span: self.token.span,
                        })?;
                self.bump();
                Ok(n)
            }
            TokenKind::LParen => {
                self.bump();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            _ => Err(self.unexpected(Expected::Expression)),
        }
    }

    fn parse_binary(&mut self, min: u32) -> Result<AstNode, ParserError> {
        let mut lhs = self.parse_atom()?;
        while let Some(op) = Op::from_token(&self.token.kind)
            && op.precedence() >= min
        {
            self.bump();
            let rhs = self.parse_binary(op.precedence() + 1)?;
            lhs = AstNode::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_expr(&mut self) -> Result<AstNode, ParserError> {
        self.parse_binary(0)
    }

    pub fn parse(&mut self) -> Result<AstNode, ParserError> {
        let expr = self.parse_expr()?;
        self.expect(TokenKind::Eof)?;
        Ok(expr)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn parse(src: &str) -> Result<AstNode, ParserError> {
        Parser::new(Lexer::new(src)).parse()
    }

    fn ast(src: &str) -> String {
        parse(src).unwrap().to_string()
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(ast("1 + 2 * 3"), "(1 + (2 * 3))");
        assert_eq!(ast("1 * 2 + 3"), "((1 * 2) + 3)");
        assert_eq!(ast("1 - 2 - 3"), "((1 - 2) - 3)");
        assert_eq!(ast("8 / 4 / 2"), "((8 / 4) / 2)");
        assert_eq!(ast("(1 - 2) * 3"), "((1 - 2) * 3)");
        assert_eq!(ast("((7))"), "7");
    }

    #[test]
    fn expected_expression() {
        assert!(matches!(
            parse(""),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Expression,
                actual: TokenKind::Eof,
                span: Span { start: 0, end: 0 },
            })
        ));
        assert!(matches!(
            parse("1 +"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Expression,
                actual: TokenKind::Eof,
                span: Span { start: 3, end: 3 },
            })
        ));
        assert!(matches!(
            parse(")"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Expression,
                actual: TokenKind::RParen,
                span: Span { start: 0, end: 1 },
            })
        ));
    }

    #[test]
    fn unclosed_paren() {
        assert!(matches!(
            parse("(1"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::RParen),
                actual: TokenKind::Eof,
                span: Span { start: 2, end: 2 },
            })
        ));
        assert!(matches!(
            parse("(1 2"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::RParen),
                actual: TokenKind::Int,
                ..
            })
        ));
    }

    #[test]
    fn trailing_tokens() {
        assert!(matches!(
            parse("1 2"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Eof),
                actual: TokenKind::Int,
                span: Span { start: 2, end: 3 },
            })
        ));
        assert!(matches!(
            parse("1)"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Eof),
                actual: TokenKind::RParen,
                ..
            })
        ));
    }

    #[test]
    fn unknown_char_is_reported_where_the_parser_reaches_it() {
        for (src, start) in [
            ("$ + 1", 0), // first token
            ("1 + $", 4), // atom position
            ("1 $", 2),   // operator position, reported by the final `expect(Eof)`
            ("(1 $", 3),  // inside parentheses
        ] {
            let err = parse(src).unwrap_err();
            assert!(
                matches!(err, ParserError::UnknownChar { span } if span.start == start),
                "input: {src:?}, got: {err:?}"
            );
            let span = err.span();
            assert_eq!(&src[span.start..span.end], "$");
        }
    }

    #[test]
    fn largest_int() {
        assert_eq!(ast("9223372036854775807"), "9223372036854775807");
    }

    #[test]
    fn invalid_int() {
        assert!(matches!(
            parse("1 + 9223372036854775808"),
            Err(ParserError::InvalidInt {
                kind: IntErrorKind::PosOverflow,
                span: Span { start: 4, end: 23 },
            })
        ));
    }

    #[test]
    fn every_error_has_a_span() {
        let cases = [
            ("1 + $", Span { start: 4, end: 5 }), // unknown char
            ("99999999999999999999", Span { start: 0, end: 20 }), // invalid int
            ("1 2", Span { start: 2, end: 3 }),   // unexpected token
            ("1 +  ", Span { start: 5, end: 5 }), // eof, after trailing whitespace
            ("(1\n", Span { start: 3, end: 3 }),  // eof, after a trailing newline
        ];
        for (src, expected) in cases {
            assert_eq!(parse(src).unwrap_err().span(), expected, "input: {src:?}");
        }
    }

    #[test]
    fn error_display_includes_span() {
        assert_eq!(
            parse("1 2").unwrap_err().to_string(),
            "Unexpected token at 2..3: expected <eof> but got <int>"
        );
        assert_eq!(
            parse("1 +").unwrap_err().to_string(),
            "Unexpected token at 3..3: expected an expression but got <eof>"
        );
        assert_eq!(
            parse("1 $").unwrap_err().to_string(),
            "Unexpected char at 2..3"
        );
        assert_eq!(
            parse("99999999999999999999").unwrap_err().to_string(),
            "Integer literal at 0..20 is too large for i64"
        );
    }
}
