// SPDX-License-Identifier: MIT
//! End-to-end snapshot of the interpreter against every PHC example.
//!
//! Drives lex → parse → resolve → typecheck → interp for each
//! `examples/*.phc` and snapshots the captured stdout, the program
//! result, and any runtime errors. Mirrors the lexer / parser /
//! semantic / typecheck snapshot harnesses so a single
//! `cargo insta review` walks every layer.
//!
//! Examples without a `main()` produce a "no `main`" runtime error
//! today; that is recorded faithfully in the snapshot so future
//! work that adds `main()` to more examples will surface as a
//! review-worthy diff.

use phc_interp::{run, RunOutput};
use phc_parser::parse;
use phc_semantic::resolve;
use phc_span::FileId;
use phc_typecheck::typecheck;

fn render(out: &RunOutput) -> String {
    let mut s = String::new();
    if !out.stdout.is_empty() {
        s.push_str("=== stdout ===\n");
        for line in &out.stdout {
            s.push_str(line);
            s.push('\n');
        }
    }
    if let Some(v) = &out.result {
        s.push_str("\n=== result ===\n");
        s.push_str(&v.display());
        s.push('\n');
    }
    if !out.errors.is_empty() {
        s.push_str("\n=== errors ===\n");
        for e in &out.errors {
            s.push_str(&e.message);
            s.push('\n');
        }
    }
    s
}

#[test]
fn run_every_example() {
    insta::glob!("../../..", "examples/*.phc", |path| {
        let src = std::fs::read_to_string(path).expect("read example");
        let parsed = parse(&src, FileId(0));
        assert!(
            parsed.diagnostics.is_empty(),
            "parser diagnostics in {}: {:?}",
            path.display(),
            parsed.diagnostics
        );
        let file = parsed.file.expect("expected SourceFile");
        let resolved = resolve(&file);
        let typed = typecheck(&file, &resolved);
        let out = run(&file, &resolved, &typed);
        let rendered = render(&out);
        insta::assert_snapshot!(rendered);
    });
}
