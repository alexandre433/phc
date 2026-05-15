// SPDX-License-Identifier: MIT
//! Inline tests for type lowering.

use crate::{lower_type_ref, Primitive, Ty};
use phc_ast::{Ident, TypeRef};
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
