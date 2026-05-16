// SPDX-License-Identifier: MIT
//! PHC test discovery and runner.
//!
//! Walks every [`Item::Test`] in a source file and executes its body
//! through the tree-walking interpreter ([`phc_interp::run_test_block`]).
//! A test passes when its body completes without a runtime error;
//! a panic or `?`-propagation that escapes the body is a failure.
//!
//! v0 surface (Phase 9 MVP):
//! - Discovery: every `test "name" { ... }` in the file becomes a
//!   [`TestCase`]. Nested tests inside class / trait bodies are
//!   **not** in scope for v0 — the grammar only allows tests at the
//!   top level today.
//! - Execution: sequential, in declaration order. Per-test
//!   captured stdout flows back through [`TestResult::stdout`].
//! - Reporting: counts pass/fail + a Vec of failure messages. The
//!   CLI prints them; library callers can format their own.
//!
//! Out of scope:
//! - Cross-file project test discovery (single-file today).
//! - Assertion helpers (`assertEq`, etc.) — tests just run code and
//!   any panic / `?` propagation is treated as failure.
//! - Filtering / parallel execution.

#[cfg(test)]
mod tests;

use phc_ast::{Item, SourceFile};
use phc_borrowcheck::borrowcheck;
use phc_errors::Diagnostic;
use phc_interp::{run_test_block, RuntimeError};
use phc_parser::parse;
use phc_semantic::resolve;
use phc_span::FileId;
use phc_typecheck::typecheck;

/// A single discovered `test "name" { ... }` block ready to run.
/// Carries the rendered name (interpolation expressions are
/// stringified by the parser into placeholder text and reproduced
/// verbatim here, since v0 has no test-time interpolation of
/// runtime values).
#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub body_idx: usize,
}

/// Outcome of running one test. `errors` is empty on pass.
#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub stdout: Vec<String>,
    pub errors: Vec<RuntimeError>,
}

/// Aggregate output across every test in a source file.
#[derive(Debug, Default, Clone)]
pub struct TestRunReport {
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<TestResult>,
    /// Pipeline-level diagnostics surfaced before any test ran
    /// (parse / resolve / typecheck / borrowcheck errors). When
    /// non-empty, no tests are executed.
    pub setup_diagnostics: Vec<Diagnostic>,
}

impl TestRunReport {
    pub fn ok(&self) -> bool {
        self.failed == 0 && self.setup_diagnostics.is_empty()
    }
}

/// Discover every `Item::Test` and render its name. Skips tests
/// silently if the parser produced a name with no parts (the
/// parser should never emit that, but the check keeps the runner
/// robust to future parser changes).
fn discover_tests(file: &SourceFile) -> Vec<TestCase> {
    let mut out = Vec::new();
    for (idx, item) in file.items.iter().enumerate() {
        if let Item::Test(t) = item {
            let name = render_name(t);
            if !name.is_empty() {
                out.push(TestCase {
                    name,
                    body_idx: idx,
                });
            }
        }
    }
    out
}

fn render_name(t: &phc_ast::TestDecl) -> String {
    let mut buf = String::new();
    for part in &t.name {
        match part {
            phc_ast::StrPart::Text(s) => buf.push_str(s),
            phc_ast::StrPart::Expr(_) => buf.push_str("{expr}"),
        }
    }
    buf
}

/// Drive the full pipeline against `source`, then run every
/// discovered test. Pipeline errors abort before any test runs;
/// once analysis is clean, each test runs independently — one
/// test's runtime error does not stop later tests.
pub fn run_source(source: &str) -> TestRunReport {
    let mut report = TestRunReport::default();
    let parsed = parse(source, FileId(0));
    report.setup_diagnostics.extend(parsed.diagnostics.clone());
    let Some(file) = parsed.file else {
        return report;
    };
    let resolved = resolve(&file);
    report
        .setup_diagnostics
        .extend(resolved.diagnostics.clone());
    let typed = typecheck(&file, &resolved);
    report.setup_diagnostics.extend(typed.diagnostics.clone());
    let borrowed = borrowcheck(&file, &resolved, &typed);
    report
        .setup_diagnostics
        .extend(borrowed.diagnostics.clone());
    if !report.setup_diagnostics.is_empty() {
        return report;
    }
    for case in discover_tests(&file) {
        let body = match &file.items[case.body_idx] {
            Item::Test(t) => &t.body,
            _ => continue,
        };
        let out = run_test_block(&file, &resolved, &typed, body);
        let passed = out.errors.is_empty();
        if passed {
            report.passed += 1;
        } else {
            report.failed += 1;
        }
        report.results.push(TestResult {
            name: case.name,
            passed,
            stdout: out.stdout,
            errors: out.errors,
        });
    }
    report
}
