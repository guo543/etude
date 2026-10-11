use std::fs;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser as _, ValueEnum};

use etude::diagnostic::Diagnostic;
use etude::eval::{self, EvalError};
use etude::lexer::Lexer;
use etude::parser::{Parser, ParserError};

/// Run an etude program: define its functions, then call `main`.
#[derive(clap::Parser)]
#[command(version, about)]
struct Cli {
    /// The program to run.
    file: PathBuf,

    /// Print the parsed program before running it.
    #[arg(long)]
    ast: bool,

    /// When to color error messages.
    #[arg(long, value_enum, value_name = "WHEN", default_value_t = Color::Auto)]
    color: Color,
}

#[derive(Clone, Copy, ValueEnum)]
enum Color {
    /// Only when writing to a terminal.
    Auto,
    /// Even when writing to a file or pipe, e.g. into `less -R`.
    Always,
    /// Plain text.
    Never,
}

impl Color {
    fn enabled(self) -> bool {
        match self {
            Color::Auto => io::stderr().is_terminal(),
            Color::Always => true,
            Color::Never => false,
        }
    }
}

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
    // On bad arguments clap prints the error and usage and exits with code 2.
    let cli = Cli::parse();
    let path = cli.file.display().to_string();

    let src = match fs::read_to_string(&cli.file) {
        Ok(src) => src,
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    match run(&src, cli.ast) {
        Ok(value) => {
            println!("Result: {value}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            e.diagnostic().eprint(&path, &src, cli.color.enabled());
            ExitCode::FAILURE
        }
    }
}
