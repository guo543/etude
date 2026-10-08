use std::process::ExitCode;
use std::{env, fs};

use etude::interpreter;
use etude::lexer::Lexer;
use etude::parser::Parser;

/// Lex, parse and evaluate a source file.
fn run(input: &str) -> Result<i64, String> {
    let ast = Parser::new(Lexer::new(input))
        .parse()
        .map_err(|e| format!("Parser: {e}"))?;
    interpreter::eval(&ast).map_err(|e| format!("Eval: {e}"))
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let (Some(path), None) = (args.next(), args.next()) else {
        eprintln!("usage: etude <file>");
        return ExitCode::from(2);
    };

    let src = match fs::read_to_string(&path) {
        Ok(src) => src,
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    match run(&src) {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
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
