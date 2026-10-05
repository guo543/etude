use std::fmt;

use crate::lexer::Token;

pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    /// The binary operator for this token, if it is one.
    pub fn from_token(tok: &Token) -> Option<Op> {
        match tok {
            Token::Add => Some(Op::Add),
            Token::Sub => Some(Op::Sub),
            Token::Mul => Some(Op::Mul),
            Token::Div => Some(Op::Div),
            _ => None,
        }
    }

    /// Higher binds tighter.
    pub fn precedence(&self) -> u32 {
        match self {
            Op::Add | Op::Sub => 1,
            Op::Mul | Op::Div => 2,
        }
    }
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::Add => write!(f, "+"),
            Op::Sub => write!(f, "-"),
            Op::Mul => write!(f, "*"),
            Op::Div => write!(f, "/"),
        }
    }
}

pub enum AstNode {
    Int(i32),
    Binary(Op, Box<AstNode>, Box<AstNode>),
}

impl fmt::Display for AstNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AstNode::Int(n) => write!(f, "{n}"),
            AstNode::Binary(op, lhs, rhs) => write!(f, "({lhs} {op} {rhs})"),
        }
    }
}
