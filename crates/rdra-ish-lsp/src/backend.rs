use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rdra_ish_core::{analyze_workspace, Diagnostic, WorkspaceAnalysis};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use url::Url;
use walkdir::WalkDir;

use crate::code_actions::code_actions;
use crate::code_lens::code_lenses;
use crate::completion::completion_items;
use crate::convert::{full_document_range, position_to_byte_offset, span_to_range};
use crate::folding::folding_ranges;
use crate::hover::{hover_content, signature_help};
use crate::inlay_hints::inlay_hints;
use crate::linked_editing::linked_editing_ranges;
use crate::refs::{find_symbol_references, reference_at_offset, resolve_decl_site, symbol_target};
use crate::rename::{prepare_rename_range, workspace_rename, RenameError};
use crate::semantic_tokens::{semantic_tokens, TOKEN_MODIFIERS, TOKEN_TYPES};
use crate::symbols::{document_symbols, workspace_symbols};

#[derive(Default)]
struct ServerState {
    documents: HashMap<PathBuf, String>,
    workspace_roots: Vec<PathBuf>,
    analysis: Option<WorkspaceAnalysis>,
}

pub struct Backend {
    client: Client,
    state: Arc<RwLock<ServerState>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(RwLock::new(ServerState::default())),
        }
    }

    async fn refresh_analysis(&self) {
        let state = self.state.read().await;
        let mut entry_paths: HashSet<PathBuf> = discover_rdra_files(&state.workspace_roots)
            .into_iter()
            .collect();
        for path in state.documents.keys() {
            if path.extension().is_some_and(|ext| ext == "rdra") {
                entry_paths.insert(path.clone());
            }
        }
        if entry_paths.is_empty() {
            return;
        }

        let include_paths = include_paths_for(&state.documents, &state.workspace_roots);
        let overlays = state.documents.clone();
        let entry_paths: Vec<PathBuf> = entry_paths.into_iter().collect();
        drop(state);

        let analysis = analyze_workspace(&entry_paths, &include_paths, &overlays);
        self.publish_diagnostics(&analysis).await;

        let mut state = self.state.write().await;
        state.analysis = Some(analysis);
    }

    async fn publish_diagnostics(&self, analysis: &WorkspaceAnalysis) {
        let mut by_source: HashMap<usize, Vec<&Diagnostic>> = HashMap::new();
        for diag in &analysis.diagnostics {
            if let Some(loc) = &diag.location {
                by_source.entry(loc.source_id).or_default().push(diag);
            }
        }

        for (source_id, (path, text, _)) in analysis.program.sources.iter().enumerate() {
            let uri = match path_to_uri(path) {
                Ok(uri) => uri,
                Err(_) => continue,
            };
            let diags = by_source.get(&source_id).cloned().unwrap_or_default();
            let diags: Vec<tower_lsp::lsp_types::Diagnostic> = diags
                .iter()
                .map(|diag| to_lsp_diagnostic(diag, text))
                .collect();
            self.client.publish_diagnostics(uri, diags, None).await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let roots = workspace_roots(&params);
        let mut state = self.state.write().await;
        state.workspace_roots = roots;

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
        let path = match uri_to_path(&params.text_document_position_params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let position = params.text_document_position_params.position;

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some(source_id) = analysis
            .program
            .sources
            .iter()
            .position(|(source_path, _, _)| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };
        let (_, text, ast) = &analysis.program.sources[source_id];
        let offset = match position_to_byte_offset(text, position) {
            Some(offset) => offset,
            None => return Ok(None),
        };
        let reference = match reference_at_offset(ast, offset) {
            Some(reference) => reference,
            None => return Ok(None),
        };
        let decl = match resolve_decl_site(&analysis.model, reference) {
            Some(decl) => decl,
            None => return Ok(None),
        };
        let (target_path, target_text, _) = &analysis.program.sources[decl.source_id];
        let target_uri = match path_to_uri(target_path) {
            Ok(uri) => uri,
            Err(_) => return Ok(None),
        };
        let range = span_to_range(target_text, decl.span);
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: target_uri,
            range,
        })))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let path = match uri_to_path(&params.text_document_position.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let position = params.text_document_position.position;

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some(source_id) = analysis
            .program
            .sources
            .iter()
            .position(|(source_path, _, _)| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };
        let (_, text, ast) = &analysis.program.sources[source_id];
        let offset = match position_to_byte_offset(text, position) {
            Some(offset) => offset,
            None => return Ok(None),
        };
        let reference = match reference_at_offset(ast, offset) {
            Some(reference) => reference,
            None => return Ok(None),
        };
        let target = match symbol_target(&analysis.model, reference) {
            Some(target) => target,
            None => return Ok(None),
        };

        let mut locations = Vec::new();
        for (ref_source_id, (ref_path, ref_text, ref_ast)) in
            analysis.program.sources.iter().enumerate()
        {
            for span in find_symbol_references(ref_ast, &target) {
                let uri = match path_to_uri(ref_path) {
                    Ok(uri) => uri,
                    Err(_) => continue,
                };
                locations.push(Location {
                    uri,
                    range: span_to_range(ref_text, span),
                });
            }
            let _ = ref_source_id;
        }

        Ok(Some(locations))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let path = match uri_to_path(&params.text_document_position.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let position = params.text_document_position.position;

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some((_, text, ast)) = analysis
            .program
            .sources
            .iter()
            .find(|(source_path, _, _)| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };
        let offset = match position_to_byte_offset(text, position) {
            Some(offset) => offset,
            None => return Ok(None),
        };
        let items = completion_items(&analysis.model, text, offset);
        let _ = ast;
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let path = match uri_to_path(&params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some((_, _, ast)) = analysis
            .program
            .sources
            .iter()
            .find(|(source_path, _, _)| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };

        Ok(Some(DocumentSymbolResponse::Nested(document_symbols(ast))))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let path = match uri_to_path(&params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };

        let state = self.state.read().await;
        let text = state.documents.get(&path).cloned().or_else(|| {
            state.analysis.as_ref().and_then(|analysis| {
                analysis
                    .program
                    .sources
                    .iter()
                    .find(|(source_path, _, _)| paths_equal(source_path, &path))
                    .map(|(_, source_text, _)| source_text.clone())
            })
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
        let symbols = workspace_symbols(analysis, &params.query);
        Ok(Some(symbols))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let path = match uri_to_path(&params.text_document_position_params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let position = params.text_document_position_params.position;

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some((_, text, ast)) = analysis
            .program
            .sources
            .iter()
            .find(|(source_path, _, _)| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };
        let offset = match position_to_byte_offset(text, position) {
            Some(offset) => offset,
            None => return Ok(None),
        };
        Ok(hover_content(&analysis.model, ast, offset))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let path = match uri_to_path(&params.text_document_position_params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let position = params.text_document_position_params.position;

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some((_, text, _)) = analysis
            .program
            .sources
            .iter()
            .find(|(source_path, _, _)| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };
        let offset = match position_to_byte_offset(text, position) {
            Some(offset) => offset,
            None => return Ok(None),
        };
        Ok(signature_help(text, offset))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let path = match uri_to_path(&params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let position = params.position;

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let offset = match analysis
            .program
            .sources
            .iter()
            .find(|(source_path, _, _)| paths_equal(source_path, &path))
            .and_then(|(_, text, _)| position_to_byte_offset(text, position))
        {
            Some(offset) => offset,
            None => return Ok(None),
        };

        Ok(prepare_rename_range(analysis, &path, offset).map(PrepareRenameResponse::Range))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let path = match uri_to_path(&params.text_document_position.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let offset = match analysis
            .program
            .sources
            .iter()
            .find(|(source_path, _, _)| paths_equal(source_path, &path))
            .and_then(|(_, text, _)| position_to_byte_offset(text, position))
        {
            Some(offset) => offset,
            None => return Ok(None),
        };

        match workspace_rename(analysis, &path, offset, &new_name) {
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
        let path = match uri_to_path(&params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some((_, _, ast)) = analysis
            .program
            .sources
            .iter()
            .find(|(source_path, _, _)| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };

        Ok(Some(SemanticTokensResult::Tokens(semantic_tokens(ast))))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let path = match uri_to_path(&params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let uri = params.text_document.uri.clone();

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some((source_id, (_, text, _))) = analysis
            .program
            .sources
            .iter()
            .enumerate()
            .find(|(_, (source_path, _, _))| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };

        let actions = code_actions(analysis, source_id, text, uri, params.range);
        if actions.is_empty() {
            return Ok(None);
        }
        Ok(Some(actions))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let path = match uri_to_path(&params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some((_, text, ast)) = analysis
            .program
            .sources
            .iter()
            .find(|(source_path, _, _)| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };

        Ok(Some(code_lenses(analysis, ast, text, &path)))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let path = match uri_to_path(&params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some((_, text, ast)) = analysis
            .program
            .sources
            .iter()
            .find(|(source_path, _, _)| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };

        Ok(Some(folding_ranges(ast, text)))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let path = match uri_to_path(&params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some((_, text, ast)) = analysis
            .program
            .sources
            .iter()
            .find(|(source_path, _, _)| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };

        Ok(Some(inlay_hints(
            &analysis.model,
            ast,
            text,
            Some(params.range),
        )))
    }

    async fn linked_editing_range(
        &self,
        params: LinkedEditingRangeParams,
    ) -> Result<Option<LinkedEditingRanges>> {
        let path = match uri_to_path(&params.text_document_position_params.text_document.uri) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let position = params.text_document_position_params.position;

        let state = self.state.read().await;
        let Some(analysis) = state.analysis.as_ref() else {
            return Ok(None);
        };
        let Some((_, text, ast)) = analysis
            .program
            .sources
            .iter()
            .find(|(source_path, _, _)| paths_equal(source_path, &path))
        else {
            return Ok(None);
        };
        let offset = match position_to_byte_offset(text, position) {
            Some(offset) => offset,
            None => return Ok(None),
        };

        Ok(linked_editing_ranges(&analysis.model, ast, text, offset))
    }
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

fn discover_rdra_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in roots {
        if root.is_file() {
            if root.extension().is_some_and(|ext| ext == "rdra") {
                files.push(root.clone());
            }
            continue;
        }
        if !root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "rdra")
            {
                files.push(entry.path().to_path_buf());
            }
        }
    }
    files.sort();
    files.dedup();
    files
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

fn include_paths_for(documents: &HashMap<PathBuf, String>, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut paths = Vec::new();

    for root in roots {
        if seen.insert(root.clone()) {
            paths.push(root.clone());
        }
    }

    for path in documents.keys() {
        if let Some(parent) = path.parent() {
            if seen.insert(parent.to_path_buf()) {
                paths.push(parent.to_path_buf());
            }
            if let Some(grandparent) = parent.parent() {
                if seen.insert(grandparent.to_path_buf()) {
                    paths.push(grandparent.to_path_buf());
                }
            }
        }
    }

    paths
}

fn uri_to_path(uri: &Url) -> std::io::Result<PathBuf> {
    uri.to_file_path()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid file uri"))
}

fn path_to_uri(path: &Path) -> std::io::Result<Url> {
    Url::from_file_path(path)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid path for uri"))
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    let ca = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let cb = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}

fn watched_path_is_rdra(uri: &Url) -> bool {
    uri_to_path(uri)
        .ok()
        .and_then(|path| path.extension().map(|ext| ext == "rdra"))
        .unwrap_or(false)
}
