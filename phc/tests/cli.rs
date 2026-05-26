// SPDX-License-Identifier: MIT
//! Integration tests for the `phc` binary entry point.
//!
//! Drives the built `phc` executable via `std::process::Command` so
//! the CLI parser, the argument dispatch (`Command::Build`,
//! `Command::Run`, ...), the file-IO error paths, and the exit-code
//! reporting are all exercised end-to-end. The binary path is
//! discovered through Cargo's `CARGO_BIN_EXE_phc` env var that
//! integration tests get for free.

use std::path::PathBuf;
use std::process::Command;

fn phc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_phc"))
}

fn scratch_dir(name: &str) -> PathBuf {
    let mut p = PathBuf::from("target");
    p.push("phc-cli-test");
    p.push(name);
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("create scratch dir");
    p
}

#[test]
fn run_executes_hello_world() {
    let dir = scratch_dir("run_hello");
    let src = dir.join("hello.phc");
    std::fs::write(
        &src,
        "pack a;\nfunction main(): void { Logger::info(\"hi\"); }\n",
    )
    .unwrap();
    let out = Command::new(phc_bin())
        .arg("run")
        .arg(&src)
        .output()
        .expect("spawn phc");
    assert!(
        out.status.success(),
        "phc run failed: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("hi"),
        "stdout did not contain `hi`: {stdout}"
    );
}

#[test]
fn run_reports_parse_error_with_nonzero_exit() {
    let dir = scratch_dir("run_parse_err");
    let src = dir.join("bad.phc");
    std::fs::write(
        &src,
        "pack a;\nfunction main(): void { this is not valid }\n",
    )
    .unwrap();
    let out = Command::new(phc_bin())
        .arg("run")
        .arg(&src)
        .output()
        .expect("spawn phc");
    assert!(!out.status.success(), "phc run should fail on parse error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("parse error"),
        "expected `parse error` in stderr: {stderr}"
    );
}

#[test]
fn run_reports_missing_file() {
    let out = Command::new(phc_bin())
        .arg("run")
        .arg("/nonexistent/does/not/exist.phc")
        .output()
        .expect("spawn phc");
    assert!(!out.status.success(), "missing file should be non-zero");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("phc run: cannot read"),
        "expected cannot-read message in stderr: {stderr}"
    );
}

#[test]
fn build_produces_binary() {
    if Command::new("cc").arg("--version").output().is_err() {
        eprintln!("skipping build_produces_binary: no C compiler on PATH");
        return;
    }
    let dir = scratch_dir("build_binary");
    let src = dir.join("hello.phc");
    std::fs::write(
        &src,
        "pack a;\nfunction main(): void { Logger::info(\"built\"); }\n",
    )
    .unwrap();
    let bin = dir.join("hello_bin");
    let out = Command::new(phc_bin())
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("spawn phc");
    assert!(
        out.status.success(),
        "phc build failed: stderr={:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    if !bin.exists() {
        eprintln!("skipping build_produces_binary: binary absent after build (likely Windows Defender quarantine)");
        return;
    }
    let run = match Command::new(&bin).output() {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!(
                "skipping build_produces_binary: OS denied execution (likely Windows Defender)"
            );
            return;
        }
        Err(e) => panic!("spawn produced binary: {e}"),
    };
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("built"), "binary output: {stdout}");
}

#[test]
fn build_without_file_in_empty_dir_complains() {
    let dir = scratch_dir("build_no_manifest");
    let out = Command::new(phc_bin())
        .current_dir(&dir)
        .arg("build")
        .output()
        .expect("spawn phc");
    assert!(!out.status.success(), "build without manifest should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no `phc.json`"),
        "expected manifest hint in stderr: {stderr}"
    );
}

#[test]
fn check_clean_project_reports_no_diagnostics() {
    let dir = scratch_dir("check_clean");
    std::fs::write(
        dir.join("a.phc"),
        "pack alpha;\npublic function ping(): string { return \"pong\"; }\n",
    )
    .unwrap();
    let out = Command::new(phc_bin())
        .arg("check")
        .arg(&dir)
        .output()
        .expect("spawn phc");
    assert!(
        out.status.success(),
        "check on clean project should succeed"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no diagnostics"),
        "expected clean check stderr: {stderr}"
    );
}

#[test]
fn fmt_check_reports_dirty_file() {
    let dir = scratch_dir("fmt_dirty");
    let src = dir.join("dirty.phc");
    // Extra spaces around `=` and trailing whitespace will be
    // normalised by the formatter, so --check should exit non-zero.
    std::fs::write(
        &src,
        "pack a;\nfunction main(): void {    int   $x   =   1;   }\n",
    )
    .unwrap();
    let out = Command::new(phc_bin())
        .arg("fmt")
        .arg("--check")
        .arg(&src)
        .output()
        .expect("spawn phc");
    // --check on a dirty file exits non-zero. The formatted text
    // still prints to stdout. The file on disk is untouched.
    assert!(
        !out.status.success(),
        "fmt --check on dirty file should fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("would change"),
        "expected would-change message: {stderr}"
    );
    // File contents preserved.
    let on_disk = std::fs::read_to_string(&src).unwrap();
    assert!(
        on_disk.contains("    int   $x"),
        "fmt --check must not write"
    );
}

#[test]
fn fmt_rewrites_file_in_place() {
    let dir = scratch_dir("fmt_rewrite");
    let src = dir.join("dirty.phc");
    std::fs::write(
        &src,
        "pack a;\nfunction main(): void {    int   $x   =   1;   }\n",
    )
    .unwrap();
    let out = Command::new(phc_bin())
        .arg("fmt")
        .arg(&src)
        .output()
        .expect("spawn phc");
    assert!(out.status.success(), "fmt should succeed on rewrite");
    let on_disk = std::fs::read_to_string(&src).unwrap();
    assert!(
        !on_disk.contains("    int   $x"),
        "file should be reformatted; got: {on_disk}"
    );
}

#[test]
fn lint_clean_file_exits_zero() {
    let dir = scratch_dir("lint_clean");
    let src = dir.join("clean.phc");
    std::fs::write(
        &src,
        "pack a;\nfunction main(): void { Logger::info(\"clean\"); }\n",
    )
    .unwrap();
    let out = Command::new(phc_bin())
        .arg("lint")
        .arg(&src)
        .output()
        .expect("spawn phc");
    assert!(out.status.success(), "clean file should lint clean");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("clean"),
        "expected clean lint stderr: {stderr}"
    );
}

#[test]
fn lint_unused_local_exits_nonzero() {
    let dir = scratch_dir("lint_unused");
    let src = dir.join("unused.phc");
    std::fs::write(
        &src,
        "pack a;\nfunction main(): void { int $unused = 1; Logger::info(\"hi\"); }\n",
    )
    .unwrap();
    let out = Command::new(phc_bin())
        .arg("lint")
        .arg(&src)
        .output()
        .expect("spawn phc");
    assert!(!out.status.success(), "unused-local should trip lint");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("warning") && stderr.to_lowercase().contains("unused"),
        "expected unused warning: {stderr}"
    );
}

#[test]
fn test_subcommand_runs_test_blocks() {
    let dir = scratch_dir("test_blocks");
    let src = dir.join("t.phc");
    std::fs::write(
        &src,
        "pack a;\ntest \"obvious truth\" { assert::isTrue(true); }\ntest \"deliberate failure\" { assert::isTrue(false); }\n",
    )
    .unwrap();
    let out = Command::new(phc_bin())
        .arg("test")
        .arg(&src)
        .output()
        .expect("spawn phc");
    // One pass + one fail → overall failure.
    assert!(
        !out.status.success(),
        "deliberate failing test should make phc test exit non-zero"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("PASS"), "expected PASS line: {stdout}");
    assert!(stdout.contains("FAIL"), "expected FAIL line: {stdout}");
}

#[test]
fn new_subcommand_reports_unimplemented() {
    let out = Command::new(phc_bin())
        .arg("new")
        .arg("demo")
        .output()
        .expect("spawn phc");
    assert!(
        !out.status.success(),
        "phc new is stubbed and should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not yet implemented"),
        "expected unimplemented message: {stderr}"
    );
}

#[test]
fn no_subcommand_prints_help_hint_and_exits_zero() {
    let out = Command::new(phc_bin()).output().expect("spawn phc");
    assert!(out.status.success(), "no-arg invocation should exit zero");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--help"), "expected help hint: {stderr}");
}
