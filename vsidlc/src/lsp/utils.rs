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
use crate::Cli;
use anyhow::anyhow;
use lsp_types::{Location, Position, Range, Url};
use protobuf::text_format;
use regenerator_proto::catalog::Catalog as CatalogUpdate;
use std::cmp::max;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use vsidl_v1::span::Span;
use vsidl_v1_parser::file_provider::FileProvider;
use vsidl_v1_parser::{offset_to_line_col, Catalog, SymbolTable, TargetApi};

/// Resolves the symbol table for a given URI.
///
/// It determines the context based on the URI and retrieves the corresponding symbol table.
/// If no context is found, it inserts a new default context and returns its symbol table.
pub(crate) fn get_symbol_table_for_file<'a>(
    uri: &Url,
    cli: &Cli,
    contexts: &'a mut HashMap<PathBuf, CatalogContext>,
) -> &'a SymbolTable {
    let (context_key, context_cli) = if let Ok(path) = uri.to_file_path() {
        resolve_context(&path, cli)
    } else {
        (PathBuf::default(), cli.clone())
    };

    let context = contexts.entry(context_key).or_insert_with(|| {
        let mut symbol_table = SymbolTable::default();
        match context_cli.target_api.as_str() {
            "latest" => symbol_table.set_target_api(vsidl_v1_parser::TargetApi::Latest),
            _ => symbol_table.set_target_api(vsidl_v1_parser::TargetApi::Pdk29),
        }
        CatalogContext {
            cli: context_cli,
            symbol_table,
            catalog: Catalog::new(),
            last_valid_symbol_table: None,
            active_findings: HashMap::new(),
            shadow_workspace: None,
            recovered_syntax_errors: HashMap::new(),
        }
    });

    context.last_valid_symbol_table.as_ref().unwrap_or(&context.symbol_table)
}

/// Converts a 0-based (LSP) Position to a 1-based index Position (Pest).
pub(crate) fn convert_position_0_based_to_1_based_index(mut position: Position) -> Position {
    position.line += 1;
    position.character += 1;
    position
}

/// Converts a 1-based (Pest) Position to a 0-based index Position (LSP).
pub(crate) fn convert_line_col_1_based_to_0_based_index((line, col): (usize, usize)) -> (u32, u32) {
    ((max(line, 1) - 1) as u32, (max(col, 1) - 1) as u32)
}

pub(crate) fn location_from_span<F: FileProvider>(
    file_provider: &F,
    span: Span,
) -> anyhow::Result<Location> {
    let uri = Url::from_file_path(&span.file_path)
        .map_err(|_| anyhow!("Failed to convert file path to URI: {}", span.file_path))?;

    // Calculate start position
    let line_col: (usize, usize) =
        offset_to_line_col(file_provider, &span.file_path, span.start as usize).map_err(|_| {
            anyhow!("Failed to convert offset '{}' to line/column: {}", span.start, span.file_path)
        })?;
    let (line, character) = convert_line_col_1_based_to_0_based_index(line_col);
    let start_position = Position { line, character };

    // Calculate end position
    let line_col: (usize, usize) =
        offset_to_line_col(file_provider, &span.file_path, span.end as usize).map_err(|_| {
            anyhow!("Failed to convert offset '{}' to line/column: {}", span.start, span.file_path)
        })?;
    let (line, character) = convert_line_col_1_based_to_0_based_index(line_col);
    let end_position = Position { line, character };

    let range = Range { start: start_position, end: end_position };
    Ok(Location { uri, range })
}

/// Resolves the catalog context (root path and CLI configuration) for a given file path.
///
/// It attempts to find a `CATALOG_UPDATE` file in the current directory or parent directories.
/// - If found, it parses the file for dependency paths and uses the directory containing
///   `CATALOG_UPDATE` as the catalog root.
/// - If not found, it falls back to using the file's parent directory as the catalog root.
///
/// # Arguments
/// * `path` - The path of the file being edited/opened.
/// * `default_cli` - The default CLI configuration to clone and modify.
///
/// # Returns
/// * `(PathBuf, Cli)` - A tuple containing the resolved catalog root path and the modified CLI configuration.
pub(crate) fn resolve_context(path: &Path, default_cli: &Cli) -> (PathBuf, Cli) {
    let (mut root, deps, target_api) =
        if let Some((root, deps, target_api)) = find_catalog_configuration(path) {
            eprintln!("Using catalog configuration from {:?} for file {:?}", root, path);
            (root, deps, target_api)
        } else {
            let root = if path.is_dir() {
                path.to_path_buf()
            } else {
                path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
            };
            eprintln!("No CATALOG_UPDATE found. Using file directory as catalog root: {:?}", root);
            (root, default_cli.dependency_catalog_path.clone(), TargetApi::Pdk29)
        };

    // Try to canonicalize root to avoid redundant contexts
    if let Ok(canonical) = std::fs::canonicalize(&root) {
        root = canonical;
    }

    let mut c = default_cli.clone();
    c.catalog_path = Some(root.clone());
    c.dependency_catalog_path = deps;
    c.target_api =
        if target_api == TargetApi::Latest { "latest".to_string() } else { "pdk29".to_string() };
    (root, c)
}

/// Searches for "CATALOG_UPDATE" in the current directory and parent directories.
/// If found, parses it to extract the catalog root (the directory containing CATALOG_UPDATE),
/// any dependency catalog paths defined within it, and the TargetApi used by the Catalog.
///
/// # Arguments
/// * `start_path` - The path to start searching from (usually the active file).
///
/// # Returns
/// * `Some((catalog_root, dependency_paths, TargetApi))` if found.
/// * `None` if not found.
fn find_catalog_configuration(start_path: &Path) -> Option<(PathBuf, Vec<PathBuf>, TargetApi)> {
    let mut current_dir = if start_path.is_dir() {
        start_path.to_path_buf()
    } else {
        start_path.parent().map(|p| p.to_path_buf())?
    };

    // \s+ matches one or more whitespace characters
    // Clippy requires this to be outside the loop
    let re = regex::Regex::new(r"--target-api\s+latest").expect("Regex should be valid");

    loop {
        let candidate = current_dir.join("CATALOG_UPDATE");
        if candidate.exists() && candidate.is_file() {
            eprintln!("Found CATALOG_UPDATE at {:?}", candidate);
            // Parse the file
            let content = match std::fs::read_to_string(&candidate) {
                Ok(content) => content,
                Err(e) => {
                    eprintln!("Failed to read catalog config {:?}: {}", candidate, e);
                    return None;
                }
            };
            let catalog_update = text_format::parse_from_str::<CatalogUpdate>(content.as_str())
                .unwrap_or_else(|_| {
                    eprintln!("Failed to parse catalog config: {candidate:#?}");
                    CatalogUpdate::new()
                });
            let dependency_paths = catalog_update
                .dependency_catalog_paths
                .into_iter()
                .map(|dep| current_dir.join(dep))
                .collect();
            let uses_latest =
                catalog_update.vsidlc_gen_args.as_deref().is_some_and(|s| re.is_match(s));
            return Some((
                current_dir,
                dependency_paths,
                if uses_latest { TargetApi::Latest } else { TargetApi::Pdk29 },
            ));
        }

        if let Some(parent) = current_dir.parent() {
            current_dir = parent.to_path_buf();
        } else {
            break;
        }
    }
    None
}
