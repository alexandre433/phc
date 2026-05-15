// SPDX-License-Identifier: MIT
//! Inline parser tests — one slice per production family.

use crate::parse;
use phc_span::FileId;

fn parse_ok(src: &str) -> phc_ast::SourceFile {
    let result = parse(src, FileId(0));
    assert!(
        result.diagnostics.is_empty(),
        "expected no diagnostics, got {:?}",
        result.diagnostics
    );
    result.file.expect("expected a SourceFile")
}

fn parse_err(src: &str) -> Vec<phc_errors::Diagnostic> {
    let result = parse(src, FileId(0));
    assert!(
        !result.diagnostics.is_empty(),
        "expected diagnostics, got none"
    );
    result.diagnostics
}

#[test]
fn pack_only_file() {
    let file = parse_ok("pack examples.hello;");
    assert_eq!(file.pack.path.segments.len(), 2);
    assert_eq!(file.pack.path.segments[0].name, "examples");
    assert_eq!(file.pack.path.segments[1].name, "hello");
    assert!(file.uses.is_empty());
    assert!(file.items.is_empty());
}

#[test]
fn single_segment_pack_path() {
    let file = parse_ok("pack root;");
    assert_eq!(file.pack.path.segments.len(), 1);
    assert_eq!(file.pack.path.segments[0].name, "root");
}

#[test]
fn pack_plus_use_imports() {
    let src = r#"
        pack app.handlers;
        use app.http.Request;
        use app.db.Connection;
    "#;
    let file = parse_ok(src);
    assert_eq!(file.uses.len(), 2);
    assert_eq!(
        file.uses[0]
            .path
            .segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["app", "http", "Request"]
    );
    assert!(file.uses[0].group.is_none());
    assert_eq!(
        file.uses[1].path.segments.last().unwrap().name,
        "Connection"
    );
}

#[test]
fn grouped_use_import() {
    let src = r#"
        pack app.handlers;
        use app.db.{Connection, Pool};
    "#;
    let file = parse_ok(src);
    assert_eq!(file.uses.len(), 1);
    assert_eq!(
        file.uses[0]
            .path
            .segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["app", "db"],
    );
    let group = file.uses[0]
        .group
        .as_ref()
        .expect("grouped import should yield Some");
    assert_eq!(
        group.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        vec!["Connection", "Pool"]
    );
}

#[test]
fn grouped_use_with_trailing_comma() {
    let src = r#"
        pack a;
        use b.{X, Y,};
    "#;
    let file = parse_ok(src);
    let group = file.uses[0].group.as_ref().unwrap();
    assert_eq!(group.len(), 2);
}

#[test]
fn missing_pack_keyword_is_an_error() {
    let diags = parse_err("function main(): void {}");
    assert!(diags.iter().any(|d| d.message.contains("`pack` keyword")));
}

#[test]
fn missing_semicolon_after_pack_is_an_error() {
    let diags = parse_err("pack a.b\n");
    assert!(diags.iter().any(|d| d.message.contains("`;`")));
}

#[test]
fn malformed_use_continues_to_next_one() {
    // First use is malformed (missing semicolon); the second one
    // should still be parsed thanks to recover_to_semicolon.
    let src = "pack a;\nuse b.c\nuse d.e;";
    let result = parse(src, FileId(0));
    assert!(!result.diagnostics.is_empty());
    let file = result.file.expect("recovery still yields a SourceFile");
    // The good `use d.e;` survived.
    assert!(file
        .uses
        .iter()
        .any(|u| u.path.segments.last().map(|s| s.name.as_str()) == Some("e")));
}

#[test]
fn function_item_parses_after_pack() {
    let src = "pack a;\nfunction main(): void {}";
    let file = parse_ok(src);
    assert_eq!(file.items.len(), 1);
    let phc_ast::Item::Function(f) = &file.items[0] else {
        panic!("expected Function, got {:?}", file.items[0]);
    };
    assert_eq!(f.name.name, "main");
}

#[test]
fn class_item_parses_after_pack() {
    let src = "pack a;\npublic class Foo {}";
    let file = parse_ok(src);
    assert_eq!(file.items.len(), 1);
    assert!(matches!(&file.items[0], phc_ast::Item::Class(_)));
}

#[test]
fn unknown_top_level_keyword_is_flagged() {
    let src = "pack a;\nasync $oops;";
    let diags = parse_err(src);
    assert!(diags
        .iter()
        .any(|d| d.message.contains("expected `function`")));
}
