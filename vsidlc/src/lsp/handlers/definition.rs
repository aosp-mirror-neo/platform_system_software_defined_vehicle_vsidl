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
use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse};
use vsidl_v1::syntax::Span;
use vsidl_v1_parser::file_provider::FileProvider;
use vsidl_v1_parser::symbol::SymbolKind;
use vsidl_v1_parser::{line_col_to_offset, SymbolTable};

pub(crate) fn go_to_definition<F: FileProvider>(
    _cli: Cli,
    params: GotoDefinitionParams,
    symbol_table: &SymbolTable,
    file_provider: &F,
) -> anyhow::Result<Option<serde_json::value::Value>> {
    eprintln!(
        "go_to_definition called with uri: {:?}, position: {:?}",
        params.text_document_position_params.text_document.uri,
        params.text_document_position_params.position
    );
    let uri = params.text_document_position_params.text_document.uri;
    let mut position = params.text_document_position_params.position;
    position = convert_position_0_based_to_1_based_index(position);
    let mut definition_location: Option<Span> = None;

    if let Ok(path) = uri.to_file_path() {
        let offset = line_col_to_offset(
            file_provider,
            path.to_str().ok_or_else(|| anyhow!("Invalid path string"))?,
            position.line as usize,
            position.character as usize,
        )?;

        if let Some(symbol_under_cursor) = symbol_table.get_symbol_at_location(&path, offset) {
            definition_location = match symbol_under_cursor {
                SymbolKind::Publisher(publisher) => {
                    if !publisher.topic_span.is_empty()
                        && offset >= publisher.topic_span[0].start as usize
                        && offset <= publisher.topic_span[0].end as usize
                    {
                        publisher.topic_symbols(symbol_table)[0].span()
                    } else if offset >= publisher.message_span.start as usize
                        && offset <= publisher.message_span.end as usize
                    {
                        publisher.message(symbol_table).span()
                    } else {
                        // The cursor is on a part of the Publisher symbol that doesn't link
                        // to a specific definition (e.g., a comment).
                        None
                    }
                }
                SymbolKind::Subscriber(subscriber) => {
                    if !subscriber.topic_span.is_empty()
                        && offset >= subscriber.topic_span[0].start as usize
                        && offset <= subscriber.topic_span[0].end as usize
                    {
                        subscriber.topic_symbols(symbol_table)[0].span()
                    } else if offset >= subscriber.message_span.start as usize
                        && offset <= subscriber.message_span.end as usize
                    {
                        subscriber.message(symbol_table).span()
                    } else {
                        // The cursor is on a part of the Subscriber symbol that doesn't link
                        // to a specific definition (e.g., a comment).
                        None
                    }
                }
                SymbolKind::RpcServer(server) => {
                    if offset >= server.service_span.start as usize
                        && offset <= server.service_span.end as usize
                    {
                        server.service(symbol_table).span()
                    } else {
                        // The cursor is on a part of the RpcServer symbol that doesn't link
                        // to a specific definition (e.g., a comment).
                        None
                    }
                }
                SymbolKind::RpcClient(client) => {
                    if offset >= client.service_span.start as usize
                        && offset <= client.service_span.end as usize
                    {
                        client.service(symbol_table).span()
                    } else {
                        // The cursor is on a part of the RpcClient symbol that doesn't link
                        // to a specific definition (e.g., a comment).
                        None
                    }
                }
                _ => None,
            };
        }
    }

    let response = definition_location
        .map(|span| location_from_span(file_provider, span).map(GotoDefinitionResponse::Scalar))
        .transpose()?;

    let result = serde_json::to_value(&response)?;
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::lsp_file_provider::LspFileProvider;
    use crate::lsp::test_utils::{setup_mock_cli, setup_test_catalog};
    use lsp_types::{
        PartialResultParams, Position, TextDocumentIdentifier, TextDocumentPositionParams, Url,
        WorkDoneProgressParams,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use vsidl_v1_parser::file_provider::DiskFileProvider;
    use vsidlc_cli::VsidlcCommonCli;

    #[test]
    fn test_go_to_definition() -> anyhow::Result<()> {
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
        let symbol_table = common_cli.create_symbol_table(Some(".".into()))?;

        // GIVEN cursor position on "TirePressure" in subscriber
        let position = Position { line: 10, character: 20 };

        let vsidl_file_path = temp_dir.path().join("test.vsidl");
        let proto_file_path = temp_dir.path().join("protos/test.proto");

        // WHEN requesting the definition of the symbol under the cursor
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(vsidl_file_path).unwrap(),
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let result = go_to_definition(cli.clone(), params, &symbol_table, &file_provider)?;

        // THEN Proto message for "TirePressure" is found
        assert!(result.is_some());
        let response: GotoDefinitionResponse = serde_json::from_value(result.unwrap())?;
        eprintln!("Found response: {:?}", response);
        match response {
            GotoDefinitionResponse::Scalar(location) => {
                assert_eq!(location.uri, Url::from_file_path(proto_file_path).unwrap());
                assert_eq!(location.range.start.line, 13);
                assert_eq!(location.range.start.character, 0);
                assert_eq!(location.range.end.line, 21);
                assert_eq!(location.range.end.character, 1);
            }
            _ => panic!("Expected a scalar response"),
        }
        Ok(())
    }
}
