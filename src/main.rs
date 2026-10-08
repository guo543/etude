use std::process::ExitCode;
use std::{env, fs};

use etude::eval;
use etude::lexer::Lexer;
use etude::parser::Parser;

/// Lex, parse and evaluate a source file.
fn run(input: &str) -> Result<eval::Value, String> {
    let ast = Parser::new(Lexer::new(input))
        .parse()
        .map_err(|e| format!("Parser: {e}"))?;
    println!("{ast}");
    eval::eval(&ast).map_err(|e| format!("Eval: {e}"))
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
            println!("Result: {value}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}
