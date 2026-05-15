// SPDX-License-Identifier: MIT
//! Top-level symbol collection.
//!
//! Walks every [`Item`](phc_ast::Item) in a [`SourceFile`] and
//! registers it in the resolver. Catches duplicate names defined in
//! the same source file. Inside a single PHC pack, two files can
//! still both define a name (D-001 — pack-scoped visibility); that
//! check belongs to the pack-level resolver, which is a follow-up
//! once a session-scope walks multiple files together.

use phc_ast::{Ident, Item, SourceFile};
use phc_errors::{Diagnostic, Severity};

use crate::{Resolved, Symbol, SymbolId, SymbolKind};

/// Register every top-level item in `file` as a [`Symbol`] inside
/// `resolved`. Duplicate names emit a diagnostic; only the first
/// definition wins in [`Resolved::top_level`].
pub fn collect_top_level(file: &SourceFile, resolved: &mut Resolved) {
    for item in &file.items {
        let (name, kind) = match item {
            Item::Function(f) => (&f.name, SymbolKind::Function),
            Item::Class(c) => (&c.name, SymbolKind::Class),
            Item::Enum(e) => (&e.name, SymbolKind::Enum),
            Item::Interface(i) => (&i.name, SymbolKind::Interface),
            Item::Trait(t) => (&t.name, SymbolKind::Trait),
            Item::Test(t) => {
                // Test names come from a string literal, not an
                // identifier. Render the textual chunks into a
                // synthetic name keyed off span so duplicates can
                // still be detected.
                let synth = render_test_name(t);
                register(
                    resolved,
                    &Ident {
                        name: synth,
                        span: t.span,
                    },
                    SymbolKind::Test,
                );
                continue;
            }
        };
        register(resolved, name, kind);
    }
}

fn register(resolved: &mut Resolved, name: &Ident, kind: SymbolKind) {
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
        return;
    }
    let id = SymbolId(resolved.symbols.len() as u32);
    resolved.symbols.push(Symbol {
        id,
        name: name.name.clone(),
        kind,
        def_span: name.span,
    });
    resolved.top_level.insert(name.name.clone(), id);
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
