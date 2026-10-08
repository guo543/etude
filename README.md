# etude
A small programming language

# Etude implementation plan

The language is described in [`grammar.ebnf`](grammar.ebnf). This plan takes
the implementation there in small steps; each one ends with something that
runs and is tested.

## Where things stand

- **Lexer:** span-only tokens, never fails (bad characters become `Unknown`).
  Tokens: `+ - * / = , ; ( ) { }`, `let`, `def`, identifiers, integers.
- **Parser:** a program of `def` items; blocks with `let` and expression
  statements; unary minus; binary `+ - * /` with precedence in the parser;
  calls `f(a)(b)` on any expression; every node has a span.
- **Interpreter:** a tree walker over the AST. Runs `main`; functions are
  values; integers with overflow and division checks.
- **Errors:** reported with ariadne (`src/diagnostic.rs`): headline, file,
  underlined source.
- **CLI:** `etude [--ast] <file>`; `--ast` prints the parsed program first.

---

## 1. Loose ends

- [x] `def` ends with `;`, as in `grammar.ebnf`: every definition, at the
      top level or in a block, ends with `;`.
- [x] Derive `Clone, Copy, PartialEq, Eq` on `UnOp`, like `BinOp`.
- [x] Replace the stale end-to-end tests in `main.rs` (see step 2).

## 2. Interpreter v1: run `main`

Evaluate what the parser already produces.

- [x] Expressions: `Int`, `Var`, `Unary` (`checked_neg`), `Binary` (checked
      arithmetic, division by zero), `Block` with `let` and expression
      statements.
- [x] Scopes: a stack of `HashMap<String, Value>`. A block pushes a scope and
      pops it at the end; `let` inserts into the top scope, and a later `let`
      with the same name shadows the earlier one. A name not in any scope is
      an "unbound variable" error.
- [x] Programs: find `def main()`; report an error if it is missing or has
      parameters; evaluate its body and print the result.
- [x] Errors carry the span of the expression that caused them.
- [x] Report parse and eval errors with **ariadne** (file name and underlined
      source line). Remember `IndexType::Byte`: spans are byte offsets.
      All of it lives in `src/diagnostic.rs`. Each error's `Display` is the
      headline, and `diagnostic()` adds the location, label and notes.
- [x] `examples/` folder of `.etd` programs with expected output, and an
      end-to-end test that runs each one (`tests/examples.rs`): `NAME.etd`
      with `NAME.out` must succeed, `errors/NAME.etd` with `NAME.err` must
      fail with that message. `BLESS=1 cargo test --test examples` rewrites
      the expected files.

## 3. Function calls

- [x] Parser: a postfix loop after atoms for `f(a, b)`.
- [ ] A `parse_comma_list` helper shared by arguments and parameters.
- [x] AST: `Call { callee: Box<Expr>, args: Vec<Expr> }`, not
      `Call { name, .. }`, so it still fits when functions become values.
- [x] Interpreter: collect all `def`s by name before running, so their order
      does not matter. Each call gets a fresh scope with parameters bound to
      the arguments.
- [x] Check the argument count.
- [ ] Call-depth limit, so deep recursion is an error, not a stack overflow.

## 4. Booleans, comparisons and `if`

- [ ] Lexer: `!`, `<`, `>`, and the two-character tokens `==`, `!=`, `<=`,
      `>=`, `&&`, `||` (longest match). Keywords `if`, `else`, `true`,
      `false`.
- [ ] Parser: the comparison level (non-associative: `a < b < c` is an
      error), `&&` and `||`, `!` in `unary_op`, and `if c { .. } else { .. }`
      as an expression, including `else if`.
- [ ] Interpreter: `enum Value { Int(i64), Bool(bool) }`, with runtime type
      errors for now. `&&` and `||` skip the right side when the left side
      decides the result.
- [ ] Example programs: factorial, fibonacci (recursion can now terminate).

## 5. Unit and `print`

- [ ] `()` literal and `Value::Unit`.
- [ ] Built-in `print(x)`, returning unit.
- [ ] Optional final expression in blocks (missing means unit).
- [ ] `if` without `else` has type unit.

## 6. Lambdas and closures

- [ ] Parser: `x => ..`, `(a, b) => ..`, `() => ..`. At `(` or an
      identifier, look ahead on a cloned lexer for `=>`. The body extends as
      far right as possible.
- [ ] Environments as a linked list of `Rc<Env>`, so closures capture their
      scope cheaply. Bindings never change, so capturing by value is enough.
- [ ] `Value::Closure`; functions become first-class values.
- [ ] `def f(params) = body` means `f = (params) => body`, visible in its own
      body and to all other `def`s.

## 7. Global `let`

- [ ] Top-level `let`, evaluated eagerly and in order before `main`.
- [ ] Reading a global before it is initialized is a runtime error.
- [ ] Top-level parsing dispatches on `def` / `let` (`parse_item`), with an
      "expected a definition" error otherwise.

## 8. Hindley-Milner type checking

A separate pass between parsing and running.

- [ ] Add spans everywhere type errors need them (already mostly there).
- [ ] Types `Int`, `Bool`, `Unit`, `Fn(Vec<Type>, Box<Type>)`, and type
      variables in a union-find table with levels.
- [ ] `unify` with occurs check; level-based `generalize`; `instantiate`.
- [ ] Let-polymorphism in blocks.
- [ ] Top-level `def`s: dependency graph and strongly connected components
      (Tarjan); each group is checked together, then generalized.
- [ ] Decide: is `==` polymorphic or only for `int` and `bool`?
- [ ] Decide: what does `main` return (any value, printed; or `int` as an
      exit code)?
- [ ] Runtime type checks from step 4 become internal errors.

## 9. Error recovery

- [ ] Collect errors in a `Vec` instead of stopping at the first one.
- [ ] Skip ahead to `;`, `}`, `def` or `let` after an error, then continue.
- [ ] `ExprKind::Error` nodes that later passes accept without producing
      follow-up errors.

---

## Later

- Optional type annotations: `def f(x: int) -> int`, `(x: int) => ..`, type
  variables like `'a`.
- Symbol interning (`Symbol(u32)`, global interner) once names are compared
  and stored a lot.
- Tuples, lists, algebraic data types and pattern matching.
- Local `def` (recursive closures inside blocks).
- A depth limit in the parser for very deeply nested input.

