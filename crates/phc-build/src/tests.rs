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
