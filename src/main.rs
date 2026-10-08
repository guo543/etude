use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::{env, fs};

use etude::diagnostic::Diagnostic;
use etude::eval::{self, EvalError};
use etude::lexer::Lexer;
use etude::parser::{Parser, ParserError};

const USAGE: &str = "usage: etude [--ast] <file>";

enum Error {
    Parse(ParserError),
    Eval(EvalError),
}

impl Error {
    fn diagnostic(&self) -> Diagnostic {
        match self {
            Error::Parse(e) => e.diagnostic(),
            Error::Eval(e) => e.diagnostic(),
        }
    }
}

/// Lex, parse and evaluate a source file. With `print_ast`, the parsed
/// program is printed before it runs.
fn run(input: &str, print_ast: bool) -> Result<eval::Value, Error> {
    let ast = Parser::new(Lexer::new(input))
        .parse()
        .map_err(Error::Parse)?;
    if print_ast {
        println!("{ast}");
    }
    eval::eval(&ast).map_err(Error::Eval)
}

fn main() -> ExitCode {
    let mut print_ast = false;
    let mut path = None;
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--ast" => print_ast = true,
            _ if arg.starts_with('-') || path.is_some() => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
            _ => path = Some(arg),
        }
    }
    let Some(path) = path else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    let src = match fs::read_to_string(&path) {
        Ok(src) => src,
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    match run(&src, print_ast) {
        Ok(value) => {
            println!("Result: {value}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            // Colors only on a terminal, not when stderr goes to a file or pipe.
            let color = io::stderr().is_terminal();
            e.diagnostic().eprint(&path, &src, color);
            ExitCode::FAILURE
        }
    }
}
