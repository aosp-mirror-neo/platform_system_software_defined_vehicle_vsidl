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
use crate::lsp::recovering_parser::parse_vsidl_with_recovery;
use crate::lsp::span_source_remapper::SpanSourceRemapper;
use anyhow::Result;
use lsp_server::{Connection, Message};
use lsp_types::{PublishDiagnosticsParams, Url};
use pest::error::Error as PestError;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use vsidl_v1::span::Span;
use vsidl_v1_parser::converter::convert_to_vsidl_entry;
use vsidl_v1_parser::file_provider::FileProvider;
use vsidl_v1_parser::pest_parser::{format_pest_error, Rule};
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
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut session = DiagnosticSession::new(context, connection, file_provider);
        session.run(changed_path)
    }));

    match result {
        Ok(res) => res,
        Err(_) => {
            eprintln!("Panic during diagnostics for: {:?}", changed_path);
            Ok(())
        }
    }
}

struct DiagnosticSession<'a> {
    context: &'a mut CatalogContext,
    connection: &'a Connection,
    provider: &'a LspFileProvider,
    cli: VsidlcCommonCli,
    touched_files: HashSet<String>,
}

#[allow(clippy::unnecessary_unwrap)]
impl<'a> DiagnosticSession<'a> {
    fn new(
        context: &'a mut CatalogContext,
        connection: &'a Connection,
        provider: &'a LspFileProvider,
    ) -> Self {
        let mut cli = VsidlcCommonCli::from(context.cli.clone());

        if let Some(ref p) = context.cli.catalog_path {
            cli.catalog_path = provider.canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        }

        cli.dependency_catalog_path = cli
            .dependency_catalog_path
            .iter()
            .map(|p| provider.canonicalize(p).unwrap_or_else(|_| p.to_path_buf()))
            .collect();

        Self { context, connection, provider, cli, touched_files: HashSet::new() }
    }

    fn run(&mut self, changed_path: Option<&Path>) -> Result<()> {
        let changed_path =
            changed_path.map(|p| self.provider.canonicalize(p).unwrap_or_else(|_| p.to_path_buf()));

        if let Some(ref p) = changed_path {
            let source_path = self.map_to_source(&p.to_string_lossy());
            self.touched_files.insert(source_path);
        }

        let mut findings =
            FindingsByStage { ast: vec![], unlinked: Some(vec![]), linked: Some(vec![]) };

        // 1. Update Catalog
        let update_result = self.update_catalog(changed_path.as_deref())?;

        if let Some(incremental) = update_result {
            // All files in the catalog are touched during a semantic project check.
            for (path, _) in &self.context.catalog.catalog_vsidl_files {
                let abs = self.cli.catalog_path.join(path).to_string_lossy().to_string();
                let source = self.map_to_source(&abs);
                self.touched_files.insert(source);
            }

            // 2. Prepare Symbols
            let creator = self.prepare_symbols(changed_path.as_deref(), incremental);
            // 3. Collect Findings
            findings = self.collect_findings(creator);
        }

        // 4. Publish (Always call if an update was attempted)
        self.publish(findings)?;
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

        if catalog_populated && !is_proto {
            if let Some(path) = changed_path {
                if let Some(success) = self.try_update_incremental(path)? {
                    return Ok(success);
                }
                eprintln!("Incremental updated failed, falling back to full disk scan");
            }
        }

        self.perform_full_update()
    }

    /// Attempts to perform an incremental update of the catalog for a single file.
    ///
    /// This method tries to parse the file at `path`, re-mapping spans if a shadow
    /// workspace is active. If parsing succeeds (or recovers from errors), it
    /// updates the catalog's entry for that file.
    ///
    /// # Returns
    ///
    /// A `Result` indicating if the operation was technically successful (no I/O crashes).
    /// The inner `Option<Option<bool>>` represents the outcome:
    ///
    /// * `Ok(Some(Some(true)))` - **Handled Incremental**: The file was successfully updated
    ///   in the catalog. The caller should proceed with incremental symbol updates.
    /// * `Ok(Some(None))` - **Handled Error**: A syntax error occurred in the file and was
    ///   reported. Downstream processing (linking) should be skipped.
    /// * `Ok(None)` - **Not Handled**: The file is not eligible for incremental update
    ///   (e.g., not in the catalog path). The caller should fallback to a full update.
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

        let mut content = match self.provider.read_to_string(&path_to_read) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to read file: {:?}", e);
                return Err(e);
            }
        };

        let (result, recovered_errors) = parse_vsidl_with_recovery(&mut content);
        let file_path_key = self.map_to_source(&abs_path.to_string_lossy());
        self.touched_files.insert(file_path_key.clone());

        match result {
            Ok(mut pairs) => {
                let Some(pair) = pairs.next() else {
                    return Ok(Some(Some(true))); // Empty file or something similar
                };
                let mut entry = convert_to_vsidl_entry(pair, abs_path.to_str().unwrap_or_default());
                if let Some(shadow) = self.context.shadow_workspace.as_deref() {
                    let mut updater = SpanSourceRemapper { shadow_workspace: Some(shadow) };
                    AstNodeMut::accept(&mut entry, &mut updater);
                }
                self.context.catalog.catalog_vsidl_files.retain(|(p, _)| p != rel);
                self.context.catalog.catalog_vsidl_files.push((rel.to_path_buf(), entry));

                if !recovered_errors.is_empty() {
                    self.store_recovered_syntax_errors(&abs_path, recovered_errors)?;
                    // Put the patched version into the shadow workspace so other tools see it as valid.
                    if let Some(shadow) = self.context.shadow_workspace.as_ref() {
                        let _shadow_path = shadow.map_to_shadow(&abs_path);
                        let _ = shadow.update_file(&abs_path, &content);
                    }
                } else {
                    self.context.recovered_syntax_errors.remove(&file_path_key);
                }

                Ok(Some(Some(true))) // Handled, is_incremental=true
            }
            Err(e) => {
                let mut errors_to_report = recovered_errors;
                if errors_to_report.is_empty() {
                    errors_to_report.push(e);
                }
                let path = errors_to_report[0]
                    .path()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| abs_path.clone());
                self.store_recovered_syntax_errors(&path, errors_to_report)?;
                Ok(Some(None)) // Handled, but error occurred, stop.
            }
        }
    }

    /// Performs a full update of the VSIDL catalog by scanning the disk.
    ///
    /// This clears the current catalog and performs a complete re-read of all
    /// files in the catalog and dependency paths. If a syntax error is encountered,
    /// it attempts to use the recovering parser and a shadow workspace to
    /// continue diagnostics.
    ///
    /// # Returns
    ///
    /// A `Result` indicating if the operation was technically successful. The inner
    /// `Option<bool>` represents the outcome:
    ///
    /// * `Ok(Some(false))` - **Full Update**: The entire catalog was successfully rebuilt.
    ///   The caller must rebuild the entire symbol table.
    /// * `Ok(None)` - **Abort**: A syntax error was reported; downstream processing
    ///   (linking) should be skipped.
    fn perform_full_update(&mut self) -> Result<Option<bool>> {
        let syntax_path =
            vsidl_v1_parser::get_vsidl_syntax_default_path().unwrap_or_else(|_| PathBuf::from("."));

        // Clear all previously recovered syntax errors for the main catalog
        // before starting a full update.
        self.context.recovered_syntax_errors.clear();

        // Track files we have already attempted to recover to avoid infinite loops
        // if the recovery parser produces content that the strict parser still rejects.
        let mut recovered_files = HashSet::new();

        loop {
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

                    // All files in the catalog are touched during a full update.
                    for (path, _) in &self.context.catalog.catalog_vsidl_files {
                        let abs = self.cli.catalog_path.join(path).to_string_lossy().to_string();
                        self.touched_files.insert(self.map_to_source(&abs));
                    }

                    return Ok(Some(false)); // Success, is_incremental=false
                }
                Err(e) => {
                    if let Some(pest_err) = e.downcast_ref::<PestError<Rule>>() {
                        let error_path_str = pest_err.path().unwrap_or_default();
                        let source_path_str = self.map_to_source(error_path_str);
                        let source_path = PathBuf::from(&source_path_str);

                        // If we've already tried to recover this file in this specific session
                        // and it's failing again, the recovery logic is insufficient for the
                        // strict parser. Stop to avoid an infinite loop.
                        if !recovered_files.insert(source_path.clone()) {
                            eprintln!(
                                "❌ Recovery failed to fix syntax in {:?}; stopping.",
                                source_path
                            );
                            self.store_recovered_syntax_errors(
                                &source_path,
                                vec![pest_err.clone()],
                            )?;
                            return Ok(None);
                        }

                        if let Some(shadow) = self.context.shadow_workspace.as_ref().cloned() {
                            // Read the current (possibly dirty) content
                            if let Ok(mut content) = self.provider.read_to_string(&source_path) {
                                let (recovery_result, recovered_errors) =
                                    parse_vsidl_with_recovery(&mut content);

                                match recovery_result {
                                    Ok(_) => {
                                        // Update shadow workspace with the RECOVERED content
                                        let _ = shadow.update_file(&source_path, &content);

                                        if !recovered_errors.is_empty() {
                                            self.store_recovered_syntax_errors(
                                                &source_path,
                                                recovered_errors,
                                            )?;
                                        }
                                        // Retry the catalog scan with the updated shadow file
                                        continue;
                                    }
                                    Err(recovery_err) => {
                                        // Even the recovering parser gave up on this file.
                                        let mut all_errors = recovered_errors;
                                        if all_errors.is_empty() {
                                            all_errors.push(recovery_err);
                                        }
                                        self.store_recovered_syntax_errors(
                                            &source_path,
                                            all_errors,
                                        )?;
                                        return Ok(None);
                                    }
                                }
                            }
                        }

                        // Fallback if no shadow workspace or failed to read/recover
                        self.store_recovered_syntax_errors(&source_path, vec![pest_err.clone()])?;
                    } else {
                        eprintln!("❌ Error creating catalog: {:#?}", e);
                    }
                    return Ok(None); // Stop
                }
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
            let Some(raw_path) = changed_path else {
                return self.prepare_symbols(None, false);
            };
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
            creator.set_target_api(match self.cli.target_api.as_str() {
                "latest" => vsidl_v1_parser::TargetApi::Latest,
                _ => vsidl_v1_parser::TargetApi::Pdk29,
            });
            creator.restrict_to_file(path_to_use);
            self.context.catalog.accept(&mut creator);
            creator
        } else {
            eprintln!("Full symbol table build");
            let mut creator = SymbolTableCreator::with_file_provider(self.provider);
            creator.set_target_api(match self.cli.target_api.as_str() {
                "latest" => vsidl_v1_parser::TargetApi::Latest,
                _ => vsidl_v1_parser::TargetApi::Pdk29,
            });
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

        match self.cli.target_api.as_str() {
            "latest" => checker.enable_latest(),
            _ => checker.enable_pdk29(),
        };

        let mut total_errors = 0;

        // 1. AST Check
        KeywordConverter::traverse(&mut self.context.catalog);
        let _ = checker.check_ast(&self.context.catalog);
        total_errors += checker.error_count();
        let ast = checker.findings();
        checker.clear_findings();

        // 2. Unlinked Check
        let created_symbol_table = creator.symbol_table();

        // Only execute next stage of CoCos if previous stage succeeded
        // Otherwise, CoCo might panic (because symbol table contains errors) and
        // found errors may just be consequences of earlier errors that do not need
        // to be addressed by the developer
        if ast.is_empty() {
            let _ = checker.check_symbol_table(created_symbol_table);
            total_errors += checker.error_count();
        }
        let unlinked = Some(checker.findings());
        checker.clear_findings();

        // 3. Linked Check
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        if ast.is_empty() && unlinked.as_deref().is_none_or(|v| v.is_empty()) {
            let _ = checker.check_linked_symbol_table(linker.symbol_table());
            total_errors += checker.error_count();
        }

        // Always update the symbol table for LSP features, even if there are errors.
        self.context.symbol_table = linker.into_symbol_table();

        if total_errors == 0 {
            self.context.last_valid_symbol_table = Some(self.context.symbol_table.clone());
        }

        FindingsByStage { ast, unlinked, linked: Some(checker.findings()) }
    }

    fn publish(&mut self, findings: FindingsByStage) -> Result<()> {
        let _total = findings.ast.len()
            + findings.unlinked.as_ref().map_or(0, |v| v.len())
            + findings.linked.as_ref().map_or(0, |v| v.len());

        let mut ast_map = self.group_findings(Some(findings.ast));

        // Merge persistent recovered syntax errors
        for (path, errors) in &self.context.recovered_syntax_errors {
            ast_map.entry(path.clone()).or_default().extend(errors.clone());
        }

        let unlinked_map = findings.unlinked.as_ref().map(|f| self.group_findings(Some(f.clone())));
        let linked_map = findings.linked.as_ref().map(|f| self.group_findings(Some(f.clone())));

        // Targets for diagnostic updates.
        let mut targets: HashSet<String> = HashSet::new();
        targets.extend(self.touched_files.clone());
        targets.extend(ast_map.keys().cloned());
        if let Some(ref m) = unlinked_map {
            targets.extend(m.keys().cloned());
        }
        if let Some(ref m) = linked_map {
            targets.extend(m.keys().cloned());
        }

        for path in targets {
            // Only send diagnostics for files that belong to the main catalog.
            // Diagnostics for dependency catalogs should be handled by their own context
            // if they are also open in the IDE.
            if !self.is_in_main_catalog(&path) {
                continue;
            }

            let ast = Some(ast_map.get(&path).cloned().unwrap_or_default());
            let unlinked = unlinked_map.as_ref().map(|m| m.get(&path).cloned().unwrap_or_default());
            let linked = linked_map.as_ref().map(|m| m.get(&path).cloned().unwrap_or_default());

            self.send_diagnostics(&path, ast, unlinked, linked)?;
        }
        Ok(())
    }

    fn group_findings(&self, findings: Option<Vec<Finding>>) -> HashMap<String, Vec<Finding>> {
        let mut map = HashMap::new();
        if let Some(findings) = findings {
            for f in findings {
                if let Some(path) = f.spans.iter().flatten().next().map(|s| s.file_path.clone()) {
                    let mapped_path = self.map_to_source(&path);
                    map.entry(mapped_path).or_insert_with(Vec::new).push(f);
                }
            }
        }
        map
    }

    fn map_to_source(&self, path: &str) -> String {
        let mapped = self
            .context
            .shadow_workspace
            .as_deref()
            .map(|s| s.map_to_source(path))
            .unwrap_or_else(|| path.to_string());
        mapped
    }

    /// Checks if a file path belongs to the main catalog being validated in this session.
    fn is_in_main_catalog(&self, path: &str) -> bool {
        let path_buf = PathBuf::from(path);
        let abs_path = self.provider.canonicalize(&path_buf).unwrap_or(path_buf);
        abs_path.starts_with(&self.cli.catalog_path)
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
            eprintln!("    Sending notification for {}: {} findings", path, all_findings.len());
            let uri =
                Url::from_file_path(path).map_err(|_| anyhow::anyhow!("Invalid URL: {}", path))?;
            let params = PublishDiagnosticsParams {
                uri,
                diagnostics: all_findings.iter().map(|f| f.to_diagnostic(self.provider)).collect(),
                version: None,
            };

            self.connection.sender.send(Message::Notification(lsp_server::Notification {
                method: "textDocument/publishDiagnostics".into(),
                params: serde_json::to_value(params)?,
            }))?;
            entry.last_sent = all_findings.clone();
        }
        Ok(())
    }

    fn store_recovered_syntax_errors(
        &mut self,
        path: &Path,
        errors: Vec<PestError<Rule>>,
    ) -> Result<()> {
        let file_path = self.map_to_source(&path.to_string_lossy());
        let mut findings = Vec::new();

        for e in errors {
            let (start, end) = match e.location {
                pest::error::InputLocation::Pos(pos) => (pos as u32, pos as u32),
                pest::error::InputLocation::Span((start, end)) => (start as u32, end as u32),
            };

            let message = format!("Syntax Error: {}", format_pest_error(&e));

            findings.push(Finding {
                severity: FindingSeverity::Fatal,
                id: FindingId::ParserError,
                message: message.clone(),
                spans: vec![Some(Span {
                    file_path: file_path.clone(),
                    start,
                    end,
                    ..Default::default()
                })],
            });
        }
        self.context.recovered_syntax_errors.insert(file_path, findings);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::lsp_file_provider::LspFileProvider;
    use crate::lsp::test_utils::{setup_mock_cli, setup_test_catalog};
    use lsp_server::Connection;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use vsidl_v1_parser::file_provider::DiskFileProvider;
    use vsidl_v1_parser::{Catalog, SymbolTable};

    #[test]
    fn test_is_in_main_catalog() {
        let temp_dir = setup_test_catalog();
        let cli = setup_mock_cli(&temp_dir);
        let catalog_path = cli.catalog_path.clone().unwrap();

        let document_store = Arc::new(Mutex::new(HashMap::new()));
        let file_provider = LspFileProvider {
            document_store: Arc::clone(&document_store),
            file_hashes: Arc::new(Mutex::new(HashMap::new())),
            disk_fallback: DiskFileProvider,
        };

        let mut symbol_table = SymbolTable::default();
        match cli.target_api.as_str() {
            "latest" => symbol_table.set_target_api(vsidl_v1_parser::TargetApi::Latest),
            _ => symbol_table.set_target_api(vsidl_v1_parser::TargetApi::Pdk29),
        }

        let mut context = CatalogContext {
            cli: cli.clone(),
            symbol_table,
            catalog: Catalog::new(),
            last_valid_symbol_table: None,
            active_findings: HashMap::new(),
            shadow_workspace: None,
            recovered_syntax_errors: HashMap::new(),
        };

        let (connection, _) = Connection::memory();
        let session = DiagnosticSession::new(&mut context, &connection, &file_provider);

        // File inside main catalog
        let inside = catalog_path.join("test.vsidl").to_string_lossy().to_string();
        assert!(session.is_in_main_catalog(&inside));

        // File in subdirectory
        let sub = catalog_path.join("protos/test.proto").to_string_lossy().to_string();
        assert!(session.is_in_main_catalog(&sub));

        // File outside main catalog
        let outside = "/tmp/outside.vsidl";
        assert!(!session.is_in_main_catalog(outside));
    }
}
