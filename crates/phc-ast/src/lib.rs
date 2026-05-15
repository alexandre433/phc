// SPDX-License-Identifier: MIT
//! Abstract syntax tree node definitions for PHC.
//!
//! Mirrors the productions in `spec/grammar.ebnf`. Every node carries
//! a [`Span`] tied to the originating source so diagnostics and IDE
//! tooling can underline the exact bytes that produced it.
//!
//! Nodes are added in lockstep with the parser. Anything not yet
//! parsed deliberately has no AST yet — see TODO comments.

use phc_span::Span;

/// A single PHC source file.
///
/// Grammar: `SourceFile = PackDecl { UseDecl } { Item }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub pack: PackDecl,
    pub uses: Vec<UseDecl>,
    pub items: Vec<Item>,
    pub span: Span,
}

/// `pack a.b.c;` declaration at the top of every source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackDecl {
    pub path: PackPath,
    pub span: Span,
}

/// `use a.b.C;` or `use a.b.{C, D};` import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDecl {
    pub path: PackPath,
    /// `Some(names)` for grouped imports `{ A, B }`; `None` for the
    /// single-item `use a.b.C;` form, in which case the last segment
    /// of `path` names the imported item.
    pub group: Option<Vec<Ident>>,
    pub span: Span,
}

/// Dot-separated path, used by `pack` and `use` declarations and by
/// type references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackPath {
    pub segments: Vec<Ident>,
    pub span: Span,
}

/// A single identifier with its source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// A type reference appearing in source (parameter type, return
/// type, field type, generic argument, ...).
///
/// Grammar: `Type = TypePath [ TypeArgs ] [ Nullable ]`. The path
/// segments are dot-separated identifiers (`int`, `app.User`). Type
/// arguments are themselves `TypeRef`s, forming a tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    pub path: Vec<Ident>,
    pub args: Vec<TypeRef>,
    pub nullable: bool,
    pub span: Span,
}

/// A generic parameter on a function, class, interface, or trait.
///
/// Grammar: `GenericParam = Identifier [ ":" BoundList ]` where
/// `BoundList = TypePath { "+" TypePath }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericParam {
    pub name: Ident,
    /// Each bound is a `TypePath`; reusing `Vec<Ident>` keeps the
    /// shape uniform with [`TypeRef::path`].
    pub bounds: Vec<Vec<Ident>>,
    pub span: Span,
}

/// Top-level item: function, class, enum, interface, trait, or test.
///
/// Only the variants that the parser handles today are present.
/// Class / enum / interface / trait / test land in later commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    // TODO(phase-2): variants land alongside their parsers.
}
