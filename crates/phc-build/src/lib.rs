// SPDX-License-Identifier: MIT
//! Compile-pipeline orchestrator.
//!
//! `build_file(input, output)` walks one PHC source file through
//! lexer → parser → resolver → typechecker → C-emit, writes the
//! generated C alongside the bundled `phc_runtime.{h,c}` to a
//! per-input scratch directory under `target/phc-build/`, invokes
//! a system C compiler, and produces a native binary at the
//! caller-chosen output path.
//!
//! Pack-level caching, parallel compilation, and the proper
//! manifest (`phc.json`) workflow are Phase 7 concerns. This MVP
//! handles a single file.

mod session;
pub use session::{
    check_pack_acyclicity, load_session, resolve_cross_pack_uses, ImportTarget, LoadedFile, Session,
};

use phc_ast::{Ident, PackDecl, PackPath, SourceFile};
use phc_borrowcheck::borrowcheck;
use phc_codegen::emit_c;
use phc_errors::{Diagnostic, Severity};
use phc_parser::parse;
use phc_semantic::resolve;
use phc_span::{FileId, Span};
use phc_typecheck::typecheck;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Outcome of compiling one source file.
#[derive(Debug, Default)]
pub struct BuildResult {
    /// Errors that prevented producing a binary (parse error,
    /// resolver error, codegen error, C compiler failure).
    pub errors: Vec<Diagnostic>,
    /// Warnings — codegen-time TODOs etc. — that did not abort
    /// the build.
    pub warnings: Vec<Diagnostic>,
    /// Path to the produced binary on success.
    pub binary: Option<PathBuf>,
}

impl BuildResult {
    pub fn ok(&self) -> bool {
        self.binary.is_some() && self.errors.is_empty()
    }
}

/// Drive the full compile pipeline for `input`. The produced
/// binary is written to `output`; intermediate `.c` and the
/// runtime sources live under `target/phc-build/<basename>/`.
///
/// The C compiler is picked from `$CC` if set, else `cc`.
pub fn build_file(input: &Path, output: &Path) -> BuildResult {
    let mut result = BuildResult::default();

    let source = match fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            result.errors.push(diag_error(format!(
                "cannot read input `{}`: {e}",
                input.display()
            )));
            return result;
        }
    };

    let parsed = parse(&source, FileId(0));
    if !parsed.diagnostics.is_empty() {
        result.errors.extend(parsed.diagnostics);
        return result;
    }
    let Some(file_ast) = parsed.file else {
        result
            .errors
            .push(diag_error("parser produced no SourceFile".into()));
        return result;
    };

    let resolved = resolve(&file_ast);
    if !resolved.diagnostics.is_empty() {
        result.errors.extend(resolved.diagnostics);
        return result;
    }

    let typed = typecheck(&file_ast, &resolved);
    if !typed.diagnostics.is_empty() {
        result.errors.extend(typed.diagnostics);
        return result;
    }

    let borrowed = borrowcheck(&file_ast, &resolved, &typed);
    if !borrowed.diagnostics.is_empty() {
        result.errors.extend(borrowed.diagnostics);
        return result;
    }

    let codegen = emit_c(&file_ast, &resolved, &typed);
    result.warnings.extend(codegen.diagnostics.iter().cloned());

    let scratch = match scratch_dir_for(input) {
        Ok(dir) => dir,
        Err(e) => {
            result.errors.push(diag_error(e));
            return result;
        }
    };
    let program_c = scratch.join("program.c");
    let runtime_c = scratch.join(phc_runtime::SOURCE_NAME);
    let runtime_h = scratch.join(phc_runtime::HEADER_NAME);

    if let Err(e) = fs::write(&program_c, &codegen.c_source) {
        result.errors.push(diag_error(format!(
            "cannot write `{}`: {e}",
            program_c.display()
        )));
        return result;
    }
    if let Err(e) = fs::write(&runtime_c, phc_runtime::SOURCE) {
        result.errors.push(diag_error(format!(
            "cannot write `{}`: {e}",
            runtime_c.display()
        )));
        return result;
    }
    if let Err(e) = fs::write(&runtime_h, phc_runtime::HEADER) {
        result.errors.push(diag_error(format!(
            "cannot write `{}`: {e}",
            runtime_h.display()
        )));
        return result;
    }

    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let status = Command::new(&cc)
        .args(["-std=c11", "-O2"])
        .arg(format!("-I{}", scratch.display()))
        .arg("-o")
        .arg(output)
        .arg(&program_c)
        .arg(&runtime_c)
        .output();

    match status {
        Err(e) => {
            result
                .errors
                .push(diag_error(format!("failed to invoke `{cc}`: {e}")));
        }
        Ok(out) if !out.status.success() => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            result.errors.push(diag_error(format!(
                "C compiler failed (exit {}):\n{stderr}",
                out.status.code().unwrap_or(-1)
            )));
        }
        Ok(_) => {
            result.binary = Some(output.to_path_buf());
        }
    }

    result
}

/// Build an entire project rooted at `root` into a single binary
/// at `output`. Drives the full multi-file pipeline: load + cross-
/// pack uses + acyclicity, then concatenates every file's items
/// into a single combined SourceFile so the existing codegen can
/// emit one C translation unit. Names are global at C level
/// (every emitted symbol is prefixed `phc_<...>`), so cross-pack
/// dispatch works without separate compilation.
pub fn build_project(root: &Path, output: &Path) -> BuildResult {
    let mut result = BuildResult::default();
    let mut session = load_session(root);
    resolve_cross_pack_uses(&mut session);
    check_pack_acyclicity(&mut session);
    if !session.ok() {
        result.errors.extend(session.diagnostics.iter().cloned());
        return result;
    }
    let combined = combine_session(&session);
    let resolved = resolve(&combined);
    if !resolved.diagnostics.is_empty() {
        result.errors.extend(resolved.diagnostics);
        return result;
    }
    let typed = typecheck(&combined, &resolved);
    if !typed.diagnostics.is_empty() {
        result.errors.extend(typed.diagnostics);
        return result;
    }
    let borrowed = borrowcheck(&combined, &resolved, &typed);
    if !borrowed.diagnostics.is_empty() {
        result.errors.extend(borrowed.diagnostics);
        return result;
    }
    let codegen = emit_c(&combined, &resolved, &typed);
    result.warnings.extend(codegen.diagnostics.iter().cloned());

    let stem = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    let scratch = match scratch_dir_named(stem) {
        Ok(dir) => dir,
        Err(e) => {
            result.errors.push(diag_error(e));
            return result;
        }
    };
    if let Err(e) = invoke_cc(&scratch, &codegen.c_source, output) {
        result.errors.push(diag_error(e));
        return result;
    }
    result.binary = Some(output.to_path_buf());
    result
}

/// Collapse every file's items into one synthetic SourceFile.
/// The pack declaration is irrelevant for codegen because every
/// emitted symbol is mangled with its source name; the resolver
/// only needs a complete top_level table.
fn combine_session(session: &Session) -> SourceFile {
    let mut items = Vec::new();
    for file in &session.files {
        items.extend(file.ast.items.clone());
    }
    let synthetic_span = Span::new(FileId(0), 0, 0);
    SourceFile {
        pack: PackDecl {
            path: PackPath {
                segments: vec![Ident {
                    name: "project".to_string(),
                    span: synthetic_span,
                }],
                span: synthetic_span,
            },
            span: synthetic_span,
        },
        // Drop cross-file `use` decls — combined file has every
        // name in scope already. Unresolved imports were already
        // surfaced by resolve_cross_pack_uses.
        uses: Vec::new(),
        items,
        span: synthetic_span,
    }
}

fn scratch_dir_named(stem: &str) -> Result<PathBuf, String> {
    let dir = PathBuf::from("target").join("phc-build").join(stem);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create scratch dir `{}`: {e}", dir.display()))?;
    Ok(dir)
}

/// Write program.c + runtime sources to `scratch` and invoke the
/// C compiler. Shared by build_file and build_project so the
/// invocation flags + error mapping stay in lock-step.
fn invoke_cc(scratch: &Path, c_source: &str, output: &Path) -> Result<(), String> {
    let program_c = scratch.join("program.c");
    let runtime_c = scratch.join(phc_runtime::SOURCE_NAME);
    let runtime_h = scratch.join(phc_runtime::HEADER_NAME);
    fs::write(&program_c, c_source)
        .map_err(|e| format!("cannot write `{}`: {e}", program_c.display()))?;
    fs::write(&runtime_c, phc_runtime::SOURCE)
        .map_err(|e| format!("cannot write `{}`: {e}", runtime_c.display()))?;
    fs::write(&runtime_h, phc_runtime::HEADER)
        .map_err(|e| format!("cannot write `{}`: {e}", runtime_h.display()))?;
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let status = Command::new(&cc)
        .args(["-std=c11", "-O2"])
        .arg(format!("-I{}", scratch.display()))
        .arg("-o")
        .arg(output)
        .arg(&program_c)
        .arg(&runtime_c)
        .output()
        .map_err(|e| format!("failed to invoke `{cc}`: {e}"))?;
    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(format!(
            "C compiler failed (exit {}):\n{stderr}",
            status.status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

fn scratch_dir_for(input: &Path) -> Result<PathBuf, String> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("invalid input filename: `{}`", input.display()))?;
    // Place the scratch under the workspace's `target/phc-build/`
    // dir. Falling back to the system temp dir would also work but
    // keeping it under target/ makes the artefacts inspectable
    // without crawling temp paths.
    let dir = PathBuf::from("target").join("phc-build").join(stem);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create scratch dir `{}`: {e}", dir.display()))?;
    Ok(dir)
}

fn diag_error(message: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message,
        span: Span::new(FileId(0), 0, 0),
    }
}

#[cfg(test)]
mod tests;
