# PHC examples

Canonical PHC snippets lifted from `spec/language-reference.md`. They
serve two purposes:

1. Developer-facing examples of every locked v0 syntax construct.
2. A corpus of parser-test fixtures. Once Phase 2 lands, every file
   here must lex and parse without error; that becomes the first
   smoke test wired into `crates/phc-parser/tests/`.

When you add a new example here, also link it from the relevant
section of `spec/language-reference.md` so the spec and the corpus
stay in lock-step.
