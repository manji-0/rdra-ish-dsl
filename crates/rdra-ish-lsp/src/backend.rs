use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rdra_ish_core::{analyze_project, load_project, Diagnostic, ProjectError, WorkspaceAnalysis};
use tokio::sync::{Mutex, RwLock};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use url::Url;

use crate::code_actions::code_actions;
use crate::code_fingerprints::{code_notes, CodeNote};
use crate::code_lens::code_lenses;
use crate::completion::completion_items;
use crate::convert::{full_document_range, position_to_byte_offset, span_to_range};
use crate::document::{open_document, OpenDocument};
use crate::folding::folding_ranges;
use crate::hover::{hover_content, signature_help};
use crate::inlay_hints::inlay_hints;
use crate::lifecycle::{self, Findings, Settings};
use crate::linked_editing::linked_editing_ranges;
use crate::refs::{find_symbol_references, reference_at_offset, resolve_decl_site, symbol_target};
use crate::rename::{prepare_rename_range, workspace_rename, RenameError};
use crate::semantic_tokens::{semantic_tokens, TOKEN_MODIFIERS, TOKEN_TYPES};
use crate::symbols::{document_symbols, workspace_symbols};
use crate::uri::{watched_path_is_rdra, ClientUris};

#[derive(Default)]
struct ServerState {
    documents: HashMap<PathBuf, String>,
    workspace_roots: Vec<PathBuf>,
    analysis: Option<Arc<WorkspaceAnalysis>>,
    published: HashSet<Url>,
    /// The diagnostics of the analysis last published, per file: a background check's
    /// findings are published together with them.
    analysis_published: HashMap<Url, Vec<tower_lsp::lsp_types::Diagnostic>>,
    /// The findings of the background check of `analysis`, once it has published them.
    lifecycle: Option<Findings>,
    /// `lifecycleCheck` of the initialization options and the settings.
    settings: Settings,
    /// The client takes `workspace/codeLens/refresh`.
    lens_refresh: bool,
    manifest_error: Option<String>,
    /// The URIs the client names the files by, which results are sent under.
    uris: ClientUris,
}

/// An analyzed document a request names: the analysis, its path and its source entry.
struct Located<'a> {
    analysis: &'a WorkspaceAnalysis,
    path: PathBuf,
    doc: OpenDocument<'a>,
}

impl ServerState {
    /// The document `uri` names, or `None` when it is not a file URI, nothing is analyzed
    /// yet, or the analysis has no such source.
    fn document(&self, uri: &Url) -> Option<Located<'_>> {
        let path = uri_to_path(uri).ok()?;
        let analysis = self.analysis.as_ref()?;
        let doc = open_document(analysis, &path)?;
        Some(Located {
            analysis,
            path,
            doc,
        })
    }

    /// `document`, plus the byte offset of an LSP cursor position inside it.
    fn cursor(&self, uri: &Url, position: Position) -> Option<(Located<'_>, usize)> {
        let found = self.document(uri)?;
        let offset = position_to_byte_offset(found.doc.text, position)?;
        Some((found, offset))
    }
}

pub struct Backend {
    client: Client,
    state: Arc<RwLock<ServerState>>,
    /// Counts refreshes. A refresh publishes only while it is still the latest one, so a
    /// slower refresh of older text cannot overwrite the result of a newer one. The
    /// background check of a refresh follows the same rule.
    generation: Arc<AtomicU64>,
    /// Held from the generation check to the end of publishing.
    publishing: Arc<Mutex<()>>,
    /// Held by the background check that runs, so that checks run one at a time.
    checking: Arc<Mutex<()>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(RwLock::new(ServerState::default())),
            generation: Arc::new(AtomicU64::new(0)),
            publishing: Arc::new(Mutex::new(())),
            checking: Arc::new(Mutex::new(())),
        }
    }

    fn checker(&self) -> Checker {
        Checker {
            client: self.client.clone(),
            state: self.state.clone(),
            generation: self.generation.clone(),
            publishing: self.publishing.clone(),
            checking: self.checking.clone(),
        }
    }

    async fn refresh_analysis(&self) {
        let state = self.state.read().await;
        // Taken under the read lock, so a refresh that saw newer documents has a later number.
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mut inputs: Vec<PathBuf> = state
            .workspace_roots
            .iter()
            .filter(|root| root.is_dir() || (root.is_file() && has_rdra_extension(root)))
            .cloned()
            .chain(
                state
                    .documents
                    .keys()
                    .filter(|path| has_rdra_extension(path))
                    .cloned(),
            )
            .collect();
        inputs.sort();
        inputs.dedup();
        let overlays = state.documents.clone();
        drop(state);

        let analyzed = tokio::task::spawn_blocking({
            let overlays = overlays.clone();
            move || {
                load_project(&inputs, Some(&overlays)).map(|project| {
                    // Fingerprints only: the LSP generates nothing and runs no command.
                    let notes = project
                        .manifest
                        .as_ref()
                        .map(|m| (m.path.clone(), code_notes(m, &project.program)));
                    // `check` explores nothing when the model did not load; neither does the
                    // background check. The analysis adds its own findings after this.
                    let loaded = !project.diags.iter().any(|d| !d.is_warning);
                    (analyze_project(project), notes, loaded)
                })
            }
        })
        .await
        .unwrap_or_else(|err| std::panic::resume_unwind(err.into_panic()));

        let _publishing = self.publishing.lock().await;
        if self.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        let (analysis, notes, loaded) = match analyzed {
            Ok(analysis) => analysis,
            Err(ProjectError::NoRdraFiles) => {
                // The last .rdra file left the workspace: nothing to analyze, and nothing
                // published before may stay.
                let mut state = self.state.write().await;
                state.analysis = None;
                state.lifecycle = None;
                let stale = std::mem::take(&mut state.published);
                drop(state);
                for uri in stale {
                    self.client.publish_diagnostics(uri, vec![], None).await;
                }
                return;
            }
            Err(err) => {
                self.report_load_error(&err, &overlays).await;
                return;
            }
        };
        self.publish_diagnostics(&analysis).await;
        if let Some((path, notes)) = notes {
            self.publish_code_notes(&path, notes).await;
        }

        let analysis = Arc::new(analysis);
        let mut state = self.state.write().await;
        state.analysis = Some(analysis.clone());
        state.manifest_error = None;
        // The diagnostics just published hold no finding of an older check; its lenses go too.
        let had_lenses = state.lifecycle.take().is_some_and(|f| !f.lenses.is_empty());
        let (settings, lens_refresh) = (state.settings, state.lens_refresh);
        drop(state);
        if had_lenses && lens_refresh {
            refresh_lenses(&self.client);
        }
        if settings.enabled && loaded {
            tokio::spawn(self.checker().run(generation, analysis, settings));
        }
    }

    async fn report_load_error(&self, err: &ProjectError, overlays: &HashMap<PathBuf, String>) {
        let message = err.to_string();
        self.client.log_message(MessageType::ERROR, &message).await;

        let mut state = self.state.write().await;
        let uris = state.uris.clone();
        state.analysis = None;
        state.lifecycle = None;
        state.analysis_published.clear();
        let stale = std::mem::take(&mut state.published);
        let announce = state.manifest_error.as_deref() != Some(message.as_str());
        state.manifest_error = Some(message.clone());
        drop(state);

        for uri in stale {
            self.client.publish_diagnostics(uri, vec![], None).await;
        }
        if !matches!(err, ProjectError::Manifest { .. }) {
            return;
        }
        if announce {
            self.client.show_message(MessageType::ERROR, &message).await;
        }
        let first_open = overlays
            .keys()
            .filter(|path| has_rdra_extension(path))
            .min()
            .and_then(|path| uris.uri(path).ok());
        if let Some(uri) = first_open {
            let diagnostic = tower_lsp::lsp_types::Diagnostic {
                range: Range::default(),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("rdra-ish".to_string()),
                message,
                ..Default::default()
            };
            self.client
                .publish_diagnostics(uri.clone(), vec![diagnostic], None)
                .await;
            self.state.write().await.published.insert(uri);
        }
    }

    /// The fingerprint notes of `[code]` as information on rdra.toml; none clears them.
    async fn publish_code_notes(&self, manifest: &std::path::Path, notes: Vec<CodeNote>) {
        let uris = self.state.read().await.uris.clone();
        let Ok(uri) = uris.uri(manifest) else {
            return;
        };
        let diagnostics = notes
            .into_iter()
            .map(|n| tower_lsp::lsp_types::Diagnostic {
                range: Range::new(Position::new(n.line, 0), Position::new(n.line, u32::MAX)),
                severity: Some(DiagnosticSeverity::INFORMATION),
                source: Some("rdra-ish".to_string()),
                message: n.message,
                ..Default::default()
            })
            .collect();
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
        self.state.write().await.published.insert(uri);
    }

    async fn publish_diagnostics(&self, analysis: &WorkspaceAnalysis) {
        let uris = self.state.read().await.uris.clone();
        let mut by_source: HashMap<usize, Vec<&Diagnostic>> = HashMap::new();
        for diag in &analysis.diagnostics {
            if let Some(loc) = &diag.location {
                by_source.entry(loc.source_id).or_default().push(diag);
            }
        }

        let mut published = HashSet::new();
        let mut kept = HashMap::new();
        for (source_id, (path, text, _)) in analysis.program.sources.iter().enumerate() {
            let uri = match uris.uri(path) {
                Ok(uri) => uri,
                Err(_) => continue,
            };
            let diags = by_source.get(&source_id).cloned().unwrap_or_default();
            let diags: Vec<tower_lsp::lsp_types::Diagnostic> = diags
                .iter()
                .map(|diag| to_lsp_diagnostic(diag, text))
                .collect();
            self.client
                .publish_diagnostics(uri.clone(), diags.clone(), None)
                .await;
            kept.insert(uri.clone(), diags);
            published.insert(uri);
        }
        // A file published before that is no source now (deleted, renamed, or no longer
        // under a root) keeps its diagnostics in the editor until they are withdrawn.
        let mut state = self.state.write().await;
        state.analysis_published = kept;
        let stale = std::mem::replace(&mut state.published, published.clone());
        drop(state);
        for uri in stale.difference(&published) {
            self.client
                .publish_diagnostics(uri.clone(), vec![], None)
                .await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let roots = workspace_roots(&params);
        let mut state = self.state.write().await;
        for root in &roots {
            state.uris.add_root(root);
        }
        state.workspace_roots = roots;
        if let Some(section) = (params.initialization_options.as_ref()).and_then(Settings::section)
        {
            state.settings = state.settings.with(section);
        }
        state.lens_refresh = (params.capabilities.workspace.as_ref())
            .and_then(|w| w.code_lens.as_ref())
            .and_then(|c| c.refresh_support)
            .unwrap_or(false);

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "rdra-ish-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ":".to_string(),
                        "(".to_string(),
                        ",".to_string(),
                    ]),
                    ..Default::default()
                }),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: TOKEN_TYPES
                                    .iter()
                                    .map(|name| SemanticTokenType::from(*name))
                                    .collect(),
                                token_modifiers: TOKEN_MODIFIERS
                                    .iter()
                                    .map(|name| SemanticTokenModifier::from(*name))
                                    .collect(),
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            work_done_progress_options: Default::default(),
                        },
                    ),
                ),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                        resolve_provider: None,
                        work_done_progress_options: Default::default(),
                    },
                )),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
                    InlayHintOptions {
                        resolve_provider: Some(false),
                        work_done_progress_options: Default::default(),
                    },
                ))),
                linked_editing_range_provider: Some(LinkedEditingRangeServerCapabilities::Simple(
                    true,
                )),
                folding_range_provider: Some(true.into()),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "rdra-ish language server initialized")
            .await;
        self.refresh_analysis().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        if let Ok(path) = uri_to_path(&params.text_document.uri) {
            let mut state = self.state.write().await;
            state.uris.open(&params.text_document.uri);
            state.documents.insert(path, params.text_document.text);
        }
        self.refresh_analysis().await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Ok(path) = uri_to_path(&params.text_document.uri) {
            if let Some(change) = params.content_changes.into_iter().next() {
                let mut state = self.state.write().await;
                state.documents.insert(path, change.text);
            }
        }
        self.refresh_analysis().await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(text) = params.text {
            if let Ok(path) = uri_to_path(&params.text_document.uri) {
                let mut state = self.state.write().await;
                state.documents.insert(path, text);
            }
        }
        self.refresh_analysis().await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if let Ok(path) = uri_to_path(&params.text_document.uri) {
            let mut state = self.state.write().await;
            state.documents.remove(&path);
            state.analysis = None;
            self.client
                .publish_diagnostics(params.text_document.uri, vec![], None)
                .await;
        }
        self.refresh_analysis().await;
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        let Some(section) = Settings::section(&params.settings) else {
            return;
        };
        let mut state = self.state.write().await;
        let settings = state.settings.with(section);
        if settings == state.settings {
            return;
        }
        state.settings = settings;
        drop(state);
        // Analyze again: the check runs on the new analysis when it is on, and the findings
        // and lenses of the last one go either way.
        self.refresh_analysis().await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        if params
            .changes
            .iter()
            .any(|change| watched_path_is_rdra(&change.uri))
        {
            self.refresh_analysis().await;
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let at = &params.text_document_position_params;
        let state = self.state.read().await;
        let Some((found, offset)) = state.cursor(&at.text_document.uri, at.position) else {
            return Ok(None);
        };
        let analysis = found.analysis;
        let Some(reference) = reference_at_offset(found.doc.ast, offset) else {
            return Ok(None);
        };
        let Some(decl) = resolve_decl_site(&analysis.model, reference) else {
            return Ok(None);
        };
        let (target_path, target_text, _) = &analysis.program.sources[decl.source_id];
        let Ok(uri) = state.uris.uri(target_path) else {
            return Ok(None);
        };
        let range = span_to_range(target_text, decl.span);
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range,
        })))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let at = &params.text_document_position;
        let state = self.state.read().await;
        let Some((found, offset)) = state.cursor(&at.text_document.uri, at.position) else {
            return Ok(None);
        };
        let analysis = found.analysis;
        let Some(reference) = reference_at_offset(found.doc.ast, offset) else {
            return Ok(None);
        };
        let Some(target) = symbol_target(&analysis.model, reference) else {
            return Ok(None);
        };

        let mut locations = Vec::new();
        for (ref_path, ref_text, ref_ast) in &analysis.program.sources {
            let spans = find_symbol_references(ref_ast, &target);
            if spans.is_empty() {
                continue;
            }
            let Ok(uri) = state.uris.uri(ref_path) else {
                continue;
            };
            locations.extend(spans.into_iter().map(|span| Location {
                uri: uri.clone(),
                range: span_to_range(ref_text, span),
            }));
        }

        Ok(Some(locations))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let at = &params.text_document_position;
        let state = self.state.read().await;
        let Some((found, offset)) = state.cursor(&at.text_document.uri, at.position) else {
            return Ok(None);
        };
        let items = completion_items(&found.analysis.model, found.doc.text, offset);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let state = self.state.read().await;
        let Some(found) = state.document(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(DocumentSymbolResponse::Nested(document_symbols(
            found.doc.ast,
        ))))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let path = match uri_to_path(&params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };

        let state = self.state.read().await;
        let text = state.documents.get(&path).cloned().or_else(|| {
            state
                .analysis
                .as_ref()
                .and_then(|analysis| open_document(analysis, &path).map(|doc| doc.text.to_string()))
        });

        let Some(text) = text else {
            return Ok(None);
        };

        let formatted = match rdra_ish_syntax::format_source(&text) {
            Ok(formatted) => formatted,
            Err(_) => return Ok(None),
        };
        if formatted == text {
            return Ok(Some(vec![]));
        }

        Ok(Some(vec![TextEdit {
            range: full_document_range(&text),
            new_text: formatted,
        }]))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let symbols = workspace_symbols(analysis, &state.uris, &params.query);
        Ok(Some(symbols))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let at = &params.text_document_position_params;
        let state = self.state.read().await;
        let Some((found, offset)) = state.cursor(&at.text_document.uri, at.position) else {
            return Ok(None);
        };
        Ok(hover_content(&found.analysis.model, found.doc.ast, offset))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let at = &params.text_document_position_params;
        let state = self.state.read().await;
        let Some((found, offset)) = state.cursor(&at.text_document.uri, at.position) else {
            return Ok(None);
        };
        Ok(signature_help(found.doc.text, offset))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let state = self.state.read().await;
        let Some((found, offset)) = state.cursor(&params.text_document.uri, params.position) else {
            return Ok(None);
        };
        Ok(prepare_rename_range(found.analysis, &found.path, offset)
            .map(PrepareRenameResponse::Range))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let at = &params.text_document_position;
        let state = self.state.read().await;
        let Some((found, offset)) = state.cursor(&at.text_document.uri, at.position) else {
            return Ok(None);
        };

        match workspace_rename(
            found.analysis,
            &state.uris,
            &found.path,
            offset,
            &params.new_name,
        ) {
            Ok(edit) => Ok(Some(edit)),
            Err(RenameError::InvalidIdentifier) => Err(tower_lsp::jsonrpc::Error::invalid_params(
                "new name must be a valid identifier",
            )),
            Err(RenameError::Conflict) => Err(tower_lsp::jsonrpc::Error::invalid_params(
                "a symbol with that name already exists",
            )),
            Err(RenameError::NoSymbol) => Ok(None),
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let state = self.state.read().await;
        let Some(found) = state.document(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(SemanticTokensResult::Tokens(semantic_tokens(
            found.doc.ast,
        ))))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let state = self.state.read().await;
        let Some(found) = state.document(&params.text_document.uri) else {
            return Ok(None);
        };

        let actions = code_actions(
            found.analysis,
            found.doc.source_id,
            found.doc.text,
            params.text_document.uri.clone(),
            params.range,
        );
        if actions.is_empty() {
            return Ok(None);
        }
        Ok(Some(actions))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let state = self.state.read().await;
        let Some(found) = state.document(&params.text_document.uri) else {
            return Ok(None);
        };
        let mut lenses = code_lenses(
            found.analysis,
            &state.uris,
            found.doc.ast,
            found.doc.text,
            &found.path,
        );
        // The findings are by the path the analysis loaded the file under, which may not be
        // the path of the URI (a symlinked root).
        let source = &found.analysis.program.sources[found.doc.source_id].0;
        let checked = (state.lifecycle.as_ref()).and_then(|f| f.lenses.get(source));
        lenses.extend(checked.into_iter().flatten().map(|l| l.to_code_lens()));
        Ok(Some(lenses))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let state = self.state.read().await;
        let Some(found) = state.document(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(folding_ranges(found.doc.ast, found.doc.text)))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let state = self.state.read().await;
        let Some(found) = state.document(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(inlay_hints(
            &found.analysis.model,
            found.doc.ast,
            found.doc.text,
            Some(params.range),
        )))
    }

    async fn linked_editing_range(
        &self,
        params: LinkedEditingRangeParams,
    ) -> Result<Option<LinkedEditingRanges>> {
        let at = &params.text_document_position_params;
        let state = self.state.read().await;
        let Some((found, offset)) = state.cursor(&at.text_document.uri, at.position) else {
            return Ok(None);
        };
        Ok(linked_editing_ranges(
            &found.analysis.model,
            found.doc.ast,
            found.doc.text,
            offset,
        ))
    }
}

/// What a background check needs of the server, owned so that it can run on its own task.
struct Checker {
    client: Client,
    state: Arc<RwLock<ServerState>>,
    generation: Arc<AtomicU64>,
    publishing: Arc<Mutex<()>>,
    checking: Arc<Mutex<()>>,
}

impl Checker {
    fn current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
    }

    /// Runs `check` on the analysis of refresh `generation` once the text has been quiet for a
    /// moment, and publishes the findings beside the analysis diagnostics while that refresh
    /// is still the latest. A newer refresh drops the run before it starts or its result after
    /// it ends; one that started runs until it ends or its time budget stops it.
    async fn run(self, generation: u64, analysis: Arc<WorkspaceAnalysis>, settings: Settings) {
        tokio::time::sleep(settings.quiet).await;
        if !self.current(generation) {
            return;
        }
        let _checking = self.checking.lock().await;
        if !self.current(generation) {
            return;
        }
        let checked = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                lifecycle::check(&analysis, &settings)
            }))
        })
        .await;
        let Ok(Ok(findings)) = checked else {
            self.client
                .log_message(MessageType::ERROR, "the background lifecycle check failed")
                .await;
            return;
        };

        let _publishing = self.publishing.lock().await;
        if !self.current(generation) {
            return;
        }
        let mut state = self.state.write().await;
        let mut out = Vec::new();
        for (path, found) in &findings.diagnostics {
            let Ok(uri) = state.uris.uri(path) else {
                continue;
            };
            let mut all = (state.analysis_published.get(&uri).cloned()).unwrap_or_default();
            all.extend(found.iter().cloned());
            out.push((uri, all));
        }
        let lenses = !findings.lenses.is_empty();
        state.lifecycle = Some(findings);
        state
            .published
            .extend(out.iter().map(|(uri, _)| uri.clone()));
        let lens_refresh = state.lens_refresh;
        drop(state);
        for (uri, diagnostics) in out {
            self.client
                .publish_diagnostics(uri, diagnostics, None)
                .await;
        }
        if lenses && lens_refresh {
            refresh_lenses(&self.client);
        }
    }
}

/// Asks the client to request the code lenses again, without waiting for its answer.
fn refresh_lenses(client: &Client) {
    let client = client.clone();
    tokio::spawn(async move {
        let _ = client
            .send_request::<tower_lsp::lsp_types::request::CodeLensRefresh>(())
            .await;
    });
}

fn to_lsp_diagnostic(diag: &Diagnostic, source: &str) -> tower_lsp::lsp_types::Diagnostic {
    let range = diag
        .location
        .as_ref()
        .map(|loc| span_to_range(source, loc.span.clone()))
        .unwrap_or(Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 1,
            },
        });

    tower_lsp::lsp_types::Diagnostic {
        range,
        severity: Some(if diag.is_warning {
            DiagnosticSeverity::WARNING
        } else {
            DiagnosticSeverity::ERROR
        }),
        source: Some("rdra-ish".to_string()),
        message: diag.error.to_string(),
        ..Default::default()
    }
}

fn has_rdra_extension(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|ext| ext == "rdra")
}

fn workspace_roots(params: &InitializeParams) -> Vec<PathBuf> {
    params
        .workspace_folders
        .as_ref()
        .into_iter()
        .flatten()
        .filter_map(|folder| uri_to_path(&folder.uri).ok())
        .chain(
            params
                .root_uri
                .as_ref()
                .and_then(|uri| uri_to_path(uri).ok()),
        )
        .collect()
}

fn uri_to_path(uri: &Url) -> std::io::Result<PathBuf> {
    uri.to_file_path()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid file uri"))
}
