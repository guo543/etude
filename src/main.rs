use std::io::{self, BufRead, Write};

use etude::interpreter;
use etude::lexer::Lexer;
use etude::parser::Parser;

/// Lex, parse and evaluate one line of input.
fn run(input: &str) -> Result<i64, String> {
    let ast = Parser::new(Lexer::new(input))
        .parse()
        .map_err(|e| format!("Parser: {e}"))?;
    interpreter::eval(&ast).map_err(|e| format!("Eval: {e}"))
}

fn main() {
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

        match run(&line) {
            Ok(value) => println!("{value}"),
            Err(msg) => eprintln!("{msg}"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::run;
    use std::assert_matches;

    fn eval(input: &str) -> Result<i64, String> {
        run(input)
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
        assert_matches!(eval("9223372036854775807 + 1"), Err(msg) if msg.contains("overflow"));
        assert_matches!(eval("1 2"), Err(msg) if msg.starts_with("Parser"));
        assert_matches!(eval("1 $ 2"), Err(msg) if msg.contains("Unexpected char"));
    }
}
