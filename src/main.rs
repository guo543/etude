mod ast;
mod interpreter;
mod lexer;
mod parser;

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser;

/// Lex, parse and evaluate one line of input.
fn run(input: &str, env: &HashMap<String, i32>) -> Result<i32, String> {
    let lexer = Lexer::new(input).map_err(|e| format!("Lexer: {e}"))?;
    let ast = Parser::new(lexer)
        .parse()
        .map_err(|e| format!("Parser: {e}"))?;
    Interpreter::eval(&ast, env).map_err(|e| format!("Eval: {e}"))
}

fn main() {
    let env = HashMap::<String, i32>::new();
    let stdin = io::stdin();

    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap() == 0 {
            break; // EOF (Ctrl-D)
        }
        if line.trim().is_empty() {
            continue;
        }

        match run(&line, &env) {
            Ok(value) => println!("{value}"),
            Err(msg) => eprintln!("{msg}"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::run;
    use crate::lexer::*;
    use std::assert_matches;
    use std::collections::HashMap;

    fn eval(input: &str) -> Result<i32, String> {
        run(input, &HashMap::new())
    }

    #[test]
    fn test_lexer_creation() {
        let input = "";
        assert_matches!(Lexer::new(input), Ok(_));
    }

    #[test]
    fn test_precedence_and_associativity() {
        assert_eq!(eval("10- 1 + 2 * (3 - 4)"), Ok(7));
        assert_eq!(eval("1 - 2 - 3"), Ok(-4));
        assert_eq!(eval("8 / 4 / 2"), Ok(1));
        assert_eq!(eval("((7))"), Ok(7));
    }

    #[test]
    fn test_errors() {
        assert_matches!(eval("1 / (2 - 2)"), Err(msg) if msg.contains("Division by zero"));
        assert_matches!(eval("2147483647 + 1"), Err(msg) if msg.contains("overflow"));
        assert_matches!(eval("1 2"), Err(msg) if msg.starts_with("Parser"));
        assert_matches!(eval("1 $ 2"), Err(msg) if msg.contains("Unknown character"));
    }
}
