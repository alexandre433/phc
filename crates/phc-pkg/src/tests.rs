// SPDX-License-Identifier: MIT
//! Inline tests for the manifest reader.

use crate::{discover_sources, read_manifest, Manifest, ManifestError};
use std::collections::BTreeMap;

fn write_temp(name: &str, body: &str) -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from("target");
    p.push("phc-pkg-test");
    std::fs::create_dir_all(&p).expect("create scratch dir");
    p.push(format!("{name}.json"));
    std::fs::write(&p, body).expect("write phc.json");
    p
}

#[test]
fn minimal_manifest_parses() {
    let path = write_temp(
        "minimal",
        r#"{"name": "app", "version": "0.1.0", "edition": "2026"}"#,
    );
    let m = read_manifest(&path).expect("parse manifest");
    assert_eq!(m.name, "app");
    assert_eq!(m.version, "0.1.0");
    assert_eq!(m.edition, "2026");
    assert!(m.authors.is_empty());
    assert!(m.license.is_none());
    assert!(m.dependencies.is_empty());
}

#[test]
fn full_manifest_parses_with_optional_fields() {
    let path = write_temp(
        "full",
        r#"{
            "name": "app.web",
            "version": "1.2.3",
            "edition": "2026",
            "authors": ["Alex"],
            "license": "MIT",
            "repository": "https://github.com/x/y",
            "description": "test",
            "dependencies": { "json": "0.1.0", "http": "1.0.0" },
            "dev-dependencies": { "smoke": "0.0.1" }
        }"#,
    );
    let m = read_manifest(&path).expect("parse manifest");
    assert_eq!(m.authors, vec!["Alex".to_string()]);
    assert_eq!(m.license.as_deref(), Some("MIT"));
    assert_eq!(m.repository.as_deref(), Some("https://github.com/x/y"));
    assert_eq!(m.description.as_deref(), Some("test"));
    let mut want: BTreeMap<String, String> = BTreeMap::new();
    want.insert("json".into(), "0.1.0".into());
    want.insert("http".into(), "1.0.0".into());
    assert_eq!(m.dependencies, want);
    assert_eq!(
        m.dev_dependencies.get("smoke").map(String::as_str),
        Some("0.0.1")
    );
}

#[test]
fn missing_required_field_is_an_error() {
    let path = write_temp("missing", r#"{"version": "0.1.0", "edition": "2026"}"#);
    let err = read_manifest(&path).expect_err("should fail without name");
    assert!(matches!(err, ManifestError::Parse(_)));
}

#[test]
fn unknown_field_is_rejected() {
    let path = write_temp(
        "unknown",
        r#"{"name": "x", "version": "0.1.0", "edition": "2026", "extra": true}"#,
    );
    let err = read_manifest(&path).expect_err("should fail on unknown field");
    assert!(matches!(err, ManifestError::Parse(_)));
}

#[test]
fn missing_file_yields_io_error() {
    let path = std::path::PathBuf::from("target/phc-pkg-test/does-not-exist.json");
    let err = read_manifest(&path).expect_err("should fail on missing file");
    assert!(matches!(err, ManifestError::Io(_)));
}

fn discover_root(name: &str) -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from("target");
    p.push("phc-pkg-discover");
    p.push(name);
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("create discover root");
    p
}

#[test]
fn discover_collects_phc_files_recursively() {
    let root = discover_root("recursive");
    std::fs::write(root.join("a.phc"), "pack a;").unwrap();
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("sub").join("b.phc"), "pack a.b;").unwrap();
    std::fs::write(root.join("sub").join("c.txt"), "ignore").unwrap();

    let mut found = discover_sources(&root).unwrap();
    found.sort();
    assert_eq!(
        found,
        vec![root.join("a.phc"), root.join("sub").join("b.phc"),]
    );
}

#[test]
fn discover_skips_target_and_dotdirs() {
    let root = discover_root("skip");
    std::fs::write(root.join("good.phc"), "pack a;").unwrap();
    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::write(root.join("target").join("ignored.phc"), "pack a;").unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join(".git").join("ignored.phc"), "pack a;").unwrap();

    let found = discover_sources(&root).unwrap();
    assert_eq!(found, vec![root.join("good.phc")]);
}

#[test]
fn discover_returns_sorted_paths() {
    let root = discover_root("sorted");
    std::fs::write(root.join("z.phc"), "pack a;").unwrap();
    std::fs::write(root.join("a.phc"), "pack a;").unwrap();
    std::fs::write(root.join("m.phc"), "pack a;").unwrap();

    let found = discover_sources(&root).unwrap();
    assert_eq!(
        found,
        vec![root.join("a.phc"), root.join("m.phc"), root.join("z.phc"),]
    );
}

#[test]
fn round_trip_via_serde() {
    let path = write_temp(
        "round",
        r#"{"name":"x","version":"0.1.0","edition":"2026"}"#,
    );
    let m = read_manifest(&path).unwrap();
    let _: Manifest = m;
}
