use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use ghl_diagnostics::DiagnosticSeverity as GhlSeverity;
use ghl_syntax::ast::Program;
use ghl_syntax::parser::parse_spanned;
use ghl_syntax::source::SourceIndex;

#[derive(Debug, Clone)]
pub struct DocumentState {
    pub text: String,
    pub index: SourceIndex,
    pub program: Option<Program>,
}

#[derive(Debug)]
pub struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, DocumentState>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn compute_diagnostics(uri: &Url, text: &str) -> (Vec<Diagnostic>, Option<Program>) {
        let index = SourceIndex::new(text);
        let mut diagnostics = Vec::new();

        match parse_spanned(text) {
            Err(syntax_errors) => {
                for err in syntax_errors {
                    let ((start_line, start_col), (end_line, end_col)) =
                        index.span_to_range(&err.span);
                    let range = Range {
                        start: Position::new(start_line, start_col),
                        end: Position::new(end_line, end_col.max(start_col + 1)),
                    };

                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String("C0001".to_string())),
                        code_description: None,
                        source: Some("ghl".to_string()),
                        message: format!(
                            "(ノ°□°)ノ [Compute Error C0001]: Syntax Error: {}",
                            err.message
                        ),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                }
                (diagnostics, None)
            }
            Ok(program) => {
                let filename = uri.path();
                if let Err(type_diagnostics) = ghl_types::check(&program, filename, text) {
                    for d in type_diagnostics {
                        let range = if let Some(span) = &d.span {
                            let ((sl, sc), (el, ec)) = index.span_to_range(span);
                            Range {
                                start: Position::new(sl, sc),
                                end: Position::new(el, ec.max(sc + 1)),
                            }
                        } else if let Some(loc) = &d.location {
                            let line = loc.line.saturating_sub(1) as u32;
                            let col = loc.column.saturating_sub(1) as u32;
                            Range {
                                start: Position::new(line, col),
                                end: Position::new(line, col + 1),
                            }
                        } else {
                            Range {
                                start: Position::new(0, 0),
                                end: Position::new(0, 1),
                            }
                        };

                        let severity = match d.severity {
                            GhlSeverity::ComputeError | GhlSeverity::StatisticalError => {
                                DiagnosticSeverity::ERROR
                            }
                            GhlSeverity::StatisticalWarning => DiagnosticSeverity::WARNING,
                            GhlSeverity::Info => DiagnosticSeverity::INFORMATION,
                            GhlSeverity::Success => DiagnosticSeverity::HINT,
                        };

                        let kaomoji = d.severity.kaomoji();
                        let prefix = d.severity.prefix();
                        let mut msg = format!("{kaomoji} [{prefix} {}]: {}", d.code, d.message);
                        if let Some(help) = &d.help {
                            msg.push_str(&format!("\nHelp: {}", help));
                        }

                        diagnostics.push(Diagnostic {
                            range,
                            severity: Some(severity),
                            code: Some(NumberOrString::String(d.code)),
                            code_description: None,
                            source: Some("ghl".to_string()),
                            message: msg,
                            related_information: None,
                            tags: None,
                            data: None,
                        });
                    }
                }
                (diagnostics, Some(program))
            }
        }
    }

    async fn validate_document(&self, uri: &Url, text: &str) {
        let index = SourceIndex::new(text);
        let (diagnostics, program) = Self::compute_diagnostics(uri, text);

        let mut docs = self.documents.write().await;
        if let Some(doc) = docs.get_mut(uri) {
            doc.text = text.to_string();
            doc.index = index;
            if program.is_some() {
                doc.program = program;
            }
        } else {
            docs.insert(
                uri.clone(),
                DocumentState {
                    text: text.to_string(),
                    index,
                    program,
                },
            );
        }
        drop(docs);

        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
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
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        ":".to_string(),
                        ">".to_string(),
                    ]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "ghl-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(
                MessageType::INFO,
                "GHL Language Server initialized ฅ(•⩊ •マ",
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.validate_document(&params.text_document.uri, &params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.validate_document(&params.text_document.uri, &change.text)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut docs = self.documents.write().await;
        docs.remove(&params.text_document.uri);
        // Clear diagnostics upon close
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let hover = crate::intel::compute_hover(
            &doc.text,
            pos.line as usize,
            pos.character as usize,
            doc.program.as_ref(),
        );

        Ok(hover)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let docs = self.documents.read().await;
        let prog = docs.get(uri).and_then(|d| d.program.as_ref());
        let items = crate::intel::compute_completions(prog);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let loc = crate::intel::compute_definition(
            uri,
            &doc.text,
            &doc.index,
            pos.line as usize,
            pos.character as usize,
            doc.program.as_ref(),
        );

        Ok(loc.map(GotoDefinitionResponse::Scalar))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        match ghl_syntax::format_source(&doc.text) {
            Ok(formatted) => {
                if formatted == doc.text {
                    return Ok(Some(vec![]));
                }
                let ((_, _), (end_line, end_col)) = doc.index.span_to_range(&(0..doc.text.len()));
                let range = Range {
                    start: Position::new(0, 0),
                    end: Position::new(end_line, end_col),
                };
                Ok(Some(vec![TextEdit {
                    range,
                    new_text: formatted,
                }]))
            }
            Err(_) => Ok(None),
        }
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        let range = params.range;
        let docs = self.documents.read().await;
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        match ghl_syntax::format_range(
            &doc.text,
            range.start.line as usize,
            range.end.line as usize,
        ) {
            Ok(Some((text_range, formatted_text))) => {
                let ((sl, sc), _) = doc
                    .index
                    .span_to_range(&(text_range.start_offset..text_range.start_offset));
                let ((el, ec), _) = doc
                    .index
                    .span_to_range(&(text_range.end_offset..text_range.end_offset));
                Ok(Some(vec![TextEdit {
                    range: Range {
                        start: Position::new(sl, sc),
                        end: Position::new(el, ec),
                    },
                    new_text: formatted_text,
                }]))
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_diagnostics_clean_code() {
        let uri = Url::parse("file:///test.gh").unwrap();
        let code = r#"
            let x = 42;
            let y = x + 10;
        "#;
        let (diags, prog) = Backend::compute_diagnostics(&uri, code);
        assert!(prog.is_some(), "AST should be generated");
        assert_eq!(diags.len(), 0, "Clean code should produce no diagnostics");
    }

    #[test]
    fn test_compute_diagnostics_syntax_error() {
        let uri = Url::parse("file:///test.gh").unwrap();
        let code = "let x = ;";
        let (diags, prog) = Backend::compute_diagnostics(&uri, code);
        assert!(
            prog.is_none(),
            "AST should not be generated for syntax error"
        );
        assert!(!diags.is_empty(), "Should produce syntax diagnostics");
        let diag = &diags[0];
        assert_eq!(diag.severity, Some(DiagnosticSeverity::ERROR));
        assert!(diag.message.contains("(ノ°□°)ノ"));
        assert!(diag.message.contains("[Compute Error C0001]"));
    }

    #[test]
    fn test_compute_diagnostics_type_error() {
        let uri = Url::parse("file:///test.gh").unwrap();
        let code = "let x: i64 = 3.14;";
        let (diags, prog) = Backend::compute_diagnostics(&uri, code);
        assert!(
            prog.is_some(),
            "Syntax is valid, so AST should be generated"
        );
        assert!(!diags.is_empty(), "Should produce type error diagnostic");
        let diag = &diags[0];
        assert_eq!(diag.severity, Some(DiagnosticSeverity::ERROR));
        assert!(diag.message.contains("C0102"));
    }

    #[tokio::test]
    async fn test_lsp_formatting_handler() {
        let (service, _) = tower_lsp::LspService::new(|client| Backend::new(client));
        let backend = service.inner();
        let uri = Url::parse("file:///test_fmt.gh").unwrap();
        let unformatted = "// Module note\nlet x=1+2*3; // inline\n";
        backend.validate_document(&uri, unformatted).await;

        let edits = backend
            .formatting(DocumentFormattingParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                options: FormattingOptions {
                    tab_size: 4,
                    insert_spaces: true,
                    ..Default::default()
                },
                work_done_progress_params: Default::default(),
            })
            .await
            .expect("formatting result ok")
            .expect("has edits");

        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0].new_text,
            "// Module note\nlet x = 1 + 2 * 3; // inline\n"
        );
    }

    #[tokio::test]
    async fn test_lsp_range_formatting_handler() {
        let (service, _) = tower_lsp::LspService::new(|client| Backend::new(client));
        let backend = service.inner();
        let uri = Url::parse("file:///test_range_fmt.gh").unwrap();
        let code = "let a = 1;\nlet b=2+3;\nlet c = 4;\n";
        backend.validate_document(&uri, code).await;

        let edits = backend
            .range_formatting(DocumentRangeFormattingParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                range: Range {
                    start: Position::new(1, 0),
                    end: Position::new(1, 10),
                },
                options: FormattingOptions {
                    tab_size: 4,
                    insert_spaces: true,
                    ..Default::default()
                },
                work_done_progress_params: Default::default(),
            })
            .await
            .expect("range formatting result ok")
            .expect("has edits");

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "let b = 2 + 3;\n");
    }
}
