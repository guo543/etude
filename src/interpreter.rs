use std::fmt;

use crate::ast::{Expr, Op};

#[derive(Debug)]
pub struct EvalError(pub String);

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn eval(_expr: &Expr) -> Result<i64, EvalError> {
    Err(EvalError(String::from("Not implemented")))
}
