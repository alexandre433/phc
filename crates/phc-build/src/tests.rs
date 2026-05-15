// SPDX-License-Identifier: MIT
//! End-to-end build tests. These actually invoke the system `cc`,
//! so they are gated on its availability — running `cargo test`
//! without a C toolchain skips them with a printed note rather
//! than failing.

use crate::build_file;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn cc_available() -> bool {
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    Command::new(cc)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn output_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from("target");
    p.push("phc-build-test");
    std::fs::create_dir_all(&p).expect("create test output dir");
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    p.push(exe);
    p
}

#[test]
fn build_and_run_hello_world() {
    if !cc_available() {
        eprintln!("skipping build_and_run_hello_world: no C compiler on PATH");
        return;
    }
    let input = Path::new("../../examples/hello.phc");
    let output = output_path("hello_world");
    let result = build_file(input, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = Command::new(&output)
        .output()
        .expect("invoke compiled binary");
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(
        stdout.contains("Hello, PHC!"),
        "unexpected stdout: {stdout:?}"
    );
}

/// Build + run a synthesised .phc that exercises arithmetic,
/// control flow, function arguments, and string interpolation —
/// the full C2 surface. Writes the source to a scratch file to
/// avoid coupling the test to an in-tree example.
#[test]
fn build_and_run_arithmetic_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_arithmetic_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function add(int $a, int $b): int { return $a + $b; }

function classify(int $n): string {
    if ($n < 0) { return "negative"; }
    else if ($n == 0) { return "zero"; }
    else { return "positive"; }
}

function main(): void {
    flip int $sum = 0;
    flip int $i = 1;
    while ($i <= 5) {
        $sum := add($sum, $i);
        $i := $i + 1;
    }
    Logger::info("sum is {$sum}");
    Logger::info("classify(7) -> {classify(7)}");
    Logger::info("classify(0) -> {classify(0)}");
    Logger::info("classify(-3) -> {classify(-3)}");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("arithmetic.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("arithmetic");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = Command::new(&output)
        .output()
        .expect("invoke compiled binary");
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec![
            "sum is 15",
            "classify(7) -> positive",
            "classify(0) -> zero",
            "classify(-3) -> negative",
        ]
    );
}

/// Build + run a class-based program that exercises constructors,
/// promoted parameters, methods, field reads, member assignment via
/// `=`, and trait method mixin — the full C3 surface.
#[test]
fn build_and_run_class_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_class_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

public trait Doubled {
    function doubled(): int { return $this->value + $this->value; }
}

public class Box {
    use Doubled;
    construct(public int $value) {}

    public function get(): int { return $this->value; }
}

function main(): void {
    Box $b = Box(7);
    Logger::info("value is {$b->get()}");
    Logger::info("doubled is {$b->doubled()}");
    $b->value = 100;
    Logger::info("after assign: {$b->get()}");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("classes.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("classes");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = Command::new(&output)
        .output()
        .expect("invoke compiled binary");
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec!["value is 7", "doubled is 14", "after assign: 100",]
    );
}
