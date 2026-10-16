use crate::ast::{Ast, BinOp, Decl, Expr, Ident, Stmt, UnOp};
use crate::diagnostic::Diagnostic;
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

/// How it is named in error messages, e.g. "expected an expression".
impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expected::Token(kind) => write!(f, "{kind}"),
            Expected::Expression => write!(f, "an expression"),
        }
    }
}

#[derive(Debug)]
pub enum ParserError {
    /// The lexer found a character that cannot start a token.
    UnknownChar {
        ch: char,
        span: Span,
    },
    /// An integer literal that does not fit in an `i64`.
    InvalidInt {
        kind: IntErrorKind,
        span: Span,
    },
    UnexpectedToken {
        expected: Expected,
        actual: TokenKind,
        span: Span,
    },
    ChainedComparison {
        span: Span,
    },
}

impl ParserError {
    pub fn span(&self) -> Span {
        match self {
            ParserError::UnknownChar { span, .. }
            | ParserError::InvalidInt { span, .. }
            | ParserError::UnexpectedToken { span, .. } => *span,
            ParserError::ChainedComparison { span } => *span,
        }
    }
}

/// The headline of the error; `diagnostic` adds the location and details.
impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParserError::UnknownChar { ch, .. } => write!(f, "unknown character `{ch}`"),
            ParserError::InvalidInt {
                kind: IntErrorKind::PosOverflow,
                ..
            } => write!(f, "integer literal is too large"),
            ParserError::InvalidInt { .. } => write!(f, "invalid integer literal"),
            ParserError::UnexpectedToken {
                expected, actual, ..
            } => write!(f, "expected {expected}, found {actual}"),
            ParserError::ChainedComparison { .. } => {
                write!(f, "comparison operators cannot be chained")
            }
        }
    }
}

impl ParserError {
    /// The error as a report for the user: the `Display` headline plus where
    /// it happened and what to say there.
    pub fn diagnostic(&self) -> Diagnostic {
        let label = match self {
            ParserError::UnknownChar { .. } => "not part of the language".to_owned(),
            ParserError::InvalidInt {
                kind: IntErrorKind::PosOverflow,
                ..
            } => "does not fit in an `i64`".to_owned(),
            ParserError::InvalidInt { .. } => "numbers can only contain digits".to_owned(),
            ParserError::UnexpectedToken { expected, .. } => {
                format!("expected {expected}")
            }
            ParserError::ChainedComparison { .. } => "cannot follow another comparison".to_owned(),
        };
        let diagnostic = Diagnostic::new(self.to_string(), self.span(), label);
        match self {
            ParserError::InvalidInt {
                kind: IntErrorKind::PosOverflow,
                ..
            } => diagnostic.with_note(format!("the largest integer is {}", i64::MAX)),
            ParserError::ChainedComparison { .. } => {
                diagnostic.with_note("use `&&` to combine comparisons, e.g. `a < b && b < c`")
            }
            _ => diagnostic,
        }
    }
}

/// How operators of the same precedence group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Assoc {
    /// `a - b - c` is `(a - b) - c`.
    Left,
    /// `a < b < c` is an error.
    None,
}

/// A recursive-descent parser in the style of rustc's: it always holds the
/// current token and moves forward with `bump`. Lookahead is just reading
/// `self.token`, which is `Copy`, so nothing stays borrowed.
pub struct Parser<'src> {
    lexer: Lexer<'src>,
    /// The token being looked at. `Eof` once the input runs out.
    token: Token,
    /// Where the previous token ended. Errors at the end of input point
    /// here, right after the last real token, not past trailing whitespace.
    prev_end: usize,
}

impl<'src> Parser<'src> {
    pub fn new(mut lexer: Lexer<'src>) -> Self {
        let token = lexer.next_token();
        Parser {
            lexer,
            token,
            prev_end: 0,
        }
    }

    fn text(&self, span: Span) -> &'src str {
        &self.lexer.src()[span.start..span.end]
    }

    fn unary_op(kind: TokenKind) -> Option<UnOp> {
        match kind {
            TokenKind::Minus => Some(UnOp::Neg),
            TokenKind::Bang => Some(UnOp::Not),
            _ => None,
        }
    }

    /// The binary operator a token stands for in infix position, with its
    /// precedence (higher binds tighter) and associativity.
    fn binary_op(kind: TokenKind) -> Option<(BinOp, u32, Assoc)> {
        match kind {
            // Logical
            TokenKind::OrOr => Some((BinOp::Or, 1, Assoc::Left)),
            TokenKind::AndAnd => Some((BinOp::And, 2, Assoc::Left)),

            // Comparison
            TokenKind::EqEq => Some((BinOp::Eq, 3, Assoc::None)),
            TokenKind::Ne => Some((BinOp::Ne, 3, Assoc::None)),
            TokenKind::Lt => Some((BinOp::Lt, 3, Assoc::None)),
            TokenKind::Le => Some((BinOp::Le, 3, Assoc::None)),
            TokenKind::Gt => Some((BinOp::Gt, 3, Assoc::None)),
            TokenKind::Ge => Some((BinOp::Ge, 3, Assoc::None)),

            // Arithmetic
            TokenKind::Plus => Some((BinOp::Add, 4, Assoc::Left)),
            TokenKind::Minus => Some((BinOp::Sub, 4, Assoc::Left)),
            TokenKind::Star => Some((BinOp::Mul, 5, Assoc::Left)),
            TokenKind::Slash => Some((BinOp::Div, 5, Assoc::Left)),

            _ => None,
        }
    }

    fn bump(&mut self) {
        self.prev_end = self.token.span.end;
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
            TokenKind::Unknown => ParserError::UnknownChar {
                // An `Unknown` token is exactly one character.
                ch: self.text(span).chars().next().unwrap_or_default(),
                span,
            },
            TokenKind::Eof => ParserError::UnexpectedToken {
                expected,
                actual: kind,
                span: Span {
                    start: self.prev_end,
                    end: self.prev_end,
                },
            },
            actual => ParserError::UnexpectedToken {
                expected,
                actual,
                span,
            },
        }
    }

    fn parse_comma_list<T>(
        &mut self,
        close: TokenKind,
        mut parse_elem: impl FnMut(&mut Self) -> Result<T, ParserError>,
    ) -> Result<Vec<T>, ParserError> {
        let mut res = Vec::new();
        while !self.peek_is(close) {
            let elem = parse_elem(self)?;
            res.push(elem);
            if self.peek_is(TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(res)
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
        let end = self.expect(TokenKind::Semicolon)?.end;
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
                if self.peek_is(TokenKind::Semicolon) {
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

    fn parse_if(&mut self) -> Result<Expr, ParserError> {
        let start = self.expect(TokenKind::If)?.start;
        let cond = self.parse_expr()?;
        let t_branch = self.parse_block()?;
        let (f_branch, end) = if let TokenKind::Else = self.token.kind {
            self.bump();
            // `else if …` chains: the else branch is another `if`.
            let f_branch = if let TokenKind::If = self.token.kind {
                self.parse_if()?
            } else {
                self.parse_block()?
            };
            let end = f_branch.span.end;
            (Some(f_branch), end)
        } else {
            (None, t_branch.span.end)
        };

        Ok(Expr::if_expr(cond, t_branch, f_branch, start, end))
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
            TokenKind::True => {
                let span = self.token.span;
                self.bump();
                Ok(Expr::bool(true, span.start, span.end))
            }
            TokenKind::False => {
                let span = self.token.span;
                self.bump();
                Ok(Expr::bool(false, span.start, span.end))
            }
            TokenKind::Ident => {
                let id = self.parse_ident()?;
                Ok(Expr::var(id.name, id.span.start, id.span.end))
            }
            TokenKind::If => self.parse_if(),
            TokenKind::Fun => {
                let start = self.token.span.start;
                self.bump();

                self.expect(TokenKind::LParen)?;
                let params = self.parse_comma_list(TokenKind::RParen, Self::parse_ident)?;
                self.expect(TokenKind::RParen)?;

                self.expect(TokenKind::FatArrow)?;

                let body = self.parse_expr()?;
                let end = body.span.end;
                Ok(Expr::lambda(params, body, start, end))
            }
            TokenKind::LParen => {
                let start = self.token.span.start;
                self.bump();
                if self.peek_is(TokenKind::RParen) {
                    // Unit literal
                    let end = self.token.span.end;
                    self.bump();
                    Ok(Expr::unit(start, end))
                } else {
                    let mut expr = self.parse_expr()?;
                    let end = self.expect(TokenKind::RParen)?.end;
                    // Widen span to include the parenthesis
                    expr.span = Span { start, end };
                    Ok(expr)
                }
            }
            TokenKind::LBrace => self.parse_block(),
            _ => Err(self.unexpected(Expected::Expression)),
        }
    }

    fn parse_call(&mut self) -> Result<Expr, ParserError> {
        let mut expr = self.parse_atom()?;

        while self.peek_is(TokenKind::LParen) {
            self.bump();
            let args = self.parse_comma_list(TokenKind::RParen, Self::parse_expr)?;
            let start = expr.span.start;
            let end = self.expect(TokenKind::RParen)?.end;
            expr = Expr::call(expr, args, start, end)
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParserError> {
        if let Some(op) = Self::unary_op(self.token.kind) {
            let start = self.token.span.start;
            self.bump();
            let expr = self.parse_unary()?;
            let end = expr.span.end;
            Ok(Expr::unary(op, expr, start, end))
        } else {
            self.parse_call()
        }
    }

    fn parse_binary(&mut self, min: u32) -> Result<Expr, ParserError> {
        let mut lhs = self.parse_unary()?;
        // Precedence of the previous operator in this loop, if it was
        // non-associative.
        let mut prev_non_assoc = None;
        while let Some((op, prec, assoc)) = Parser::binary_op(self.token.kind)
            && prec >= min
        {
            // A non-associative operator cannot follow another one of the
            // same precedence: `a < b < c` is an error, not `(a < b) < c`.
            if assoc == Assoc::None && prev_non_assoc == Some(prec) {
                return Err(ParserError::ChainedComparison {
                    span: self.token.span,
                });
            }
            self.bump();
            // `prec + 1`: an operator of the same precedence on the right ends
            // the operand, so it comes back to this loop. Left-associative
            // operators continue here; non-associative ones hit the check
            // above.
            let rhs = self.parse_binary(prec + 1)?;
            let start = lhs.span.start;
            let end = rhs.span.end;
            lhs = Expr::binary(op, lhs, rhs, start, end);
            prev_non_assoc = (assoc == Assoc::None).then_some(prec);
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
        let params = self.parse_comma_list(TokenKind::RParen, Self::parse_ident)?;

        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::Eq)?;

        let body = self.parse_expr()?;
        let end = self.expect(TokenKind::Semicolon)?.end;
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
    fn calls() {
        assert_eq!(ast("f()"), "f()");
        assert_eq!(ast("f(1)"), "f(1)");
        assert_eq!(ast("f(1, 2, 3)"), "f(1, 2, 3)");
        assert_eq!(ast("f (1)"), "f(1)");
        assert_eq!(ast("f(g(1), h())"), "f(g(1), h())");
        assert_eq!(ast("f({ 1 }, 2 + 3)"), "f(BLOCK: { 1 }, (2 + 3))");
    }

    #[test]
    fn calls_bind_tighter_than_operators() {
        assert_eq!(ast("1 + f(2) * 3"), "(1 + (f(2) * 3))");
        assert_eq!(ast("f(1) - g(2)"), "(f(1) - g(2))");
        assert_eq!(ast("-f(1)"), "(- f(1))");
        assert_eq!(ast("-f(1)(2)"), "(- f(1)(2))");
    }

    #[test]
    fn chained_calls() {
        assert_eq!(ast("f(1)(2)"), "f(1)(2)");
        assert_eq!(ast("f()()()"), "f()()()");

        // The outer call's callee is the inner call.
        let src = "f(1)(2)";
        let expr = parse(src).unwrap();
        let ExprKind::Call { callee, args } = &expr.kind else {
            panic!("expected a call, got {expr}");
        };
        assert!(matches!(callee.kind, ExprKind::Call { .. }));
        assert_eq!(text(src, callee.span), "f(1)");
        assert_eq!(text(src, args[0].span), "2");
    }

    #[test]
    fn any_expression_can_be_called() {
        assert_eq!(ast("(f)(1)"), "f(1)");
        assert_eq!(ast("{ f }(1)"), "BLOCK: { f }(1)");
    }

    #[test]
    fn call_spans() {
        let src = "1 + f(2, x * 3)";
        let expr = parse(src).unwrap();
        let ExprKind::Binary(_, _, call) = &expr.kind else {
            panic!("expected a binary expression, got {expr}");
        };
        assert_eq!(text(src, call.span), "f(2, x * 3)");
        let ExprKind::Call { callee, args } = &call.kind else {
            panic!("expected a call, got {call}");
        };
        assert_eq!(text(src, callee.span), "f");
        let arg_texts: Vec<_> = args.iter().map(|a| text(src, a.span)).collect();
        assert_eq!(arg_texts, ["2", "x * 3"]);

        let src = "(f)(1)";
        assert_eq!(text(src, parse(src).unwrap().span), "(f)(1)");
    }

    #[test]
    fn trailing_commas() {
        assert_eq!(ast("f(1,)"), "f(1)");
        assert_eq!(ast("f(1, 2,)"), "f(1, 2)");
        assert_eq!(ast("f(\n    1,\n    2,\n)"), "f(1, 2)");
        assert_eq!(ast("f(1,)(2,)"), "f(1)(2)");
        assert_eq!(ast("fun(x,) => x"), "FUN (x) => x");
        assert_eq!(ast("fun(x, y,) => x"), "FUN (x, y) => x");
        assert_eq!(
            parse_program("def f(a, b,) = a;").unwrap().to_string(),
            "AST:\nDEF f (a, b) = a\n"
        );
        // The comma does not count as an argument or end up in the call's span.
        let src = "f(1,)";
        let expr = parse(src).unwrap();
        let ExprKind::Call { args, .. } = &expr.kind else {
            panic!("expected a call, got {expr}");
        };
        assert_eq!(args.len(), 1);
        assert_eq!(text(src, expr.span), "f(1,)");
    }

    #[test]
    fn call_errors() {
        // A trailing comma is fine, but an empty argument is not.
        for (src, expected, actual) in [
            ("f(,)", Expected::Expression, TokenKind::Comma),
            ("f(1,,2)", Expected::Expression, TokenKind::Comma),
            ("f(1,", Expected::Expression, TokenKind::Eof),
            ("f(", Expected::Expression, TokenKind::Eof),
            ("f(1 2)", Expected::Token(TokenKind::RParen), TokenKind::Int),
            ("f(1", Expected::Token(TokenKind::RParen), TokenKind::Eof),
            ("f(1)(", Expected::Expression, TokenKind::Eof),
        ] {
            let err = parse(src).unwrap_err();
            assert!(
                matches!(
                    err,
                    ParserError::UnexpectedToken { expected: e, actual: a, .. }
                        if e == expected && a == actual
                ),
                "input: {src:?}, got: {err:?}"
            );
        }
    }

    #[test]
    fn calls_in_programs() {
        let src = "def add(a, b) = a + b; def main() = add(1, add(2, 3));";
        assert_eq!(
            parse_program(src).unwrap().to_string(),
            "AST:\nDEF add (a, b) = (a + b)\nDEF main () = add(1, add(2, 3))\n"
        );
    }

    #[test]
    fn not() {
        assert_eq!(ast("!a"), "(! a)");
        assert_eq!(ast("!!a"), "(! (! a))");
        assert_eq!(ast("!-a"), "(! (- a))");
        // Prefix operators bind tighter than any binary operator.
        assert_eq!(ast("!a && b"), "((! a) && b)");
        assert_eq!(ast("-a < b"), "((- a) < b)");
        assert_eq!(ast("!f(x)"), "(! f(x))");
    }

    #[test]
    fn operator_precedence() {
        // Lowest to highest: `||`, `&&`, comparisons, `+ -`, `* /`.
        assert_eq!(ast("a || b && c"), "(a || (b && c))");
        assert_eq!(ast("a && b || c"), "((a && b) || c)");
        assert_eq!(ast("a == b && c"), "((a == b) && c)");
        assert_eq!(ast("a && b != c"), "(a && (b != c))");
        assert_eq!(ast("1 + 2 < 3 * 4"), "((1 + 2) < (3 * 4))");
        assert_eq!(ast("a < b || c >= d && e"), "((a < b) || ((c >= d) && e))");
    }

    #[test]
    fn comparison_operators() {
        for op in ["==", "!=", "<", "<=", ">", ">="] {
            let src = format!("a {op} b");
            assert_eq!(ast(&src), format!("(a {op} b)"), "input: {src:?}");
        }
    }

    #[test]
    fn logical_operators_are_left_associative() {
        assert_eq!(ast("a || b || c"), "((a || b) || c)");
        assert_eq!(ast("a && b && c"), "((a && b) && c)");
    }

    #[test]
    fn comparisons_cannot_be_chained() {
        // The error points at the second comparison operator.
        for (src, at) in [
            ("a < b < c", 6),
            ("a == b != c", 7),
            ("a < b == c", 6),
            ("a < b + 1 >= c", 10),
            ("x && a <= b > c", 12),
        ] {
            let err = parse(src).unwrap_err();
            assert!(
                matches!(err, ParserError::ChainedComparison { span } if span.start == at),
                "input: {src:?}, got: {err:?}"
            );
        }
    }

    #[test]
    fn separate_comparisons_are_fine() {
        assert_eq!(ast("a < b && b < c"), "((a < b) && (b < c))");
        // Parentheses make the grouping explicit.
        assert_eq!(ast("(a < b) == c"), "((a < b) == c)");
        assert_eq!(ast("a == (b < c)"), "(a == (b < c))");
    }

    #[test]
    fn chained_comparison_diagnostic() {
        let src = "a < b < c";
        let diag = parse(src).unwrap_err().diagnostic();
        assert_eq!(diag.message, "comparison operators cannot be chained");
        let (span, label) = diag.label.unwrap();
        assert_eq!(text(src, span), "<");
        assert_eq!(label, "cannot follow another comparison");
        assert_eq!(
            diag.notes,
            ["use `&&` to combine comparisons, e.g. `a < b && b < c`"]
        );
    }

    #[test]
    fn if_expressions() {
        assert_eq!(ast("if c { 1 }"), "(IF c BLOCK: { 1 })");
        assert_eq!(
            ast("if c { 1 } else { 2 }"),
            "(IF c BLOCK: { 1 } BLOCK: { 2 })"
        );
        assert_eq!(
            ast("if a < b && c { 1 } else { 2 }"),
            "(IF ((a < b) && c) BLOCK: { 1 } BLOCK: { 2 })"
        );
    }

    #[test]
    fn else_if_chains() {
        assert_eq!(
            ast("if a { 1 } else if b { 2 } else { 3 }"),
            "(IF a BLOCK: { 1 } (IF b BLOCK: { 2 } BLOCK: { 3 }))"
        );
        assert_eq!(
            ast("if a { 1 } else if b { 2 }"),
            "(IF a BLOCK: { 1 } (IF b BLOCK: { 2 }))"
        );
    }

    #[test]
    fn if_is_an_expression() {
        assert_eq!(
            ast("1 + if c { 2 } else { 3 }"),
            "(1 + (IF c BLOCK: { 2 } BLOCK: { 3 }))"
        );
        assert_eq!(
            ast("{ let x = if c { 1 } else { 2 }; x }"),
            "BLOCK: { LET: x = (IF c BLOCK: { 1 } BLOCK: { 2 }) ; x }"
        );
        assert_eq!(
            ast("f(if c { 1 } else { 2 })"),
            "f((IF c BLOCK: { 1 } BLOCK: { 2 }))"
        );
    }

    #[test]
    fn if_spans() {
        for src in [
            "if c { 1 }",
            "if c { 1 } else { 2 }",
            "if a { 1 } else if b { 2 } else { 3 }",
        ] {
            assert_eq!(text(src, parse(src).unwrap().span), src);
        }
        let src = "if a { 1 } else if b { 2 }";
        let ExprKind::If {
            f_branch: else_branch,
            ..
        } = &parse(src).unwrap().kind
        else {
            panic!("expected an if");
        };
        assert_eq!(text(src, else_branch.as_ref().unwrap().span), "if b { 2 }");
    }

    #[test]
    fn if_errors() {
        for (src, expected, actual) in [
            // Branches must be blocks.
            ("if c 1", Expected::Token(TokenKind::LBrace), TokenKind::Int),
            (
                "if c { 1 } else 2",
                Expected::Token(TokenKind::LBrace),
                TokenKind::Int,
            ),
            (
                "if c { 1 } else",
                Expected::Token(TokenKind::LBrace),
                TokenKind::Eof,
            ),
            // A missing condition: the block is read as the condition.
            (
                "if { 1 }",
                Expected::Token(TokenKind::LBrace),
                TokenKind::Eof,
            ),
            // `else` on its own.
            ("else { 1 }", Expected::Expression, TokenKind::Else),
        ] {
            let err = parse(src).unwrap_err();
            assert!(
                matches!(
                    err,
                    ParserError::UnexpectedToken { expected: e, actual: a, .. }
                        if e == expected && a == actual
                ),
                "input: {src:?}, got: {err:?}"
            );
        }
    }

    #[test]
    fn booleans() {
        assert_eq!(ast("true"), "true");
        assert_eq!(ast("false"), "false");
        assert!(matches!(parse("true").unwrap().kind, ExprKind::Bool(true)));
        assert!(matches!(
            parse("false").unwrap().kind,
            ExprKind::Bool(false)
        ));
    }

    #[test]
    fn booleans_in_expressions() {
        assert_eq!(ast("f(true, false)"), "f(true, false)");
        assert_eq!(ast("{ let b = true; b }"), "BLOCK: { LET: b = true ; b }");
        assert_eq!(ast("fun() => false"), "FUN () => false");
        // Type errors are the evaluator's job; these parse.
        assert_eq!(ast("true + 1"), "(true + 1)");
        assert_eq!(ast("-false"), "(- false)");
    }

    #[test]
    fn boolean_spans() {
        let src = "f(1, true)";
        let ExprKind::Call { args, .. } = &parse(src).unwrap().kind else {
            panic!("expected a call");
        };
        assert_eq!(text(src, args[1].span), "true");
    }

    #[test]
    fn booleans_are_not_names() {
        for (src, actual) in [
            ("{ let true = 1; 1 }", TokenKind::True),
            ("fun(false) => 1", TokenKind::False),
        ] {
            assert!(
                matches!(
                    parse(src),
                    Err(ParserError::UnexpectedToken {
                        expected: Expected::Token(TokenKind::Ident),
                        actual: a,
                        ..
                    }) if a == actual
                ),
                "input: {src:?}"
            );
        }
        assert!(matches!(
            parse_program("def true() = 1;"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Ident),
                actual: TokenKind::True,
                ..
            })
        ));
    }

    #[test]
    fn unit_literal() {
        assert_eq!(ast("()"), "()");
        assert_eq!(ast("( )"), "()");
        assert!(matches!(parse("()").unwrap().kind, ExprKind::Unit));
        // `(())` is unit in parentheses, not a nested unit type.
        assert!(matches!(parse("(())").unwrap().kind, ExprKind::Unit));
    }

    #[test]
    fn unit_in_expressions() {
        assert_eq!(ast("f(())"), "f(())");
        assert_eq!(ast("f((), 1)"), "f((), 1)");
        assert_eq!(ast("{ let u = (); u }"), "BLOCK: { LET: u = () ; u }");
        assert_eq!(ast("fun() => ()"), "FUN () => ()");
        assert_eq!(ast("() == ()"), "(() == ())");
        // A call with no arguments is still a call, not a call on unit.
        assert_eq!(ast("f()"), "f()");
    }

    #[test]
    fn unit_spans() {
        let src = "f(1, ( ))";
        let ExprKind::Call { args, .. } = &parse(src).unwrap().kind else {
            panic!("expected a call");
        };
        assert_eq!(text(src, args[1].span), "( )");
        // Parentheses around unit widen the span like any other expression.
        let src = "(())";
        assert_eq!(text(src, parse(src).unwrap().span), "(())");
    }

    #[test]
    fn unclosed_unit() {
        assert!(matches!(
            parse("("),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Expression,
                actual: TokenKind::Eof,
                ..
            })
        ));
    }

    #[test]
    fn lambdas() {
        assert_eq!(ast("fun() => 1"), "FUN () => 1");
        assert_eq!(ast("fun(x) => x"), "FUN (x) => x");
        assert_eq!(ast("fun(a, b) => a * b"), "FUN (a, b) => (a * b)");
        assert_eq!(ast("fun (x) => x"), "FUN (x) => x");
        assert_eq!(
            ast("fun(x) => { let y = x; y }"),
            "FUN (x) => BLOCK: { LET: y = x ; y }"
        );
    }

    #[test]
    fn lambda_body_extends_as_far_right_as_possible() {
        assert_eq!(ast("fun(x) => x + 1 * 2"), "FUN (x) => (x + (1 * 2))");
        // A lambda returning a lambda (currying).
        assert_eq!(
            ast("fun(a) => fun(b) => a + b"),
            "FUN (a) => FUN (b) => (a + b)"
        );
        // A call after the body belongs to the body, not to the lambda.
        let expr = parse("fun(f) => f(1)").unwrap();
        let ExprKind::Lambda { body, .. } = &expr.kind else {
            panic!("expected a lambda, got {expr}");
        };
        assert!(matches!(body.kind, ExprKind::Call { .. }), "got {body}");
    }

    #[test]
    fn lambda_body_ends_at_comma_and_closing_paren() {
        assert_eq!(ast("g(fun(x) => x * 2, 3)"), "g(FUN (x) => (x * 2), 3)");
        // Parenthesized, a lambda can be called right away.
        let expr = parse("(fun(x) => x + 1)(41)").unwrap();
        let ExprKind::Call { callee, args } = &expr.kind else {
            panic!("expected a call, got {expr}");
        };
        assert!(
            matches!(callee.kind, ExprKind::Lambda { .. }),
            "got {callee}"
        );
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn lambda_spans() {
        let src = "g(fun(x, yy) => x * yy, 3)";
        let expr = parse(src).unwrap();
        let ExprKind::Call { args, .. } = &expr.kind else {
            panic!("expected a call, got {expr}");
        };
        assert_eq!(text(src, args[0].span), "fun(x, yy) => x * yy");
        let ExprKind::Lambda { params, body } = &args[0].kind else {
            panic!("expected a lambda, got {}", args[0]);
        };
        let names: Vec<_> = params.iter().map(|p| text(src, p.span)).collect();
        assert_eq!(names, ["x", "yy"]);
        assert_eq!(text(src, body.span), "x * yy");

        // Including the parentheses around it.
        let src = "(fun() => 1)";
        assert_eq!(text(src, parse(src).unwrap().span), "(fun() => 1)");
    }

    #[test]
    fn lambda_errors() {
        for (src, expected, actual) in [
            (
                "fun x = 1",
                Expected::Token(TokenKind::LParen),
                TokenKind::Ident,
            ),
            (
                "fun(x) x",
                Expected::Token(TokenKind::FatArrow),
                TokenKind::Ident,
            ),
            // `=` is for `def`; a lambda's body follows `=>`.
            (
                "fun(x) = x",
                Expected::Token(TokenKind::FatArrow),
                TokenKind::Eq,
            ),
            (
                "fun x => 1",
                Expected::Token(TokenKind::LParen),
                TokenKind::Ident,
            ),
            (
                "fun(x,,y) => x",
                Expected::Token(TokenKind::Ident),
                TokenKind::Comma,
            ),
            (
                "fun(1) => x",
                Expected::Token(TokenKind::Ident),
                TokenKind::Int,
            ),
            ("fun(x) =>", Expected::Expression, TokenKind::Eof),
            (
                "fun(x = x",
                Expected::Token(TokenKind::RParen),
                TokenKind::Eq,
            ),
        ] {
            let err = parse(src).unwrap_err();
            assert!(
                matches!(
                    err,
                    ParserError::UnexpectedToken { expected: e, actual: a, .. }
                        if e == expected && a == actual
                ),
                "input: {src:?}, got: {err:?}"
            );
        }
    }

    #[test]
    fn lambdas_in_programs() {
        let src = "def add(a) = fun(b) => a + b; def main() = add(1)(2);";
        assert_eq!(
            parse_program(src).unwrap().to_string(),
            "AST:\nDEF add (a) = FUN (b) => (a + b)\nDEF main () = add(1)(2)\n"
        );
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
                matches!(err, ParserError::UnknownChar { ch: '$', span } if span.start == start),
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
                expected: Expected::Token(TokenKind::Semicolon),
                ..
            })
        ));
    }

    #[test]
    fn functions() {
        let src = "def main() = 1; def add(a, b) = a + b;";
        assert_eq!(
            parse_program(src).unwrap().to_string(),
            "AST:\nDEF main () = 1\nDEF add (a, b) = (a + b)\n"
        );
    }

    #[test]
    fn function_parameters_and_spans() {
        let src = "def f(x, yy, zzz) = { x };";
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
            parse_program("def f() = 1; let x = 2;"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Def),
                actual: TokenKind::Let,
                ..
            })
        ));
        // A trailing comma is fine, but an empty parameter is not.
        assert!(matches!(
            parse_program("def f(a,,b) = 1"),
            Err(ParserError::UnexpectedToken {
                expected: Expected::Token(TokenKind::Ident),
                actual: TokenKind::Comma,
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
    fn def_requires_semicolon() {
        for (src, actual) in [
            ("def f() = 1", TokenKind::Eof),
            ("def f() = 1 def g() = 2;", TokenKind::Def),
            ("def f() = { 1 }", TokenKind::Eof),
            ("def f() = 1 2;", TokenKind::Int),
        ] {
            assert!(
                matches!(
                    parse_program(src),
                    Err(ParserError::UnexpectedToken {
                        expected: Expected::Token(TokenKind::Semicolon),
                        actual: a,
                        ..
                    }) if a == actual
                ),
                "input: {src:?}"
            );
        }
        assert_eq!(parse_program("def f() = { 1 };").unwrap().decls.len(), 1);
    }

    #[test]
    fn every_error_has_a_span() {
        let cases = [
            ("1 + $", Span { start: 4, end: 5 }), // unknown char
            ("99999999999999999999", Span { start: 0, end: 20 }), // invalid int
            ("1 2", Span { start: 2, end: 3 }),   // unexpected token
            // At end of input, the error points right after the last token.
            ("1 +  ", Span { start: 3, end: 3 }), // eof, before trailing whitespace
            ("(1\n\n", Span { start: 2, end: 2 }), // eof, before trailing newlines
        ];
        for (src, expected) in cases {
            assert_eq!(parse(src).unwrap_err().span(), expected, "input: {src:?}");
        }
    }

    #[test]
    fn error_display_is_the_headline() {
        let cases = [
            ("1 2", "expected end of input, found an integer"),
            ("1 +", "expected an expression, found end of input"),
            ("(1 2", "expected `)`, found an integer"),
            ("1 $", "unknown character `$`"),
            ("99999999999999999999", "integer literal is too large"),
            ("123abc", "invalid integer literal"),
        ];
        for (src, expected) in cases {
            assert_eq!(
                parse(src).unwrap_err().to_string(),
                expected,
                "input: {src:?}"
            );
        }
    }

    #[test]
    fn diagnostics() {
        let src = "1 $ 2";
        let diag = parse(src).unwrap_err().diagnostic();
        assert_eq!(diag.message, "unknown character `$`");
        assert_eq!(
            diag.label,
            Some((Span { start: 2, end: 3 }, "not part of the language".into()))
        );

        let src = "{ 1";
        let diag = parse(src).unwrap_err().diagnostic();
        assert_eq!(diag.message, "expected `}`, found end of input");
        assert_eq!(
            diag.label,
            Some((Span { start: 3, end: 3 }, "expected `}`".into()))
        );

        let src = "99999999999999999999";
        let diag = parse(src).unwrap_err().diagnostic();
        assert_eq!(diag.notes, ["the largest integer is 9223372036854775807"]);
    }
}
