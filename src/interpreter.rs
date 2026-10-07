use std::fmt;

use crate::ast::{AstNode, Op};

#[derive(Debug)]
pub struct EvalError(pub String);

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn eval(expr: &AstNode) -> Result<i64, EvalError> {
    match expr {
        AstNode::Int(n) => Ok(*n),
        AstNode::Binary(op, lhs, rhs) => {
            let l = eval(lhs)?;
            let r = eval(rhs)?;
            let result = match op {
                Op::Add => l.checked_add(r),
                Op::Sub => l.checked_sub(r),
                Op::Mul => l.checked_mul(r),
                Op::Div if r == 0 => {
                    return Err(EvalError(format!("Division by zero: {l} / 0")));
                }
                Op::Div => l.checked_div(r),
            };
            result.ok_or_else(|| EvalError(format!("Integer overflow: {l} {op} {r}")))
        }
    }
}
