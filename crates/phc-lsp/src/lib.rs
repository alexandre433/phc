// SPDX-License-Identifier: MIT
//! Language Server Protocol server for PHC.
//!
//! Minimal v0 surface (Phase 8 starter):
//! - text document sync (full): every `didOpen` / `didChange` runs
//!   the full pipeline (parse → resolve → typecheck → borrowcheck)
//!   and pushes diagnostics to the client.
//! - hover: returns the typechecker's recovered `Ty` for the
//!   smallest `expr_types` span containing the cursor, formatted
//!   via `Ty::display`. Falls back to "no type info" when the
//!   cursor is in dead air.
//!
//! Out of scope (tracked as follow-ups):
//! - Goto-definition / references (needs a span → SymbolId map
//!   the resolver can already serve, but the wire format is its
//!   own slice).
//! - Completion, signature help, code actions.
//! - Multi-file / project-aware analysis (today every document is
//!   analysed in isolation; cross-pack diagnostics need session
//!   plumbing the `phc-build` Session crate already owns).
//!
//! The server is started via [`run_stdio`] from the `phc lsp` CLI
//! subcommand; library callers (e.g. tests) drive [`Backend`]
//! directly through the trait methods.

mod analyze;
mod backend;

#[cfg(test)]
mod tests;

pub use backend::Backend;

use tower_lsp::{LspService, Server};

/// Run the LSP server over stdio. Blocks until the client closes
/// the connection. The `phc lsp` CLI command is a thin wrapper
/// around this entry point.
pub async fn run_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
