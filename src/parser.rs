use crate::ast::{AstNode, Op};
use crate::lexer::{Lexer, Span, Token, TokenKind};
use std::fmt;
use std::num::IntErrorKind;

/// What the parser was looking for when it hit an error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Expected<'a> {
    Token(TokenKind<'a>),
    Expression,
}

impl fmt::Display for Expected<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expected::Token(kind) => write!(f, "{kind}"),
            Expected::Expression => write!(f, "an expression"),
        }
    }
}

#[derive(Debug)]
pub enum ParserError<'a> {
    /// The lexer found a character that cannot start a token.
    UnknownChar { ch: char, span: Span },
    /// An integer literal that does not fit in an `i64`.
    InvalidInt { kind: IntErrorKind, span: Span },
    UnexpectedToken {
        expected: Expected<'a>,
        actual: TokenKind<'a>,
        span: Span,
    },
}

impl ParserError<'_> {
    pub fn span(&self) -> Span {
        match self {
            ParserError::UnknownChar { span, .. }
            | ParserError::InvalidInt { span, .. }
            | ParserError::UnexpectedToken { span, .. } => *span,
        }
    }
}

impl std::error::Error for ParserError<'_> {}

impl fmt::Display for ParserError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParserError::UnknownChar { ch, span } => {
                write!(f, "Unexpected char '{ch}' at {span}")
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
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    /// The token being looked at. `Eof` once the input runs out.
    token: Token<'a>,
}

impl<'a> Parser<'a> {
    pub fn new(mut lexer: Lexer<'a>) -> Self {
        let token = lexer.next_token();
        Parser { lexer, token }
    }

    fn bump(&mut self) {
        self.token = self.lexer.next_token();
    }

    /// Consume the current token if it is `kind`, or fail.
    fn expect(&mut self, kind: TokenKind<'a>) -> Result<Span, ParserError<'a>> {
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
    fn unexpected(&self, expected: Expected<'a>) -> ParserError<'a> {
        let Token { kind, span } = self.token;
        match kind {
            TokenKind::Unknown(ch) => ParserError::UnknownChar { ch, span },
            TokenKind::InvalidInt(kind) => ParserError::InvalidInt { kind, span },
            actual => ParserError::UnexpectedToken {
                expected,
                actual,
                span,
            },
        }
    }

    fn parse_atom(&mut self) -> Result<AstNode, ParserError<'a>> {
        match self.token.kind {
            TokenKind::Int(n) => {
                self.bump();
                Ok(AstNode::Int(n))
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

    fn parse_binary(&mut self, min: u32) -> Result<AstNode, ParserError<'a>> {
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

    fn parse_expr(&mut self) -> Result<AstNode, ParserError<'a>> {
        self.parse_binary(0)
    }

    pub fn parse(&mut self) -> Result<AstNode, ParserError<'a>> {
        let expr = self.parse_expr()?;
        self.expect(TokenKind::Eof)?;
        Ok(expr)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn parse(src: &str) -> Result<AstNode, ParserError<'_>> {
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
                actual: TokenKind::Int(2),
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
                actual: TokenKind::Int(2),
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
                matches!(err, ParserError::UnknownChar { ch: '$', span } if span.start == start),
                "input: {src:?}, got: {err:?}"
            );
        }
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
            ("(1\n", Span { start: 3, end: 3 }),  // eof, REPL-style trailing newline
        ];
        for (src, expected) in cases {
            assert_eq!(parse(src).unwrap_err().span(), expected, "input: {src:?}");
        }
    }

    #[test]
    fn error_display_includes_span() {
        assert_eq!(
            parse("1 2").unwrap_err().to_string(),
            "Unexpected token at 2..3: expected <eof> but got <int: 2>"
        );
        assert_eq!(
            parse("1 +").unwrap_err().to_string(),
            "Unexpected token at 3..3: expected an expression but got <eof>"
        );
        assert_eq!(
            parse("1 $").unwrap_err().to_string(),
            "Unexpected char '$' at 2..3"
        );
        assert_eq!(
            parse("99999999999999999999").unwrap_err().to_string(),
            "Integer literal at 0..20 is too large for i64"
        );
    }
}
