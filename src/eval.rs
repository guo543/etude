use crate::{
    ast::{Ast, BinOp, Decl, DeclKind, Expr, ExprKind, Ident, Stmt, UnOp},
    diagnostic::Diagnostic,
    span::Span,
};
use std::{collections::HashMap, fmt};

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
pub enum Value {
    Int(i64),
    Fun { params: Vec<Ident>, body: Expr },
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
            Value::Fun { params, body } => {
                let param_list = params
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<String>>()
                    .join(", ");
                write!(f, "fun ({param_list}) = {body}")
            }
        }
    }
}

fn search(id: &str, env: &[HashMap<String, Value>]) -> Option<Value> {
    for map in env.iter().rev() {
        if let Some(v) = map.get(id).cloned() {
            return Some(v);
        }
    }
    None
}

fn eval_stmt(stmt: &Stmt, env: &mut Vec<HashMap<String, Value>>) -> Result<(), EvalError> {
    match &stmt.kind {
        crate::ast::StmtKind::Let { id, init } => {
            let v = eval_expr(init, env)?;
            let last = env.last_mut().unwrap();
            last.insert(id.name.clone(), v.clone());
            Ok(())
        }
        crate::ast::StmtKind::ExprStmt(expr) => {
            let _ = eval_expr(expr, env)?;
            Ok(())
        }
    }
}

fn eval_expr(expr: &Expr, env: &mut Vec<HashMap<String, Value>>) -> Result<Value, EvalError> {
    match &expr.kind {
        ExprKind::Int(n) => Ok(Value::Int(*n)),
        ExprKind::Var(name) => search(name, env).ok_or_else(|| EvalError::UnboundIdent {
            name: name.clone(),
            span: expr.span,
        }),
        ExprKind::Call { callee, args } => {
            let f = eval_expr(callee, env)?;
            let Value::Fun { params, body } = f else {
                return Err(EvalError::TypeMismatch {
                    expected: Type::Fun,
                    actual: f.ty(),
                    span: callee.span,
                });
            };
            let mut clone = vec![env.first().cloned().unwrap()];
            let mut new_env = HashMap::<String, Value>::new();
            if params.len() != args.len() {
                return Err(EvalError::ArgumentCountMismatch {
                    expected: params.len(),
                    actual: args.len(),
                    span: expr.span,
                });
            }
            for (param, arg) in params.iter().zip(args.iter()) {
                let v = eval_expr(arg, env)?;
                new_env.insert(param.name.clone(), v);
            }
            clone.push(new_env);

            eval_expr(&body, &mut clone)
        }
        ExprKind::Unary(op, operand) => {
            let e = eval_expr(operand, env)?;
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
            let lv = eval_expr(lhs, env)?;
            let rv = eval_expr(rhs, env)?;

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
            env.push(HashMap::new());
            for stmt in stmts {
                eval_stmt(stmt, env)?;
            }
            let res = eval_expr(tail, env)?;
            env.pop();
            Ok(res)
        }
    }
}

fn eval_decl(decl: &Decl, root: &mut HashMap<String, Value>) {
    match &decl.kind {
        DeclKind::FunDecl { id, params, body } => {
            let f = Value::Fun {
                params: params.clone(),
                body: body.clone(),
            };
            root.insert(id.name.clone(), f);
        }
    }
}

pub fn eval(ast: &Ast) -> Result<Value, EvalError> {
    let main_params = ast
        .decls
        .iter()
        .find_map(|decl| match &decl.kind {
            DeclKind::FunDecl { id, params, .. } if id.name == "main" => Some(params),
            _ => None,
        })
        .ok_or(EvalError::MissingMain)?;
    if let (Some(first), Some(last)) = (main_params.first(), main_params.last()) {
        return Err(EvalError::MainHasParams(Span {
            start: first.span.start,
            end: last.span.end,
        }));
    }

    let mut root = HashMap::<String, Value>::new();

    ast.decls.iter().for_each(|d| eval_decl(d, &mut root));

    let mut env: Vec<HashMap<String, Value>> = vec![root];

    let main = Expr::var("main".to_string(), 0, 0);
    let call = Expr::call(main, vec![], 0, 0);
    eval_expr(&call, &mut env)
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
}
