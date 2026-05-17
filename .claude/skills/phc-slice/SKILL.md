---
name: phc-slice
description: Run validated PHC slice cadence (feat branch → spec → impl → tests → fmt+clippy → ff-merge to development → delete branch). User-only — has side effects (creates branch, writes spec, commits). Invoke with `/phc-slice <D-###> <short-summary>`.
disable-model-invocation: true
---

# PHC Slice Cadence

Encodes Alex's validated slice loop. See memory `feedback_slice_cadence.md`.

## Inputs

Parse from `$ARGUMENTS`:
- `D-###` identifier (e.g. `D-038`). If missing, ask user.
- Short summary (e.g. `list<T> reduce`). If missing, ask user.

## Steps

### 1. Pre-flight

- Confirm on `development` branch, working tree clean.
  - `git status --porcelain` → must be empty.
  - `git rev-parse --abbrev-ref HEAD` → must be `development`.
- If dirty / wrong branch: STOP and ask.

### 2. Spec gap check

- `grep -n "^### D-" spec/design-decisions.md | tail -5` — confirm next D-### slot.
- If gap or conflict with planned slice: surface 2–4 options (per `feedback_slice_cadence.md`), don't pick silently.

### 3. Branch

- `git switch -c feat/D-NNN-<kebab-summary>` from `development`.

### 4. Spec entry

- Append to `spec/design-decisions.md`:

```
### D-NNN — <Title> (vXa)
- **Decision**: ...
- **Alternatives considered**: ...
- **Rationale**: ...
- **Date**: <today>
- **Status**: locked.
```

- If introducing new keyword / namespace / collection / runtime call, cross-reference relevant prior D-### entries.

### 5. Implementation

- Touch only crates the slice requires. Mirror compiler-phase boundaries (lexer → parser → typecheck → borrowcheck → ir → lower → codegen → runtime → interp).
- Surgical changes per CLAUDE.md §"Surgical changes". Every changed line traces to slice.
- Public items: `///` doc on every `pub`, `//!` on `lib.rs`.

### 6. Tests

- Encode WHY, not just WHAT (CLAUDE.md Rule 9).
- Crate-local unit tests + integration test in highest-touched crate.
- For runtime / codegen slices: e2e test through `phc run` / `phc test`.
- Windows AV: wrap freshly-built-binary spawn tests in `run_or_skip_on_av` (memory `reference_windows_av_test_skip.md`).

### 7. Quality gate

```
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must pass. Fix until green.

### 8. Commit

- Caveman-commit style (subject ≤50 char, `[phase-N]` tag, `Refs #N` footer).
- See `AGENTS.md` §"Commit guidance".
- Example: `feat(stdlib): D-038 — list<T> reduce [phase-6]`

### 9. Docs refresh

- Update `spec/README.md` slice status if the phase status table mentions this slice.
- Update CLAUDE.md "Locked Design Decisions" if a new D-### should appear there.

### 10. Fast-forward merge

- `git switch development && git merge --ff-only feat/D-NNN-<...>`
- `git branch -d feat/D-NNN-<...>`

### 11. Final checkpoint

Report to user:
- D-### shipped.
- Crates touched.
- Tests added.
- Commit SHA.

## Halt conditions

Stop and ask if any:
- Spec gap requires design choice (offer 2–4 options).
- Tests fail and root cause unclear after 2 attempts.
- Slice exceeds one D-### worth of scope (split it).
- Touches `spec/design-decisions.md` D-### entries already marked `locked`.
