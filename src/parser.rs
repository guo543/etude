use crate::ast::{Ast, BinOp, Decl, Expr, Ident, Stmt, UnOp};
use crate::lexer::{Lexer, Token, TokenKind};
use crate::span::Span;
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

    fn text(&self, span: Span) -> &'src str {
        &self.lexer.src()[span.start..span.end]
    }

    fn unary_op(kind: TokenKind) -> Option<UnOp> {
        match kind {
            TokenKind::Minus => Some(UnOp::Neg),
            _ => None,
        }
    }

    /// The binary operator a token stands for in infix position, with its
    /// precedence. Higher binds tighter; all of them are left-associative.
    fn binary_op(kind: TokenKind) -> Option<(BinOp, u32)> {
        match kind {
            TokenKind::Plus => Some((BinOp::Add, 1)),
            TokenKind::Minus => Some((BinOp::Sub, 1)),
            TokenKind::Star => Some((BinOp::Mul, 2)),
            TokenKind::Slash => Some((BinOp::Div, 2)),
            _ => None,
        }
    }

    fn bump(&mut self) {
        self.token = self.lexer.next_token();
    }

    fn peek_is(&self, kind: TokenKind) -> bool {
        self.token.kind == kind
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

    fn parse_ident(&mut self) -> Result<Ident, ParserError> {
        let span = self.expect(TokenKind::Ident)?;
        Ok(Ident {
            name: self.text(span).to_owned(),
            span,
        })
    }

    fn parse_let(&mut self) -> Result<Stmt, ParserError> {
        let start = self.expect(TokenKind::Let)?.start;

        let id = self.parse_ident()?;

        self.expect(TokenKind::Eq)?;

        let init = self.parse_expr()?;
        let end = self.expect(TokenKind::SemiColon)?.end;
        Ok(Stmt::let_decl(id, init, start, end))
    }

    fn parse_block(&mut self) -> Result<Expr, ParserError> {
        let start = self.expect(TokenKind::LBrace)?.start;

        let mut stmts = Vec::<Stmt>::new();
        loop {
            if self.peek_is(TokenKind::Let) {
                stmts.push(self.parse_let()?);
            } else {
                let expr = self.parse_expr()?;
                if self.peek_is(TokenKind::SemiColon) {
                    let start = expr.span.start;
                    let end = self.token.span.end;
                    self.bump();
                    stmts.push(Stmt::expr_stmt(expr, start, end));
                } else {
                    let end = self.expect(TokenKind::RBrace)?.end;
                    break Ok(Expr::block(stmts, expr, start, end));
                }
            }
        }
    }

    fn parse_atom(&mut self) -> Result<Expr, ParserError> {
        match self.token.kind {
            TokenKind::Int => {
                let span = self.token.span;
                let text = self.text(span);
                let n = text
                    .parse::<i64>()
                    .map(|n| Expr::int(n, span.start, span.end))
                    .map_err(|e| ParserError::InvalidInt {
                        kind: *e.kind(),
                        span,
                    })?;
                self.bump();
                Ok(n)
            }
            TokenKind::Ident => {
                let id = self.parse_ident()?;
                Ok(Expr::var(id.name, id.span.start, id.span.end))
            }
            TokenKind::LParen => {
                let start = self.token.span.start;
                self.bump();
                let mut expr = self.parse_expr()?;
                let end = self.expect(TokenKind::RParen)?.end;
                // Widen span to include the parenthesis
                expr.span = Span { start, end };
                Ok(expr)
            }
            TokenKind::LBrace => self.parse_block(),
            _ => Err(self.unexpected(Expected::Expression)),
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, ParserError> {
        if let Some(op) = Self::unary_op(self.token.kind) {
            let start = self.token.span.start;
            self.bump();
            let expr = self.parse_unary()?;
            let end = expr.span.end;
            Ok(Expr::unary(op, expr, start, end))
        } else {
            self.parse_atom()
        }
    }

    fn parse_binary(&mut self, min: u32) -> Result<Expr, ParserError> {
        let mut lhs = self.parse_unary()?;
        while let Some((op, prec)) = Parser::binary_op(self.token.kind)
            && prec >= min
        {
            self.bump();
            // `prec + 1`: an operator of the same precedence on the right
            // ends the operand, which makes the operators left-associative.
            let rhs = self.parse_binary(prec + 1)?;
            let start = lhs.span.start;
            let end = rhs.span.end;
            lhs = Expr::binary(op, lhs, rhs, start, end);
        }
        Ok(lhs)
    }

    fn parse_expr(&mut self) -> Result<Expr, ParserError> {
        self.parse_binary(0)
    }

    fn parse_function(&mut self) -> Result<Decl, ParserError> {
        let start = self.expect(TokenKind::Def)?.start;

        let id = self.parse_ident()?;

        self.expect(TokenKind::LParen)?;
        let mut params = Vec::<Ident>::new();
        if !self.peek_is(TokenKind::RParen) {
            loop {
                let param = self.parse_ident()?;
                params.push(param);
                if self.peek_is(TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
        }

        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::Eq)?;

        let body = self.parse_expr()?;
        let end = body.span.end;
        Ok(Decl::function(id, params, body, start, end))
    }

    pub fn parse(&mut self) -> Result<Ast, ParserError> {
        let mut decls = Vec::<Decl>::new();
        while !self.peek_is(TokenKind::Eof) {
            decls.push(self.parse_function()?);
        }
        Ok(Ast { decls })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ast::{DeclKind, ExprKind, StmtKind};

    /// Parse a single expression that must make up the whole input.
    fn parse(src: &str) -> Result<Expr, ParserError> {
        let mut parser = Parser::new(Lexer::new(src));
        let expr = parser.parse_expr()?;
        parser.expect(TokenKind::Eof)?;
        Ok(expr)
    }

    fn ast(src: &str) -> String {
        parse(src).unwrap().to_string()
    }

    fn parse_program(src: &str) -> Result<Ast, ParserError> {
        Parser::new(Lexer::new(src)).parse()
    }

    fn text(src: &str, span: Span) -> &str {
        &src[span.start..span.end]
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
    fn unary_minus() {
        assert_eq!(ast("-1"), "(- 1)");
        assert_eq!(ast("-2 * 3"), "((- 2) * 3)");
        assert_eq!(ast("2 * -3"), "(2 * (- 3))");
        assert_eq!(ast("1 - -2"), "(1 - (- 2))");
        assert_eq!(ast("--x"), "(- (- x))");
        assert_eq!(ast("-(1 + 2)"), "(- (1 + 2))");
        assert_eq!(ast("-{ 1 }"), "(- BLOCK: { 1 })");
    }

    #[test]
    fn unary_minus_spans() {
        let src = "1 + -x";
        let expr = parse(src).unwrap();
        let ExprKind::Binary(_, _, rhs) = &expr.kind else {
            panic!("expected a binary expression, got {expr}");
        };
        assert_eq!(text(src, rhs.span), "-x");
        let src = "-(1 + 2)";
        assert_eq!(text(src, parse(src).unwrap().span), "-(1 + 2)");
    }

    #[test]
    fn unary_minus_needs_an_operand() {
        for src in ["-", "1 + -", "- )"] {
            assert!(
                matches!(
                    parse(src),
                    Err(ParserError::UnexpectedToken {
                        expected: Expected::Expression,
                        ..
                    })
                ),
                "input: {src:?}"
            );
        }
    }

    #[test]
    fn variables() {
        assert_eq!(ast("x"), "x");
        assert_eq!(ast("x + y * 2"), "(x + (y * 2))");
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
            assert_eq!(text(src, err.span()), "$");
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
    fn number_followed_by_letters_is_invalid() {
        assert!(matches!(
            parse("123abc"),
            Err(ParserError::InvalidInt {
                kind: IntErrorKind::InvalidDigit,
                span: Span { start: 0, end: 6 },
            })
        ));
    }

    #[test]
    fn expression_spans() {
        let src = "1 + x * 3";
        let expr = parse(src).unwrap();
        assert_eq!(text(src, expr.span), "1 + x * 3");
        let ExprKind::Binary(_, lhs, rhs) = &expr.kind else {
            panic!("expected a binary expression, got {expr}");
        };
        assert_eq!(text(src, lhs.span), "1");
        assert_eq!(text(src, rhs.span), "x * 3");
    }

    #[test]
    fn blocks() {
        assert_eq!(ast("{ 1 }"), "BLOCK: { 1 }");
        assert_eq!(
            ast("{ let a = 1; let b = a + 2; a * b }"),
            "BLOCK: { LET: a = 1 ; LET: b = (a + 2) ; (a * b) }"
        );
        assert_eq!(ast("{ 1; 2 }"), "BLOCK: { EXPRSTMT: 1 ; 2 }");
        assert_eq!(
            ast("{ let x = { 1 }; x }"),
            "BLOCK: { LET: x = BLOCK: { 1 } ; x }"
        );
        assert_eq!(ast("1 + { 2 }"), "(1 + BLOCK: { 2 })");
    }

    #[test]
    fn block_and_statement_spans() {
        let src = "{ let a = 1; a + 1; a }";
        let expr = parse(src).unwrap();
        assert_eq!(text(src, expr.span), src);
        let ExprKind::Block { stmts, tail } = &expr.kind else {
            panic!("expected a block, got {expr}");
        };
        assert_eq!(text(src, stmts[0].span), "let a = 1;");
        assert_eq!(text(src, stmts[1].span), "a + 1;");
        assert_eq!(text(src, tail.span), "a");
        let StmtKind::Let { id, .. } = &stmts[0].kind else {
            panic!("expected a let statement");
        };
        assert_eq!(text(src, id.span), "a");
    }

    #[test]
    fn block_errors() {
        // The block must end in an expression.
        assert!(matches!(
            parse("{ let a = 1; }"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Expression,
                actual: TokenKind::RBrace,
                ..
            })
        ));
        assert!(matches!(
            parse("{ }"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Expression,
                actual: TokenKind::RBrace,
                ..
            })
        ));
        // Missing `;` between statements.
        assert!(matches!(
            parse("{ 1 2 }"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::RBrace),
                actual: TokenKind::Int,
                ..
            })
        ));
        assert!(matches!(
            parse("{ 1"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::RBrace),
                actual: TokenKind::Eof,
                ..
            })
        ));
        // `let` needs a name, `=` and `;`.
        assert!(matches!(
            parse("{ let = 1; 1 }"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Ident),
                ..
            })
        ));
        assert!(matches!(
            parse("{ let a 1; 1 }"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Eq),
                ..
            })
        ));
        assert!(matches!(
            parse("{ let a = 1 a }"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::SemiColon),
                ..
            })
        ));
    }

    #[test]
    fn functions() {
        let src = "def main() = 1 def add(a, b) = a + b";
        assert_eq!(
            parse_program(src).unwrap().to_string(),
            "AST:\nDEF main () = 1\nDEF add (a, b) = (a + b)\n"
        );
    }

    #[test]
    fn function_parameters_and_spans() {
        let src = "def f(x, yy, zzz) = { x }";
        let ast = parse_program(src).unwrap();
        let DeclKind::FunDecl { id, params, body } = &ast.decls[0].kind;
        assert_eq!(id.name, "f");
        let names: Vec<_> = params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["x", "yy", "zzz"]);
        for param in params {
            assert_eq!(text(src, param.span), param.name);
        }
        assert_eq!(text(src, body.span), "{ x }");
        assert_eq!(text(src, ast.decls[0].span), src);
    }

    #[test]
    fn empty_program() {
        assert!(parse_program("").unwrap().decls.is_empty());
    }

    #[test]
    fn program_errors() {
        // Only `def` is allowed at the top level for now.
        assert!(matches!(
            parse_program("1 + 2"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Def),
                actual: TokenKind::Int,
                span: Span { start: 0, end: 1 },
            })
        ));
        assert!(matches!(
            parse_program("def f() = 1 let x = 2;"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Def),
                actual: TokenKind::Let,
                ..
            })
        ));
        // No trailing comma, and no empty parameter between commas.
        assert!(matches!(
            parse_program("def f(a,) = 1"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Ident),
                actual: TokenKind::RParen,
                span: Span { start: 8, end: 9 },
            })
        ));
        assert!(matches!(
            parse_program("def f(,) = 1"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Ident),
                actual: TokenKind::Comma,
                ..
            })
        ));
        assert!(matches!(
            parse_program("def = 1"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Ident),
                ..
            })
        ));
        assert!(matches!(
            parse_program("def f = 1"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::LParen),
                ..
            })
        ));
        assert!(matches!(
            parse_program("def f(a b) = 1"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::RParen),
                actual: TokenKind::Ident,
                ..
            })
        ));
        assert!(matches!(
            parse_program("def f() 1"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Eq),
                ..
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
