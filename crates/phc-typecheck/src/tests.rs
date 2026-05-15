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

#[test]
fn class_methods_are_skipped_for_now() {
    // Classes do not yet have SymbolIds for their methods, so the
    // collector silently skips them. Future work fills this in.
    let typed = typed_for(
        r#"pack a;
           public class Box {
               public function get(): int { return 1; }
           }"#,
    );
    assert!(typed.function_sigs.is_empty());
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
