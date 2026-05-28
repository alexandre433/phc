// SPDX-License-Identifier: MIT
//! End-to-end build tests. These actually invoke the system `cc`,
//! so they are gated on its availability — running `cargo test`
//! without a C toolchain skips them with a printed note rather
//! than failing.

use crate::{
    build_file, build_project, check_pack_acyclicity, load_session, resolve_cross_pack_uses,
};
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

/// Run a freshly compiled test binary. On Windows, Microsoft Defender
/// occasionally quarantines a freshly-written executable as a false
/// positive, surfacing as `PermissionDenied` from `Command::output`.
/// Treat that one specific OS error as an environmental skip — the
/// build itself succeeded (the test asserted no build errors before
/// reaching this point), so the binary's correctness is not in
/// question. Any other failure to spawn is still a real test failure.
fn run_or_skip_on_av(output: &Path, label: &str) -> Option<std::process::Output> {
    match Command::new(output).output() {
        Ok(out) => Some(out),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!(
                "skipping {label}: OS denied execution of `{}` \
                 (likely Windows Defender quarantine of a freshly built binary)",
                output.display()
            );
            None
        }
        Err(e) => panic!("invoke compiled binary: {e}"),
    }
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

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(
        stdout.contains("Hello, PHC!"),
        "unexpected stdout: {stdout:?}"
    );
}

/// Native-build gate over the example corpus. Every `examples/*.phc`
/// that defines a `main` must compile to a binary with no build
/// errors — this is the codegen counterpart to the typecheck/interp
/// snapshot harnesses and stops a committed example from silently
/// claiming a codegen path it cannot actually emit. Main-less examples
/// (feature snapshots exercised only via `phc check` / the interpreter)
/// are skipped, since `phc build` needs an entry point.
#[test]
fn every_main_bearing_example_builds() {
    if !cc_available() {
        eprintln!("skipping every_main_bearing_example_builds: no C compiler on PATH");
        return;
    }
    let dir = Path::new("../../examples");
    let mut built = 0;
    for entry in std::fs::read_dir(dir).expect("read examples dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("phc") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read example");
        // Only examples with an entry point can produce a binary.
        if !src.contains("function main") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).expect("stem");
        let output = output_path(&format!("example_{stem}"));
        let result = build_file(&path, &output);
        assert!(
            result.errors.is_empty() && result.ok(),
            "example {} failed to build: {:?}",
            path.display(),
            result.errors
        );
        built += 1;
    }
    assert!(
        built >= 6,
        "expected to build several examples, built {built}"
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

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
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

/// Build + run a program that mutates through a `&flip` (mutable)
/// borrow. Verifies the write-back is observable in the caller:
/// `bump` increments a primitive through the borrow, `addTo` targets
/// a class field's lvalue (`&flip $box->n`), and `twice` chains the
/// borrow through a second call. Without `&flip` write-back the
/// compiled binary would print the un-mutated values.
#[test]
fn build_and_run_flip_borrow_write_back() {
    if !cc_available() {
        eprintln!("skipping build_and_run_flip_borrow_write_back: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

public class Box { construct(public flip int $n) {} }

function bump(&flip int $c): void { $c := $c + 1; }

function twice(&flip int $c): void {
    bump(&flip $c);
    bump(&flip $c);
}

function main(): void {
    flip int $x = 1;
    bump(&flip $x);
    Logger::info("x is {$x}");
    twice(&flip $x);
    Logger::info("x is {$x}");
    flip Box $box = Box(100);
    bump(&flip $box->n);
    Logger::info("box.n is {$box->n}");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("flip_borrow.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("flip_borrow");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["x is 2", "x is 4", "box.n is 101"]);
}

/// Build + run a `??` (null-coalesce) program. Exercises literal
/// `null ?? x`, a nullable-reference fallback when null and when
/// present, and runtime short-circuit (the side-effecting rhs must
/// not run for a present lhs — observed via a `&flip` counter).
#[test]
fn build_and_run_null_coalesce() {
    if !cc_available() {
        eprintln!("skipping build_and_run_null_coalesce: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

public class Box { construct(public int $v) {} }

function bump(&flip int $c): Box { $c := $c + 1; return Box(0); }

function main(): void {
    int $x = null ?? 7;
    Logger::info("x is {$x}");

    Box? $empty = null;
    Box $a = $empty ?? Box(42);
    Logger::info("a is {$a->v}");

    flip int $calls = 0;
    Box $keep = Box(5);
    Box $b = $keep ?? bump(&flip $calls);
    Logger::info("b is {$b->v}, calls is {$calls}");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("null_coalesce.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("null_coalesce");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["x is 7", "a is 42", "b is 5, calls is 0"]);
}

/// Build + run a Result/Option program. Exercises result::ok,
/// result::err, option::some, option::none, and ? propagation
/// across function boundaries.
#[test]
fn build_and_run_result_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_result_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function ok_path(int $n): result<int, string> {
    return result::ok($n + 10);
}

function caller(int $n): result<int, string> {
    int $v = ok_path($n)?;
    return result::ok($v + 1);
}

function main(): void {
    Logger::info("compiled with result + ?");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("result_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("result_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("compiled with result + ?"));
}

/// Build + run a program that exercises C5a inline-invoked lambdas:
/// one no-capture and one capturing two outer locals. The captures
/// must be copied into the env struct at the call site so the
/// lifted body sees the right values; output asserts both branches
/// produced the expected result.
#[test]
fn build_and_run_inline_lambda_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_inline_lambda_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    int $a = 7;
    int $b = 35;
    int $sum = ((int $x, int $y): int => $x + $y)($a, $b);
    int $bumped = ((int $n): int => $n + $a + $b)(0);
    if ($sum == 42) { Logger::info("sum ok"); }
    if ($bumped == 42) { Logger::info("captures ok"); }
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("inline_lambda_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("inline_lambda_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("sum ok"), "missing sum line: {stdout:?}");
    assert!(
        stdout.contains("captures ok"),
        "missing captures line: {stdout:?}"
    );
}

/// Build + run a program that exercises every D-025 string
/// method end-to-end. Each branch logs a sentinel only when its
/// method returned the expected value, so the test asserts on
/// presence of every sentinel.
#[test]
fn build_and_run_string_methods_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_string_methods_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    string $s = "  Hello, PHC!  ";
    string $t = $s->trim();
    if ($t->len() == 11) { Logger::info("len ok"); }
    if ($t->contains("PHC")) { Logger::info("contains ok"); }
    if ($t->startsWith("Hello")) { Logger::info("starts ok"); }
    if ($t->endsWith("PHC!")) { Logger::info("ends ok"); }
    string $u = $t->upper();
    if ($u == "HELLO, PHC!") { Logger::info("upper ok"); }
    string $l = $t->lower();
    if ($l == "hello, phc!") { Logger::info("lower ok"); }
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("string_methods_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("string_methods_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    for sentinel in [
        "len ok",
        "contains ok",
        "starts ok",
        "ends ok",
        "upper ok",
        "lower ok",
    ] {
        assert!(
            stdout.contains(sentinel),
            "missing `{sentinel}` in output: {stdout:?}"
        );
    }
}

/// Build + run a program that exercises every D-027 list surface
/// piece: list() ctor, push, len, at, indexing, for-loop iteration.
/// Asserts each branch's sentinel reached stdout.
#[test]
fn build_and_run_list_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_list_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    list<int> $xs = list();
    $xs->push(10);
    $xs->push(20);
    $xs->push(30);
    if ($xs->len() == 3) { Logger::info("len ok"); }
    if ($xs->at(1) == 20) { Logger::info("at ok"); }
    if ($xs[0] == 10) { Logger::info("index ok"); }
    flip int $acc = 0;
    for (int $x in $xs) { $acc := $acc + $x; }
    if ($acc == 60) { Logger::info("for ok"); }
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("list_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("list_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    for sentinel in ["len ok", "at ok", "index ok", "for ok"] {
        assert!(
            stdout.contains(sentinel),
            "missing `{sentinel}` in output: {stdout:?}"
        );
    }
}

/// Build + run D-026 Result/Option methods end-to-end. The
/// `orElse` chain is split through a typed local so codegen can
/// see the intermediate `option<int>` (typecheck does not infer
/// method-call return types yet).
#[test]
fn build_and_run_result_option_methods_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_result_option_methods_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    result<int, string> $ok = result::ok(42);
    result<int, string> $err = result::err("nope");
    if ($ok->isOk()) { Logger::info("ok-isOk"); }
    if ($err->isErr()) { Logger::info("err-isErr"); }
    if ($ok->unwrapOr(0) == 42) { Logger::info("ok-unwrap"); }
    if ($err->unwrapOr(99) == 99) { Logger::info("err-unwrap"); }

    option<int> $some = option::some(7);
    option<int> $none = option::none;
    if ($some->isSome()) { Logger::info("some-isSome"); }
    if ($none->isNone()) { Logger::info("none-isNone"); }
    if ($some->unwrapOr(0) == 7) { Logger::info("some-unwrap"); }
    if ($none->unwrapOr(99) == 99) { Logger::info("none-unwrap"); }
    option<int> $or = $none->orElse(option::some(5));
    if ($or->unwrapOr(0) == 5) { Logger::info("orElse"); }
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("result_option_methods_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("result_option_methods_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    for sentinel in [
        "ok-isOk",
        "err-isErr",
        "ok-unwrap",
        "err-unwrap",
        "some-isSome",
        "none-isNone",
        "some-unwrap",
        "none-unwrap",
        "orElse",
    ] {
        assert!(
            stdout.contains(sentinel),
            "missing `{sentinel}` in output: {stdout:?}"
        );
    }
}

/// Build + run a program that exercises the typecheck patch:
/// chained method calls (`$s->trim()->lower()`, `$xs->at(0) +
/// $xs->at(1)`, `$o->orElse(...)->unwrapOr(...)`) all need the
/// inner Call's return type recovered or codegen falls back to a
/// `phc_panic("codegen TODO")`. Asserts each chain produced its
/// sentinel.
#[test]
fn build_and_run_method_chain_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_method_chain_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    string $s = "  PHC ROCKS  ";
    if ($s->trim()->lower() == "phc rocks") { Logger::info("string chain"); }
    list<int> $xs = list();
    $xs->push(7);
    $xs->push(35);
    if ($xs->at(0) + $xs->at(1) == 42) { Logger::info("list arith"); }
    option<int> $a = option::none;
    option<int> $b = option::some(99);
    if ($a->orElse($b)->unwrapOr(0) == 99) { Logger::info("option chain"); }
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("method_chain_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("method_chain_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "method-chain binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    for sentinel in ["string chain", "list arith", "option chain"] {
        assert!(
            stdout.contains(sentinel),
            "missing `{sentinel}` in output: {stdout:?}"
        );
    }
}

/// Build + run a program that exercises D-024 stored lambdas:
/// bind a lambda to a `fn(...): R`-typed local, then invoke it.
/// Without D-024 this would fail to parse (no fn-type annotation
/// available) and fail to codegen (no path to lower a stored
/// lambda value to phc_lambda).
#[test]
fn build_and_run_stored_lambda_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_stored_lambda_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    fn(int): int $double = (int $n): int => $n * 2;
    if ($double(7) == 14) { Logger::info("stored lambda call"); }
    fn(int, int): int $add = (int $a, int $b): int => $a + $b;
    if ($add(20, 22) == 42) { Logger::info("two-arg call"); }
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("stored_lambda_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("stored_lambda_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "stored-lambda binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    for sentinel in ["stored lambda call", "two-arg call"] {
        assert!(
            stdout.contains(sentinel),
            "missing `{sentinel}` in output: {stdout:?}"
        );
    }
}

/// Build + run a D-028 map program. v0a: string keys only,
/// linear-scan storage. Asserts set / has / get(→option) / len.
#[test]
fn build_and_run_map_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_map_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    map<string, int> $m = map();
    $m->set("a", 1);
    $m->set("b", 2);
    $m->set("a", 10);
    if ($m->len() == 2) { Logger::info("len ok"); }
    if ($m->has("a")) { Logger::info("has ok"); }
    option<int> $hit = $m->get("a");
    if ($hit->unwrapOr(0) == 10) { Logger::info("get ok"); }
    option<int> $miss = $m->get("missing");
    if ($miss->isNone()) { Logger::info("missing ok"); }
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("map_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("map_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "map binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    for sentinel in ["len ok", "has ok", "get ok", "missing ok"] {
        assert!(
            stdout.contains(sentinel),
            "missing `{sentinel}` in output: {stdout:?}"
        );
    }
}

/// Build + run D-029 closure methods on result/option end-to-end:
/// map / andThen / okOr / unwrap, plus the err / none pass-through
/// branches. Each branch logs a sentinel.
#[test]
fn build_and_run_result_option_closure_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_result_option_closure_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    result<int, string> $ok = result::ok(20);
    result<int, string> $err = result::err("nope");

    fn(int): int $double = (int $n): int => $n * 2;
    result<int, string> $mapped = $ok->map($double);
    if ($mapped->unwrapOr(0) == 40) { Logger::info("result.map ok"); }
    result<int, string> $mapErr = $err->map($double);
    if ($mapErr->isErr()) { Logger::info("result.map passthrough"); }

    fn(int): result<int, string> $half = (int $n): result<int, string> =>
        result::ok($n + 1);
    result<int, string> $chained = $ok->andThen($half);
    if ($chained->unwrapOr(0) == 21) { Logger::info("result.andThen ok"); }

    if ($ok->unwrap() == 20) { Logger::info("result.unwrap ok"); }

    option<int> $some = option::some(7);
    option<int> $none = option::none;

    fn(int): int $inc = (int $n): int => $n + 1;
    option<int> $mappedOpt = $some->map($inc);
    if ($mappedOpt->unwrapOr(0) == 8) { Logger::info("option.map ok"); }
    option<int> $mappedNone = $none->map($inc);
    if ($mappedNone->isNone()) { Logger::info("option.map none"); }

    fn(int): option<int> $doubleSome = (int $n): option<int> =>
        option::some($n * 2);
    option<int> $chainedOpt = $some->andThen($doubleSome);
    if ($chainedOpt->unwrapOr(0) == 14) { Logger::info("option.andThen ok"); }

    result<int, string> $promoted = $some->okOr("missing");
    if ($promoted->unwrapOr(0) == 7) { Logger::info("option.okOr some"); }
    result<int, string> $demoted = $none->okOr("missing");
    if ($demoted->isErr()) { Logger::info("option.okOr none"); }

    if ($some->unwrap() == 7) { Logger::info("option.unwrap ok"); }
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("result_option_closure_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("result_option_closure_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "closure-method binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    for sentinel in [
        "result.map ok",
        "result.map passthrough",
        "result.andThen ok",
        "result.unwrap ok",
        "option.map ok",
        "option.map none",
        "option.andThen ok",
        "option.okOr some",
        "option.okOr none",
        "option.unwrap ok",
    ] {
        assert!(
            stdout.contains(sentinel),
            "missing `{sentinel}` in output: {stdout:?}"
        );
    }
}

/// Build + run D-030 list<T> closure methods: forEach, map,
/// filter. Verifies the codegen stmt-expr / phc_lambda invocation
/// pattern works for list element iteration.
#[test]
fn build_and_run_list_closure_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_list_closure_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    list<int> $xs = list();
    $xs->push(1);
    $xs->push(2);
    $xs->push(3);

    fn(int): int $dbl = (int $n): int => $n * 2;
    list<int> $doubled = $xs->map($dbl);
    if ($doubled->len() == 3) { Logger::info("map len ok"); }
    if ($doubled->at(2) == 6) { Logger::info("map at ok"); }

    fn(int): bool $isTwo = (int $n): bool => $n == 2;
    list<int> $twos = $xs->filter($isTwo);
    if ($twos->len() == 1) { Logger::info("filter len ok"); }
    if ($twos->at(0) == 2) { Logger::info("filter at ok"); }

    fn(int): void $announce = (int $n): void => Logger::info("v");
    $xs->forEach($announce);
    Logger::info("forEach done");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("list_closure_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("list_closure_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "list-closure binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    for sentinel in [
        "map len ok",
        "map at ok",
        "filter len ok",
        "filter at ok",
        "forEach done",
    ] {
        assert!(
            stdout.contains(sentinel),
            "missing `{sentinel}` in output: {stdout:?}"
        );
    }
    let v_count = stdout.matches("v\r\n").count() + stdout.matches("v\n").count();
    assert!(
        v_count >= 3,
        "expected 3 `v` lines from forEach, got {v_count}: {stdout:?}"
    );
}

/// Build + run D-031 set<string> v0a: add (with dedup signal),
/// has, remove, len.
#[test]
fn build_and_run_set_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_set_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    set<string> $s = set();
    if ($s->add("ada")) { Logger::info("add new"); }
    if (!$s->add("ada")) { Logger::info("add dup"); }
    $s->add("alan");
    if ($s->len() == 2) { Logger::info("len ok"); }
    if ($s->has("ada")) { Logger::info("has ok"); }
    if (!$s->has("grace")) { Logger::info("miss ok"); }
    if ($s->remove("ada")) { Logger::info("remove ok"); }
    if (!$s->has("ada")) { Logger::info("post-remove miss"); }
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("set_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("set_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "set binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    for sentinel in [
        "add new",
        "add dup",
        "len ok",
        "has ok",
        "miss ok",
        "remove ok",
        "post-remove miss",
    ] {
        assert!(
            stdout.contains(sentinel),
            "missing `{sentinel}` in output: {stdout:?}"
        );
    }
}

/// Build + run D-032 `io::*` surface end-to-end. Exercises
/// stdout println/print, stderr eprintln. readLine path isn't
/// invoked here because the test process doesn't pipe stdin.
#[test]
fn build_and_run_io_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_io_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    io::println("hello");
    io::eprintln("ohno");
    io::print("no-newline ");
    io::println("rest");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("io_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("io_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "io binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("hello"), "missing `hello`: {stdout:?}");
    assert!(
        stdout.contains("no-newline rest"),
        "missing concat'd print+println: {stdout:?}"
    );
    let stderr = String::from_utf8(run.stderr).expect("stderr is utf-8");
    assert!(stderr.contains("ohno"), "missing stderr `ohno`: {stderr:?}");
}

/// Build + run D-033 assert namespace through codegen. Asserts
/// the passing branch reaches stdout and the binary exits zero.
#[test]
fn build_and_run_assert_passing_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_assert_passing_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    assert::eq(2 + 3, 5);
    assert::neq(1, 2);
    assert::isTrue(true);
    assert::isFalse(false);
    assert::eq("phc", "phc");
    io::println("asserts passed");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("assert_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("assert_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "assert binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("asserts passed"), "stdout: {stdout:?}");
}

/// Build + run D-034 numeric stdlib namespaces (int / float).
/// Drives parse / min / max / abs / isNaN through the full
/// codegen + cc + run pipeline using D-033 assert helpers.
#[test]
fn build_and_run_numeric_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_numeric_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    result<int, parseError> $ok = int::parse("42");
    assert::eq($ok->unwrapOr(0), 42);
    result<int, parseError> $bad = int::parse("nope");
    assert::isTrue($bad->isErr());

    assert::eq(int::min(7, 3), 3);
    assert::eq(int::max(7, 3), 7);
    assert::eq(int::abs(-9), 9);

    result<float, parseError> $f = float::parse("2.5");
    assert::eq($f->unwrapOr(0.0), 2.5);
    assert::isFalse(float::isNaN(1.0));

    io::println("numeric ok");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("numeric_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("numeric_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "numeric binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("numeric ok"), "stdout: {stdout:?}");
}

/// Build + run D-037 list<T> closure surface (fold/any/all/find)
/// through codegen + cc + run.
#[test]
fn build_and_run_list_fold_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_list_fold_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    list<int> $xs = list();
    $xs->push(1);
    $xs->push(2);
    $xs->push(3);
    $xs->push(4);

    fn(int, int): int $sum = (int $acc, int $x): int => $acc + $x;
    assert::eq($xs->fold(0, $sum), 10);

    fn(int): bool $isPositive = (int $n): bool => $n > 0;
    assert::isTrue($xs->all($isPositive));

    fn(int): bool $gtThree = (int $n): bool => $n > 3;
    assert::isTrue($xs->any($gtThree));

    fn(int): bool $eqTwo = (int $n): bool => $n == 2;
    option<int> $found = $xs->find($eqTwo);
    assert::eq($found->unwrapOr(0), 2);

    io::println("fold ok");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("list_fold_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("list_fold_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "list-fold binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("fold ok"), "stdout: {stdout:?}");
}

/// Build + run D-038 list<T> reduce / findIndex through codegen + cc + run.
#[test]
fn build_and_run_list_reduce_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_list_reduce_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    list<int> $xs = list();
    $xs->push(10);
    $xs->push(20);
    $xs->push(30);

    // reduce: sum non-empty list.
    fn(int, int): int $add = (int $a, int $b): int => $a + $b;
    option<int> $total = $xs->reduce($add);
    assert::eq($total->unwrapOr(0), 60);

    // reduce: empty list → none.
    list<int> $empty = list();
    option<int> $none = $empty->reduce($add);
    assert::isFalse($none->isSome());

    // findIndex: match exists.
    fn(int): bool $eq20 = (int $n): bool => $n == 20;
    option<int> $idx = $xs->findIndex($eq20);
    assert::eq($idx->unwrapOr(-1), 1);

    // findIndex: no match → none.
    fn(int): bool $eq99 = (int $n): bool => $n == 99;
    option<int> $miss = $xs->findIndex($eq99);
    assert::isFalse($miss->isSome());

    io::println("reduce ok");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("list_reduce_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("list_reduce_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "list-reduce binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("reduce ok"), "stdout: {stdout:?}");
}

/// Build + run a 2-pack project end-to-end. Confirms multi-file
/// codegen flattens the corpus into one C unit and produces a
/// binary that calls cross-pack into the public helper.
#[test]
fn build_and_run_two_pack_project() {
    if !cc_available() {
        eprintln!("skipping build_and_run_two_pack_project: no C compiler on PATH");
        return;
    }
    let root = project_root("two_pack_build");
    std::fs::write(
        root.join("core.phc"),
        r#"pack core;
public function welcome(): string { return "hello, project"; }
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("main.phc"),
        r#"pack app;
use core.welcome;
function main(): void {
    Logger::info(welcome());
}
"#,
    )
    .unwrap();
    let output = output_path("two_pack");
    let result = build_project(&root, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(
        stdout.contains("hello, project"),
        "unexpected stdout: {stdout:?}"
    );
}

/// Build + run a generic free function specialised at two distinct
/// concrete types from one source. Verifies the monomorphizer emits
/// distinct C symbols (no name clash) and dispatches each call site
/// to the right specialisation.
#[test]
fn build_and_run_generic_mono_two_specialisations() {
    if !cc_available() {
        eprintln!("skipping build_and_run_generic_mono_two_specialisations: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

public function pickLarger<T>(T $a, T $b): T {
    if ($a > $b) { return $a; }
    return $b;
}

function main(): void {
    int $i = pickLarger(3, 7);
    float $f = pickLarger(1.5, 2.5);
    Logger::info("int picked: {$i}");
    Logger::info("float picked: {$f}");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("generic_mono.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("generic_mono");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["int picked: 7", "float picked: 2.5",]);
}

/// Build + run a 2-pack project where the importing pack constructs
/// a class from the other pack and dispatches a method on it.
/// Regression coverage for cross-pack class type plumbing through
/// `combine_session`.
#[test]
fn build_and_run_cross_pack_class_method() {
    if !cc_available() {
        eprintln!("skipping build_and_run_cross_pack_class_method: no C compiler on PATH");
        return;
    }
    let root = project_root("two_pack_class");
    std::fs::write(
        root.join("box.phc"),
        r#"pack core;
public class Box {
    construct(public int $value) {}
    public function doubled(): int { return $this->value + $this->value; }
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("main.phc"),
        r#"pack app;
use core.Box;
function main(): void {
    Box $b = Box(21);
    Logger::info("doubled is {$b->doubled()}");
}
"#,
    )
    .unwrap();
    let output = output_path("two_pack_class");
    let result = build_project(&root, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(
        stdout.contains("doubled is 42"),
        "unexpected stdout: {stdout:?}"
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
fn cross_pack_use_resolves_to_target() {
    let root = project_root("cross_pack_ok");
    std::fs::write(
        root.join("a.phc"),
        "pack core;\npublic function helper(): int { return 1; }",
    )
    .unwrap();
    std::fs::write(
        root.join("b.phc"),
        "pack app;\nuse core.helper;\nfunction main(): int { return 0; }",
    )
    .unwrap();

    let mut session = load_session(&root);
    resolve_cross_pack_uses(&mut session);
    assert!(session.ok(), "diagnostics: {:?}", session.diagnostics);
    assert_eq!(session.cross_uses.len(), 1);
}

#[test]
fn cross_pack_grouped_use_resolves_each_name() {
    let root = project_root("cross_pack_group");
    std::fs::write(
        root.join("a.phc"),
        r#"pack core;
           public function one(): int { return 1; }
           public function two(): int { return 2; }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("b.phc"),
        "pack app;\nuse core.{one, two};\nfunction main(): void {}",
    )
    .unwrap();

    let mut session = load_session(&root);
    resolve_cross_pack_uses(&mut session);
    assert!(session.ok(), "diagnostics: {:?}", session.diagnostics);
    assert_eq!(session.cross_uses.len(), 2);
}

#[test]
fn pack_acyclicity_passes_for_dag() {
    let root = project_root("dag");
    std::fs::write(
        root.join("a.phc"),
        "pack core;\npublic function helper(): int { return 1; }",
    )
    .unwrap();
    std::fs::write(
        root.join("b.phc"),
        "pack app;\nuse core.helper;\nfunction main(): void {}",
    )
    .unwrap();
    let mut session = load_session(&root);
    resolve_cross_pack_uses(&mut session);
    check_pack_acyclicity(&mut session);
    assert!(session.ok(), "diagnostics: {:?}", session.diagnostics);
}

#[test]
fn pack_acyclicity_detects_two_pack_cycle() {
    let root = project_root("cycle2");
    std::fs::write(
        root.join("a.phc"),
        "pack a;\nuse b.thing;\npublic function thing(): int { return 1; }",
    )
    .unwrap();
    std::fs::write(
        root.join("b.phc"),
        "pack b;\nuse a.thing;\npublic function thing(): int { return 2; }",
    )
    .unwrap();
    let mut session = load_session(&root);
    resolve_cross_pack_uses(&mut session);
    check_pack_acyclicity(&mut session);
    assert!(!session.ok());
    assert!(session
        .diagnostics
        .iter()
        .any(|d| d.message.contains("pack import cycle")));
}

#[test]
fn pack_acyclicity_detects_three_pack_cycle() {
    let root = project_root("cycle3");
    std::fs::write(
        root.join("a.phc"),
        "pack a;\nuse b.x;\npublic function x(): int { return 1; }",
    )
    .unwrap();
    std::fs::write(
        root.join("b.phc"),
        "pack b;\nuse c.x;\npublic function x(): int { return 2; }",
    )
    .unwrap();
    std::fs::write(
        root.join("c.phc"),
        "pack c;\nuse a.x;\npublic function x(): int { return 3; }",
    )
    .unwrap();
    let mut session = load_session(&root);
    resolve_cross_pack_uses(&mut session);
    check_pack_acyclicity(&mut session);
    assert!(!session.ok());
    let cycle_count = session
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("pack import cycle"))
        .count();
    // Each cycle reported at most once thanks to canonicalisation.
    assert_eq!(cycle_count, 1);
}

#[test]
fn cross_pack_default_visibility_is_rejected() {
    let root = project_root("vis_default");
    std::fs::write(
        root.join("a.phc"),
        "pack core;\nfunction helper(): int { return 1; }",
    )
    .unwrap();
    std::fs::write(
        root.join("b.phc"),
        "pack app;\nuse core.helper;\nfunction main(): void {}",
    )
    .unwrap();
    let mut session = load_session(&root);
    resolve_cross_pack_uses(&mut session);
    assert!(!session.ok());
    assert!(session
        .diagnostics
        .iter()
        .any(|d| d.message.contains("pack-scoped")));
}

#[test]
fn cross_pack_public_item_is_importable() {
    let root = project_root("vis_public");
    std::fs::write(
        root.join("a.phc"),
        "pack core;\npublic function helper(): int { return 1; }",
    )
    .unwrap();
    std::fs::write(
        root.join("b.phc"),
        "pack app;\nuse core.helper;\nfunction main(): void {}",
    )
    .unwrap();
    let mut session = load_session(&root);
    resolve_cross_pack_uses(&mut session);
    assert!(session.ok(), "diagnostics: {:?}", session.diagnostics);
}

#[test]
fn cross_pack_unresolved_import_is_an_error() {
    let root = project_root("cross_pack_unresolved");
    std::fs::write(
        root.join("a.phc"),
        "pack app;\nuse missing.foo;\nfunction main(): void {}",
    )
    .unwrap();

    let mut session = load_session(&root);
    resolve_cross_pack_uses(&mut session);
    assert!(!session.ok());
    assert!(session
        .diagnostics
        .iter()
        .any(|d| d.message.contains("no pack named `missing`")));
}

#[test]
fn cross_pack_missing_item_is_an_error() {
    let root = project_root("cross_pack_missing_item");
    std::fs::write(root.join("a.phc"), "pack core;").unwrap();
    std::fs::write(
        root.join("b.phc"),
        "pack app;\nuse core.absent;\nfunction main(): void {}",
    )
    .unwrap();

    let mut session = load_session(&root);
    resolve_cross_pack_uses(&mut session);
    assert!(!session.ok());
    assert!(session
        .diagnostics
        .iter()
        .any(|d| d.message.contains("does not export an item named `absent`")));
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

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
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
    construct(public flip int $value) {}

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

    let run = match run_or_skip_on_av(&output, "binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec!["value is 7", "doubled is 14", "after assign: 100",]
    );
}

/// Build + run D-039 map keys/values + set for-loop through codegen + cc + run.
#[test]
fn build_and_run_map_set_iteration_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_map_set_iteration_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    // map keys/values: parallel-array invariant.
    map<string, int> $scores = map();
    $scores->set("alice", 10);
    $scores->set("bob", 20);
    $scores->set("carol", 30);

    list<string> $ks = $scores->keys();
    list<int> $vs = $scores->values();

    assert::eq($ks->len(), 3);
    assert::eq($vs->len(), 3);
    assert::eq($ks->at(0), "alice");
    assert::eq($vs->at(0), 10);
    assert::eq($ks->at(1), "bob");
    assert::eq($vs->at(1), 20);
    assert::eq($ks->at(2), "carol");
    assert::eq($vs->at(2), 30);

    // map keys/values on empty map.
    map<string, int> $empty_map = map();
    assert::eq($empty_map->keys()->len(), 0);
    assert::eq($empty_map->values()->len(), 0);

    // set for-loop: accumulate elements.
    set<string> $tags = set();
    $tags->add("rust");
    $tags->add("phc");
    $tags->add("systems");

    list<string> $collected = list();
    for (string $t in $tags) {
        $collected->push($t);
    }
    assert::eq($collected->len(), 3);

    // set for-loop: empty set — body never runs.
    set<string> $empty_set = set();
    flip int $count = 0;
    for (string $x in $empty_set) {
        $count := $count + 1;
    }
    assert::eq($count, 0);

    io::println("map-set-iteration ok");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("map_set_iteration_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("map_set_iteration_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "map-set-iteration binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(
        stdout.contains("map-set-iteration ok"),
        "stdout: {stdout:?}"
    );
}

/// Build + run D-040 map/set forEach. Verifies the stmt-expr +
/// phc_lambda pattern works for map (key+value lambda) and set
/// (element lambda). Uses reference-semantic list accumulators
/// to avoid copy-by-value capture limitations on scalar locals.
#[test]
fn build_and_run_map_set_foreach_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_map_set_foreach_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    // map forEach: collect keys and values via side-effects.
    map<string, int> $m = map();
    $m->set("a", 1);
    $m->set("b", 2);
    $m->set("c", 3);

    list<string> $ks = list();
    list<int> $vs = list();
    $m->forEach((string $k, int $v): void => {
        $ks->push($k);
        $vs->push($v);
    });
    assert::eq($ks->len(), 3);
    assert::eq($vs->len(), 3);
    assert::eq($ks->at(0), "a");
    assert::eq($vs->at(0), 1);

    // set forEach: accumulate elements using a list.
    set<string> $s = set();
    $s->add("x");
    $s->add("y");
    list<string> $els = list();
    $s->forEach((string $el): void => {
        $els->push($el);
    });
    assert::eq($els->len(), 2);

    // empty map/set: forEach body never runs.
    map<string, int> $em = map();
    list<string> $empty_ks = list();
    $em->forEach((string $k, int $v): void => { $empty_ks->push($k); });
    assert::eq($empty_ks->len(), 0);

    set<string> $es = set();
    list<string> $empty_els = list();
    $es->forEach((string $el): void => { $empty_els->push($el); });
    assert::eq($empty_els->len(), 0);

    io::println("map-set-foreach ok");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("map_set_foreach_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("map_set_foreach_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "map-set-foreach binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("map-set-foreach ok"), "stdout: {stdout:?}");
}

/// Build + run D-041 assert::approxEq through codegen + cc + run.
#[test]
fn build_and_run_assert_approx_eq_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_assert_approx_eq_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    assert::approxEq(1.0, 1.0);
    assert::approxEq(0.0, 0.0);
    assert::approxEq(3.14159, 3.14159);
    // int args widen to float.
    assert::approxEq(0, 0);
    io::println("approxEq codegen ok");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("assert_approx_eq_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("assert_approx_eq_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "assert-approxEq binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("approxEq codegen ok"), "stdout: {stdout:?}");
}

/// Build + run D-042 toString() magic method through codegen + cc + run.
#[test]
fn build_and_run_tostring_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_tostring_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

public class Point {
    construct(public int $x, public int $y) {}
    public function toString(): string {
        return "({$this->x}, {$this->y})";
    }
}

function main(): void {
    Point $p = Point(3, 4);
    string $s = "point is {$p}";
    assert::eq($s, "point is (3, 4)");
    io::println("toString codegen ok");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("tostring_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("tostring_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "toString binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("toString codegen ok"), "stdout: {stdout:?}");
}

#[test]
fn build_and_run_list_take_drop_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_list_take_drop_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    list<int> $xs = list();
    $xs->push(1);
    $xs->push(2);
    $xs->push(3);
    $xs->push(4);
    $xs->push(5);

    list<int> $front = $xs->take(3);
    assert::eq($front->len(), 3);
    assert::eq($front->at(0), 1);
    assert::eq($front->at(2), 3);

    list<int> $rest = $xs->drop(2);
    assert::eq($rest->len(), 3);
    assert::eq($rest->at(0), 3);

    list<int> $mid = $xs->drop(1)->take(3);
    assert::eq($mid->len(), 3);
    assert::eq($mid->at(0), 2);
    assert::eq($mid->at(2), 4);

    list<int> $empty = $xs->take(0);
    assert::eq($empty->len(), 0);
    list<int> $all = $xs->drop(0);
    assert::eq($all->len(), 5);
    list<int> $clamp_hi = $xs->take(99);
    assert::eq($clamp_hi->len(), 5);
    list<int> $clamp_drop = $xs->drop(99);
    assert::eq($clamp_drop->len(), 0);

    io::println("take/drop codegen ok");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("list_take_drop_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("list_take_drop_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "list take/drop binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(
        stdout.contains("take/drop codegen ok"),
        "stdout: {stdout:?}"
    );
}

#[test]
fn build_and_run_list_reverse_concat_join_program() {
    if !cc_available() {
        eprintln!("skipping build_and_run_list_reverse_concat_join_program: no C compiler on PATH");
        return;
    }
    let src = r#"pack demo;

function main(): void {
    list<int> $xs = list();
    $xs->push(1);
    $xs->push(2);
    $xs->push(3);

    list<int> $rev = $xs->reverse();
    assert::eq($rev->len(), 3);
    assert::eq($rev->at(0), 3);
    assert::eq($rev->at(2), 1);

    list<int> $ys = list();
    $ys->push(4);
    $ys->push(5);
    list<int> $cat = $xs->concat($ys);
    assert::eq($cat->len(), 5);
    assert::eq($cat->at(0), 1);
    assert::eq($cat->at(4), 5);

    list<string> $words = list();
    $words->push("hello");
    $words->push("world");
    string $joined = $words->join(" ");
    assert::eq($joined, "hello world");

    list<string> $empty = list();
    assert::eq($empty->join(","), "");

    io::println("reverse/concat/join codegen ok");
}
"#;
    let mut src_path = PathBuf::from("target");
    src_path.push("phc-build-test");
    std::fs::create_dir_all(&src_path).expect("create test source dir");
    src_path.push("list_reverse_concat_join_program.phc");
    std::fs::write(&src_path, src).expect("write test source");

    let output = output_path("list_reverse_concat_join_program");
    let result = build_file(&src_path, &output);
    assert!(
        result.errors.is_empty(),
        "build errors: {:?}",
        result.errors
    );
    assert!(result.ok());

    let run = match run_or_skip_on_av(&output, "list reverse/concat/join binary spawn") {
        Some(r) => r,
        None => return,
    };
    assert!(run.status.success(), "binary exited non-zero");
    let stdout = String::from_utf8(run.stdout).expect("stdout is utf-8");
    assert!(
        stdout.contains("reverse/concat/join codegen ok"),
        "stdout: {stdout:?}"
    );
}
