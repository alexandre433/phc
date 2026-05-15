<!--
  PHC pull request template.
  Replace the placeholder text in each section. Delete sections that
  do not apply (e.g. "Spec refs" for a chore-only PR).
-->

## Summary

<!-- One or two sentences explaining what this PR does and why. -->

## Phase

<!-- Tick the phase this PR advances. Tick zero or one. -->

- [ ] phase-1 (spec)
- [ ] phase-2 (lexer / parser)
- [ ] phase-3 (typecheck / semantic)
- [ ] phase-4 (IR / lowering)
- [ ] phase-5 (codegen / runtime)
- [ ] phase-6 (stdlib)
- [ ] phase-7 (pkg / build)
- [ ] phase-8 (tooling: fmt / lint / lsp)
- [ ] phase-9 (testing framework)
- [ ] phase-10 (docs / release)
- [ ] meta only — no phase advance

## Spec refs

<!-- D-NNN ids touched by this PR (see spec/design-decisions.md). -->
<!-- Example: D-005, D-017. Delete this section if no spec rule is touched. -->

## Linked issues

<!-- Refs #N or Closes #N. -->

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] Every new `pub` item has a `///` doc comment
- [ ] Every new `.rs` file has the SPDX header
- [ ] AGENTS.md / spec/ updated if a rule or decision changed
- [ ] Commit messages follow AGENTS.md §"Commit guidance"
