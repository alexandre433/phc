// SPDX-License-Identifier: MIT
//! Top-level symbol collection.
//!
//! Walks every [`Item`](phc_ast::Item) in a [`SourceFile`] and
//! registers it in the resolver. Catches duplicate names defined in
//! the same source file. Inside a single PHC pack, two files can
//! still both define a name (D-001 — pack-scoped visibility); that
//! check belongs to the pack-level resolver, which is a follow-up
//! once a session-scope walks multiple files together.

use phc_ast::{ClassMember, Ident, InterfaceDecl, Item, SourceFile, TraitDecl};
use phc_errors::{Diagnostic, Severity};
use phc_span::Span;

use crate::{Resolved, Symbol, SymbolId, SymbolKind};

/// Register every top-level item in `file` as a [`Symbol`] inside
/// `resolved`. Duplicate names emit a diagnostic; only the first
/// definition wins in [`Resolved::top_level`].
pub fn collect_top_level(file: &SourceFile, resolved: &mut Resolved) {
    for item in &file.items {
        match item {
            Item::Function(f) => {
                register(resolved, &f.name, SymbolKind::Function);
            }
            Item::Class(c) => {
                if let Some(class_id) = register(resolved, &c.name, SymbolKind::Class) {
                    let mut members = Vec::new();
                    for member in &c.members {
                        if let Some(id) = collect_class_member(resolved, member) {
                            members.push(id);
                        }
                    }
                    resolved.members_of.insert(class_id, members);
                }
            }
            Item::Enum(e) => {
                register(resolved, &e.name, SymbolKind::Enum);
                // Enum variants are addressed via `Type::Variant`, not
                // as standalone names. Variant symbols land alongside
                // member access resolution.
            }
            Item::Interface(i) => {
                if let Some(iface_id) = register(resolved, &i.name, SymbolKind::Interface) {
                    let methods = collect_interface_methods(resolved, i);
                    resolved.members_of.insert(iface_id, methods);
                }
            }
            Item::Trait(t) => {
                if let Some(trait_id) = register(resolved, &t.name, SymbolKind::Trait) {
                    let methods = collect_trait_methods(resolved, t);
                    resolved.members_of.insert(trait_id, methods);
                }
            }
            Item::Test(t) => {
                let synth = render_test_name(t);
                register(
                    resolved,
                    &Ident {
                        name: synth,
                        span: t.span,
                    },
                    SymbolKind::Test,
                );
            }
        }
    }
}

fn collect_class_member(resolved: &mut Resolved, member: &ClassMember) -> Option<SymbolId> {
    match member {
        ClassMember::Method(m) => Some(intro_member(resolved, &m.name, SymbolKind::Method)),
        ClassMember::Field(f) => Some(intro_member(resolved, &f.name, SymbolKind::Field)),
        ClassMember::Construct(c) => {
            // `construct` is a positional method named after the
            // keyword. Its def_span points at the `construct` token.
            Some(intro_member_synthetic(
                resolved,
                "construct",
                c.span,
                SymbolKind::Method,
            ))
        }
        // `use Trait;` mixes a trait into the class. The trait
        // itself is the symbol of interest; the use line does not
        // introduce a new identifier.
        ClassMember::TraitUse(_) => None,
    }
}

fn collect_interface_methods(resolved: &mut Resolved, i: &InterfaceDecl) -> Vec<SymbolId> {
    i.methods
        .iter()
        .map(|sig| intro_member(resolved, &sig.name, SymbolKind::Method))
        .collect()
}

fn collect_trait_methods(resolved: &mut Resolved, t: &TraitDecl) -> Vec<SymbolId> {
    t.methods
        .iter()
        .map(|m| intro_member(resolved, &m.name, SymbolKind::Method))
        .collect()
}

/// Append a member symbol to the global symbol table without
/// touching `top_level`. Members are addressed via `members_of`,
/// not by bare name.
fn intro_member(resolved: &mut Resolved, name: &Ident, kind: SymbolKind) -> SymbolId {
    intro_member_synthetic(resolved, &name.name, name.span, kind)
}

fn intro_member_synthetic(
    resolved: &mut Resolved,
    name: &str,
    def_span: Span,
    kind: SymbolKind,
) -> SymbolId {
    let id = SymbolId(resolved.symbols.len() as u32);
    resolved.symbols.push(Symbol {
        id,
        name: name.to_string(),
        kind,
        def_span,
    });
    id
}

fn register(resolved: &mut Resolved, name: &Ident, kind: SymbolKind) -> Option<SymbolId> {
    if let Some(prev) = resolved.top_level.get(&name.name).copied() {
        let prev_def = resolved.symbol(prev).def_span;
        resolved.diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: format!(
                "duplicate top-level item `{}` (previous definition at byte {}..{})",
                name.name, prev_def.lo, prev_def.hi
            ),
            span: name.span,
        });
        return None;
    }
    let id = SymbolId(resolved.symbols.len() as u32);
    resolved.symbols.push(Symbol {
        id,
        name: name.name.clone(),
        kind,
        def_span: name.span,
    });
    resolved.top_level.insert(name.name.clone(), id);
    Some(id)
}

fn render_test_name(decl: &phc_ast::TestDecl) -> String {
    let mut buf = String::new();
    for part in &decl.name {
        match part {
            phc_ast::StrPart::Text(t) => buf.push_str(t),
            // Interpolation in a test name is unusual but the
            // grammar admits it; render the placeholder verbatim
            // so two identical-text tests still collide.
            phc_ast::StrPart::Expr(_) => buf.push_str("<expr>"),
        }
    }
    buf
}
