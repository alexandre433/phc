// SPDX-License-Identifier: MIT
//! Semantic analysis for PHC: name resolution, pack acyclicity,
//! visibility, and the rest of the checks listed in issue #4.
//!
//! This first slice covers symbol identity and the data structures
//! every later check will read from. Top-level collection lives in
//! the [`collect`] module; local-binding scope walking lands in a
//! follow-up commit.

mod collect;
mod resolve;

#[cfg(test)]
mod tests;

use phc_ast::SourceFile;
use phc_errors::Diagnostic;
use phc_span::Span;
use std::collections::HashMap;

pub use collect::collect_top_level;
pub use resolve::resolve_bodies;

/// Stable identifier assigned to every named entity the resolver
/// discovers (top-level item, parameter, local binding, ...).
///
/// Ids are dense and assigned in source order. Consumers (later
/// passes, the LSP) treat them as opaque keys into [`Resolved`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SymbolId(pub u32);

/// Kind of a [`Symbol`]. Distinguishes the introducer so later
/// passes can reject e.g. a `class` name where a `$var` is required.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SymbolKind {
    Function,
    Class,
    Enum,
    Interface,
    Trait,
    Test,
    /// Class / trait method, including the `construct` constructor
    /// (which is recorded as a method named `construct`).
    Method,
    /// Class field — declared inside a class body with the
    /// local-binding shape.
    Field,
    /// Parameter, local binding, lambda param, for-elem, match
    /// pattern Var. The finer breakdown lands when typecheck needs
    /// it.
    Value,
}

/// One named entity in the program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    /// Span of the definition (the identifier token, not the whole
    /// declaration). Used by IDE go-to-definition.
    pub def_span: Span,
}

/// Result of running the resolver over one source file.
///
/// Every symbol the resolver sees is recorded once in [`Self::symbols`]
/// and indexed by [`SymbolId`]. Diagnostics for malformed inputs
/// (duplicates, unresolved references) accumulate in
/// [`Self::diagnostics`] without aborting the pass.
#[derive(Debug, Default)]
pub struct Resolved {
    pub symbols: Vec<Symbol>,
    /// Top-level scope: name → symbol id for every Item in the file.
    pub top_level: HashMap<String, SymbolId>,
    /// For each Class / Trait / Interface symbol id, the ordered
    /// list of its member symbol ids (methods, fields, constructor).
    /// Empty for non-container symbols.
    pub members_of: HashMap<SymbolId, Vec<SymbolId>>,
    /// Use-site → symbol id for every resolved `$name` / `$this`
    /// reference. Keyed by the use-site span; collisions across
    /// files are not possible while the resolver is single-file
    /// scoped (see [`resolve`]).
    pub uses: HashMap<Span, SymbolId>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Resolved {
    /// Borrow a symbol by id. Panics if the id was not produced by
    /// this resolver — never reachable from well-typed callers.
    pub fn symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0 as usize]
    }
}

/// Top-level entry point. Runs every implemented check on `file`
/// and returns the accumulated [`Resolved`].
pub fn resolve(file: &SourceFile) -> Resolved {
    let mut resolved = Resolved::default();
    collect_top_level(file, &mut resolved);
    resolve_bodies(file, &mut resolved);
    resolved
}
