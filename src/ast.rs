use crate::span::Span;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl fmt::Display for UnOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnOp::Neg => write!(f, "-"),
        }
    }
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinOp::Add => write!(f, "+"),
            BinOp::Sub => write!(f, "-"),
            BinOp::Mul => write!(f, "*"),
            BinOp::Div => write!(f, "/"),
        }
    }
}

#[derive(Debug)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Debug)]
pub enum StmtKind {
    Let { id: Ident, init: Expr },
    ExprStmt(Expr),
}

#[derive(Debug)]
pub enum ExprKind {
    Int(i64),
    Var(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Block { stmts: Vec<Stmt>, tail: Box<Expr> },
}

#[derive(Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Stmt {
    pub fn let_decl(id: Ident, init: Expr, start: usize, end: usize) -> Self {
        Stmt {
            kind: StmtKind::Let { id, init },
            span: Span { start, end },
        }
    }

    pub fn expr_stmt(expr: Expr, start: usize, end: usize) -> Self {
        Stmt {
            kind: StmtKind::ExprStmt(expr),
            span: Span { start, end },
        }
    }
}

impl Expr {
    pub fn int(n: i64, start: usize, end: usize) -> Self {
        Expr {
            kind: ExprKind::Int(n),
            span: Span { start, end },
        }
    }

    pub fn var(s: String, start: usize, end: usize) -> Self {
        Expr {
            kind: ExprKind::Var(s),
            span: Span { start, end },
        }
    }

    pub fn unary(op: UnOp, expr: Expr, start: usize, end: usize) -> Self {
        Expr {
            kind: ExprKind::Unary(op, Box::new(expr)),
            span: Span { start, end },
        }
    }

    pub fn binary(op: BinOp, lhs: Expr, rhs: Expr, start: usize, end: usize) -> Self {
        Expr {
            kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)),
            span: Span { start, end },
        }
    }

    pub fn block(stmts: Vec<Stmt>, tail: Expr, start: usize, end: usize) -> Self {
        Expr {
            kind: ExprKind::Block {
                stmts,
                tail: Box::new(tail),
            },
            span: Span { start, end },
        }
    }
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            StmtKind::Let { id, init } => write!(f, "LET: {} = {init} ;", id.name),
            StmtKind::ExprStmt(expr) => write!(f, "EXPRSTMT: {expr} ;"),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ExprKind::Int(n) => write!(f, "{n}"),
            ExprKind::Var(s) => write!(f, "{s}"),
            ExprKind::Unary(op, expr) => write!(f, "({op} {expr})"),
            ExprKind::Binary(op, lhs, rhs) => write!(f, "({lhs} {op} {rhs})"),
            ExprKind::Block { stmts, tail } => {
                write!(f, "BLOCK: {{ ")?;
                for stmt in stmts {
                    write!(f, "{stmt} ")?;
                }
                write!(f, "{tail} ")?;
                write!(f, "}}")
            }
        }
    }
}

pub enum DeclKind {
    // VarDecl {
    //     id: String,
    //     init: Expr,
    // },
    FunDecl {
        id: Ident,
        params: Vec<Ident>,
        body: Expr,
    },
}

pub struct Decl {
    pub kind: DeclKind,
    pub span: Span,
}

impl Decl {
    pub fn function(id: Ident, params: Vec<Ident>, body: Expr, start: usize, end: usize) -> Self {
        Decl {
            kind: DeclKind::FunDecl { id, params, body },
            span: Span { start, end },
        }
    }
}

impl fmt::Display for Decl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            DeclKind::FunDecl { id, params, body } => {
                let param_list = params
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<String>>()
                    .join(", ");
                write!(f, "DEF {} ({param_list}) = {body}", id.name)
            }
        }
    }
}

pub struct Ast {
    pub decls: Vec<Decl>,
}

impl fmt::Display for Ast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "AST:")?;
        for decl in &self.decls {
            writeln!(f, "{decl}")?;
        }
        Ok(())
    }
}
