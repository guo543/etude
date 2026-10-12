# etude

A small, expression-oriented programming language that leans functional
(immutable bindings, first-class functions, closures) while still allowing
side effects. Written in Rust as a learning project.

```
def make_adder(n) = fun(x) => x + n;

def compose(f, g) = fun(x) => f(g(x));

def main() = {
    let add10 = make_adder(10);
    let double = fun(x) => x * 2;
    compose(double, add10)(1)
};
```

The planned language is described in [`grammar.ebnf`](grammar.ebnf).

## Usage

```
cargo run -- examples/closures.etd          # run a program: prints `Result: …`
cargo run -- --ast examples/closures.etd    # print the parsed program first
cargo run -- --help
```

Tests:

```
cargo test                                  # unit tests and all examples
BLESS=1 cargo test --test examples          # rewrite expected example output
```

`examples/NAME.etd` must run and print `examples/NAME.out`;
`examples/errors/NAME.etd` must fail with the report in
`examples/errors/NAME.err`.

## Where things stand

- **Lexer** (`src/lexer.rs`): tokens carry only a kind and a span; it never
  fails (bad characters become `Unknown`). Tokens: `+ - * / = => , ; ( ) { }`,
  keywords `let def fun true false`, identifiers (ASCII letters, digits,
  `_`), integers.
- **Parser** (`src/parser.rs`): rustc-style recursive descent that always
  holds the current token. A program is a list of `def name(params) = expr;`.
  Expressions: integers, `true`/`false`, variables, unary `-`, binary `+ - * /` (precedence
  table in the parser), calls `f(a)(b)` on any expression, lambdas
  `fun(params) => body`, and blocks `{ let x = e; e; tail }`. Every node has
  a span; errors at end of input point right after the last token.
- **Evaluator** (`src/eval.rs`): a tree walker that borrows from the AST.
  Top-level `def`s live in a global table and are closures with an empty
  captured environment; locals live in an immutable linked environment, so
  closures capture by sharing it. Values: integers, booleans and functions. Integer
  overflow, division by zero, wrong argument counts and type mismatches are
  runtime errors.
- **Errors** (`src/diagnostic.rs`): reported with ariadne. Each error's
  `Display` is the headline; `diagnostic()` adds the underlined location,
  label and notes.
- **CLI** (`src/main.rs`, clap): `etude [--ast] [--color auto|always|never]
  <file>`.
- **Tests:** 99 unit tests; 29 example programs (10 that run, 19 that must
  fail with a specific report).

## Design decisions so far

- **Blocks, not `let … in`.** `{ let x = e; …; tail }` is an expression whose
  value is its tail; it means `let x = e in { … }`.
- **`def` defines functions only,** always with a parameter list, and ends
  with `;`. `def f(a) = body;` means `f = fun(a) => body`.
- **Lambdas:** `fun(params) => body`. `=` is for definitions, `=>` maps
  parameters to a body. The body extends as far right as possible.
- **Scoping is lexical.** A function sees its parameters, its own locals,
  whatever it captured where it was written, and globals — never its
  caller's locals.
- **Shadowing is allowed for locals,** in nested and in the same scope
  (`let x = x + 1;`), as in OCaml and Rust. Top-level names must be unique,
  because top-level definitions see each other in any order.
- **Functions take a parameter list and are not curried.** Currying is
  written out: `def add(a) = fun(b) => a + b;`.
- **The lexer never fails;** bad input becomes a token that the parser
  reports where it reaches it.

---

# Roadmap

Each step ends with something that runs and is tested.

## Done

- Lexer, parser and AST with spans; `def` items with `;`; blocks with `let`
  and expression statements; unary minus; calls on any expression.
- Tree-walking evaluator: globals table, immutable linked environment,
  borrowing from the AST; `main` must exist and take no parameters.
- Lambdas and closures (`fun(params) => body`), currying, functions as values.
- Error reports with ariadne; clap CLI with `--ast` and `--color`.
- End-to-end example tests with `BLESS=1`.

## 1. Loose ends

- [ ] **Duplicate top-level names are an error:** "`f` is defined more than
      once". Today the last definition silently wins. Give `Diagnostic`
      secondary labels so the report can also point at the first definition.
- [ ] **Call-depth limit:** `def main() = main();` currently overflows the
      Rust stack and aborts. Count call depth in the evaluator and report
      "recursion too deep" instead.
- [ ] **`parse_comma_list`:** the parameter/argument loop exists three times
      (`def` parameters, lambda parameters, call arguments).
- [ ] **Share `def` and lambda:** a `Lambda { params, body }` struct used by
      both `DeclKind::FunDecl { id, lambda }` and `ExprKind::Lambda`, since a
      `def` is a name bound to a lambda.

## 2. Booleans, comparisons and `if`

- [ ] Lexer: `!`, `<`, `>`, and `==`, `!=`, `<=`, `>=`, `&&`, `||` (longest
      match, like `=>`). Keywords `if`, `else` (`true`, `false` are already
      lexed).
- [x] `true`/`false` atoms in the parser; `Value::Bool` and `Type::Bool` in
      the evaluator.
- [ ] Parser: the comparison level (non-associative: `a < b < c` is an
      error), `&&` and `||`, `!` in `unary_op`, and `if c { .. } else { .. }`
      as an expression, including `else if`.
- [ ] Evaluator: comparisons produce booleans; `&&` and `||` skip the right
      side when the left side decides the result.
- [ ] Examples: factorial and fibonacci (recursion can now terminate).

## 3. Unit and `print`

- [ ] `()` literal and `Value::Unit`.
- [ ] Built-in `print(x)`, returning unit.
- [ ] Optional block tail (a missing tail means unit); `if` without `else`
      has type unit.

## 4. Global `let`

- [ ] Top-level `let`, evaluated eagerly and in order before `main`.
- [ ] Reading a global before it is initialized is a runtime error.
- [ ] Top-level parsing dispatches on `def` / `let` (`parse_item`), with an
      "expected a definition" error otherwise.

## 5. Strings

- [ ] String literals with escapes, `Value::Str`, a `string` type, and
      concatenation.

## 6. Algebraic data types and `match` (checked at run time)

```
type list<a> = | Nil | Cons(a, list<a>);
type shape = | Circle(int) | Rect { w: int, h: int };

def sum(xs) = match xs {
    Nil => 0,
    Cons(x, rest) => x + sum(rest),
};
```

- [ ] `type` items with type parameters; constructors start with an
      uppercase letter, variables and types with a lowercase one.
- [ ] Type syntax (`int`, `list<a>`, `(int, int) -> int`), shared with the
      later optional annotations.
- [ ] Constructor expressions and `match` with patterns: `_`, variables,
      literals, nested constructors; later `|` alternatives and `if` guards.
- [ ] `Value::Data { tag, fields }`.
- [ ] Possibly: `bool` and `option` as ordinary ADTs, `if` as sugar for
      `match`.

## 7. Name resolution

- [ ] A pass between parsing and evaluation that links every variable use to
      its binding: "cannot find `x`" before running, and the information the
      type checker needs.

## 8. Hindley-Milner type checking

A separate pass between parsing and running.

- [ ] Types `int`, `bool`, `unit`, `string`, functions, ADTs with parameters,
      and type variables in a union-find table with levels.
- [ ] `unify` with occurs check; level-based `generalize`; `instantiate`;
      let-polymorphism in blocks.
- [ ] Top-level `def`s and `type`s: dependency graph and strongly connected
      components (Tarjan); each group is checked together, then generalized.
- [ ] Exhaustiveness and redundancy checking for `match` (Maranget).
- [ ] Decide: is `==` polymorphic or only for `int` and `bool`?
- [ ] Decide: what does `main` return (any value, printed; or `int` as an
      exit code)?
- [ ] Discarding a non-unit value in a statement (`1 + 1;`) is an error.
- [ ] Runtime type checks in the evaluator become internal errors.

## 9. Records

- [ ] Nominal records: `type person = { name: string, age: int };`, field
      access `p.name`, functional update.
- [ ] Record literals vs. blocks: `{ name: …` (identifier then `:`) starts a
      record; in `if`/`match` conditions record literals need parentheses.

## 10. Error recovery

- [ ] Collect errors in a `Vec` instead of stopping at the first one.
- [ ] Skip ahead to `;`, `}`, `def`, `let` or `type` after an error.
- [ ] `ExprKind::Error` nodes that later passes accept without producing
      follow-up errors.

## Later

- Optional type annotations: `def f(x: int) -> int`, `fun(x: int) => …`;
  polymorphic recursion with an annotated signature.
- Symbol interning (`Symbol(u32)`), so names are copied and compared as
  integers.
- Faster closures: with name resolution, array frames or flat closures
  instead of the linked environment.
- Local recursive functions (`def` or `let rec` inside blocks).
- A depth limit in the parser for very deeply nested input.
- Warnings: unused variables, shadowed bindings that are never read.
