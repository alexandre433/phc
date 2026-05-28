// SPDX-License-Identifier: MIT
//! PHC compiler CLI entry point.
//!
//! `phc run <FILE>` lexes, parses, resolves, typechecks, and
//! tree-walks the program through `phc-interp` until codegen lands.
//! The other subcommands are still stubbed.

use clap::Parser;
use phc_borrowcheck::borrowcheck;
use phc_build::{
    build_file, build_project, check_pack_acyclicity, load_session, resolve_cross_pack_uses,
};
use phc_interp::{run as interp_run, RunOutput, Value};
use phc_parser::parse as parse_source;
use phc_pkg::read_manifest;
use phc_semantic::resolve;
use phc_span::FileId;
use phc_typecheck::typecheck;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "phc",
    about = "The PHC language compiler and toolchain",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Compile a PHC source to a native binary. Either pass a single
    /// `.phc` file, or run with no arguments inside a project that
    /// contains a `phc.json` (the file with `main()` is built).
    Build {
        /// Optional path to a single `.phc` source file. When omitted,
        /// the project root is the current directory.
        file: Option<PathBuf>,
        /// Output binary path. Defaults to the source file's stem
        /// (or the manifest name) in the current directory.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Run the multi-file pipeline (load + cross-pack uses +
    /// acyclicity) and report diagnostics. No binary output.
    Check {
        /// Project root. Defaults to the current directory.
        #[arg(default_value = ".")]
        root: PathBuf,
    },
    /// Run a PHC source file through the tree-walking interpreter
    Run {
        /// Path to a `.phc` source file containing a `function main()`
        file: PathBuf,
    },
    /// Start the PHC language server over stdio (LSP). Editors that
    /// speak LSP can spawn `phc lsp` and get diagnostics + hover
    /// types.
    Lsp,
    /// Run every `test "name" { ... }` block in a `.phc` file
    Test {
        /// Path to a `.phc` source file whose tests should be run
        file: PathBuf,
    },
    /// Reformat a PHC source file in place (or print to stdout
    /// with --check).
    Fmt {
        /// Path to a `.phc` source file
        file: PathBuf,
        /// Don't write — print the formatted source to stdout and
        /// exit non-zero if the file isn't already canonical.
        #[arg(long)]
        check: bool,
    },
    /// Run the D-036 v0a lint ruleset over a PHC source file.
    Lint {
        /// Path to a `.phc` source file
        file: PathBuf,
    },
    /// Create a new PHC project
    New { name: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run { file }) => run_file(&file),
        Some(Command::Lsp) => run_lsp(),
        Some(Command::Build { file, output }) => build_cmd(file.as_deref(), output.as_deref()),
        Some(Command::Check { root }) => check_cmd(&root),
        Some(Command::Test { file }) => test_cmd(&file),
        Some(Command::Fmt { file, check }) => fmt_cmd(&file, check),
        Some(Command::Lint { file }) => lint_cmd(&file),
        Some(Command::New { name }) => {
            eprintln!("phc new {name}: not yet implemented");
            ExitCode::from(1)
        }
        None => {
            eprintln!("PHC compiler — run with --help for usage");
            ExitCode::SUCCESS
        }
    }
}

fn build_cmd(input: Option<&Path>, output: Option<&Path>) -> ExitCode {
    let result = match input {
        Some(file) => {
            let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("a");
            let default_output: PathBuf = if cfg!(windows) {
                PathBuf::from(format!("{stem}.exe"))
            } else {
                PathBuf::from(stem)
            };
            let output_path = output.unwrap_or(default_output.as_path());
            build_file(file, output_path)
        }
        None => {
            let root = Path::new(".");
            let manifest_path = root.join("phc.json");
            if !manifest_path.exists() {
                eprintln!(
                    "phc build: no `phc.json` in `{}`. Pass a single source file or run inside a project root.",
                    root.display()
                );
                return ExitCode::from(1);
            }
            let manifest = match read_manifest(&manifest_path) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("phc build: cannot read `{}`: {e}", manifest_path.display());
                    return ExitCode::from(1);
                }
            };
            let stem = &manifest.name;
            let default_output: PathBuf = if cfg!(windows) {
                PathBuf::from(format!("{stem}.exe"))
            } else {
                PathBuf::from(stem)
            };
            let output_path = output.unwrap_or(default_output.as_path());
            build_project(root, output_path)
        }
    };
    for w in &result.warnings {
        eprintln!("warning at {}..{}: {}", w.span.lo, w.span.hi, w.message);
    }
    if !result.errors.is_empty() {
        for e in &result.errors {
            eprintln!("error at {}..{}: {}", e.span.lo, e.span.hi, e.message);
        }
        return ExitCode::from(1);
    }
    if let Some(bin) = &result.binary {
        eprintln!("phc build: produced `{}`", bin.display());
    }
    ExitCode::SUCCESS
}

fn check_cmd(root: &Path) -> ExitCode {
    let mut session = load_session(root);
    resolve_cross_pack_uses(&mut session);
    check_pack_acyclicity(&mut session);
    if session.diagnostics.is_empty() {
        eprintln!(
            "phc check: {} files, {} packs — no diagnostics",
            session.files.len(),
            session.packs.len()
        );
        return ExitCode::SUCCESS;
    }
    let mut errors = 0usize;
    let mut warnings = 0usize;
    for d in &session.diagnostics {
        let label = match d.severity {
            phc_errors::Severity::Error => {
                errors += 1;
                "error"
            }
            phc_errors::Severity::Warning => {
                warnings += 1;
                "warning"
            }
            phc_errors::Severity::Note => "note",
        };
        eprintln!("{label} at {}..{}: {}", d.span.lo, d.span.hi, d.message);
    }
    eprintln!("phc check: {errors} error(s), {warnings} warning(s)");
    if errors > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Run every `test "name" { ... }` block in `path` and print a
/// pass/fail report. Exit non-zero on any failure or pipeline
/// diagnostic so CI scripts can branch on outcome.
fn test_cmd(path: &Path) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("phc test: cannot read `{}`: {e}", path.display());
            return ExitCode::from(2);
        }
    };
    let report = phc_test::run_source(&source);
    for d in &report.setup_diagnostics {
        eprintln!(
            "{kind} at {}..{}: {}",
            d.span.lo,
            d.span.hi,
            d.message,
            kind = match d.severity {
                phc_errors::Severity::Error => "error",
                phc_errors::Severity::Warning => "warning",
                phc_errors::Severity::Note => "note",
            }
        );
    }
    if !report.setup_diagnostics.is_empty() {
        eprintln!("phc test: setup diagnostics present, no tests run");
        return ExitCode::from(1);
    }
    for r in &report.results {
        if r.passed {
            println!("PASS  {}", r.name);
        } else {
            println!("FAIL  {}", r.name);
            for e in &r.errors {
                println!("        {}", e.message);
            }
        }
    }
    println!(
        "\nphc test: {} passed, {} failed",
        report.passed, report.failed
    );
    if report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Read `path`, run the D-036 v0a lint ruleset, and print every
/// warning + setup error. Exits non-zero on any setup error or
/// at least one warning so CI scripts can gate on cleanliness.
fn lint_cmd(path: &Path) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("phc lint: cannot read `{}`: {e}", path.display());
            return ExitCode::from(2);
        }
    };
    let report = phc_lint::lint_source(&source);
    for d in &report.setup_errors {
        eprintln!("error at {}..{}: {}", d.span.lo, d.span.hi, d.message);
    }
    for d in &report.warnings {
        eprintln!("warning at {}..{}: {}", d.span.lo, d.span.hi, d.message);
    }
    if !report.setup_errors.is_empty() {
        eprintln!(
            "phc lint: setup errors prevented analysis of `{}`",
            path.display()
        );
        return ExitCode::from(1);
    }
    if report.warnings.is_empty() {
        eprintln!("phc lint: `{}` — clean", path.display());
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "phc lint: `{}` — {} warning(s)",
            path.display(),
            report.warnings.len()
        );
        ExitCode::from(1)
    }
}

/// Read `path`, run the token-stream formatter (D-035), and
/// either write the result back or (with `check`) print to stdout
/// and exit non-zero if the file wasn't already canonical.
fn fmt_cmd(path: &Path, check: bool) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("phc fmt: cannot read `{}`: {e}", path.display());
            return ExitCode::from(2);
        }
    };
    let formatted = match phc_fmt::format(&source) {
        Ok(s) => s,
        Err(diags) => {
            for d in &diags {
                eprintln!("fmt error at {}..{}: {}", d.span.lo, d.span.hi, d.message);
            }
            return ExitCode::from(1);
        }
    };
    if check {
        print!("{formatted}");
        if source == formatted {
            ExitCode::SUCCESS
        } else {
            eprintln!("phc fmt: `{}` would change", path.display());
            ExitCode::from(1)
        }
    } else if source == formatted {
        ExitCode::SUCCESS
    } else {
        match std::fs::write(path, &formatted) {
            Ok(()) => {
                eprintln!("phc fmt: reformatted `{}`", path.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("phc fmt: cannot write `{}`: {e}", path.display());
                ExitCode::from(1)
            }
        }
    }
}

/// Spin up a tokio runtime and serve the LSP over stdio. Blocks
/// until the client closes the connection. Errors from the
/// runtime bubble up as a non-zero exit code.
fn run_lsp() -> ExitCode {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("phc lsp: cannot start tokio runtime: {e}");
            return ExitCode::from(1);
        }
    };
    runtime.block_on(phc_lsp::run_stdio());
    ExitCode::SUCCESS
}

fn run_file(path: &PathBuf) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("phc run: cannot read `{}`: {e}", path.display());
            return ExitCode::from(2);
        }
    };

    let parsed = parse_source(&source, FileId(0));
    if !parsed.diagnostics.is_empty() {
        for d in &parsed.diagnostics {
            eprintln!("parse error at {}..{}: {}", d.span.lo, d.span.hi, d.message);
        }
        return ExitCode::from(1);
    }
    let Some(file_ast) = parsed.file else {
        eprintln!("phc run: parser produced no SourceFile");
        return ExitCode::from(1);
    };

    let resolved = resolve(&file_ast);
    if !resolved.diagnostics.is_empty() {
        for d in &resolved.diagnostics {
            eprintln!(
                "resolve error at {}..{}: {}",
                d.span.lo, d.span.hi, d.message
            );
        }
        return ExitCode::from(1);
    }

    let typed = typecheck(&file_ast, &resolved);
    if !typed.diagnostics.is_empty() {
        for d in &typed.diagnostics {
            eprintln!("type error at {}..{}: {}", d.span.lo, d.span.hi, d.message);
        }
        return ExitCode::from(1);
    }
    let borrowed = borrowcheck(&file_ast, &resolved, &typed);
    if !borrowed.diagnostics.is_empty() {
        for d in &borrowed.diagnostics {
            eprintln!(
                "borrow error at {}..{}: {}",
                d.span.lo, d.span.hi, d.message
            );
        }
        return ExitCode::from(1);
    }
    let RunOutput {
        stdout,
        result,
        errors,
    } = interp_run(&file_ast, &resolved, &typed);

    for line in &stdout {
        println!("{line}");
    }

    if !errors.is_empty() {
        for e in &errors {
            eprintln!("runtime error: {}", e.message);
        }
        return ExitCode::from(1);
    }

    // Non-zero exit on a Result/Option failure surfaced as the
    // program's result (so shell scripts can branch on outcome).
    match result {
        Some(Value::ResultErr(_)) | Some(Value::OptionNone) => ExitCode::from(1),
        _ => ExitCode::SUCCESS,
    }
}
