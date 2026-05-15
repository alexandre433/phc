---
name: Bug report
about: Something is broken in the compiler, toolchain, or spec
title: "bug: <short description>"
labels: bug
---

## Repro

<!-- Minimal steps or `.phc` snippet that triggers the bug. -->

## Expected

<!-- What should happen. -->

## Actual

<!-- What actually happens. Paste compiler output, panics, or stack
     traces inside fenced code blocks. -->

## Environment

- PHC commit: <!-- output of `git rev-parse HEAD` -->
- Rust toolchain: <!-- output of `rustc --version` -->
- OS:

## Phase

<!-- Which phase does this bug live in? See AGENTS.md "Phase model". -->
