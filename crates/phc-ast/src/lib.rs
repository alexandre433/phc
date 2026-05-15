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
    Function(FunctionDecl),
    // TODO(phase-2): Class, Enum, Interface, Trait, Test variants.
}

/// `[public] [async] function name<T>(p1, p2): RetType { ... }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDecl {
    pub visibility: Visibility,
    pub is_async: bool,
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub body: Block,
    pub span: Span,
}

/// Visibility marker (D-008). Only two levels in v0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Pack-scoped — visible to every file in the same pack only.
    Default,
    /// Cross-pack — visible to any pack that imports the item.
    Public,
}

/// A parameter on a function or lambda.
///
/// Grammar: `Param = [ BorrowMod ] Type VarRef`. The `$` sigil on
/// the variable name is consumed by the lexer; the parser stores
/// only the bare identifier in [`Self::name`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub borrow: Borrow,
    pub ty: TypeRef,
    pub name: Ident,
    pub span: Span,
}

/// Borrow modifier on a parameter or expression operand (D-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Borrow {
    /// No leading `&`. Owned-by-value at the call boundary.
    None,
    /// Leading `&` only — shared (read) borrow.
    Shared,
    /// Leading `&flip` — mutable (exclusive) borrow.
    Mutable,
}

/// A `{ ... }` block.
///
/// Statements land in P5 alongside the rest of the statement
/// grammar. For now a `Block` is purely the brace pair plus its
/// span, which is enough to round-trip a function declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

/// Placeholder for the statement enum. Variants land in P5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    // TODO(phase-2): LocalBinding, Reassign, If, While, For, Return,
    // Break, Continue, ExprStmt.
}
