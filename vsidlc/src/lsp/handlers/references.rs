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

use crate::lsp::utils::{convert_position_0_based_to_1_based_index, location_from_span};
use crate::Cli;
use anyhow::anyhow;
use lsp_types::{Location, ReferenceParams};
use vsidl_v1::syntax::Span;
use vsidl_v1_parser::file_provider::FileProvider;
use vsidl_v1_parser::symbol::SymbolKind;
use vsidl_v1_parser::{line_col_to_offset, SymbolTable};

pub(crate) fn find_references<F: FileProvider>(
    _cli: Cli,
    params: ReferenceParams,
    symbol_table: &SymbolTable,
    file_provider: &F,
) -> anyhow::Result<Option<serde_json::value::Value>> {
    eprintln!(
        "find_references called with uri: {:?}, position: {:?}",
        params.text_document_position.text_document.uri, params.text_document_position.position
    );
    let uri = params.text_document_position.text_document.uri;
    let mut position = params.text_document_position.position;
    position = convert_position_0_based_to_1_based_index(position);
    let mut references_spans: Vec<Span> = Vec::new();

    if let Ok(path) = uri.to_file_path() {
        let offset = line_col_to_offset(
            file_provider,
            path.to_str().ok_or_else(|| anyhow!("Invalid path string"))?,
            position.line as usize,
            position.character as usize,
        )?;

        if let Some(symbol_under_cursor) = symbol_table.get_symbol_at_location(&path, offset) {
            match symbol_under_cursor {
                SymbolKind::Publisher(publisher) => {
                    let subscribers = publisher.publication(symbol_table).subscribers(symbol_table);
                    for subscriber in subscribers {
                        if let Some(span) = subscriber.span() {
                            references_spans.push(span);
                        }
                    }
                }
                SymbolKind::Subscriber(subscriber) => {
                    let publishers = subscriber.publication(symbol_table).publishers(symbol_table);
                    for publisher in publishers {
                        if let Some(span) = publisher.span() {
                            references_spans.push(span);
                        }
                    }
                }
                SymbolKind::RpcServer(server) => {
                    let clients = server.service(symbol_table).clients(symbol_table);
                    for client in clients {
                        if let Some(span) = client.span() {
                            references_spans.push(span);
                        }
                    }
                }
                SymbolKind::RpcClient(client) => {
                    let servers = client.service(symbol_table).servers(symbol_table);
                    for server in servers {
                        if let Some(span) = server.span() {
                            references_spans.push(span);
                        }
                    }
                }
                _ => (),
            };
        }
    }

    let locations: Vec<Location> = references_spans
        .iter()
        .map(|span| location_from_span(file_provider, span.clone()))
        .filter_map(std::result::Result::ok)
        .collect();

    let result = serde_json::to_value(&locations)?;
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::lsp_file_provider::LspFileProvider;
    use crate::lsp::test_utils::{setup_mock_cli, setup_test_catalog};
    use lsp_types::{
        PartialResultParams, Position, ReferenceContext, TextDocumentIdentifier,
        TextDocumentPositionParams, Url, WorkDoneProgressParams,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use vsidl_v1_parser::file_provider::DiskFileProvider;
    use vsidlc_cli::VsidlcCommonCli;

    #[test]
    fn test_find_references() -> anyhow::Result<()> {
        // GIVEN A valid catalog
        let temp_dir = setup_test_catalog();
        let cli = setup_mock_cli(&temp_dir);
        let common_cli = VsidlcCommonCli::from(cli.clone());

        let document_store = Arc::new(Mutex::new(HashMap::new()));
        let file_provider = LspFileProvider {
            document_store: Arc::clone(&document_store),
            file_hashes: Arc::new(Mutex::new(HashMap::new())),
            disk_fallback: DiskFileProvider,
        };
        let symbol_table = common_cli.create_symbol_table(None)?;

        // GIVEN cursor position on "TirePressure" in subscriber
        let position = Position { line: 10, character: 20 };

        let vsidl_file_path = temp_dir.path().join("test.vsidl");

        // WHEN requesting references of the symbol under the cursor
        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(vsidl_file_path.clone()).unwrap(),
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: ReferenceContext { include_declaration: false },
        };
        let result = find_references(cli.clone(), params, &symbol_table, &file_provider)?;

        // THEN publisher for "TirePressure" is found
        assert!(result.is_some());
        let locations: Vec<Location> = serde_json::from_value(result.unwrap())?;
        assert_eq!(locations.len(), 1);
        let location = &locations[0];
        assert_eq!(location.uri, Url::from_file_path(vsidl_file_path).unwrap());
        // publisher is from line 5 to 8 (1-indexed), which is 4 to 7 (0-indexed).
        assert_eq!(location.range.start.line, 4);
        assert_eq!(location.range.start.character, 2);
        assert_eq!(location.range.end.line, 7);
        assert_eq!(location.range.end.character, 3);

        Ok(())
    }
}
