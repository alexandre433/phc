// SPDX-License-Identifier: MIT
//! tower-lsp `LanguageServer` implementation. One [`Backend`] per
//! server process; per-document state lives behind an async Mutex
//! so concurrent LSP requests do not race.

use crate::analyze::{analyze, AnalysisOutput};
use phc_errors::{Diagnostic as PhcDiagnostic, Severity};
use phc_span::Span;
use phc_typecheck::Ty;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

/// Per-document snapshot kept after the last successful analysis.
/// `source` is the buffer the snapshot was computed from; used to
/// translate `(line, character)` positions into byte offsets so
/// hover lookups land on the right span.
struct DocumentState {
    source: String,
    output: AnalysisOutput,
}

pub struct Backend {
    client: Client,
    documents: Mutex<HashMap<Url, DocumentState>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Mutex::new(HashMap::new()),
        }
    }

    /// Run the pipeline for `uri` against `source`, store the
    /// snapshot, and push diagnostics to the client.
    async fn refresh(&self, uri: Url, source: String) {
        let output = analyze(&source);
        let lsp_diags: Vec<Diagnostic> = output
            .diagnostics
            .iter()
            .map(|d| to_lsp_diagnostic(&source, d))
            .collect();
        self.client
            .publish_diagnostics(uri.clone(), lsp_diags, None)
            .await;
        let mut docs = self.documents.lock().await;
        docs.insert(uri, DocumentState { source, output });
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "phc-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "phc-lsp ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.refresh(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // Full sync — the last content change is the entire buffer.
        if let Some(change) = params.content_changes.into_iter().last() {
            self.refresh(params.text_document.uri, change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut docs = self.documents.lock().await;
        docs.remove(&params.text_document.uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let position = params.text_document_position_params.position;
        let docs = self.documents.lock().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };
        let Some(typed) = doc.output.typed.as_ref() else {
            return Ok(None);
        };
        let Some(byte) = lsp_position_to_byte(&doc.source, position) else {
            return Ok(None);
        };
        // Find the smallest expr_types span containing `byte`. The
        // smallest span gives the most specific type — outer
        // expressions enclose inner ones.
        let mut best: Option<(Span, &Ty)> = None;
        for (span, ty) in &typed.expr_types {
            if span.lo as usize <= byte && byte < span.hi as usize {
                let width = span.hi - span.lo;
                let take = match best {
                    Some((b, _)) => width < (b.hi - b.lo),
                    None => true,
                };
                if take {
                    best = Some((*span, ty));
                }
            }
        }
        let Some((span, ty)) = best else {
            return Ok(None);
        };
        let contents = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::PlainText,
            value: format!("type: {}", ty.display()),
        });
        Ok(Some(Hover {
            contents,
            range: byte_span_to_lsp_range(&doc.source, span),
        }))
    }
}

/// Translate a phc-errors `Diagnostic` into the LSP wire shape.
/// Spans are byte ranges into the original source; convert to
/// (line, character) positions on the way out.
fn to_lsp_diagnostic(source: &str, d: &PhcDiagnostic) -> Diagnostic {
    let range = byte_span_to_lsp_range(source, d.span).unwrap_or(Range {
        start: Position::new(0, 0),
        end: Position::new(0, 0),
    });
    Diagnostic {
        range,
        severity: Some(match d.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Note => DiagnosticSeverity::INFORMATION,
        }),
        source: Some("phc".to_string()),
        message: d.message.clone(),
        ..Diagnostic::default()
    }
}

/// (line, character) → byte offset, treating `character` as a
/// UTF-8 code-unit count (LSP defaults to UTF-16; PHC tooling is
/// ASCII-heavy enough that the gap rarely surfaces, and UTF-16
/// support lands when the runtime grows real Unicode awareness).
fn lsp_position_to_byte(source: &str, pos: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut col = 0u32;
    for (idx, ch) in source.char_indices() {
        if line == pos.line && col == pos.character {
            return Some(idx);
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    if line == pos.line && col == pos.character {
        return Some(source.len());
    }
    None
}

fn byte_offset_to_lsp_position(source: &str, byte: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (idx, ch) in source.char_indices() {
        if idx >= byte {
            return Position::new(line, col);
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position::new(line, col)
}

fn byte_span_to_lsp_range(source: &str, span: Span) -> Option<Range> {
    Some(Range {
        start: byte_offset_to_lsp_position(source, span.lo as usize),
        end: byte_offset_to_lsp_position(source, span.hi as usize),
    })
}
