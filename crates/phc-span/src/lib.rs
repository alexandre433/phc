// SPDX-License-Identifier: MIT
//! Source-span types shared across the PHC compiler.
//!
//! Every AST and IR node, and every diagnostic, carries a [`Span`] so
//! diagnostics can underline the exact source range that caused the
//! error. Spans are byte-indexed into the original UTF-8 source after
//! newline normalisation.

/// Identifier of a source file inside the compilation session.
///
/// Resolved to a path via the session's file map; opaque otherwise.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FileId(pub u32);

/// Byte range inside a source file.
///
/// `lo` is inclusive, `hi` is exclusive. Both are byte offsets into
/// the UTF-8 source after newline normalisation.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Span {
    pub file: FileId,
    pub lo: u32,
    pub hi: u32,
}

impl Span {
    /// Construct a new span covering bytes `lo..hi` in `file`.
    ///
    /// Callers must ensure `hi >= lo`; spans where `hi < lo` are a
    /// programming error and are not validated at runtime.
    pub fn new(file: FileId, lo: u32, hi: u32) -> Self {
        Self { file, lo, hi }
    }

    /// Number of bytes covered by this span.
    pub fn len(self) -> u32 {
        self.hi - self.lo
    }

    /// Whether this span covers zero bytes.
    pub fn is_empty(self) -> bool {
        self.hi == self.lo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_len_matches_range() {
        let s = Span::new(FileId(0), 4, 10);
        assert_eq!(s.len(), 6);
        assert!(!s.is_empty());
    }

    #[test]
    fn empty_span_reports_empty() {
        let s = Span::new(FileId(7), 12, 12);
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }
}
