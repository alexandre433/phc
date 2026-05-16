// SPDX-License-Identifier: MIT
//! Inline tests for type lowering and signature collection.

use crate::{lower_type_ref, typecheck, Primitive, Ty, Typed};
use phc_ast::{Borrow, Ident, TypeRef};
use phc_parser::parse;
use phc_span::{FileId, Span};

fn ident(name: &str) -> Ident {
    Ident {
        name: name.into(),
        span: Span::new(FileId(0), 0, 0),
    }
}

fn type_ref(path: &[&str], args: Vec<TypeRef>, nullable: bool) -> TypeRef {
    TypeRef {
        path: path.iter().map(|s| ident(s)).collect(),
        args,
        nullable,
        fn_return: None,
        span: Span::new(FileId(0), 0, 0),
    }
}

#[test]
fn primitive_recognition_covers_d_022_set() {
    for (name, expected) in [
        ("int", Primitive::Int),
        ("float", Primitive::Float),
        ("byte", Primitive::Byte),
        ("bytes", Primitive::Bytes),
        ("bool", Primitive::Bool),
        ("string", Primitive::String),
        ("void", Primitive::Void),
    ] {
        let tr = type_ref(&[name], vec![], false);
        assert_eq!(lower_type_ref(&tr), Ty::Primitive(expected));
    }
}

#[test]
fn nullable_primitive_kept_distinct() {
    let tr = type_ref(&["int"], vec![], true);
    assert_eq!(lower_type_ref(&tr), Ty::NullablePrimitive(Primitive::Int));
    assert_ne!(
        lower_type_ref(&tr),
        Ty::Primitive(Primitive::Int),
        "nullable form must differ from non-nullable"
    );
}

#[test]
fn user_type_lowers_to_path() {
    let tr = type_ref(&["app", "User"], vec![], false);
    match lower_type_ref(&tr) {
        Ty::Path {
            path,
            args,
            nullable,
        } => {
            assert_eq!(path, vec!["app".to_string(), "User".to_string()]);
            assert!(args.is_empty());
            assert!(!nullable);
        }
        other => panic!("expected Path, got {other:?}"),
    }
}

#[test]
fn generic_arguments_lower_recursively() {
    // map<string, list<int>>
    let inner_list = type_ref(&["list"], vec![type_ref(&["int"], vec![], false)], false);
    let map_tr = type_ref(
        &["map"],
        vec![type_ref(&["string"], vec![], false), inner_list],
        false,
    );
    let lowered = lower_type_ref(&map_tr);
    assert_eq!(lowered.display(), "map<string, list<int>>");
}

#[test]
fn nullable_outermost_for_user_path() {
    let tr = type_ref(&["list"], vec![type_ref(&["int"], vec![], false)], true);
    match lower_type_ref(&tr) {
        Ty::Path { nullable, args, .. } => {
            assert!(nullable);
            assert!(matches!(args[0], Ty::Primitive(Primitive::Int)));
        }
        other => panic!("expected Path, got {other:?}"),
    }
}

fn typed_for(src: &str) -> Typed {
    let parsed = parse(src, FileId(0));
    assert!(
        parsed.diagnostics.is_empty(),
        "parser diagnostics: {:?}",
        parsed.diagnostics
    );
    let file = parsed.file.expect("expected SourceFile");
    let resolved = phc_semantic::resolve(&file);
    assert!(
        resolved.diagnostics.is_empty(),
        "resolver diagnostics: {:?}",
        resolved.diagnostics
    );
    typecheck(&file, &resolved)
}

#[test]
fn function_sig_records_param_and_return_types() {
    let typed = typed_for("pack a;\nfunction add(int $a, int $b): int { return $a; }");
    assert_eq!(typed.function_sigs.len(), 1);
    let sig = typed.function_sigs.values().next().unwrap();
    assert_eq!(sig.params.len(), 2);
    assert_eq!(sig.params[0].name, "a");
    assert_eq!(sig.params[0].ty.display(), "int");
    assert_eq!(sig.params[0].borrow, Borrow::None);
    assert_eq!(sig.return_ty.display(), "int");
    assert!(sig.generic_params.is_empty());
}

#[test]
fn function_sig_preserves_borrow_modifiers() {
    let typed = typed_for("pack a;\nfunction touch(&flip int $c, &string $s): void { return; }");
    let sig = typed.function_sigs.values().next().unwrap();
    assert_eq!(sig.params[0].borrow, Borrow::Mutable);
    assert_eq!(sig.params[1].borrow, Borrow::Shared);
}

#[test]
fn generic_function_records_param_names() {
    let typed = typed_for("pack a;\nfunction max<T: Ord, U>(T $a, T $b): T { return $a; }");
    let sig = typed.function_sigs.values().next().unwrap();
    assert_eq!(sig.generic_params, vec!["T".to_string(), "U".to_string()]);
    // Generic-typed param lowers to a Path (T is unknown until
    // generic resolution) — recorded verbatim today.
    assert_eq!(sig.params[0].ty.display(), "T");
    assert_eq!(sig.return_ty.display(), "T");
}

#[test]
fn nullable_and_generic_return_types_round_trip() {
    let typed =
        typed_for("pack a;\nfunction lookup(string $key): result<int, string?> { return key; }");
    let sig = typed.function_sigs.values().next().unwrap();
    assert_eq!(sig.return_ty.display(), "result<int, string?>");
}

fn first_function_body_types(typed: &Typed) -> Vec<String> {
    let mut entries: Vec<(_, _)> = typed
        .expr_types
        .iter()
        .map(|(span, ty)| (span.lo, ty.display()))
        .collect();
    entries.sort_by_key(|(lo, _)| *lo);
    entries.into_iter().map(|(_, t)| t).collect()
}

#[test]
fn literal_expression_types_are_inferred() {
    let typed = typed_for(
        r#"pack a;
           function f(): int {
               int $x = 1;
               float $y = 1.5;
               bool $b = true;
               string $s = "hi";
               return $x;
           }"#,
    );
    let types: std::collections::HashSet<String> =
        typed.expr_types.values().map(|t| t.display()).collect();
    assert!(types.contains("int"));
    assert!(types.contains("float"));
    assert!(types.contains("bool"));
    assert!(types.contains("string"));
}

#[test]
fn variable_use_resolves_to_binding_type() {
    let typed = typed_for(
        r#"pack a;
           function add(int $a, int $b): int { return $a + $b; }"#,
    );
    let types = first_function_body_types(&typed);
    // Expect: $a (int), $b (int), Add binary (int).
    assert!(types.contains(&"int".to_string()));
    let int_count = types.iter().filter(|t| t.as_str() == "int").count();
    assert!(
        int_count >= 3,
        "expected ≥3 int types (params + binary), got {types:?}"
    );
}

#[test]
fn binary_arithmetic_int_plus_int_yields_int() {
    let typed = typed_for(
        r#"pack a;
           function f(): int { return 1 + 2 * 3; }"#,
    );
    let types = first_function_body_types(&typed);
    // Three int literals + two int Binary nodes = 5 ints.
    let int_count = types.iter().filter(|t| t.as_str() == "int").count();
    assert_eq!(int_count, 5, "got {types:?}");
}

#[test]
fn comparison_yields_bool() {
    let typed = typed_for(
        r#"pack a;
           function lt(int $a, int $b): bool { return $a < $b; }"#,
    );
    let types: Vec<String> = typed.expr_types.values().map(|t| t.display()).collect();
    assert!(types.contains(&"bool".to_string()));
}

#[test]
fn unknown_type_for_call_and_member() {
    let typed = typed_for(
        r#"pack a;
           function f(): int {
               int $x = doStuff();
               return $x;
           }"#,
    );
    // The Call site is Unknown today; the binding $x is still int.
    let unknowns = typed
        .expr_types
        .values()
        .filter(|t| matches!(t, Ty::Unknown))
        .count();
    assert!(unknowns >= 1, "expected at least one Unknown for the call");
}

#[test]
fn enum_match_covering_all_variants_passes() {
    let typed = typed_for(
        r#"pack a;
           public enum Status { Ok, NotFound }
           function classify(Status $s): int {
               return match ($s) {
                   Status::Ok => 0,
                   Status::NotFound => 404,
               };
           }
           function main(): void {}"#,
    );
    assert!(
        typed
            .diagnostics
            .iter()
            .all(|d| !d.message.contains("non-exhaustive")),
        "got diagnostics: {:?}",
        typed.diagnostics
    );
}

#[test]
fn enum_match_missing_variant_is_an_error() {
    let typed = typed_for(
        r#"pack a;
           public enum Status { Ok, NotFound, Gone }
           function classify(Status $s): int {
               return match ($s) {
                   Status::Ok => 0,
                   Status::NotFound => 404,
               };
           }
           function main(): void {}"#,
    );
    assert!(typed.diagnostics.iter().any(|d| d
        .message
        .contains("non-exhaustive `match` on enum `Status`")));
}

#[test]
fn enum_match_with_wildcard_passes() {
    let typed = typed_for(
        r#"pack a;
           public enum Status { Ok, NotFound, Gone }
           function classify(Status $s): int {
               return match ($s) {
                   Status::Ok => 0,
                   _ => 1,
               };
           }
           function main(): void {}"#,
    );
    assert!(
        typed
            .diagnostics
            .iter()
            .all(|d| !d.message.contains("non-exhaustive")),
        "got diagnostics: {:?}",
        typed.diagnostics
    );
}

#[test]
fn non_enum_match_without_wildcard_is_an_error() {
    let typed = typed_for(
        r#"pack a;
           function f(int $n): int {
               return match ($n) {
                   1 => 100,
                   2 => 200,
               };
           }
           function main(): void {}"#,
    );
    assert!(typed
        .diagnostics
        .iter()
        .any(|d| d.message.contains("a `_` wildcard arm is required")));
}

#[test]
fn guarded_catchall_does_not_satisfy_exhaustiveness() {
    let typed = typed_for(
        r#"pack a;
           function f(int $n): int {
               return match ($n) {
                   $x if $x > 0 => 1,
               };
           }
           function main(): void {}"#,
    );
    assert!(typed
        .diagnostics
        .iter()
        .any(|d| d.message.contains("non-exhaustive")));
}

#[test]
fn class_method_sig_collected() {
    let typed = typed_for(
        r#"pack a;
           public class Box {
               public function get(): int { return 1; }
           }"#,
    );
    // Construct (synthesised) is absent; only the method.
    assert_eq!(typed.function_sigs.len(), 1);
    let sig = typed.function_sigs.values().next().unwrap();
    assert_eq!(sig.return_ty.display(), "int");
}

#[test]
fn class_construct_sig_returns_class_type() {
    let typed = typed_for(
        r#"pack a;
           public class User {
               construct(public string $name) {}
               public function greet(): string { return "hi"; }
           }"#,
    );
    // construct + method = 2 sigs.
    assert_eq!(typed.function_sigs.len(), 2);
    let construct = typed
        .function_sigs
        .values()
        .find(|s| s.return_ty.display() == "User")
        .expect("construct sig should return User");
    assert_eq!(construct.params.len(), 1);
    assert_eq!(construct.params[0].name, "name");
    assert_eq!(construct.params[0].ty.display(), "string");
}

#[test]
fn trait_and_interface_method_sigs_collected() {
    let typed = typed_for(
        r#"pack a;
           public interface Greet { function greet(): string; }
           public trait Loggable { function log(): void {} }"#,
    );
    let return_types: std::collections::HashSet<String> = typed
        .function_sigs
        .values()
        .map(|s| s.return_ty.display())
        .collect();
    assert!(return_types.contains("string"));
    assert!(return_types.contains("void"));
}

#[test]
fn display_round_trips_common_shapes() {
    assert_eq!(Ty::Primitive(Primitive::Int).display(), "int");
    assert_eq!(
        Ty::NullablePrimitive(Primitive::String).display(),
        "string?"
    );
    let user = lower_type_ref(&type_ref(&["app", "User"], vec![], true));
    assert_eq!(user.display(), "app.User?");
    let result = lower_type_ref(&type_ref(
        &["result"],
        vec![
            type_ref(&["int"], vec![], false),
            type_ref(&["ParseError"], vec![], false),
        ],
        false,
    ));
    assert_eq!(result.display(), "result<int, ParseError>");
}
