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
    // Top-level kinds — body-introduced Value symbols (e.g. trait
    // method `$this`) are excluded by filtering on top_level.
    let mut kinds: Vec<SymbolKind> = r.top_level.values().map(|id| r.symbol(*id).kind).collect();
    kinds.sort_by_key(|k| match k {
        SymbolKind::Function => 0,
        SymbolKind::Class => 1,
        SymbolKind::Enum => 2,
        SymbolKind::Interface => 3,
        SymbolKind::Trait => 4,
        SymbolKind::Test => 5,
        SymbolKind::Method => 6,
        SymbolKind::Field => 7,
        SymbolKind::Value => 8,
    });
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

#[test]
fn parameter_used_in_body_resolves() {
    let r = resolved_for(
        r#"pack a;
           function add(int $a, int $b): int { return $a + $b; }"#,
    );
    assert!(r.diagnostics.is_empty(), "got diags: {:?}", r.diagnostics);
    // Two use-sites recorded ($a and $b in the return expr).
    assert_eq!(r.uses.len(), 2);
}

#[test]
fn local_binding_visible_after_declaration() {
    let r = resolved_for(
        r#"pack a;
           function f(): int {
               int $x = 1;
               int $y = $x + 2;
               return $y;
           }"#,
    );
    assert!(r.diagnostics.is_empty(), "got diags: {:?}", r.diagnostics);
    // Two distinct use-sites for $x and $y.
    assert_eq!(r.uses.len(), 2);
}

#[test]
fn unresolved_var_emits_diagnostic() {
    let r = resolved_for(
        r#"pack a;
           function f(): int { return $missing; }"#,
    );
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.message.contains("unresolved name `$missing`")));
}

#[test]
fn this_resolves_inside_method_but_not_top_level_function() {
    let r = resolved_for(
        r#"pack a;
           public class Box {
               public function get(): int { return $this->value; }
           }"#,
    );
    assert!(r.diagnostics.is_empty(), "got diags: {:?}", r.diagnostics);

    let r2 = resolved_for(
        r#"pack a;
           function f(): int { return $this->value; }"#,
    );
    assert!(r2
        .diagnostics
        .iter()
        .any(|d| d.message.contains("unresolved name `$this`")));
}

#[test]
fn for_loop_introduces_element_binding() {
    let r = resolved_for(
        r#"pack a;
           function sum(list<int> $xs): int {
               flip int $total = 0;
               for (int $n in $xs) { $total := $total + $n; }
               return $total;
           }"#,
    );
    assert!(r.diagnostics.is_empty(), "got diags: {:?}", r.diagnostics);
}

#[test]
fn match_var_pattern_binds_in_arm_body() {
    let r = resolved_for(
        r#"pack a;
           function classify(int $code): int {
               return match ($code) {
                   $n if $n > 500 => 500,
                   _ => 0,
               };
           }"#,
    );
    assert!(r.diagnostics.is_empty(), "got diags: {:?}", r.diagnostics);
}

#[test]
fn lambda_param_scoped_to_body() {
    let r = resolved_for(
        r#"pack a;
           function f(): int {
               int $factor = 2;
               int $result = doubled((int $n): int => $n * $factor);
               return $result;
           }"#,
    );
    assert!(r.diagnostics.is_empty(), "got diags: {:?}", r.diagnostics);
}

#[test]
fn class_members_register_under_members_of() {
    let r = resolved_for(
        r#"pack a;
           public class User {
               construct(public string $name) {}
               int $age = 0;
               public function greet(): string { return "hi"; }
           }"#,
    );
    let class_id = r.top_level.get("User").copied().expect("User registered");
    let members = r.members_of.get(&class_id).expect("members recorded");
    assert_eq!(members.len(), 3, "construct + field + method");
    let kinds: Vec<SymbolKind> = members.iter().map(|id| r.symbol(*id).kind).collect();
    assert_eq!(
        kinds,
        vec![SymbolKind::Method, SymbolKind::Field, SymbolKind::Method]
    );
    let names: Vec<&str> = members
        .iter()
        .map(|id| r.symbol(*id).name.as_str())
        .collect();
    assert_eq!(names, vec!["construct", "age", "greet"]);
}

#[test]
fn trait_methods_register_under_members_of() {
    let r = resolved_for(
        r#"pack a;
           public trait Loggable { function log(): void {} }"#,
    );
    let trait_id = r
        .top_level
        .get("Loggable")
        .copied()
        .expect("Loggable registered");
    let members = r.members_of.get(&trait_id).expect("methods recorded");
    assert_eq!(members.len(), 1);
    assert_eq!(r.symbol(members[0]).name, "log");
    assert_eq!(r.symbol(members[0]).kind, SymbolKind::Method);
}

#[test]
fn interface_method_sigs_register_under_members_of() {
    let r = resolved_for(
        r#"pack a;
           public interface Greet { function greet(): string; function bye(): void; }"#,
    );
    let iface_id = r.top_level.get("Greet").copied().expect("Greet registered");
    let members = r.members_of.get(&iface_id).expect("methods recorded");
    assert_eq!(members.len(), 2);
}

#[test]
fn local_binding_does_not_leak_out_of_block() {
    let r = resolved_for(
        r#"pack a;
           function f(): int {
               if (true) { int $temp = 1; }
               return $temp;
           }"#,
    );
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.message.contains("unresolved name `$temp`")));
}
