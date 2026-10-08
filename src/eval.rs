use crate::{
    ast::{Ast, BinOp, Decl, DeclKind, Expr, ExprKind, Ident, Stmt, UnOp},
    span::Span,
};
use std::{collections::HashMap, fmt};

#[derive(Debug)]
pub enum Type {
    Int,
    Fun,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Fun => write!(f, "fun"),
        }
    }
}

#[derive(Debug)]
pub enum EvalError {
    DivisionByZero(Span),
    UnboundIdent(Span),
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
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EvalError::DivisionByZero(span) => write!(f, "Cannot divide by zero at {span}"),
            EvalError::UnboundIdent(span) => write!(f, "Unbound identifier at {span}"),
            EvalError::IntegerOverflow(span) => write!(f, "Integer overflow at {span}"),
            EvalError::TypeMismatch {
                expected,
                actual,
                span,
            } => write!(
                f,
                "Type mismatch at  {span}: expected {expected} but got {actual}"
            ),
            EvalError::ArgumentCountMismatch {
                expected,
                actual,
                span,
            } => write!(
                f,
                "Argument number mismatch at {span}: expected {expected} but got {actual}"
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Fun { params: Vec<Ident>, body: Expr },
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

fn search(id: &str, env: &Vec<HashMap<String, Value>>) -> Option<Value> {
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
        ExprKind::Var(s) => search(s, env).ok_or_else(|| EvalError::UnboundIdent(expr.span)),
        ExprKind::Call { callee, args } => {
            let f = eval_expr(callee, env)?;
            let Value::Fun { params, body } = f else {
                return Err(EvalError::TypeMismatch {
                    expected: Type::Fun,
                    actual: Type::Int,
                    span: expr.span,
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
        ExprKind::Unary(op, expr) => {
            let e = eval_expr(expr, env)?;
            match op {
                UnOp::Neg => {
                    // Check int
                    let Value::Int(n) = e else {
                        return Err(EvalError::TypeMismatch {
                            expected: Type::Int,
                            actual: Type::Fun,
                            span: expr.span,
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
                    actual: Type::Fun,
                    span: lhs.span,
                });
            };

            let Value::Int(r) = rv else {
                return Err(EvalError::TypeMismatch {
                    expected: Type::Int,
                    actual: Type::Fun,
                    span: rhs.span,
                });
            };

            let res = match op {
                BinOp::Add => l.checked_add(r),
                BinOp::Sub => l.checked_sub(r),
                BinOp::Mul => l.checked_mul(r),
                BinOp::Div if r == 0 => {
                    return Err(EvalError::DivisionByZero(expr.span));
                }
                BinOp::Div => l.checked_div(r),
            };
            res.ok_or_else(|| EvalError::IntegerOverflow(expr.span))
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
    let mut root = HashMap::<String, Value>::new();

    ast.decls.iter().for_each(|d| eval_decl(d, &mut root));

    let mut env: Vec<HashMap<String, Value>> = vec![root];

    let main = Expr::var("main".to_string(), 0, 0);
    let call = Expr::call(main, vec![], 0, 0);
    eval_expr(&call, &mut env)
}
