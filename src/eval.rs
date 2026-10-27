use crate::{
    ast::{Ast, BinOp, Decl, DeclKind, Expr, ExprKind, Ident, Stmt, StmtKind, UnOp},
    diagnostic::Diagnostic,
    span::Span,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fmt,
    io::{self, Write},
    rc::Rc,
};

/// How many calls may be in progress at once. Deeper recursion is reported
/// as an error, the same in every build, so that a runaway program stops
/// with a clear message.
pub const MAX_CALL_DEPTH: usize = 10_000;

/// When less than this much stack is left, `eval_expr` continues on a new
/// segment of `STACK_SEGMENT` bytes. The evaluator recurses once per nested
/// expression, so without this a deeply nested body overflows the native
/// stack (about 400 calls deep in a debug build) long before
/// `MAX_CALL_DEPTH`. The values are rustc's.
const STACK_RED_ZONE: usize = 100 * 1024;
const STACK_SEGMENT: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Unit,
    Int,
    Bool,
    Fun,
    Str,
}

/// The type's name, as it will appear in signatures and type errors.
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Unit => write!(f, "()"),
            Type::Int => write!(f, "int"),
            Type::Fun => write!(f, "fun"),
            Type::Bool => write!(f, "bool"),
            Type::Str => write!(f, "string"),
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
    /// More than `limit` calls in progress at once, almost always infinite
    /// recursion; the span is the call that went over.
    RecursionLimit {
        limit: usize,
        span: Span,
    },
    /// The program defines no `main` function.
    MissingMain,
    /// `main` has parameters; the span covers them.
    MainHasParams(Span),
    /// `==` or `!=` on a value that cannot be compared, such as a function;
    /// the span is that operand.
    NotComparable {
        ty: Type,
        span: Span,
    },
    /// A top-level `def` uses a builtin's name; the span is the name.
    RedefinesBuiltin {
        name: String,
        span: Span,
    },
    /// Writing the program's output failed, e.g. because stdout was closed;
    /// the span is the call that wrote. Holds the I/O error's message,
    /// since `io::Error` is neither `Clone` nor `PartialEq`.
    Output {
        message: String,
        span: Span,
    },
}

/// The headline of the error; `diagnostic` adds the location and details.
impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::DivisionByZero(_) => write!(f, "division by zero"),
            EvalError::UnboundIdent { name, .. } => {
                write!(f, "cannot find `{name}` in this scope")
            }
            EvalError::IntegerOverflow(_) => write!(f, "integer overflow"),
            EvalError::TypeMismatch { .. } => write!(f, "mismatched types"),
            EvalError::ArgumentCountMismatch { .. } => write!(f, "wrong number of arguments"),
            EvalError::RecursionLimit { .. } => write!(f, "recursion limit reached"),
            EvalError::MissingMain => write!(f, "no `main` function"),
            EvalError::MainHasParams(_) => write!(f, "`main` must not take parameters"),
            EvalError::NotComparable { ty, .. } => {
                write!(f, "values of type `{ty}` cannot be compared")
            }
            EvalError::RedefinesBuiltin { name, .. } => {
                write!(f, "`{name}` is a builtin function")
            }
            EvalError::Output { .. } => write!(f, "cannot write output"),
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
            EvalError::RecursionLimit { limit, span } => {
                at(span, format!("this call would exceed {limit} nested calls"))
                    .with_note("does the function have a base case that stops the recursion?")
            }
            EvalError::MissingMain => Diagnostic::without_location(self.to_string())
                .with_note("a program starts by calling `def main() = ...;`"),
            EvalError::MainHasParams(span) => at(span, "remove these parameters".into()),
            EvalError::NotComparable { span, .. } => at(span, "this is a function".into()),
            EvalError::RedefinesBuiltin { span, .. } => at(span, "cannot be redefined".into())
                .with_note("pick another name; a local `let` or parameter may shadow it"),
            EvalError::Output { message, span } => at(span, message.clone()),
        }
    }
}

/// A function implemented in Rust. Each one is bound in the globals under
/// its `name`; `TreeEval::call_builtin` runs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Print,
    Println,
    IntToStr,
}

impl Builtin {
    /// Every builtin, for pre-filling the globals.
    const ALL: [Builtin; 3] = [Builtin::Print, Builtin::Println, Builtin::IntToStr];

    fn name(self) -> &'static str {
        match self {
            Builtin::Print => "print",
            Builtin::Println => "println",
            Builtin::IntToStr => "int_to_str",
        }
    }

    /// The type of each parameter. The evaluator checks the arguments
    /// against these before running the builtin.
    fn params(self) -> &'static [Type] {
        match self {
            Builtin::Print | Builtin::Println => &[Type::Str],
            Builtin::IntToStr => &[Type::Int],
        }
    }
}

impl fmt::Display for Builtin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, Clone)]
pub struct Closure<'ast> {
    params: &'ast [Ident],
    body: &'ast Expr,
    env: Env<'ast>,
}

#[derive(Debug, Clone)]
pub enum Value<'ast> {
    Unit,
    Int(i64),
    Bool(bool),
    Str(Rc<str>),
    Fun(Rc<Closure<'ast>>),
    Builtin(Builtin),
}

impl Value<'_> {
    pub fn ty(&self) -> Type {
        match self {
            Value::Unit => Type::Unit,
            Value::Int(_) => Type::Int,
            Value::Fun { .. } => Type::Fun,
            Value::Bool(_) => Type::Bool,
            Value::Str(_) => Type::Str,
            Value::Builtin(_) => Type::Fun,
        }
    }
}

impl fmt::Display for Value<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Unit => write!(f, "()"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => write!(f, "{s:?}"),
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
            Value::Builtin(builtin) => write!(f, "BUILTIN {builtin}"),
        }
    }
}

#[derive(Debug, Clone)]
struct Binding<'ast> {
    id: &'ast Ident,
    value: Value<'ast>,

    parent: Option<Rc<Binding<'ast>>>,
}

#[derive(Debug, Clone)]
struct Env<'ast> {
    head: Option<Rc<Binding<'ast>>>,
}

impl<'ast> Env<'ast> {
    fn empty() -> Self {
        Env { head: None }
    }

    fn bind(self, id: &'ast Ident, value: Value<'ast>) -> Self {
        Env {
            head: Some(Rc::new(Binding {
                id,
                value,
                parent: self.head,
            })),
        }
    }

    fn search(&self, name: &str) -> Option<&Value<'ast>> {
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

struct TreeEval<'ast, 'out> {
    globals: HashMap<&'ast str, Value<'ast>>,
    /// Calls currently in progress. A `Cell` because evaluation takes
    /// `&self`.
    depth: Cell<usize>,
    max_depth: usize,
    /// Where `println` writes. A `RefCell`, not a `Cell`, because writing
    /// needs a `&mut` to the writer, and only `RefCell` lends one out
    /// through a `&self`.
    out: RefCell<&'out mut dyn Write>,
}

fn expect_int(value: Value<'_>, span: Span) -> Result<i64, EvalError> {
    match value {
        Value::Int(n) => Ok(n),
        other => Err(EvalError::TypeMismatch {
            expected: Type::Int,
            actual: other.ty(),
            span,
        }),
    }
}

fn expect_bool(value: Value<'_>, span: Span) -> Result<bool, EvalError> {
    match value {
        Value::Bool(b) => Ok(b),
        other => Err(EvalError::TypeMismatch {
            expected: Type::Bool,
            actual: other.ty(),
            span,
        }),
    }
}

impl<'ast, 'out> TreeEval<'ast, 'out> {
    fn new(out: &'out mut dyn Write) -> Self {
        Self::with_max_depth(out, MAX_CALL_DEPTH)
    }

    fn with_max_depth(out: &'out mut dyn Write, max_depth: usize) -> Self {
        TreeEval {
            globals: Builtin::ALL
                .into_iter()
                .map(|b| (b.name(), Value::Builtin(b)))
                .collect(),
            depth: Cell::new(0),
            max_depth,
            out: RefCell::new(out),
        }
    }

    fn search<'env>(&'env self, name: &str, env: &'env Env<'ast>) -> Option<&'env Value<'ast>> {
        env.search(name).or_else(|| self.globals.get(name))
    }

    fn eval_decl(&mut self, decl: &'ast Decl) -> Result<(), EvalError> {
        match &decl.kind {
            DeclKind::FunDecl { id, params, body } => {
                if let Some(Value::Builtin(_)) = self.globals.get(id.name.as_str()) {
                    return Err(EvalError::RedefinesBuiltin {
                        name: id.name.clone(),
                        span: id.span,
                    });
                }
                let f = Value::Fun(Rc::new(Closure {
                    params,
                    body,
                    env: Env::empty(),
                }));
                self.globals.insert(&id.name, f);
                Ok(())
            }
        }
    }

    fn eval_stmt(&self, stmt: &'ast Stmt, env: Env<'ast>) -> Result<Env<'ast>, EvalError> {
        match &stmt.kind {
            StmtKind::Let { id, init } => {
                let v = self.eval_expr(init, &env)?;
                let env = env.bind(id, v);
                Ok(env)
            }
            StmtKind::ExprStmt(expr) => {
                let _ = self.eval_expr(expr, &env)?;
                Ok(env)
            }
        }
    }

    fn eval_expr(&self, expr: &'ast Expr, env: &Env<'ast>) -> Result<Value<'ast>, EvalError> {
        stacker::maybe_grow(STACK_RED_ZONE, STACK_SEGMENT, || {
            self.eval_expr_inner(expr, env)
        })
    }

    fn eval_expr_inner(&self, expr: &'ast Expr, env: &Env<'ast>) -> Result<Value<'ast>, EvalError> {
        match &expr.kind {
            ExprKind::Unit => Ok(Value::Unit),
            ExprKind::Int(n) => Ok(Value::Int(*n)),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Str(s) => Ok(Value::Str(s.clone())),
            ExprKind::Var(name) => {
                self.search(name, env)
                    .cloned()
                    .ok_or_else(|| EvalError::UnboundIdent {
                        name: name.clone(),
                        span: expr.span,
                    })
            }
            ExprKind::Call { callee, args } => match self.eval_expr(callee, env)? {
                Value::Fun(closure) => self.call_closure(&closure, args, env, expr.span),
                Value::Builtin(builtin) => self.call_builtin(builtin, args, env, expr.span),
                other => Err(EvalError::TypeMismatch {
                    expected: Type::Fun,
                    actual: other.ty(),
                    span: callee.span,
                }),
            },
            ExprKind::Unary(op, operand) => {
                let e = self.eval_expr(operand, env)?;
                match op {
                    UnOp::Neg => {
                        let n = expect_int(e, operand.span)?;
                        let Some(res) = n.checked_neg() else {
                            return Err(EvalError::IntegerOverflow(expr.span));
                        };
                        Ok(Value::Int(res))
                    }
                    UnOp::Not => {
                        let b = expect_bool(e, operand.span)?;
                        Ok(Value::Bool(!b))
                    }
                }
            }
            // `&&` and `||` short-circuit: the right operand is evaluated only
            // when the left one does not already decide the result, so they
            // come before the arm that evaluates both operands.
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), lhs, rhs) => {
                let l = expect_bool(self.eval_expr(lhs, env)?, lhs.span)?;
                match (op, l) {
                    (BinOp::And, false) => Ok(Value::Bool(false)),
                    (BinOp::Or, true) => Ok(Value::Bool(true)),
                    _ => {
                        let r = expect_bool(self.eval_expr(rhs, env)?, rhs.span)?;
                        Ok(Value::Bool(r))
                    }
                }
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let lv = self.eval_expr(lhs, env)?;
                let rv = self.eval_expr(rhs, env)?;

                match op {
                    BinOp::And | BinOp::Or => unreachable!("handled above"),
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                        let l = expect_int(lv, lhs.span)?;
                        let r = expect_int(rv, rhs.span)?;
                        let result = match op {
                            BinOp::Add => l.checked_add(r),
                            BinOp::Sub => l.checked_sub(r),
                            BinOp::Mul => l.checked_mul(r),
                            BinOp::Div if r == 0 => {
                                return Err(EvalError::DivisionByZero(rhs.span));
                            }
                            BinOp::Div => l.checked_div(r),
                            _ => unreachable!(),
                        };
                        result
                            .ok_or(EvalError::IntegerOverflow(expr.span))
                            .map(Value::Int)
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        let l = expect_int(lv, lhs.span)?;
                        let r = expect_int(rv, rhs.span)?;
                        Ok(Value::Bool(match op {
                            BinOp::Lt => l < r,
                            BinOp::Le => l <= r,
                            BinOp::Gt => l > r,
                            BinOp::Ge => l >= r,
                            _ => unreachable!(),
                        }))
                    }
                    BinOp::Eq | BinOp::Ne => {
                        let equal = match (&lv, &rv) {
                            (Value::Int(l), Value::Int(r)) => l == r,
                            (Value::Bool(l), Value::Bool(r)) => l == r,
                            (Value::Str(l), Value::Str(r)) => l == r,
                            (Value::Unit, Value::Unit) => true,
                            (Value::Fun(_) | Value::Builtin(_), _) => {
                                return Err(EvalError::NotComparable {
                                    ty: Type::Fun,
                                    span: lhs.span,
                                });
                            }
                            (_, Value::Fun(_) | Value::Builtin(_)) => {
                                return Err(EvalError::NotComparable {
                                    ty: Type::Fun,
                                    span: rhs.span,
                                });
                            }
                            // An integer and a boolean.
                            _ => {
                                return Err(EvalError::TypeMismatch {
                                    expected: lv.ty(),
                                    actual: rv.ty(),
                                    span: rhs.span,
                                });
                            }
                        };
                        Ok(Value::Bool(if let BinOp::Eq = op { equal } else { !equal }))
                    }
                }
            }
            ExprKind::If {
                cond,
                t_branch,
                f_branch,
            } => {
                let c = expect_bool(self.eval_expr(cond, env)?, cond.span)?;
                match (c, f_branch) {
                    (true, Some(_)) => self.eval_expr(t_branch, env),
                    (false, Some(f_branch)) => self.eval_expr(f_branch, env),
                    (true, None) => {
                        // evaluate and discard for side effects
                        let _ = self.eval_expr(t_branch, env)?;
                        Ok(Value::Unit)
                    }
                    (_, None) => Ok(Value::Unit),
                }
            }
            ExprKind::Lambda { params, body } => {
                let f = Value::Fun(Rc::new(Closure {
                    params,
                    body,
                    env: env.clone(),
                }));
                Ok(f)
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

    /// Call `closure` with `args`, evaluated in the caller's `env`; `span`
    /// is the whole call.
    fn call_closure(
        &self,
        closure: &Closure<'ast>,
        args: &'ast [Expr],
        env: &Env<'ast>,
        span: Span,
    ) -> Result<Value<'ast>, EvalError> {
        if closure.params.len() != args.len() {
            return Err(EvalError::ArgumentCountMismatch {
                expected: closure.params.len(),
                actual: args.len(),
                span,
            });
        }
        // Start from what the closure captured, not the caller's
        // environment: that is what makes scoping lexical.
        let mut closure_env = closure.env.clone();
        for (param, arg) in closure.params.iter().zip(args) {
            let v = self.eval_expr(arg, env)?;
            closure_env = closure_env.bind(param, v);
        }

        let depth = self.depth.get();
        if depth == self.max_depth {
            return Err(EvalError::RecursionLimit {
                limit: self.max_depth,
                span,
            });
        }
        // Nothing between `set` and the restore can return early,
        // so the count stays right even when the body fails.
        self.depth.set(depth + 1);
        let result = self.eval_expr(closure.body, &closure_env);
        self.depth.set(depth);
        result
    }

    /// Call `builtin` with `args`, evaluated in the caller's `env`; `span`
    /// is the whole call. Builtins cannot recurse, so they do not count
    /// towards the call depth.
    fn call_builtin(
        &self,
        builtin: Builtin,
        args: &'ast [Expr],
        env: &Env<'ast>,
        span: Span,
    ) -> Result<Value<'ast>, EvalError> {
        let params = builtin.params();
        if params.len() != args.len() {
            return Err(EvalError::ArgumentCountMismatch {
                expected: params.len(),
                actual: args.len(),
                span,
            });
        }
        let mut values = Vec::with_capacity(args.len());
        for (&expected, arg) in params.iter().zip(args) {
            let v = self.eval_expr(arg, env)?;
            if v.ty() != expected {
                return Err(EvalError::TypeMismatch {
                    expected,
                    actual: v.ty(),
                    span: arg.span,
                });
            }
            values.push(v);
        }

        match (builtin, values.as_slice()) {
            (Builtin::Print, [Value::Str(s)]) => {
                write!(self.out.borrow_mut(), "{s}").map_err(|e| EvalError::Output {
                    message: e.to_string(),
                    span,
                })?;
                Ok(Value::Unit)
            }
            (Builtin::Println, [Value::Str(s)]) => {
                writeln!(self.out.borrow_mut(), "{s}").map_err(|e| EvalError::Output {
                    message: e.to_string(),
                    span,
                })?;
                Ok(Value::Unit)
            }
            (Builtin::IntToStr, [Value::Int(n)]) => Ok(Value::Str(n.to_string().into())),
            _ => unreachable!("the arguments were checked against `params`"),
        }
    }

    pub fn eval(&mut self, ast: &'ast Ast) -> Result<Value<'ast>, EvalError> {
        for decl in &ast.decls {
            self.eval_decl(decl)?;
        }
        let main = self.globals.get("main").ok_or(EvalError::MissingMain)?;

        if let Value::Fun(closure) = main {
            if let (Some(first), Some(last)) = (closure.params.first(), closure.params.last()) {
                return Err(EvalError::MainHasParams(Span {
                    start: first.span.start,
                    end: last.span.end,
                }));
            }
            self.eval_expr(closure.body, &closure.env)
        } else {
            Err(EvalError::MissingMain)
        }
    }
}

/// Run a program: define every top-level function, then call `main`.
/// The program's output goes to stdout.
pub fn eval(ast: &Ast) -> Result<Value<'_>, EvalError> {
    eval_with_output(ast, &mut io::stdout().lock())
}

/// Like `eval`, but the program's output goes to `out`.
pub fn eval_with_output<'ast>(
    ast: &'ast Ast,
    out: &mut dyn Write,
) -> Result<Value<'ast>, EvalError> {
    TreeEval::new(out).eval(ast)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    // Values borrow from the AST, so the helpers keep the AST alive and
    // return only owned results.

    fn error(src: &str) -> EvalError {
        let ast = Parser::new(Lexer::new(src)).parse().unwrap();
        eval_with_output(&ast, &mut io::sink()).unwrap_err()
    }

    fn text(src: &str, span: Span) -> &str {
        &src[span.start..span.end]
    }

    #[test]
    fn main_must_exist() {
        assert_eq!(error("def f() = 1;"), EvalError::MissingMain);
        assert_eq!(error(""), EvalError::MissingMain);
    }

    /// Run `src` with a lower call limit, so tests stay small and fast.
    fn eval_with_max_depth(src: &str, max_depth: usize) -> Result<String, EvalError> {
        let ast = Parser::new(Lexer::new(src)).parse().unwrap();
        TreeEval::with_max_depth(&mut io::sink(), max_depth)
            .eval(&ast)
            .map(|v| v.to_string())
    }

    #[test]
    fn infinite_recursion_hits_the_limit() {
        let src = "def f(n) = f(n + 1); def main() = f(0);";
        let EvalError::RecursionLimit { limit, span } = error(src) else {
            panic!("expected RecursionLimit");
        };
        assert_eq!(limit, MAX_CALL_DEPTH);
        // The call that went over is the recursive one, not the one in `main`.
        assert_eq!(span.start, src.find("f(n + 1)").unwrap());
        assert_eq!(text(src, span), "f(n + 1)");
    }

    #[test]
    fn recursion_up_to_the_limit_is_fine() {
        let count = "def f(n) = if n == 0 { 0 } else { 1 + f(n - 1) };";
        // `f(n)` makes n + 1 calls.
        let src = format!("{count} def main() = f({});", MAX_CALL_DEPTH - 1);
        assert_eq!(printed(&src), (MAX_CALL_DEPTH - 1).to_string());
        let src = format!("{count} def main() = f({});", MAX_CALL_DEPTH);
        assert!(matches!(error(&src), EvalError::RecursionLimit { .. }));
    }

    #[test]
    fn depth_counts_calls_in_progress_not_calls_made() {
        // Each `f(9)` returns before the next starts, so the depth goes back
        // down and 30 calls in total fit under a limit of 10.
        let src = "def f(n) = if n == 0 { 0 } else { 1 + f(n - 1) };
                   def main() = f(9) + f(9) + f(9);";
        assert_eq!(eval_with_max_depth(src, 10).unwrap(), "27");
        assert!(matches!(
            eval_with_max_depth(
                "def f(n) = if n == 0 { 0 } else { f(n - 1) }; def main() = f(10);",
                10
            ),
            Err(EvalError::RecursionLimit { limit: 10, .. })
        ));
    }

    #[test]
    fn mutual_and_lambda_recursion_count_too() {
        let src = "def ping(n) = pong(n + 1); def pong(n) = ping(n + 1); def main() = ping(0);";
        assert!(matches!(
            eval_with_max_depth(src, 50),
            Err(EvalError::RecursionLimit { limit: 50, .. })
        ));
        // Recursion through a function passed as an argument.
        let src =
            "def apply(g, x) = g(g, x); def main() = apply(fun(self, n) => self(self, n), 0);";
        assert!(matches!(
            eval_with_max_depth(src, 50),
            Err(EvalError::RecursionLimit { limit: 50, .. })
        ));
    }

    #[test]
    fn deeply_nested_bodies_do_not_overflow_the_stack() {
        // Each call nests 30 blocks and `if`s, many native frames per call.
        // Without stack growth this overflows long before the limit.
        let mut body = String::from("f(n + 1)");
        for i in 0..30 {
            body = format!("{{ let x{i} = 1; if true {{ 1 + ({body}) }} else {{ 0 }} }}");
        }
        let src = format!("def f(n) = {body}; def main() = f(0);");
        assert!(matches!(
            eval_with_max_depth(&src, 1_000),
            Err(EvalError::RecursionLimit { limit: 1_000, .. })
        ));
    }

    #[test]
    fn recursion_limit_diagnostic() {
        let diag = EvalError::RecursionLimit {
            limit: 10,
            span: Span { start: 0, end: 1 },
        }
        .diagnostic();
        assert_eq!(diag.message, "recursion limit reached");
        assert_eq!(
            diag.label.unwrap().1,
            "this call would exceed 10 nested calls"
        );
        assert_eq!(diag.notes.len(), 1);
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
        let ast = Parser::new(Lexer::new(src)).parse().unwrap();
        match eval(&ast) {
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

        let x = Ident {
            name: "x".into(),
            span: Span { start: 0, end: 0 },
        };
        let params = vec![];
        let captured = Env::empty().bind(&x, Value::Int(5));
        let closure = Value::Fun(Rc::new(Closure {
            params: &params,
            body,
            env: captured,
        }));

        let mut sink = io::sink();
        let mut interp = TreeEval::new(&mut sink);
        interp.globals.insert("g", closure);
        let call = Parser::new(Lexer::new("def main() = g();"))
            .parse()
            .unwrap();
        let DeclKind::FunDecl { body: call, .. } = &call.decls[0].kind;
        let result = interp.eval_expr(call, &Env::empty()).unwrap();
        assert!(matches!(result, Value::Int(6)), "got {result:?}");
    }

    #[test]
    fn lambda_captures_its_environment() {
        let src = "def main() = { let a = 10; let f = fun(x) => x + a; f(1) };";
        assert_eq!(value(src), 11);
    }

    #[test]
    fn captured_parameter_outlives_the_call() {
        // `z` belongs to `make`'s call, which has returned by the time
        // `inc` runs.
        let src = "def make(z) = fun(x, y) => x + y + z;
                   def main() = { let inc = make(1); inc(10000, 2) };";
        assert_eq!(value(src), 10003);
    }

    #[test]
    fn capture_is_not_affected_by_later_shadowing() {
        let src = "def main() = { let x = 1; let f = fun() => x; let x = 2; f() };";
        assert_eq!(value(src), 1);
    }

    #[test]
    fn lambda_does_not_see_later_bindings() {
        let src = "def main() = { let f = fun() => y; let y = 2; f() };";
        assert!(matches!(error(src), EvalError::UnboundIdent { name, .. } if name == "y"));
    }

    #[test]
    fn lambda_sees_where_it_was_written_not_where_it_is_called() {
        // `apply` has its own `z`, but the lambda captured `main`'s.
        let src = "def apply(f) = { let z = 100; f() };
                   def main() = { let z = 1; apply(fun() => z) };";
        assert_eq!(value(src), 1);
    }

    #[test]
    fn currying() {
        assert_eq!(
            value("def add(a) = fun(b) => a + b; def main() = add(3)(4);"),
            7
        );
        let src = "def main() = { let a = 1; let f = fun(b) => fun(c) => a + b + c; f(10)(100) };";
        assert_eq!(value(src), 111);
    }

    #[test]
    fn lambdas_as_arguments() {
        let src = "def twice(f, x) = f(f(x)); def main() = twice(fun(n) => n * 3, 2);";
        assert_eq!(value(src), 18);
    }

    #[test]
    fn calling_a_lambda_directly() {
        assert_eq!(value("def main() = (fun(x) => x + 1)(41);"), 42);
        assert_eq!(value("def main() = (fun() => 7)();"), 7);
    }

    #[test]
    fn lambda_argument_count_is_checked() {
        let src = "def main() = (fun(x) => x)(1, 2);";
        let EvalError::ArgumentCountMismatch {
            expected: 1,
            actual: 2,
            span,
        } = error(src)
        else {
            panic!("expected ArgumentCountMismatch");
        };
        assert_eq!(text(src, span), "(fun(x) => x)(1, 2)");
    }

    #[test]
    fn a_lambda_is_a_value() {
        let src = "def main() = fun(x) => x;";
        let ast = Parser::new(Lexer::new(src)).parse().unwrap();
        let result = eval(&ast).unwrap();
        assert_eq!(result.ty(), Type::Fun);
    }

    /// Run `src` and return its result as printed by `Display`.
    fn printed(src: &str) -> String {
        let ast = Parser::new(Lexer::new(src)).parse().unwrap();
        eval(&ast).unwrap().to_string()
    }

    #[test]
    fn booleans_are_values() {
        assert_eq!(printed("def main() = true;"), "true");
        assert_eq!(printed("def main() = false;"), "false");
        let ast = Parser::new(Lexer::new("def main() = true;"))
            .parse()
            .unwrap();
        assert_eq!(eval(&ast).unwrap().ty(), Type::Bool);
    }

    #[test]
    fn booleans_flow_through_bindings_calls_and_closures() {
        assert_eq!(printed("def main() = { let b = false; b };"), "false");
        assert_eq!(printed("def id(x) = x; def main() = id(true);"), "true");
        let src = "def always(b) = fun() => b; def main() = always(false)();";
        assert_eq!(printed(src), "false");
    }

    #[test]
    fn booleans_are_not_numbers() {
        // As the left or right operand of arithmetic.
        let src = "def main() = true + 1;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Int, actual: Type::Bool, span }
                if text(src, span) == "true"
        ));
        let src = "def main() = 2 * false;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Int, actual: Type::Bool, span }
                if text(src, span) == "false"
        ));
        // Negation.
        let src = "def main() = -true;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Int, actual: Type::Bool, span }
                if text(src, span) == "true"
        ));
    }

    #[test]
    fn booleans_cannot_be_called() {
        let src = "def main() = true(1);";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Fun, actual: Type::Bool, span }
                if text(src, span) == "true"
        ));
    }

    #[test]
    fn bool_type_in_diagnostics() {
        let src = "def main() = true + 1;";
        let (_, label) = error(src).diagnostic().label.unwrap();
        assert_eq!(label, "expected `int`, found `bool`");
    }

    #[test]
    fn logical_operators() {
        for (src, expected) in [
            ("true && true", "true"),
            ("true && false", "false"),
            ("false && true", "false"),
            ("false || false", "false"),
            ("false || true", "true"),
            ("true || false", "true"),
        ] {
            assert_eq!(printed(&format!("def main() = {src};")), expected, "{src}");
        }
    }

    #[test]
    fn logical_operators_short_circuit() {
        // The right side would divide by zero; it must not be evaluated.
        assert_eq!(printed("def main() = false && (1 / 0 == 0);"), "false");
        assert_eq!(printed("def main() = true || (1 / 0 == 0);"), "true");
        // When the left side does not decide, the right side runs.
        assert!(matches!(
            error("def main() = true && (1 / 0 == 0);"),
            EvalError::DivisionByZero(_)
        ));
    }

    #[test]
    fn logical_operators_need_booleans() {
        let src = "def main() = true && 1;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Bool, actual: Type::Int, span }
                if text(src, span) == "1"
        ));
        let src = "def main() = 0 || true;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Bool, actual: Type::Int, span }
                if text(src, span) == "0"
        ));
        let (_, label) = error("def main() = true && 1;").diagnostic().label.unwrap();
        assert_eq!(label, "expected `bool`, found `int`");
    }

    #[test]
    fn equality_on_integers_and_booleans() {
        for (src, expected) in [
            ("1 == 1", "true"),
            ("1 == 2", "false"),
            ("1 != 2", "true"),
            ("1 != 1", "false"),
            ("true == true", "true"),
            ("true == false", "false"),
            ("true != false", "true"),
            ("false != false", "false"),
            // The results of comparisons are booleans too.
            ("(1 < 2) == (3 < 4)", "true"),
        ] {
            assert_eq!(printed(&format!("def main() = {src};")), expected, "{src}");
        }
    }

    #[test]
    fn equality_needs_matching_types() {
        let src = "def main() = 1 == true;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Int, actual: Type::Bool, span }
                if text(src, span) == "true"
        ));
        let src = "def main() = false != 0;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Bool, actual: Type::Int, span }
                if text(src, span) == "0"
        ));
    }

    #[test]
    fn functions_cannot_be_compared() {
        // Whichever side the function is on, that side is reported.
        for (src, operand) in [
            ("def f() = 1; def main() = f == f;", "f"),
            ("def f() = 1; def main() = f != 1;", "f"),
            ("def f() = 1; def main() = 1 == f;", "f"),
            ("def main() = 1 == fun() => 1;", "fun() => 1"),
        ] {
            let err = error(src);
            assert!(
                matches!(&err, EvalError::NotComparable { ty: Type::Fun, span } if text(src, *span) == operand),
                "input: {src:?}, got: {err:?}"
            );
        }
        // `f == 1`: the left side is reported even though the right is an integer.
        let src = "def f() = 1; def main() = f == 1;";
        let EvalError::NotComparable { span, .. } = error(src) else {
            panic!("expected NotComparable");
        };
        assert_eq!(span.start, src.find("f ==").unwrap());
    }

    #[test]
    fn not_comparable_diagnostic() {
        let src = "def f() = 1; def main() = f == f;";
        let diag = error(src).diagnostic();
        assert_eq!(diag.message, "values of type `fun` cannot be compared");
        assert_eq!(diag.label.unwrap().1, "this is a function");
    }

    #[test]
    fn if_else_picks_a_branch() {
        assert_eq!(printed("def main() = if true { 1 } else { 2 };"), "1");
        assert_eq!(printed("def main() = if false { 1 } else { 2 };"), "2");
        assert_eq!(
            printed("def main() = if 1 < 2 && 2 < 3 { 1 } else { 2 };"),
            "1"
        );
        // The branches are blocks with their own scope.
        let src = "def main() = { let x = 1; if x == 1 { let x = 10; x } else { x } };";
        assert_eq!(printed(src), "10");
    }

    #[test]
    fn else_if_chains() {
        let src = "def sign(n) = if n < 0 { -1 } else if n == 0 { 0 } else { 1 };
                   def main() = sign(N);";
        for (n, expected) in [("-5", "-1"), ("0", "0"), ("7", "1")] {
            assert_eq!(printed(&src.replace('N', n)), expected, "n = {n}");
        }
    }

    #[test]
    fn only_the_chosen_branch_is_evaluated() {
        // The other branch would divide by zero.
        assert_eq!(printed("def main() = if true { 1 } else { 1 / 0 };"), "1");
        assert_eq!(printed("def main() = if false { 1 / 0 } else { 2 };"), "2");
        assert_eq!(printed("def main() = if false { 1 / 0 };"), "()");
    }

    #[test]
    fn if_without_else_is_unit() {
        assert_eq!(printed("def main() = if true { 1 };"), "()");
        assert_eq!(printed("def main() = if false { 1 };"), "()");
        let ast = Parser::new(Lexer::new("def main() = if true { 1 };"))
            .parse()
            .unwrap();
        assert_eq!(eval(&ast).unwrap().ty(), Type::Unit);
        // The branch still runs: an error inside it is reported.
        assert!(matches!(
            error("def main() = if true { 1 / 0 };"),
            EvalError::DivisionByZero(_)
        ));
    }

    #[test]
    fn if_condition_must_be_a_boolean() {
        let src = "def main() = if 1 { 2 } else { 3 };";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Bool, actual: Type::Int, span }
                if text(src, span) == "1"
        ));
        let src = "def f() = 1; def main() = if f { 2 } else { 3 };";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Bool, actual: Type::Fun, span }
                if text(src, span) == "f"
        ));
    }

    #[test]
    fn unit_values() {
        // Unit compares equal to itself.
        assert_eq!(
            printed("def main() = (if false { 1 }) == (if true { 2 });"),
            "true"
        );
        assert_eq!(
            printed("def main() = (if false { 1 }) != (if false { 2 });"),
            "false"
        );
        // But it is not a number or a boolean.
        let src = "def main() = (if false { 1 }) + 1;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch {
                expected: Type::Int,
                actual: Type::Unit,
                ..
            }
        ));
        let src = "def main() = (if false { 1 }) == 1;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Unit, actual: Type::Int, span }
                if text(src, span) == "1"
        ));
        let (_, label) = error(src).diagnostic().label.unwrap();
        assert_eq!(label, "expected `()`, found `int`");
    }

    #[test]
    fn unit_literal() {
        assert_eq!(printed("def main() = ();"), "()");
        assert_eq!(printed("def main() = () == ();"), "true");
        // The literal and a missing `else` produce the same value.
        assert_eq!(printed("def main() = () == if false { 1 };"), "true");
        assert_eq!(
            printed("def id(x) = x; def main() = { let u = (); id(u) };"),
            "()"
        );
        let src = "def main() = () + 1;";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Int, actual: Type::Unit, span }
                if text(src, span) == "()"
        ));
        let src = "def main() = ()();";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Fun, actual: Type::Unit, span }
                if text(src, span) == "()"
        ));
    }

    #[test]
    fn recursion_terminates_with_if() {
        let fact = "def fact(n) = if n == 0 { 1 } else { n * fact(n - 1) };";
        assert_eq!(
            printed(&format!("{fact} def main() = fact(10);")),
            "3628800"
        );
        assert_eq!(printed(&format!("{fact} def main() = fact(0);")), "1");
        let fib = "def fib(n) = if n < 2 { n } else { fib(n - 1) + fib(n - 2) };";
        assert_eq!(printed(&format!("{fib} def main() = fib(20);")), "6765");
    }

    #[test]
    fn mutual_recursion() {
        let src = "def even(n) = if n == 0 { true } else { odd(n - 1) };
                   def odd(n) = if n == 0 { false } else { even(n - 1) };
                   def main() = even(10) && odd(7) && !even(3);";
        assert_eq!(printed(src), "true");
    }

    #[test]
    fn strings_are_values() {
        assert_eq!(printed(r#"def main() = "hi";"#), r#""hi""#);
        assert_eq!(printed(r#"def main() = "";"#), r#""""#);
        let ast = Parser::new(Lexer::new(r#"def main() = "hi";"#))
            .parse()
            .unwrap();
        assert_eq!(eval(&ast).unwrap().ty(), Type::Str);
    }

    #[test]
    fn string_results_are_displayed_escaped() {
        assert_eq!(printed(r#"def main() = "a\tb\nc";"#), r#""a\tb\nc""#);
        assert_eq!(printed(r#"def main() = "say \"hi\"";"#), r#""say \"hi\"""#);
        assert_eq!(printed(r#"def main() = "\\";"#), r#""\\""#);
    }

    #[test]
    fn strings_flow_through_bindings_calls_and_closures() {
        assert_eq!(printed(r#"def main() = { let s = "x"; s };"#), r#""x""#);
        assert_eq!(printed(r#"def id(x) = x; def main() = id("y");"#), r#""y""#);
        let src = r#"def greet(name) = fun() => name; def main() = greet("z")();"#;
        assert_eq!(printed(src), r#""z""#);
    }

    #[test]
    fn string_equality() {
        for (src, expected) in [
            (r#""a" == "a""#, "true"),
            (r#""a" == "b""#, "false"),
            (r#""a" != "b""#, "true"),
            (r#""" == """#, "true"),
            (r#""é" == "é""#, "true"),
            // Escapes are compared after they are replaced.
            (r#""\t" == "	""#, "true"),
            (r#"{ let s = "x"; s == s }"#, "true"),
        ] {
            assert_eq!(printed(&format!("def main() = {src};")), expected, "{src}");
        }
    }

    #[test]
    fn strings_are_not_numbers() {
        // No concatenation or ordering on strings (yet).
        let src = r#"def main() = "a" + "b";"#;
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Int, actual: Type::Str, span }
                if text(src, span) == r#""a""#
        ));
        let src = r#"def main() = "a" < "b";"#;
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch {
                expected: Type::Int,
                actual: Type::Str,
                ..
            }
        ));
        let src = r#"def main() = -"a";"#;
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch {
                expected: Type::Int,
                actual: Type::Str,
                ..
            }
        ));
    }

    #[test]
    fn strings_compare_only_with_strings() {
        let src = r#"def main() = "x" == 1;"#;
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Str, actual: Type::Int, span }
                if text(src, span) == "1"
        ));
        let (_, label) = error(src).diagnostic().label.unwrap();
        assert_eq!(label, "expected `string`, found `int`");
    }

    /// Run `src`, returning the printed result and what the program wrote.
    fn run(src: &str) -> (String, String) {
        let ast = Parser::new(Lexer::new(src)).parse().unwrap();
        let mut out = Vec::new();
        let value = eval_with_output(&ast, &mut out).unwrap().to_string();
        (value, String::from_utf8(out).unwrap())
    }

    #[test]
    fn println_writes_the_raw_string_and_a_newline() {
        let src = r#"def main() = { println("a\tb"); println("say \"hi\"") };"#;
        assert_eq!(run(src), ("()".into(), "a\tb\nsay \"hi\"\n".into()));
        assert_eq!(run(r#"def main() = println("");"#).1, "\n");
    }

    #[test]
    fn output_follows_evaluation_order() {
        let src = r#"
            def say(s, n) = { println(s); n };
            def main() = say("left", 1) + say("right", 2);
        "#;
        assert_eq!(run(src), ("3".into(), "left\nright\n".into()));
        // Only the branch taken runs.
        let src = r#"def main() = if false { println("no") } else { println("yes") };"#;
        assert_eq!(run(src).1, "yes\n");
    }

    #[test]
    fn int_to_str_formats_in_decimal() {
        assert_eq!(run("def main() = int_to_str(42);").0, r#""42""#);
        assert_eq!(run("def main() = int_to_str(-7);").0, r#""-7""#);
        let src = "def main() = int_to_str(-9223372036854775807 - 1);";
        assert_eq!(run(src).0, r#""-9223372036854775808""#);
        let src = "def main() = println(int_to_str(10 * 10));";
        assert_eq!(run(src).1, "100\n");
    }

    #[test]
    fn builtins_are_values() {
        assert_eq!(run("def main() = println;").0, "BUILTIN println");
        let src = "def apply(f, x) = f(x); def main() = apply(int_to_str, 5);";
        assert_eq!(run(src).0, r#""5""#);
        let src = r#"def main() = { let p = println; p("via let") };"#;
        assert_eq!(run(src).1, "via let\n");
    }

    #[test]
    fn builtins_check_argument_count() {
        let src = "def main() = println();";
        assert!(matches!(
            error(src),
            EvalError::ArgumentCountMismatch { expected: 1, actual: 0, span }
                if text(src, span) == "println()"
        ));
        let src = r#"def main() = int_to_str(1, 2);"#;
        assert!(matches!(
            error(src),
            EvalError::ArgumentCountMismatch {
                expected: 1,
                actual: 2,
                ..
            }
        ));
    }

    #[test]
    fn builtins_check_argument_types() {
        let src = "def main() = println(1 + 2);";
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Str, actual: Type::Int, span }
                if text(src, span) == "1 + 2"
        ));
        let src = r#"def main() = int_to_str("1");"#;
        assert!(matches!(
            error(src),
            EvalError::TypeMismatch { expected: Type::Int, actual: Type::Str, span }
                if text(src, span) == r#""1""#
        ));
    }

    #[test]
    fn builtins_cannot_be_compared() {
        let src = "def main() = println == println;";
        assert!(matches!(
            error(src),
            EvalError::NotComparable { ty: Type::Fun, span }
                if span.start == src.find("println").unwrap()
        ));
        let src = "def main() = 1 == int_to_str;";
        assert!(matches!(
            error(src),
            EvalError::NotComparable { ty: Type::Fun, span }
                if text(src, span) == "int_to_str"
        ));
    }

    #[test]
    fn top_level_def_cannot_redefine_a_builtin() {
        let src = "def main() = 1; def println(s) = s;";
        assert!(matches!(
            error(src),
            EvalError::RedefinesBuiltin { name, span }
                if name == "println" && span.start == src.find("println").unwrap()
                    && text(src, span) == "println"
        ));
    }

    #[test]
    fn locals_may_shadow_builtins() {
        let src = "def main() = { let println = fun(x) => x + 1; println(1) };";
        assert_eq!(run(src), ("2".into(), String::new()));
        let src = "def f(int_to_str) = int_to_str * 2; def main() = f(3);";
        assert_eq!(run(src).0, "6");
    }

    #[test]
    fn output_errors_point_at_the_call() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("stream closed"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let src = r#"def main() = println("x");"#;
        let ast = Parser::new(Lexer::new(src)).parse().unwrap();
        let err = eval_with_output(&ast, &mut Closed).unwrap_err();
        assert!(matches!(
            &err,
            EvalError::Output { message, span }
                if message == "stream closed" && text(src, *span) == r#"println("x")"#
        ));
    }

    #[test]
    fn builtin_names_are_unique() {
        let mut names: Vec<_> = Builtin::ALL.iter().map(|b| b.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Builtin::ALL.len());
    }

    #[test]
    fn print_writes_without_a_newline() {
        let src = r#"def main() = { print("a"); print("b"); println("c"); print("d") };"#;
        assert_eq!(run(src), ("()".into(), "abc\nd".into()));
    }
}
