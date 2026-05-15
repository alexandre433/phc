// SPDX-License-Identifier: MIT
//! PHC compiler CLI entry point.

// TODO(phase-7): wire real subcommands as the build system lands.

use clap::Parser;

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
    /// Build the current project
    Build,
    /// Type-check the current project without producing output
    Check,
    /// Run the current project
    Run,
    /// Run tests
    Test,
    /// Format source files
    Fmt,
    /// Run the linter
    Lint,
    /// Create a new PHC project
    New { name: String },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Build) => eprintln!("phc build: not yet implemented"),
        Some(Command::Check) => eprintln!("phc check: not yet implemented"),
        Some(Command::Run) => eprintln!("phc run: not yet implemented"),
        Some(Command::Test) => eprintln!("phc test: not yet implemented"),
        Some(Command::Fmt) => eprintln!("phc fmt: not yet implemented"),
        Some(Command::Lint) => eprintln!("phc lint: not yet implemented"),
        Some(Command::New { name }) => eprintln!("phc new {name}: not yet implemented"),
        None => eprintln!("PHC compiler — run with --help for usage"),
    }
}
