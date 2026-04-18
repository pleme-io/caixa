//! `caixa-lsp` — Language Server Protocol bridge for tatara-lisp / caixa
//! sources.
//!
//! Capabilities (phase 1.B):
//!   - **Diagnostics** — re-run `caixa-lint` on every open/change, publish
//!     the rule violations as LSP `Diagnostic`s.
//!   - **Formatting** — pipe the document through `caixa-fmt` and return a
//!     single whole-document `TextEdit`.
//!   - **Document symbols** — list every top-level `(defX …)` as a symbol
//!     so `:Telescope lsp_document_symbols` / Goto-Symbol works.
//!   - **Hover** — show the `TataraDomain` docstring for the form under the
//!     cursor (best-effort; defaults to the form's head keyword).
//!
//! Transport: stdio. Launch via `caixa-lsp` with no arguments. Nvim wires
//! it up through `caixa.nvim`'s `lua/caixa/lsp.lua`.

use std::sync::Arc;

use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, Hover, HoverContents,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams,
    MarkedString, MessageType, OneOf, Position, Range, ServerCapabilities, ServerInfo, SymbolKind,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| CaixaLsp {
        client,
        documents: Arc::new(DashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

struct CaixaLsp {
    client: Client,
    documents: Arc<DashMap<Url, Rope>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for CaixaLsp {
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "caixa-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "caixa-lsp ready")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let rope = Rope::from_str(&params.text_document.text);
        self.documents.insert(uri.clone(), rope);
        self.publish_diagnostics(uri).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.pop() {
            let rope = Rope::from_str(&change.text);
            self.documents.insert(uri.clone(), rope);
            self.publish_diagnostics(uri).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let Some(rope) = self.documents.get(&uri) else {
            return Ok(None);
        };
        let src = rope.to_string();
        let cfg = caixa_fmt::FmtConfig::default();
        let Ok(formatted) = caixa_fmt::format_source(&src, &cfg) else {
            return Ok(None);
        };
        if formatted == src {
            return Ok(Some(vec![]));
        }
        let end_line = u32::try_from(rope.len_lines().saturating_sub(1)).unwrap_or(0);
        let end_char =
            u32::try_from(rope.line(rope.len_lines().saturating_sub(1)).len_chars()).unwrap_or(0);
        Ok(Some(vec![TextEdit {
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(end_line, end_char),
            },
            new_text: formatted,
        }]))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let Some(rope) = self.documents.get(&uri) else {
            return Ok(None);
        };
        let src = rope.to_string();
        let nodes = match caixa_ast::parse(&src) {
            Ok(n) => n,
            Err(_) => return Ok(None),
        };
        let mut out = Vec::new();
        for n in &nodes {
            if let Some(head) = n.head_symbol() {
                let range = span_to_range(n.span, &src);
                #[allow(deprecated)]
                let sym = DocumentSymbol {
                    name: head.to_string(),
                    detail: n.kwarg("nome").and_then(|v| match &v.kind {
                        caixa_ast::NodeKind::Str(s) | caixa_ast::NodeKind::Symbol(s) => {
                            Some(s.clone())
                        }
                        _ => None,
                    }),
                    kind: kind_for(head),
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                };
                out.push(sym);
            }
        }
        Ok(Some(DocumentSymbolResponse::Nested(out)))
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(rope) = self.documents.get(&uri) else {
            return Ok(None);
        };
        let src = rope.to_string();
        let offset = match position_to_offset(&src, params.text_document_position_params.position) {
            Some(o) => o,
            None => return Ok(None),
        };
        let nodes = match caixa_ast::parse(&src) {
            Ok(n) => n,
            Err(_) => return Ok(None),
        };
        for n in &nodes {
            if n.span.contains(offset) {
                if let Some(head) = n.head_symbol() {
                    return Ok(Some(Hover {
                        contents: HoverContents::Scalar(MarkedString::String(hover_doc(head))),
                        range: Some(span_to_range(n.span, &src)),
                    }));
                }
            }
        }
        Ok(None)
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }
}

impl CaixaLsp {
    async fn publish_diagnostics(&self, uri: Url) {
        let Some(rope) = self.documents.get(&uri) else {
            return;
        };
        let src = rope.to_string();
        let lint = match caixa_lint::lint_source(&src) {
            Ok(diags) => diags,
            Err(_) => return,
        };
        let lsp_diags: Vec<LspDiagnostic> = lint
            .into_iter()
            .map(|d| LspDiagnostic {
                range: span_to_range(d.span, &src),
                severity: Some(match d.severity {
                    caixa_lint::Severity::Error => DiagnosticSeverity::ERROR,
                    caixa_lint::Severity::Warning => DiagnosticSeverity::WARNING,
                    caixa_lint::Severity::Info => DiagnosticSeverity::INFORMATION,
                    caixa_lint::Severity::Hint => DiagnosticSeverity::HINT,
                }),
                source: Some("caixa-lint".into()),
                code: Some(tower_lsp::lsp_types::NumberOrString::String(
                    d.rule_id.to_string(),
                )),
                message: d.message,
                ..LspDiagnostic::default()
            })
            .collect();
        self.client.publish_diagnostics(uri, lsp_diags, None).await;
    }
}

fn span_to_range(span: caixa_ast::Span, src: &str) -> Range {
    Range {
        start: offset_to_position(src, span.start),
        end: offset_to_position(src, span.end),
    }
}

fn offset_to_position(src: &str, offset: u32) -> Position {
    let p = caixa_ast::line_column(src, offset);
    // LSP line/col are 0-indexed; caixa-ast is 1-indexed.
    Position::new(p.line.saturating_sub(1), p.column.saturating_sub(1))
}

fn position_to_offset(src: &str, pos: Position) -> Option<u32> {
    let mut offset = 0u32;
    let mut line = 0u32;
    let mut col = 0u32;
    for ch in src.chars() {
        if line == pos.line && col == pos.character {
            return Some(offset);
        }
        offset += u32::try_from(ch.len_utf8()).ok()?;
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Some(offset)
}

fn kind_for(head: &str) -> SymbolKind {
    match head {
        "defcaixa" | "deflacre" | "defflake" => SymbolKind::PACKAGE,
        "defteia" | "defteia-schema" => SymbolKind::STRUCT,
        "defarquitetura" => SymbolKind::MODULE,
        "defmonitor" | "defalertpolicy" | "defnotify" | "defpoint" => SymbolKind::EVENT,
        s if s.starts_with("def") => SymbolKind::FUNCTION,
        _ => SymbolKind::NAMESPACE,
    }
}

fn hover_doc(head: &str) -> String {
    match head {
        "defcaixa" => "**defcaixa** — caixa manifest.\n\nFields: :nome :versao :kind (Biblioteca | Binario | Servico) :edicao :descricao :deps :deps-dev :bibliotecas :exe :servicos".to_string(),
        "deflacre" => "**deflacre** — lock file.\n\nFields: :versao-lacre :raiz (BLAKE3 root) :entradas (per-dep resolved entries)".to_string(),
        "defflake" => "**defflake** — flake.lisp flake description.\n\nFields: :descricao :entradas :saidas".to_string(),
        "defteia" => "**defteia** — typed resource instance.\n\nFields: :tipo :nome :atributos".to_string(),
        "defarquitetura" => "**defarquitetura** — composable infra architecture.\n\nFields: :parametros :realizacao".to_string(),
        s => format!("**{s}** — user-defined TataraDomain form"),
    }
}
