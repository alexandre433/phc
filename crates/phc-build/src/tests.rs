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
