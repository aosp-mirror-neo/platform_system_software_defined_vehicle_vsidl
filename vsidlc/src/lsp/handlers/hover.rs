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

use crate::Cli;
use anyhow::anyhow;
use lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind};
use vsidl_v1_parser::file_provider::FileProvider;
use vsidl_v1_parser::symbol::SymbolKind;
use vsidl_v1_parser::{line_col_to_offset, SymbolTable};

pub(crate) fn hover<F: FileProvider>(
    _cli: Cli,
    params: &HoverParams,
    symbol_table: &SymbolTable,
    file_provider: &F,
) -> anyhow::Result<Option<serde_json::value::Value>> {
    eprintln!(
        "hover called with uri: {:?}, position: {:?}",
        params.text_document_position_params.text_document.uri,
        params.text_document_position_params.position
    );
    let uri = &params.text_document_position_params.text_document.uri;
    let mut position = params.text_document_position_params.position;
    position.line += 1;
    position.character += 1;
    let mut hover_text: Option<String> = None;

    if let Ok(path) = uri.to_file_path() {
        let offset = line_col_to_offset(
            file_provider,
            path.to_str().ok_or_else(|| anyhow!("Invalid path string"))?,
            position.line as usize,
            position.character as usize,
        )?;

        if let Some(symbol_under_cursor) = symbol_table.get_symbol_at_location(&path, offset) {
            hover_text = match symbol_under_cursor {
                SymbolKind::Publisher(publisher) => {
                    if !publisher.channel_span.is_empty()
                        && offset >= publisher.channel_span[0].start as usize
                        && offset <= publisher.channel_span[0].end as usize
                    {
                        Some(
                            publisher
                                .publication(symbol_table)
                                .channels_enum(symbol_table)
                                .doc_string
                                .clone(),
                        )
                    } else if offset >= publisher.message_span.start as usize
                        && offset <= publisher.message_span.end as usize
                    {
                        Some(publisher.publication(symbol_table).doc_string.clone())
                    } else {
                        None
                    }
                }
                SymbolKind::Subscriber(subscriber) => {
                    if !subscriber.channel_span.is_empty()
                        && offset >= subscriber.channel_span[0].start as usize
                        && offset <= subscriber.channel_span[0].end as usize
                    {
                        Some(
                            subscriber
                                .publication(symbol_table)
                                .channels_enum(symbol_table)
                                .doc_string
                                .clone(),
                        )
                    } else if offset >= subscriber.message_span.start as usize
                        && offset <= subscriber.message_span.end as usize
                    {
                        Some(subscriber.publication(symbol_table).doc_string.clone())
                    } else {
                        None
                    }
                }
                SymbolKind::RpcServer(server) => {
                    if offset >= server.service_span.start as usize
                        && offset <= server.service_span.end as usize
                    {
                        Some(server.service(symbol_table).doc_string.clone())
                    } else {
                        None
                    }
                }
                SymbolKind::RpcClient(client) => {
                    if offset >= client.service_span.start as usize
                        && offset <= client.service_span.end as usize
                    {
                        Some(client.service(symbol_table).doc_string.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            };
        }
    }

    if let Some(hover_text) = hover_text {
        let result = Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover_text,
            }),
            range: None,
        });
        let result = serde_json::to_value(&result)?;
        return Ok(Some(result));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::lsp_file_provider::LspFileProvider;
    use crate::lsp::test_utils::{setup_mock_cli, setup_test_catalog};
    use lsp_types::{
        HoverParams, Position, TextDocumentIdentifier, TextDocumentPositionParams, Url,
        WorkDoneProgressParams,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use vsidl_v1_parser::file_provider::DiskFileProvider;
    use vsidlc_cli::VsidlcCommonCli;

    #[test]
    fn test_hover() -> anyhow::Result<()> {
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

        // WHEN requesting hover info for the symbol under the cursor
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(vsidl_file_path).unwrap(),
                },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let result = hover(cli.clone(), &params, &symbol_table, &file_provider)?;

        // THEN the correct doc string for "TirePressure" is found
        assert!(result.is_some());
        let hover_response: Hover = serde_json::from_value(result.unwrap())?;
        match hover_response.contents {
            HoverContents::Markup(markup) => {
                assert_eq!(
                    markup.value,
                    "Here's a multi-line\nDoc String for TirePressure".to_string()
                );
            }
            _ => panic!("Expected MarkupContent for hover response"),
        }
        Ok(())
    }
}
