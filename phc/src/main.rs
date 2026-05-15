// SPDX-License-Identifier: MIT
//! PHC compiler CLI entry point.
//!
//! `phc run <FILE>` lexes, parses, resolves, typechecks, and
//! tree-walks the program through `phc-interp` until codegen lands.
//! The other subcommands are still stubbed.

use clap::Parser;
use phc_build::build_file;
use phc_interp::{run as interp_run, RunOutput, Value};
use phc_parser::parse as parse_source;
use phc_semantic::resolve;
use phc_span::FileId;
use phc_typecheck::typecheck;
use std::path::PathBuf;
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
    /// Compile a PHC source file to a native binary via the C-emit
    /// backend.
    Build {
        /// Path to a `.phc` source file containing a `function main()`.
        file: PathBuf,
        /// Output binary path. Defaults to the source file's stem
        /// in the current directory (with `.exe` on Windows).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Type-check the current project without producing output
    Check,
    /// Run a PHC source file through the tree-walking interpreter
    Run {
        /// Path to a `.phc` source file containing a `function main()`
        file: PathBuf,
    },
    /// Run tests
    Test,
    /// Format source files
    Fmt,
    /// Run the linter
    Lint,
    /// Create a new PHC project
    New { name: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run { file }) => run_file(&file),
        Some(Command::Build { file, output }) => build_cmd(&file, output.as_deref()),
        Some(Command::Check) => {
            eprintln!("phc check: not yet implemented");
            ExitCode::from(1)
        }
        Some(Command::Test) => {
            eprintln!("phc test: not yet implemented");
            ExitCode::from(1)
        }
        Some(Command::Fmt) => {
            eprintln!("phc fmt: not yet implemented");
            ExitCode::from(1)
        }
        Some(Command::Lint) => {
            eprintln!("phc lint: not yet implemented");
            ExitCode::from(1)
        }
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

fn build_cmd(input: &std::path::Path, output: Option<&std::path::Path>) -> ExitCode {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("a");
    let default_output: PathBuf = if cfg!(windows) {
        PathBuf::from(format!("{stem}.exe"))
    } else {
        PathBuf::from(stem)
    };
    let output_path = output.unwrap_or(default_output.as_path());
    let result = build_file(input, output_path);
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
