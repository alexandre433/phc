// SPDX-License-Identifier: MIT
//! PHC C runtime — bundled as static strings so `phc-build` can
//! write them next to generated C and feed both to a C compiler.
//!
//! Two assets ship today:
//! - [`HEADER`] — `phc_runtime.h`, included by every emitted C file.
//! - [`SOURCE`] — `phc_runtime.c`, compiled alongside the emitted C.
//!
//! Both are committed under `crates/phc-runtime/runtime/` and
//! embedded at compile time via `include_str!`. The Rust crate has
//! no native code of its own — everything that lives at runtime is
//! C, written exactly to drive `phc build`'s hello-world target.

/// `phc_runtime.h` — the public C header.
pub const HEADER: &str = include_str!("../runtime/phc_runtime.h");

/// `phc_runtime.c` — the implementations to compile + link.
pub const SOURCE: &str = include_str!("../runtime/phc_runtime.c");

/// Filename the header is referenced as inside emitted C
/// (`#include "phc_runtime.h"`).
pub const HEADER_NAME: &str = "phc_runtime.h";

/// Filename to write the runtime source as alongside the emitted
/// program when invoking the C compiler.
pub const SOURCE_NAME: &str = "phc_runtime.c";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_declares_phc_print() {
        assert!(HEADER.contains("phc_print"));
    }

    #[test]
    fn source_implements_phc_print() {
        assert!(SOURCE.contains("void phc_print"));
    }

    #[test]
    fn assets_are_non_empty() {
        assert!(!HEADER.is_empty());
        assert!(!SOURCE.is_empty());
    }
}
