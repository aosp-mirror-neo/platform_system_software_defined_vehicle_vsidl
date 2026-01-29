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
use crate::lsp::file_diagnostics::FindingsByStage;
use crate::lsp::lsp_file_provider::LspFileProvider;
use crate::lsp::span_source_remapper::SpanSourceRemapper;
use anyhow::Result;
use lsp_server::{Connection, Message};
use lsp_types::{PublishDiagnosticsParams, Url};
use pest::error::Error as PestError;
use pest::Parser;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use vsidl_v1::syntax::Span;
use vsidl_v1_parser::converter::convert_to_vsidl_entry;
use vsidl_v1_parser::file_provider::FileProvider;
use vsidl_v1_parser::pest_parser::{Rule, VsidlParser};
use vsidl_v1_parser::visitors::keyword_converter::KeywordConverter;
use vsidl_v1_parser::visitors::visitor_mut::AstNodeMut;
use vsidl_v1_parser::AstNode;
use vsidl_v1_parser::{
    Finding, FindingId, FindingSeverity, SymbolTableCreator, SymbolTableLinker, SymbolTableState,
};
use vsidlc_cli::VsidlcCommonCli;

/// Validates a VSIDL document and publishes any findings as diagnostics.
pub(crate) fn validate_and_publish_diagnostics(
    context: &mut CatalogContext,
    connection: &Connection,
    file_provider: &LspFileProvider,
    changed_path: Option<&Path>,
) -> Result<()> {
    let mut session = DiagnosticSession::new(context, connection, file_provider);
    session.run(changed_path)
}

struct DiagnosticSession<'a> {
    context: &'a mut CatalogContext,
    connection: &'a Connection,
    provider: &'a LspFileProvider,
    cli: VsidlcCommonCli,
}

#[allow(clippy::unnecessary_unwrap)]
impl<'a> DiagnosticSession<'a> {
    fn new(
        context: &'a mut CatalogContext,
        connection: &'a Connection,
        provider: &'a LspFileProvider,
    ) -> Self {
        let mut cli = VsidlcCommonCli::from(context.cli.clone());

        cli.dependency_catalog_path = cli
            .dependency_catalog_path
            .iter()
            .map(|p| provider.canonicalize(p).unwrap_or_else(|_| p.to_path_buf()))
            .collect();

        Self { context, connection, provider, cli }
    }

    fn run(&mut self, changed_path: Option<&Path>) -> Result<()> {
        // 1. Update Catalog
        let update_result = self.update_catalog(changed_path)?;
        if let Some(incremental) = update_result {
            // 2. Prepare Symbols
            let creator = self.prepare_symbols(changed_path, incremental);
            // 3. Collect Findings
            let findings = self.collect_findings(creator);
            // 4. Publish
            self.publish(findings)?;
        }
        Ok(())
    }

    /// Updates the VSIDL catalog, attempting an incremental update if possible.
    ///
    /// If the catalog is already populated and a specific VSIDL file has changed,
    /// this method attempts to update just that file in the catalog. If the file
    /// is not part of the catalog, is a proto file, or incremental update fails,
    /// it falls back to a full catalog scan.
    ///
    /// # Returns
    ///
    /// A `Result` indicating if the operation was technically successful (no I/O or IPC crashes).
    /// The inner `Option<bool>` gives the next steps for the diagnostic session:
    ///
    /// * `Ok(Some(true))` - **Incremental Update**: The caller should perform a partial symbol table update.
    /// * `Ok(Some(false))` - **Full Update**: The caller must rebuild the entire symbol table.
    /// * `Ok(None)` - **Abort**: A syntax error was reported; downstream processing (linking) should be skipped.
    fn update_catalog(&mut self, changed_path: Option<&Path>) -> Result<Option<bool>> {
        let catalog_populated = !self.context.catalog.catalog_vsidl_files.is_empty();
        let is_proto = changed_path.and_then(|p| p.extension()).is_some_and(|e| e == "proto");

        if catalog_populated && changed_path.is_some() && !is_proto {
            if let Some(success) = self.try_update_incremental(changed_path.unwrap())? {
                return Ok(success);
            }
            eprintln!("Incremental updated failed, falling back to full disk scan");
        }

        self.perform_full_update()
    }

    fn try_update_incremental(&mut self, path: &Path) -> Result<Option<Option<bool>>> {
        let abs_path = self.provider.canonicalize(path).unwrap_or(path.to_path_buf());
        eprintln!("Incremental catalog update for: {:?}", abs_path);

        let Ok(rel) = abs_path.strip_prefix(&self.cli.catalog_path) else {
            eprintln!(
                "Updated file '{:?}' is not in catalog path '{:?}'",
                abs_path, &self.cli.catalog_path
            );
            return Ok(None); // Not in catalog, fallback
        };

        let mut path_to_read = abs_path.clone();
        if let Some(shadow) = self.context.shadow_workspace.as_deref() {
            path_to_read = shadow.map_to_shadow(&path_to_read);
        }

        let content = match self.provider.read_to_string(&path_to_read) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to read file: {:?}", e);
                return Err(e);
            }
        };

        match VsidlParser::parse(Rule::vsidl_file, &content) {
            Ok(mut pairs) => {
                let pair = pairs.next().unwrap();
                let mut entry = convert_to_vsidl_entry(pair, abs_path.to_str().unwrap_or_default());
                if let Some(shadow) = self.context.shadow_workspace.as_deref() {
                    let mut updater = SpanSourceRemapper { shadow_workspace: Some(shadow) };
                    AstNodeMut::accept(&mut entry, &mut updater);
                }
                self.context.catalog.catalog_vsidl_files.retain(|(p, _)| p != rel);
                self.context.catalog.catalog_vsidl_files.push((rel.to_path_buf(), entry));
                Ok(Some(Some(true))) // Handled, is_incremental=true
            }
            Err(e) => {
                self.report_syntax_error(&abs_path, e)?;
                Ok(Some(None)) // Handled, but error occurred, stop.
            }
        }
    }

    fn perform_full_update(&mut self) -> Result<Option<bool>> {
        let syntax_path =
            vsidl_v1_parser::get_vsidl_syntax_default_path().unwrap_or_else(|_| PathBuf::from("."));

        // Choose catalog path based on shadow workspace or canonicalization
        let mut catalog_path = self.cli.catalog_path.clone();
        if let Some(shadow) = self.context.shadow_workspace.as_deref() {
            catalog_path = shadow.root_path().clone();
        } else if let Ok(p) = self.provider.canonicalize(&self.cli.catalog_path) {
            catalog_path = p.clone();
        }

        let result = vsidl_v1_parser::read_catalog_internal(
            self.provider,
            &catalog_path,
            &self.cli.dependency_catalog_path.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
            &syntax_path,
            true,
        );

        match result {
            Ok(mut new_catalog) => {
                // Hide shadow workspace, by converting back to the original paths
                if let Some(shadow) = self.context.shadow_workspace.as_deref() {
                    let mut updater = SpanSourceRemapper { shadow_workspace: Some(shadow) };
                    AstNodeMut::accept(&mut new_catalog, &mut updater);
                    new_catalog.catalog_path = Some(self.cli.catalog_path.clone());
                }
                eprintln!("✅ Successfully created catalog!");
                self.context.catalog = new_catalog;
                Ok(Some(false)) // Success, is_incremental=false
            }
            Err(e) => {
                if let Some(pest_err) = e.downcast_ref::<PestError<Rule>>() {
                    let path = pest_err.path().map(PathBuf::from).unwrap_or_default();
                    self.report_syntax_error(&path, pest_err.clone())?;
                } else {
                    eprintln!("❌ Error creating catalog: {:#?}", e);
                }
                Ok(None) // Stop
            }
        }
    }

    fn prepare_symbols(
        &mut self,
        changed_path: Option<&Path>,
        is_incremental: bool,
    ) -> SymbolTableCreator<&'a LspFileProvider> {
        let symbols_ready = self.context.symbol_table.get_state() != SymbolTableState::NotCreated;

        if is_incremental && symbols_ready {
            let raw_path = changed_path.unwrap();
            let abs_path = self.provider.canonicalize(raw_path).unwrap_or(raw_path.to_path_buf());

            // Determine relative path for symbol restriction
            let path_to_use = abs_path
                .strip_prefix(&self.cli.catalog_path)
                .ok()
                .or_else(|| {
                    self.cli
                        .dependency_catalog_path
                        .iter()
                        .find_map(|d| abs_path.strip_prefix(d).ok())
                })
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| abs_path.clone());

            eprintln!("Incremental symbols for: {:?} (Rel: {:?})", abs_path, path_to_use);

            self.context.symbol_table.remove_symbols_for_file(&abs_path);
            let mut creator = SymbolTableCreator::from_symbol_table_with_provider(
                self.context.symbol_table.clone(),
                self.provider,
            );
            creator.restrict_to_file(path_to_use);
            self.context.catalog.accept(&mut creator);
            creator
        } else {
            eprintln!("Full symbol table build");
            let mut creator = SymbolTableCreator::with_file_provider(self.provider);
            self.context.catalog.accept(&mut creator);
            creator
        }
    }

    fn collect_findings(
        &mut self,
        creator: SymbolTableCreator<&LspFileProvider>,
    ) -> FindingsByStage {
        let mut checker = vsidl_v1_parser::ContextConditionChecker::default();
        checker.set_quiet(true);
        if !self.cli.no_warn {
            checker.enable_warnings();
        }

        // 1. AST Check
        KeywordConverter::traverse(&mut self.context.catalog);
        let _ = checker.check_ast(&self.context.catalog);
        let ast = checker.findings();
        if checker.error_count() > 0 {
            return FindingsByStage { ast, unlinked: None, linked: None };
        }
        checker.clear_findings();

        // 2. Unlinked Check
        let _ = checker.check_symbol_table(creator.symbol_table());
        let unlinked = Some(checker.findings());
        if checker.error_count() > 0 {
            return FindingsByStage { ast, unlinked, linked: None };
        }
        checker.clear_findings();

        // 3. Linked Check
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();
        let _ = checker.check_linked_symbol_table(linker.symbol_table());

        if checker.error_count() == 0 {
            self.context.symbol_table = linker.into_symbol_table();
        }

        FindingsByStage { ast, unlinked, linked: Some(checker.findings()) }
    }

    fn publish(&mut self, findings: FindingsByStage) -> Result<()> {
        let total = findings.ast.len()
            + findings.unlinked.as_ref().map_or(0, |v| v.len())
            + findings.linked.as_ref().map_or(0, |v| v.len());
        eprintln!("All findings: {}", total);

        let ast_map = self.group_findings(Some(findings.ast));
        let unlinked_map = self.group_findings(findings.unlinked);
        let linked_map = self.group_findings(findings.linked);

        // Ensure we clear diagnostics for files that no longer have issues
        let mut targets: HashSet<String> = self.context.active_findings.keys().cloned().collect();

        // Add all catalog files (so clean files get cleared)
        for (path, _) in &self.context.catalog.catalog_vsidl_files {
            let abs = self.cli.catalog_path.join(path).to_string_lossy().to_string();
            targets.insert(self.map_to_source(&abs));
        }
        targets.extend(ast_map.keys().cloned());
        targets.extend(unlinked_map.keys().cloned());
        targets.extend(linked_map.keys().cloned());

        for path in targets {
            let ast = Some(ast_map.get(&path).cloned().unwrap_or_default());
            let unlinked = Some(unlinked_map.get(&path).cloned().unwrap_or_default());
            let linked = Some(linked_map.get(&path).cloned().unwrap_or_default());
            self.send_diagnostics(&path, ast, unlinked, linked)?;
        }
        Ok(())
    }

    fn group_findings(&self, findings: Option<Vec<Finding>>) -> HashMap<String, Vec<Finding>> {
        let mut map = HashMap::new();
        if let Some(findings) = findings {
            for f in findings {
                if let Some(path) = f.spans.iter().flatten().next().map(|s| s.file_path.clone()) {
                    map.entry(self.map_to_source(&path)).or_insert_with(Vec::new).push(f);
                }
            }
        }
        map
    }

    fn map_to_source(&self, path: &str) -> String {
        self.context
            .shadow_workspace
            .as_deref()
            .map(|s| s.map_to_source(path))
            .unwrap_or_else(|| path.to_string())
    }

    fn send_diagnostics(
        &mut self,
        path: &str,
        ast: Option<Vec<Finding>>,
        unlinked: Option<Vec<Finding>>,
        linked: Option<Vec<Finding>>,
    ) -> Result<()> {
        let entry = self.context.active_findings.entry(path.to_string()).or_default();

        if let Some(v) = ast {
            entry.ast = v;
        }
        if let Some(v) = unlinked {
            entry.unlinked = v;
        }
        if let Some(v) = linked {
            entry.linked = v;
        }

        let mut all_findings = Vec::new();
        all_findings.extend(entry.ast.iter().cloned());
        all_findings.extend(entry.unlinked.iter().cloned());
        all_findings.extend(entry.linked.iter().cloned());

        if all_findings != entry.last_sent {
            eprintln!("Sending diagnostics for {}: {} findings", path, all_findings.len());
            let params = PublishDiagnosticsParams {
                uri: Url::from_file_path(path)
                    .map_err(|_| anyhow::anyhow!("Invalid URL: {}", path))?,
                diagnostics: all_findings.iter().map(|f| f.to_diagnostic(self.provider)).collect(),
                version: None,
            };

            self.connection.sender.send(Message::Notification(lsp_server::Notification {
                method: "textDocument/publishDiagnostics".into(),
                params: serde_json::to_value(params)?,
            }))?;
            entry.last_sent = all_findings;
        }
        Ok(())
    }

    fn report_syntax_error(&mut self, path: &Path, e: PestError<Rule>) -> Result<()> {
        eprintln!("❌ Syntax Error: {:#?}", e);
        let file_path = self.map_to_source(&path.to_string_lossy());
        let start = match e.location {
            pest::error::InputLocation::Pos(pos) => pos as u32,
            pest::error::InputLocation::Span((start, _)) => start as u32,
        };

        let finding = Finding {
            severity: FindingSeverity::Fatal,
            id: FindingId::ParserError,
            message: format!("Syntax Error: {}", e.variant.message()),
            spans: vec![Some(Span {
                file_path: file_path.clone(),
                start,
                end: start,
                ..Default::default()
            })],
        };

        self.send_diagnostics(&file_path, Some(vec![finding]), None, None)
    }
}
