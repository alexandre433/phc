// SPDX-License-Identifier: MIT
//! Multi-file project session.
//!
//! Walks a project root, lexes + parses every `.phc` file, and
//! groups the results by the `pack <path>;` declaration each file
//! ships with. Cross-pack name resolution and pack acyclicity
//! enforcement live in follow-up commits; this module's job is
//! just to get the corpus loaded and indexed.

use phc_ast::SourceFile;
use phc_errors::{Diagnostic, Severity};
use phc_parser::parse;
use phc_pkg::discover_sources;
use phc_semantic::{resolve, Resolved};
use phc_span::{FileId, Span};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One parsed source file, plus its per-file resolver output.
#[derive(Debug)]
pub struct LoadedFile {
    pub file_id: FileId,
    pub path: PathBuf,
    pub source: String,
    pub ast: SourceFile,
    pub resolved: Resolved,
    /// Dotted pack path joined with `.` — e.g. `app.auth`.
    pub pack: String,
}

/// All files in the project plus their by-pack grouping.
#[derive(Debug, Default)]
pub struct Session {
    pub files: Vec<LoadedFile>,
    /// Pack name → indices into `files`. Sorted on insert so iteration
    /// is deterministic.
    pub packs: BTreeMap<String, Vec<usize>>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Session {
    pub fn ok(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|d| !matches!(d.severity, Severity::Error))
    }
}

/// Load every `.phc` file under `root`, parse + resolve each one,
/// and group them by their `pack` declaration. Failures (I/O, parse
/// errors, missing pack decl) accumulate in the returned Session's
/// diagnostics.
pub fn load_session(root: &Path) -> Session {
    let mut session = Session::default();
    let paths = match discover_sources(root) {
        Ok(p) => p,
        Err(e) => {
            session.diagnostics.push(diag_error(format!(
                "cannot walk project root `{}`: {e}",
                root.display()
            )));
            return session;
        }
    };
    for (idx, path) in paths.iter().enumerate() {
        let file_id = FileId(idx as u32);
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                session
                    .diagnostics
                    .push(diag_error(format!("cannot read `{}`: {e}", path.display())));
                continue;
            }
        };
        let parsed = parse(&source, file_id);
        if !parsed.diagnostics.is_empty() {
            session.diagnostics.extend(parsed.diagnostics);
            continue;
        }
        let Some(ast) = parsed.file else {
            session.diagnostics.push(diag_error(format!(
                "parser produced no SourceFile for `{}`",
                path.display()
            )));
            continue;
        };
        let pack = ast
            .pack
            .path
            .segments
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>()
            .join(".");
        let resolved = resolve(&ast);
        if !resolved.diagnostics.is_empty() {
            session.diagnostics.extend(resolved.diagnostics.clone());
        }
        session.files.push(LoadedFile {
            file_id,
            path: path.clone(),
            source,
            ast,
            resolved,
            pack,
        });
    }
    // Group by pack.
    for (idx, file) in session.files.iter().enumerate() {
        session
            .packs
            .entry(file.pack.clone())
            .or_default()
            .push(idx);
    }
    session
}

fn diag_error(message: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message,
        span: Span::new(FileId(0), 0, 0),
    }
}
