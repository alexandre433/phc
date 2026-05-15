// SPDX-License-Identifier: MIT
//! Inline tests for the semantic resolver.

use crate::{resolve, SymbolKind};
use phc_parser::parse;
use phc_span::FileId;

fn resolved_for(src: &str) -> crate::Resolved {
    let parsed = parse(src, FileId(0));
    assert!(
        parsed.diagnostics.is_empty(),
        "parser diagnostics in fixture: {:?}",
        parsed.diagnostics
    );
    let file = parsed.file.expect("expected SourceFile");
    resolve(&file)
}

#[test]
fn empty_file_has_no_symbols() {
    let r = resolved_for("pack a;");
    assert!(r.symbols.is_empty());
    assert!(r.diagnostics.is_empty());
}

#[test]
fn one_function_registered() {
    let r = resolved_for("pack a;\nfunction main(): void {}");
    assert_eq!(r.symbols.len(), 1);
    assert_eq!(r.symbols[0].name, "main");
    assert_eq!(r.symbols[0].kind, SymbolKind::Function);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn each_item_kind_gets_the_right_symbol_kind() {
    let src = r#"
        pack a;
        function f(): void {}
        public class C {}
        public enum E { A, B }
        public interface I { function m(): void; }
        public trait T { function m(): void {} }
        test "smoke" { return; }
    "#;
    let r = resolved_for(src);
    let kinds: Vec<SymbolKind> = r.symbols.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        vec![
            SymbolKind::Function,
            SymbolKind::Class,
            SymbolKind::Enum,
            SymbolKind::Interface,
            SymbolKind::Trait,
            SymbolKind::Test,
        ]
    );
}

#[test]
fn duplicate_top_level_name_is_an_error() {
    let src = r#"
        pack a;
        function dup(): void {}
        public class dup {}
    "#;
    let r = resolved_for(src);
    // First definition wins; second emits diagnostic.
    assert_eq!(r.symbols.len(), 1);
    assert_eq!(r.symbols[0].kind, SymbolKind::Function);
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.message.contains("duplicate top-level item `dup`")));
}

#[test]
fn lookup_round_trips_through_top_level() {
    let r = resolved_for("pack a;\nfunction add(): int {}\npublic class User {}");
    let add_id = r.top_level.get("add").copied().expect("add registered");
    assert_eq!(r.symbol(add_id).kind, SymbolKind::Function);
    let user_id = r.top_level.get("User").copied().expect("User registered");
    assert_eq!(r.symbol(user_id).kind, SymbolKind::Class);
}
