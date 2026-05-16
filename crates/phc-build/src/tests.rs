// SPDX-License-Identifier: MIT
//! End-to-end build tests. These actually invoke the system `cc`,
//! so they are gated on its availability — running `cargo test`
//! without a C toolchain skips them with a printed note rather
//! than failing.

use crate::{build_file, load_session};
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

fn project_root(name: &str) -> PathBuf {
    let mut p = PathBuf::from("target");
    p.push("phc-build-session");
    p.push(name);
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("create project root");
    p
}

#[test]
fn session_loads_files_and_groups_by_pack() {
    let root = project_root("group_by_pack");
    std::fs::write(
        root.join("a.phc"),
        "pack app.auth;\nfunction main(): void {}",
    )
    .unwrap();
    std::fs::write(
        root.join("b.phc"),
        "pack app.auth;\nfunction helper(): int { return 0; }",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("db")).unwrap();
    std::fs::write(
        root.join("db").join("conn.phc"),
        "pack app.db;\nfunction connect(): int { return 1; }",
    )
    .unwrap();

    let session = load_session(&root);
    assert!(session.ok(), "diagnostics: {:?}", session.diagnostics);
    assert_eq!(session.files.len(), 3);
    assert_eq!(session.packs.len(), 2);
    assert_eq!(session.packs.get("app.auth").map(Vec::len), Some(2));
    assert_eq!(session.packs.get("app.db").map(Vec::len), Some(1));
}

#[test]
fn session_surfaces_parse_errors() {
    let root = project_root("parse_error");
    std::fs::write(root.join("ok.phc"), "pack a;").unwrap();
    std::fs::write(root.join("bad.phc"), "function").unwrap();

    let session = load_session(&root);
    assert!(!session.ok());
    assert!(!session.diagnostics.is_empty());
    // Good file still loaded.
    assert!(session.packs.contains_key("a"));
}

/// Build + run an enum + match program. Exercises enum decls,
/// `Type::Variant` static access, match expressions with literal,
/// EnumVariant, OR, and Var patterns plus a guard.
#[test]
fn build_and_run_enum_match_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_enum_match_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

public enum Status { Ok, NotFound, Gone }

function classify(Status $s): int {
    return match ($s) {
        Status::Ok => 0,
        Status::NotFound | Status::Gone => 404,
        _ => -1,
    };
}

function bucket(int $n): string {
    return match ($n) {
        $code if $code > 500 => "server",
        0 => "zero",
        _ => "other",
    };
}

function main(): void {
    Logger::info("Ok -> {classify(Status::Ok)}");
    Logger::info("NotFound -> {classify(Status::NotFound)}");
    Logger::info("Gone -> {classify(Status::Gone)}");
    Logger::info("750 -> {bucket(750)}");
    Logger::info("0 -> {bucket(0)}");
    Logger::info("3 -> {bucket(3)}");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("enum_match.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("enum_match");
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
            "Ok -> 0",
            "NotFound -> 404",
            "Gone -> 404",
            "750 -> server",
            "0 -> zero",
            "3 -> other",
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
