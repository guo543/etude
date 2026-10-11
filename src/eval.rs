use crate::{
    ast::{Ast, BinOp, Decl, DeclKind, Expr, ExprKind, Ident, Stmt, StmtKind, UnOp},
    diagnostic::Diagnostic,
    span::Span,
};
use std::{collections::HashMap, fmt, rc::Rc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Int,
    Fun,
}

/// The type's name, as it will appear in signatures and type errors.
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Fun => write!(f, "fun"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    /// The divisor evaluated to zero; the span is the divisor.
    DivisionByZero(Span),
    UnboundIdent {
        name: String,
        span: Span,
    },
    IntegerOverflow(Span),
    TypeMismatch {
        expected: Type,
        actual: Type,
        span: Span,
    },
    ArgumentCountMismatch {
        expected: usize,
        actual: usize,
        span: Span,
    },
    /// The program defines no `main` function.
    MissingMain,
    /// `main` has parameters; the span covers them.
    MainHasParams(Span),
}

/// The headline of the error; `diagnostic` adds the location and details.
impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EvalError::DivisionByZero(_) => write!(f, "division by zero"),
            EvalError::UnboundIdent { name, .. } => {
                write!(f, "cannot find `{name}` in this scope")
            }
            EvalError::IntegerOverflow(_) => write!(f, "integer overflow"),
            EvalError::TypeMismatch { .. } => write!(f, "mismatched types"),
            EvalError::ArgumentCountMismatch { .. } => write!(f, "wrong number of arguments"),
            EvalError::MissingMain => write!(f, "no `main` function"),
            EvalError::MainHasParams(_) => write!(f, "`main` must not take parameters"),
        }
    }
}

impl EvalError {
    /// The error as a report for the user: the `Display` headline plus where
    /// it happened and what to say there.
    pub fn diagnostic(&self) -> Diagnostic {
        let at = |span: &Span, label: String| Diagnostic::new(self.to_string(), *span, label);
        match self {
            EvalError::DivisionByZero(span) => at(span, "this is zero".into()),
            EvalError::UnboundIdent { span, .. } => at(span, "not found in this scope".into()),
            EvalError::IntegerOverflow(span) => {
                at(span, "the result does not fit in an `i64`".into())
            }
            EvalError::TypeMismatch {
                expected,
                actual,
                span,
            } => at(span, format!("expected `{expected}`, found `{actual}`")),
            EvalError::ArgumentCountMismatch {
                expected,
                actual,
                span,
            } => at(
                span,
                format!(
                    "expected {expected} argument{}, found {actual}",
                    if *expected == 1 { "" } else { "s" }
                ),
            ),
            EvalError::MissingMain => Diagnostic::without_location(self.to_string())
                .with_note("a program starts by calling `def main() = ...;`"),
            EvalError::MainHasParams(span) => at(span, "remove these parameters".into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Closure {
    params: Vec<Ident>,
    body: Expr,
    env: Env,
}

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Fun(Rc<Closure>),
}

impl Value {
    pub fn ty(&self) -> Type {
        match self {
            Value::Int(_) => Type::Int,
            Value::Fun { .. } => Type::Fun,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Fun(closure) => {
                write!(f, "FUN (")?;
                for (i, param) in closure.params.iter().enumerate() {
                    write!(f, "{}", param.name)?;
                    if i != closure.params.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, ") => {}", closure.body)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Binding {
    id: Ident,
    value: Value,

    parent: Option<Rc<Binding>>,
}

#[derive(Debug, Clone)]
struct Env {
    head: Option<Rc<Binding>>,
}

impl Env {
    fn insert(&self, id: Ident, value: Value) -> Self {
        Env {
            head: Some(Rc::new(Binding {
                id,
                value,
                parent: self.head.clone(),
            })),
        }
    }

    fn search(&self, name: &str) -> Option<&Value> {
        let mut curr = &self.head;
        loop {
            match curr {
                Some(binding) if name == binding.id.name => {
                    break Some(&binding.value);
                }
                Some(binding) => {
                    curr = &binding.parent;
                }
                None => {
                    break None;
                }
            }
        }
    }
}

struct TreeEval {
    globals: HashMap<String, Value>,
}

impl TreeEval {
    fn new() -> Self {
        TreeEval {
            globals: HashMap::new(),
        }
    }

    fn search<'env>(&'env self, name: &str, env: &'env Env) -> Option<&'env Value> {
        env.search(name).or_else(|| self.globals.get(name))
    }

    fn eval_decl(&mut self, decl: &Decl) {
        match &decl.kind {
            DeclKind::FunDecl { id, params, body } => {
                let f = Value::Fun(Rc::new(Closure {
                    params: params.clone(),
                    body: body.clone(),
                    env: Env { head: None },
                }));
                self.globals.insert(id.name.clone(), f);
            }
        }
    }

    fn eval_stmt(&self, stmt: &Stmt, env: Env) -> Result<Env, EvalError> {
        match &stmt.kind {
            StmtKind::Let { id, init } => {
                let v = self.eval_expr(init, &env)?;
                let env = env.insert(id.clone(), v.clone());
                Ok(env)
            }
            StmtKind::ExprStmt(expr) => {
                let _ = self.eval_expr(expr, &env)?;
                Ok(env)
            }
        }
    }

    fn eval_expr(&self, expr: &Expr, env: &Env) -> Result<Value, EvalError> {
        match &expr.kind {
            ExprKind::Int(n) => Ok(Value::Int(*n)),
            ExprKind::Var(name) => {
                self.search(name, env)
                    .cloned()
                    .ok_or_else(|| EvalError::UnboundIdent {
                        name: name.clone(),
                        span: expr.span,
                    })
            }
            ExprKind::Call { callee, args } => {
                let f = self.eval_expr(callee, env)?;
                let Value::Fun(closure) = f else {
                    return Err(EvalError::TypeMismatch {
                        expected: Type::Fun,
                        actual: f.ty(),
                        span: callee.span,
                    });
                };
                if closure.params.len() != args.len() {
                    return Err(EvalError::ArgumentCountMismatch {
                        expected: closure.params.len(),
                        actual: args.len(),
                        span: expr.span,
                    });
                }
                // Start from what the closure captured, not the caller's
                // environment: that is what makes scoping lexical.
                let mut closure_env = closure.env.clone();
                for (param, arg) in closure.params.iter().zip(args.iter()) {
                    let v = self.eval_expr(arg, env)?;
                    closure_env = closure_env.insert(param.clone(), v);
                }

                self.eval_expr(&closure.body, &closure_env)
            }
            ExprKind::Unary(op, operand) => {
                let e = self.eval_expr(operand, env)?;
                match op {
                    UnOp::Neg => {
                        let Value::Int(n) = e else {
                            return Err(EvalError::TypeMismatch {
                                expected: Type::Int,
                                actual: e.ty(),
                                span: operand.span,
                            });
                        };
                        let Some(res) = n.checked_neg() else {
                            return Err(EvalError::IntegerOverflow(expr.span));
                        };
                        Ok(Value::Int(res))
                    }
                }
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let lv = self.eval_expr(lhs, env)?;
                let rv = self.eval_expr(rhs, env)?;

                let Value::Int(l) = lv else {
                    return Err(EvalError::TypeMismatch {
                        expected: Type::Int,
                        actual: lv.ty(),
                        span: lhs.span,
                    });
                };

                let Value::Int(r) = rv else {
                    return Err(EvalError::TypeMismatch {
                        expected: Type::Int,
                        actual: rv.ty(),
                        span: rhs.span,
                    });
                };

                let res = match op {
                    BinOp::Add => l.checked_add(r),
                    BinOp::Sub => l.checked_sub(r),
                    BinOp::Mul => l.checked_mul(r),
                    BinOp::Div if r == 0 => {
                        return Err(EvalError::DivisionByZero(rhs.span));
                    }
                    BinOp::Div => l.checked_div(r),
                };
                res.ok_or(EvalError::IntegerOverflow(expr.span))
                    .map(Value::Int)
            }
            ExprKind::Block { stmts, tail } => {
                let mut new_env = env.clone();

                for stmt in stmts {
                    new_env = self.eval_stmt(stmt, new_env)?;
                }
                let res = self.eval_expr(tail, &new_env)?;
                Ok(res)
            }
        }
    }

    pub fn eval(&mut self, ast: &Ast) -> Result<Value, EvalError> {
        for decl in &ast.decls {
            self.eval_decl(decl);
        }
        let main = self.globals.get("main").ok_or(EvalError::MissingMain)?;

        if let Value::Fun(closure) = main {
            if let (Some(first), Some(last)) = (closure.params.first(), closure.params.last()) {
                return Err(EvalError::MainHasParams(Span {
                    start: first.span.start,
                    end: last.span.end,
                }));
            }
            self.eval_expr(&closure.body, &closure.env)
        } else {
            Err(EvalError::MissingMain)
        }
    }
}

/// Run a program: define every top-level function, then call `main`.
pub fn eval(ast: &Ast) -> Result<Value, EvalError> {
    TreeEval::new().eval(ast)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn run(src: &str) -> Result<Value, EvalError> {
        eval(&Parser::new(Lexer::new(src)).parse().unwrap())
    }

    fn error(src: &str) -> EvalError {
        run(src).unwrap_err()
    }

    fn text(src: &str, span: Span) -> &str {
        &src[span.start..span.end]
    }

    #[test]
    fn main_must_exist() {
        assert_eq!(error("def f() = 1;"), EvalError::MissingMain);
        assert_eq!(error(""), EvalError::MissingMain);
    }

    #[test]
    fn main_must_not_take_parameters() {
        let src = "def main(a, bc) = a;";
        let EvalError::MainHasParams(span) = error(src) else {
            panic!("expected MainHasParams");
        };
        assert_eq!(text(src, span), "a, bc");
    }

    #[test]
    fn division_by_zero_points_at_divisor() {
        let src = "def main() = 1 / (2 - 2);";
        let EvalError::DivisionByZero(span) = error(src) else {
            panic!("expected DivisionByZero");
        };
        assert_eq!(text(src, span), "(2 - 2)");
    }

    #[test]
    fn negation_overflow_points_at_whole_negation() {
        let src = "def main() = -(-9223372036854775807 - 1);";
        let EvalError::IntegerOverflow(span) = error(src) else {
            panic!("expected IntegerOverflow");
        };
        assert_eq!(text(src, span), "-(-9223372036854775807 - 1)");
    }

    #[test]
    fn type_mismatch_reports_actual_type() {
        // Calling an integer: the callee is underlined.
        let src = "def main() = { let x = 1; x(2) };";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Fun, actual: Type::Int, span }
                if text(src, span) == "x"
        ));
        // Using a function as a number.
        let src = "def f() = 1; def main() = f + 1;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Int, actual: Type::Fun, span }
                if text(src, span) == "f"
        ));
        let src = "def f() = 1; def main() = -f;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Int, actual: Type::Fun, span }
                if text(src, span) == "f"
        ));
    }

    #[test]
    fn diagnostics() {
        let src = "def main() = x;";
        let diag = error(src).diagnostic();
        assert_eq!(diag.message, "cannot find `x` in this scope");

        let src = "def f(a) = a; def main() = f(1, 2);";
        let diag = error(src).diagnostic();
        assert_eq!(diag.message, "wrong number of arguments");
        let (span, label) = diag.label.unwrap();
        assert_eq!(text(src, span), "f(1, 2)");
        assert_eq!(label, "expected 1 argument, found 2");

        let src = "def f(a, b) = a; def main() = f(1);";
        let (_, label) = error(src).diagnostic().label.unwrap();
        assert_eq!(label, "expected 2 arguments, found 1");

        let diag = EvalError::MissingMain.diagnostic();
        assert_eq!(diag.label, None);
        assert_eq!(diag.notes.len(), 1);
    }

    fn value(src: &str) -> i64 {
        match run(src) {
            Ok(Value::Int(n)) => n,
            other => panic!("expected an integer, got {other:?}"),
        }
    }

    #[test]
    fn let_shadows_in_the_same_block() {
        assert_eq!(value("def main() = { let x = 1; let x = x + 10; x };"), 11);
    }

    #[test]
    fn inner_block_bindings_end_with_the_block() {
        let src = "def main() = { let x = 1; let y = { let x = 100; x }; x + y };";
        assert_eq!(value(src), 101);
    }

    #[test]
    fn locals_shadow_globals() {
        assert_eq!(value("def x() = 1; def main() = { let x = 5; x };"), 5);
        // Parameters too.
        assert_eq!(value("def x() = 1; def f(x) = x; def main() = f(7);"), 7);
    }

    #[test]
    fn functions_do_not_see_the_callers_locals() {
        let src = "def f() = y; def main() = { let y = 1; f() };";
        assert!(matches!(error(src), EvalError::UnboundIdent { name, .. } if name == "y"));
    }

    #[test]
    fn arguments_are_evaluated_in_the_callers_scope() {
        let src = "def f(a) = a * 2; def main() = { let a = 21; f(a) };";
        assert_eq!(value(src), 42);
    }

    #[test]
    fn functions_are_values() {
        let src = "def inc(x) = x + 1; def twice(f, x) = f(f(x)); def main() = twice(inc, 5);";
        assert_eq!(value(src), 7);
    }

    /// There are no lambdas yet, so build a closure that captured `x = 5`
    /// by hand and check that calling it sees `x`.
    #[test]
    fn calls_start_from_the_captured_environment() {
        let body_src = "def main() = x + 1;";
        let ast = Parser::new(Lexer::new(body_src)).parse().unwrap();
        let DeclKind::FunDecl { body, .. } = &ast.decls[0].kind;

        let captured = Env { head: None }.insert(
            Ident {
                name: "x".into(),
                span: Span { start: 0, end: 0 },
            },
            Value::Int(5),
        );
        let closure = Value::Fun(Rc::new(Closure {
            params: vec![],
            body: body.clone(),
            env: captured,
        }));

        let mut interp = TreeEval::new();
        interp.globals.insert("g".into(), closure);
        let call = Parser::new(Lexer::new("def main() = g();"))
            .parse()
            .unwrap();
        let DeclKind::FunDecl { body: call, .. } = &call.decls[0].kind;
        let result = interp.eval_expr(call, &Env { head: None }).unwrap();
        assert!(matches!(result, Value::Int(6)), "got {result:?}");
    }
}
