/*
 * Copyright (C) 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use crate::lsp::catalog_context::CatalogContext;
use crate::lsp::diagnostics::validate_and_publish_diagnostics;
use crate::lsp::handlers::completion::on_completion;
use crate::lsp::handlers::definition::go_to_definition;
use crate::lsp::handlers::document_symbols::handle_document_symbols;
use crate::lsp::handlers::folding::folding_ranges;
use crate::lsp::handlers::formatting::format_document;
use crate::lsp::handlers::hover::hover;
use crate::lsp::handlers::references::find_references;
use crate::lsp::handlers::semantic_tokens::semantic_tokens;
use crate::lsp::lsp_file_provider::{calculate_hash, LspFileProvider};
use crate::lsp::shadow_workspace::ShadowWorkspace;
use crate::lsp::token_builder::TOKEN_TYPES;
use crate::lsp::utils::{get_symbol_table_for_file, resolve_context};
use crate::Cli;
use anyhow::{anyhow, Result};
use lsp_server::{Connection, Message, Request, Response, ResponseError};
use lsp_types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use vsidl_v1_parser::file_provider::{DiskFileProvider, FileProvider};
use vsidl_v1_parser::{Catalog, SymbolTable, TargetApi};

/// Starts the VSIDL Language Server Protocol (LSP) server.
///
/// This function initializes the server, handling the LSP handshake and setting up
/// server capabilities. It then enters the main loop to process messages.
///
/// # Arguments
///
/// * `cli` - The command-line arguments, which may contain configuration for the server.
///
/// # Returns
/// * `Ok(())` if the server runs and shuts down gracefully.
/// * `Err(anyhow::Error)` if there is an error during initialization or during the server loop.
pub(crate) fn lsp_server_start(cli: Cli) -> anyhow::Result<()> {
    eprintln!("Server started.");
    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = serde_json::to_value(&ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        // OneOf::Left(true) because we don't show work progress
        definition_provider: Some(OneOf::Left(true)),
        // OneOf::Left(true) because we don't show work progress
        references_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                work_done_progress_options: WorkDoneProgressOptions::default(),
                legend: SemanticTokensLegend {
                    token_types: TOKEN_TYPES.to_vec(),
                    token_modifiers: vec![],
                },
                range: Some(false),
                full: Some(SemanticTokensFullOptions::Delta { delta: Some(false) }),
            },
        )),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(false),
            trigger_characters: Some(vec![":".to_string(), " ".to_string()]),
            ..CompletionOptions::default()
        }),
        ..ServerCapabilities::default()
    })?;

    let _initialization_params = connection.initialize(server_capabilities)?;
    let rt = Runtime::new()?;

    let state = Arc::new(ServerState::new(cli, Arc::new(connection)));
    lsp_server_loop(state, &rt)?;

    io_threads.join()?;
    eprintln!("Server shut down.");
    Ok(())
}

pub(crate) struct ServerState {
    pub(crate) cli: Cli,
    pub(crate) connection: Arc<Connection>,
    pub(crate) contexts: Arc<Mutex<HashMap<PathBuf, CatalogContext>>>,
    pub(crate) file_provider: LspFileProvider,
}

impl ServerState {
    pub(crate) fn new(cli: Cli, connection: Arc<Connection>) -> Self {
        let document_store = Arc::new(Mutex::new(HashMap::new()));
        let file_hashes = Arc::new(Mutex::new(HashMap::new()));
        let file_provider =
            LspFileProvider { document_store, file_hashes, disk_fallback: DiskFileProvider };

        Self { cli, connection, contexts: Arc::new(Mutex::new(HashMap::new())), file_provider }
    }

    pub(crate) fn lock_contexts(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<PathBuf, CatalogContext>> {
        self.contexts.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The main message loop for the LSP server.
///
/// It listens for incoming messages from the client and processes them.
/// It handles shutdown requests, and notifications for document changes
/// (opening, changing), triggering validation for the affected documents.
///
/// # Arguments
///
/// * `state` - The state of the LSP server, storing information from previous analyses of the catalog.
/// * `rt` - The runtime for tasks that handle notifications and requests.
///
/// # Returns
/// * `Ok(())` when the shutdown request is received and the loop terminates.
/// * `Err(anyhow::Error)` if an error occurs while processing messages.
pub(crate) fn lsp_server_loop(state: Arc<ServerState>, rt: &Runtime) -> Result<()> {
    eprintln!("Main loop started.");

    for msg in &state.connection.receiver {
        eprintln!("Received message: {:?}", msg);
        match msg {
            Message::Request(req) => {
                if state.connection.handle_shutdown(&req)? {
                    eprintln!("Shutdown request received.");
                    return Ok(());
                }
                let state = state.clone();
                rt.spawn(async move {
                    handle_request(state, req).await;
                });
            }
            Message::Notification(not) => {
                let state = state.clone();
                rt.spawn(async move {
                    handle_notification(state, not).await;
                });
            }
            _ => {}
        }
    }
    Ok(())
}

async fn handle_request(state: Arc<ServerState>, req: Request) {
    let method = req.method.clone();
    let id = req.id.clone();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match method.as_str() {
        "textDocument/completion" => Some(handle_completion(&state, req)),
        "textDocument/definition" => Some(handle_definition(&state, req)),
        "textDocument/references" => Some(handle_references(&state, req)),
        "textDocument/hover" => Some(handle_hover(&state, req)),
        "textDocument/formatting" => Some(handle_formatting(&state, req)),
        "textDocument/semanticTokens/full" => Some(handle_semantic_tokens(&state, req)),
        "textDocument/foldingRange" => Some(handle_folding_range(&state, req)),
        "textDocument/documentSymbol" => Some(handle_document_symbol(&state, req)),
        _ => None,
    }));

    let resp = match result {
        // Method found and executed successfully
        Ok(Some(Ok(maybe_result))) => Response {
            id,
            result: Some(maybe_result.unwrap_or(serde_json::Value::Null)),
            error: None,
        },
        // Method found but handler returned an error
        Ok(Some(Err(error))) => Response {
            id,
            result: None,
            error: Some(ResponseError { code: 0, message: format!("{}", error), data: None }),
        },
        // Method not found
        Ok(None) => Response {
            id,
            result: None,
            error: Some(ResponseError {
                code: lsp_server::ErrorCode::MethodNotFound as i32,
                message: format!("Method not found: {}", method),
                data: None,
            }),
        },
        // Panic in handler (or during method matching)
        Err(_) => {
            eprintln!("Panic while handling request {}: {:?}", method, id);
            Response { id, result: Some(serde_json::Value::Null), error: None }
        }
    };

    let _ = state.connection.sender.send(Message::Response(resp.clone()));
    eprintln!("Sent response: {:?}", resp);
}

fn handle_definition(state: &ServerState, req: Request) -> Result<Option<serde_json::Value>> {
    let params: GotoDefinitionParams = serde_json::from_value(req.params)?;
    let uri = &params.text_document_position_params.text_document.uri;
    let mut ctxs = state.lock_contexts();
    let sym_table = get_symbol_table_for_file(uri, &state.cli, &mut ctxs);
    go_to_definition(state.cli.clone(), params, sym_table, &state.file_provider)
}

fn handle_references(state: &ServerState, req: Request) -> Result<Option<serde_json::Value>> {
    let params: ReferenceParams = serde_json::from_value(req.params)?;
    let uri = &params.text_document_position.text_document.uri;
    let mut ctxs = state.lock_contexts();
    let sym_table = get_symbol_table_for_file(uri, &state.cli, &mut ctxs);
    find_references(state.cli.clone(), params, sym_table, &state.file_provider)
}

fn handle_hover(state: &ServerState, req: Request) -> Result<Option<serde_json::Value>> {
    let params: HoverParams = serde_json::from_value(req.params)?;
    let uri = &params.text_document_position_params.text_document.uri;
    let mut ctxs = state.lock_contexts();
    let sym_table = get_symbol_table_for_file(uri, &state.cli, &mut ctxs);
    hover(state.cli.clone(), &params, sym_table, &state.file_provider)
}

fn handle_formatting(state: &ServerState, req: Request) -> Result<Option<serde_json::Value>> {
    let params: DocumentFormattingParams = serde_json::from_value(req.params)?;
    format_document(&params, &state.file_provider)
}

fn handle_semantic_tokens(state: &ServerState, req: Request) -> Result<Option<serde_json::Value>> {
    let params: SemanticTokensParams = serde_json::from_value(req.params)?;
    semantic_tokens(params, &state.file_provider)
}

fn handle_folding_range(state: &ServerState, req: Request) -> Result<Option<serde_json::Value>> {
    let params: FoldingRangeParams = serde_json::from_value(req.params)?;
    folding_ranges(params, &state.file_provider)
}

fn handle_document_symbol(state: &ServerState, req: Request) -> Result<Option<serde_json::Value>> {
    let params: DocumentSymbolParams = serde_json::from_value(req.params)?;
    handle_document_symbols(params, &state.file_provider)
}

fn handle_completion(state: &ServerState, req: Request) -> Result<Option<serde_json::Value>> {
    let params: CompletionParams = serde_json::from_value(req.params)?;
    let uri = &params.text_document_position.text_document.uri;
    let path = uri.to_file_path().map_err(|_| anyhow!("Invalid URI"))?;
    let (key, cli_template) = resolve_context(&path, &state.cli);

    let mut contexts = state.lock_contexts();
    let context = contexts.entry(key).or_insert_with(|| CatalogContext {
        cli: cli_template,
        symbol_table: SymbolTable::default(),
        catalog: Catalog::new(),
        last_valid_symbol_table: None,
        recovered_syntax_errors: HashMap::new(),
        active_findings: HashMap::new(),
        shadow_workspace: None,
    });

    let response = on_completion(&state.file_provider, context, params)?;
    Ok(serde_json::to_value(response).ok())
}

async fn handle_notification(state: Arc<ServerState>, not: lsp_server::Notification) {
    let method = not.method.clone();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match method.as_str() {
        "textDocument/didOpen" => {
            if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(not.params) {
                update_document_store(
                    &state,
                    params.text_document.uri.clone(),
                    params.text_document.text,
                );
                process_doc_update(&state, &params.text_document.uri);
            }
        }
        "textDocument/didChange" => {
            if let Ok(params) = serde_json::from_value::<DidChangeTextDocumentParams>(not.params) {
                if let Some(change) = params.content_changes.into_iter().next() {
                    update_document_store(&state, params.text_document.uri.clone(), change.text);
                }
                process_doc_update(&state, &params.text_document.uri);
            }
        }
        "textDocument/didSave" => {
            if let Ok(params) = serde_json::from_value::<DidSaveTextDocumentParams>(not.params) {
                process_doc_update(&state, &params.text_document.uri);
            }
        }
        "textDocument/didClose" => {
            if let Ok(params) = serde_json::from_value::<DidCloseTextDocumentParams>(not.params) {
                let mut store =
                    state.file_provider.document_store.lock().unwrap_or_else(|e| e.into_inner());
                store.remove(&params.text_document.uri);
                if let Ok(path) = params.text_document.uri.to_file_path() {
                    revert_shadow_file(&state, &path);
                }
            }
        }
        _ => {}
    }));
}

fn update_document_store(state: &ServerState, uri: Url, text: String) {
    let mut store = state.file_provider.document_store.lock().unwrap_or_else(|e| e.into_inner());
    store.insert(uri, text);
}

fn process_doc_update(state: &ServerState, uri: &Url) {
    let mut contexts = state.contexts.lock().unwrap_or_else(|e| e.into_inner());
    if let Ok(path) = uri.to_file_path() {
        if let Err(e) = process_document_internal(state, &mut contexts, uri, Some(&path)) {
            eprintln!("Error processing document: {:?}", e);
        }
    }
}

fn revert_shadow_file(state: &ServerState, path: &Path) {
    let (key, _) = resolve_context(path, &state.cli);
    let contexts = state.contexts.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ctx) = contexts.get(&key) {
        if let Some(shadow) = &ctx.shadow_workspace {
            if let Err(e) = shadow.revert_file(path) {
                eprintln!("Failed to revert file: {:?}", e);
            }
        }
    }
}

fn process_document_internal(
    state: &ServerState,
    contexts: &mut HashMap<PathBuf, CatalogContext>,
    uri: &Url,
    changed_path: Option<&Path>,
) -> Result<()> {
    let path = uri.to_file_path().map_err(|_| anyhow!("Invalid URI"))?;
    let (context_key, cli_template) = resolve_context(&path, &state.cli);

    let context = contexts.entry(context_key).or_insert_with(|| {
        let mut symbol_table = SymbolTable::default();
        match cli_template.target_api.as_str() {
            "latest" => symbol_table.set_target_api(TargetApi::Latest),
            _ => symbol_table.set_target_api(TargetApi::Pdk29),
        }
        CatalogContext {
            cli: cli_template,
            symbol_table,
            catalog: Catalog::new(),
            last_valid_symbol_table: None,
            active_findings: HashMap::new(),
            shadow_workspace: None,
            recovered_syntax_errors: HashMap::new(),
        }
    });

    // Initialize shadow workspace if needed
    if context.shadow_workspace.is_none() {
        if let Some(root_path) = &context.cli.catalog_path {
            match ShadowWorkspace::new(root_path.clone()) {
                Ok(ws) => {
                    // Sync all currently open dirty files
                    let store = state
                        .file_provider
                        .document_store
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    for (uri, content) in store.iter() {
                        if let Ok(p) = uri.to_file_path() {
                            let _ = ws.update_file(&p, content);
                        }
                    }
                    context.shadow_workspace = Some(Arc::new(ws));
                }
                Err(e) => eprintln!("Failed to create shadow workspace: {:?}", e),
            }
        }
    }

    let last_hash: u64 = state.file_provider.get_hash(&path).unwrap_or(0);
    if let Ok(content) = state.file_provider.read_to_string(&path) {
        let current_hash = calculate_hash(&content);

        if current_hash != last_hash || path.extension() == Some(std::ffi::OsStr::new("proto")) {
            eprintln!("File content changed, reprocessing incrementally: {:?}", path);

            // Update the current file in shadow workspace
            if let Some(ws) = &context.shadow_workspace {
                if let Err(e) = ws.update_file(&path, &content) {
                    eprintln!("Failed to update shadow file: {:?}", e);
                }
            }

            state.file_provider.update_hash(path.to_path_buf(), current_hash);
            validate_and_publish_diagnostics(
                context,
                &state.connection,
                &state.file_provider,
                changed_path,
            )?;
        } else {
            eprintln!("File content not changed, skipping reprocessing: {:?}", path);
        }
    } else {
        eprintln!("Failed to read file content for hash check: {:?}", path);
        // Fallback to full reparse if content read fails
        validate_and_publish_diagnostics(context, &state.connection, &state.file_provider, None)?;
    }
    Ok(())
}
