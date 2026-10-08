//! End-to-end tests: run the `etude` binary on every program in `examples/`.
//!
//! - `examples/NAME.etd` must succeed, and what it prints must equal
//!   `examples/NAME.out`.
//! - `examples/errors/NAME.etd` must fail with exit code 1, and what it prints
//!   to stderr must equal `examples/errors/NAME.err`.
//!
//! To add a case, add the `.etd` file and run
//!
//!     BLESS=1 cargo test --test examples
//!
//! which writes the actual output into the expected files instead of
//! comparing. Review the result with `git diff` before committing.
//!
//! Trailing whitespace is ignored on every line: error reports contain some,
//! and editors tend to strip it from the expected files.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The `.etd` files directly inside `dir`, relative to the project root, so
/// that the paths `etude` prints are the same on every machine.
fn etd_files(dir: &str) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(root().join(dir))
        .unwrap_or_else(|e| panic!("cannot read {dir}: {e}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "etd"))
        .map(|path| Path::new(dir).join(path.file_name().unwrap()))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .etd files in {dir}");
    files
}

fn run_etude(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_etude"))
        .arg(path)
        .current_dir(root())
        .output()
        .expect("failed to run etude")
}

fn normalize(text: &str) -> String {
    let mut lines: Vec<_> = text.lines().map(str::trim_end).collect();
    while lines.last() == Some(&"") {
        lines.pop();
    }
    lines.join("\n")
}

fn bless() -> bool {
    env::var_os("BLESS").is_some()
}

/// Compare `actual` with the expected file for `path`, or overwrite the
/// expected file when blessing. Returns a description of any mismatch.
fn check(path: &Path, ext: &str, actual: &str) -> Option<String> {
    let expected_path = root().join(path.with_extension(ext));
    let actual = normalize(actual);
    if bless() {
        fs::write(&expected_path, format!("{actual}\n")).unwrap();
        return None;
    }
    let expected = match fs::read_to_string(&expected_path) {
        Ok(text) => normalize(&text),
        Err(e) => {
            return Some(format!(
                "{}: cannot read expected output: {e}",
                path.display()
            ));
        }
    };
    (expected != actual).then(|| {
        format!(
            "{}:\n--- expected\n{expected}\n--- actual\n{actual}",
            path.display()
        )
    })
}

/// Panics with every failure at once, not just the first.
fn report(failures: Vec<String>) {
    assert!(
        failures.is_empty(),
        "{} example(s) failed (rerun with BLESS=1 to update the expected output):\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn examples_run() {
    let mut failures = Vec::new();
    for path in etd_files("examples") {
        let output = run_etude(&path);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() {
            failures.push(format!(
                "{}: expected success, got {}\n{}",
                path.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim_end()
            ));
        } else if let Some(failure) = check(&path, "out", &stdout) {
            failures.push(failure);
        }
    }
    report(failures);
}

#[test]
fn examples_fail_with_expected_error() {
    let mut failures = Vec::new();
    for path in etd_files("examples/errors") {
        let output = run_etude(&path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.code() != Some(1) {
            failures.push(format!(
                "{}: expected exit code 1, got {}",
                path.display(),
                output.status
            ));
        } else if let Some(failure) = check(&path, "err", &stderr) {
            failures.push(failure);
        }
    }
    report(failures);
}
