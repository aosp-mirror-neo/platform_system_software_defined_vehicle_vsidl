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

use crate::lsp::lsp_file_provider::LspFileProvider;
use crate::lsp::recovering_parser::parse_vsidl_with_recovery;
use crate::lsp::utils::convert_line_col_1_based_to_0_based_index;
use anyhow::anyhow;
use lsp_types::{DocumentSymbol, DocumentSymbolParams, Position, Range, SymbolKind, Url};
use vsidl_v1_parser::file_provider::FileProvider;
use vsidl_v1_parser::pest_parser::Rule;

pub(crate) fn handle_document_symbols(
    params: DocumentSymbolParams,
    file_provider: &LspFileProvider,
) -> anyhow::Result<Option<serde_json::value::Value>> {
    eprintln!("documentSymbol called with uri: {:?}", params.text_document.uri);

    let uri: &Url = &params.text_document.uri;
    let path = uri.to_file_path().map_err(|_| anyhow!("invalid file path"))?;
    let mut content = file_provider.read_to_string(&path).unwrap_or_default();

    let mut symbols: Vec<DocumentSymbol> = Vec::new();
    let (result, _) = parse_vsidl_with_recovery(&mut content);
    if let Ok(mut pairs) = result {
        if let Some(pair) = pairs.next() {
            for inner_pair in pair.into_inner() {
                extract_document_symbols(&mut symbols, inner_pair);
            }
        }
    }

    let result = serde_json::to_value(symbols)?;
    Ok(Some(result))
}

fn extract_document_symbols(symbols: &mut Vec<DocumentSymbol>, pair: pest::iterators::Pair<Rule>) {
    let symbol_kind = match pair.as_rule() {
        Rule::package => Some(SymbolKind::PACKAGE),
        Rule::sdv_service_bundle => Some(SymbolKind::CLASS),
        Rule::publisher => Some(SymbolKind::FUNCTION),
        Rule::subscriber => Some(SymbolKind::FUNCTION),
        Rule::server => Some(SymbolKind::FUNCTION),
        Rule::client => Some(SymbolKind::FUNCTION),
        Rule::unit_type => Some(SymbolKind::FUNCTION),
        Rule::diagnostics_declaration => Some(SymbolKind::CLASS),
        Rule::diagnostics_data_item => Some(SymbolKind::FIELD),
        Rule::diagnostics_io_control => Some(SymbolKind::FIELD),
        Rule::diagnostics_routine => Some(SymbolKind::METHOD),
        Rule::diagnostics_event => Some(SymbolKind::ENUM_MEMBER),
        Rule::some_ip_mapping => Some(SymbolKind::CLASS),
        Rule::some_ip_service_oriented_mapping => Some(SymbolKind::CLASS),
        Rule::some_ip_rpc_interface_mapping => Some(SymbolKind::CLASS),
        Rule::some_ip_rpc_channel_mapping => Some(SymbolKind::CLASS),
        Rule::some_ip_publication_mapping => Some(SymbolKind::CLASS),
        Rule::some_ip_topic_mapping => Some(SymbolKind::CLASS),
        Rule::vhal_mapping => Some(SymbolKind::CLASS),
        _ => None,
    };

    if let Some(kind) = symbol_kind {
        let (start_line, start_character) =
            convert_line_col_1_based_to_0_based_index(pair.as_span().start_pos().line_col());
        let (end_line, end_character) =
            convert_line_col_1_based_to_0_based_index(pair.as_span().end_pos().line_col());

        let range = Range {
            start: Position { line: start_line, character: start_character },
            end: Position { line: end_line, character: end_character },
        };

        let name = match pair.as_rule() {
            Rule::package => pair
                .clone()
                .into_inner()
                .next()
                .map(|p| p.as_str().trim_matches('"').to_string())
                .unwrap_or_else(|| "package".to_string()),
            Rule::sdv_service_bundle => pair
                .clone()
                .into_inner()
                .find(|p| p.as_rule() == Rule::service_bundle_body)
                .and_then(|body| {
                    body.into_inner().find(|p| p.as_rule() == Rule::service_bundle_name_prop)
                })
                .and_then(|prop| prop.into_inner().next())
                .map(|v| v.as_str().trim_matches('"').to_string())
                .unwrap_or_else(|| "unnamed bundle".to_string()),
            Rule::publisher | Rule::subscriber => {
                let rule = pair.as_rule();
                let body_rule = if rule == Rule::publisher {
                    Rule::publisher_body
                } else {
                    Rule::subscriber_body
                };
                let msg_rule = if rule == Rule::publisher {
                    Rule::publisher_message
                } else {
                    Rule::subscriber_message
                };

                pair.clone()
                    .into_inner()
                    .find(|p| p.as_rule() == body_rule)
                    .and_then(|body| body.into_inner().find(|p| p.as_rule() == msg_rule))
                    .and_then(|msg| msg.into_inner().next())
                    .map(|v| v.as_str().trim_matches('"').to_string())
                    .unwrap_or_else(|| {
                        if rule == Rule::publisher {
                            "publisher".to_string()
                        } else {
                            "subscriber".to_string()
                        }
                    })
            }
            Rule::server | Rule::client => {
                let rule = pair.as_rule();
                let body_rule =
                    if rule == Rule::server { Rule::server_body } else { Rule::client_body };
                let iface_rule = if rule == Rule::server {
                    Rule::server_interface
                } else {
                    Rule::client_interface
                };

                pair.clone()
                    .into_inner()
                    .find(|p| p.as_rule() == body_rule)
                    .and_then(|body| body.into_inner().find(|p| p.as_rule() == iface_rule))
                    .and_then(|iface| iface.into_inner().next())
                    .map(|v| v.as_str().trim_matches('"').to_string())
                    .unwrap_or_else(|| {
                        if rule == Rule::server {
                            "server".to_string()
                        } else {
                            "client".to_string()
                        }
                    })
            }
            Rule::some_ip_service_oriented_mapping => "service_oriented_mapping".to_string(),
            Rule::some_ip_rpc_interface_mapping => "rpc_interface_mapping".to_string(),
            Rule::some_ip_publication_mapping => "publication_mapping".to_string(),
            Rule::some_ip_rpc_channel_mapping => pair
                .clone()
                .into_inner()
                .find(|p| p.as_rule() == Rule::some_ip_rpc_channel_mapping_channel)
                .and_then(|p| p.into_inner().next())
                .map(|p| p.as_str().trim_matches('"').to_string())
                .unwrap_or_else(|| "rpc_channel_mapping".to_string()),
            Rule::some_ip_topic_mapping => pair
                .clone()
                .into_inner()
                .find(|p| p.as_rule() == Rule::some_ip_topic_mapping_topic)
                .and_then(|p| p.into_inner().next())
                .map(|p| p.as_str().trim_matches('"').to_string())
                .unwrap_or_else(|| "topic_mapping".to_string()),
            _ => format!("{:?}", pair.as_rule()),
        };

        let mut children: Vec<DocumentSymbol> = Vec::new();
        for inner_pair in pair.clone().into_inner() {
            extract_document_symbols(&mut children, inner_pair);
        }

        symbols.push(DocumentSymbol {
            name,
            detail: None,
            kind,
            tags: None,
            deprecated: None,
            range,
            selection_range: range,
            children: if children.is_empty() { None } else { Some(children) },
        });
    } else {
        for inner_pair in pair.into_inner() {
            extract_document_symbols(symbols, inner_pair);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::lsp_file_provider::LspFileProvider;
    use crate::lsp::test_utils::setup_test_catalog;
    use lsp_types::Url;
    use lsp_types::{DocumentSymbol, DocumentSymbolParams, TextDocumentIdentifier};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use vsidl_v1_parser::file_provider::DiskFileProvider;

    #[test]
    fn test_document_symbols() -> anyhow::Result<()> {
        // GIVEN a VSIDL file with symbols from setup_test_catalog
        let temp_dir = setup_test_catalog();
        let vsidl_file_path = temp_dir.path().join("test.vsidl");
        let vsidl_content = std::fs::read_to_string(&vsidl_file_path)?;

        let document_store = Arc::new(Mutex::new(HashMap::new()));
        document_store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(Url::from_file_path(&vsidl_file_path).unwrap(), vsidl_content.clone());
        let file_provider = LspFileProvider {
            document_store,
            file_hashes: Arc::new(Mutex::new(HashMap::new())),
            disk_fallback: DiskFileProvider,
        };

        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(vsidl_file_path).unwrap(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        // WHEN document symbols are requested
        let result = handle_document_symbols(params, &file_provider)?;

        // THEN the correct symbols are returned
        assert!(result.is_some());
        let symbols: Vec<DocumentSymbol> = serde_json::from_value(result.unwrap())?;

        eprintln!("Symbols: {:?}", symbols);
        // Expecting: Package, ServiceBundle (Manager), and its children (Publisher, Subscriber)
        assert!(symbols.iter().any(|s| s.name == "com.sdv.tm" && s.kind == SymbolKind::PACKAGE));
        assert!(symbols.iter().any(|s| s.name == "Manager" && s.kind == SymbolKind::CLASS));

        let manager = symbols.iter().find(|s| s.name == "Manager").unwrap();
        let children = manager.children.as_ref().unwrap();
        assert!(children
            .iter()
            .any(|s| s.name == "TirePressure" && s.kind == SymbolKind::FUNCTION));

        Ok(())
    }
}
