//! End-to-end tests: run the `etude` binary on every program in `examples/`.
//!
//! - `examples/NAME.etd` must succeed, and the last line it prints must equal
//!   `examples/NAME.out`. Only the last line is compared because `etude` also
//!   prints the AST before the result.
//! - `examples/errors/NAME.etd` must fail with exit code 1, and what it prints
//!   to stderr must equal `examples/errors/NAME.err`.
//!
//! To add a case, add the `.etd` file and its expected output next to it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// The `.etd` files directly inside `dir`, sorted so failures are reported
/// in a stable order.
fn etd_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "etd"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .etd files in {}", dir.display());
    files
}

fn expected(path: &Path, ext: &str) -> String {
    let expected = path.with_extension(ext);
    fs::read_to_string(&expected)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", expected.display()))
        .trim_end()
        .to_owned()
}

fn run_etude(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_etude"))
        .arg(path)
        .output()
        .expect("failed to run etude")
}

/// Panics with every failure at once, not just the first.
fn report(failures: Vec<String>) {
    assert!(
        failures.is_empty(),
        "{} example(s) failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn examples_run() {
    let mut failures = Vec::new();
    for path in etd_files(&examples_dir()) {
        let want = expected(&path, "out");
        let output = run_etude(&path);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let got = stdout.lines().last().unwrap_or("");

        if !output.status.success() {
            failures.push(format!(
                "{}: expected success, got {}\nstderr: {}",
                path.display(),
                output.status,
                stderr.trim_end()
            ));
        } else if got != want {
            failures.push(format!(
                "{}:\n  expected: {want}\n  got:      {got}",
                path.display()
            ));
        }
    }
    report(failures);
}

#[test]
fn examples_fail_with_expected_error() {
    let mut failures = Vec::new();
    for path in etd_files(&examples_dir().join("errors")) {
        let want = expected(&path, "err");
        let output = run_etude(&path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let got = stderr.trim_end();

        if output.status.code() != Some(1) {
            failures.push(format!(
                "{}: expected exit code 1, got {}",
                path.display(),
                output.status
            ));
        } else if got != want {
            failures.push(format!(
                "{}:\n  expected: {want}\n  got:      {got}",
                path.display()
            ));
        }
    }
    report(failures);
}
