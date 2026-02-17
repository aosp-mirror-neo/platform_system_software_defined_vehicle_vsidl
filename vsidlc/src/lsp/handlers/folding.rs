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
use crate::lsp::token_builder::keyword_length;
use lsp_types::Url;
use lsp_types::{FoldingRange, FoldingRangeKind, FoldingRangeParams};
use std::cmp::max;
use vsidl_v1_parser::converter::{
    convert_to_client, convert_to_publisher, convert_to_server, convert_to_service_bundle,
    convert_to_subscriber,
};
use vsidl_v1_parser::file_provider::FileProvider;
use vsidl_v1_parser::pest_parser::Rule;

pub(crate) fn folding_ranges(
    params: FoldingRangeParams,
    file_provider: &LspFileProvider,
) -> anyhow::Result<Option<serde_json::value::Value>> {
    // Parse file
    let uri: &Url = &params.text_document.uri;
    let url = Url::parse(uri.as_ref())?;
    let path = url.to_file_path().map_err(|_| anyhow::anyhow!("invalid file path"))?;
    let mut content = file_provider.read_to_string(&path)?;
    let (result, _) = parse_vsidl_with_recovery(&mut content);

    let mut ranges: Vec<FoldingRange> = Vec::new();
    if let Ok(mut pairs) = result {
        if let Some(parsed) = pairs.next() {
            extract_ranges(&mut ranges, parsed);
        }
    }

    let result = serde_json::to_value(&ranges)?;
    Ok(Some(result))
}

fn extract_ranges(ranges: &mut Vec<FoldingRange>, pair: pest::iterators::Pair<Rule>) {
    let collapse_text = match pair.as_rule() {
        Rule::sdv_service_bundle => Some(convert_to_service_bundle(pair.clone(), "").name),
        Rule::publisher => {
            let message = convert_to_publisher(pair.clone(), "").message;
            let last_part = message.rsplit('.').next().unwrap_or(&message);
            Some(last_part.to_string())
        }
        Rule::subscriber => {
            let message = convert_to_subscriber(pair.clone(), "").message;
            let last_part = message.rsplit('.').next().unwrap_or(&message);
            Some(last_part.to_string())
        }
        Rule::server => {
            let service = convert_to_server(pair.clone(), "").service;
            let last_part = service.rsplit('.').next().unwrap_or(&service);
            Some(last_part.to_string())
        }
        Rule::client => {
            let service = convert_to_client(pair.clone(), "").service;
            let last_part = service.rsplit('.').next().unwrap_or(&service);
            Some(last_part.to_string())
        }
        _ => None,
    };

    let range = match pair.as_rule() {
        // --- Top Level ---
        Rule::sdv_service_bundle
        | Rule::some_ip_mapping
        | Rule::vhal_mapping
        | Rule::extension
        | Rule::diagnostics_declaration

        // --- Service Bundle ---
        | Rule::publisher
        | Rule::subscriber
        | Rule::server
        | Rule::client
        | Rule::unit_type
        | Rule::build_configuration
        | Rule::message_field // Textproto-like field

        // --- Diagnostics ---
        | Rule::diagnostics_data_item
        | Rule::diagnostics_io_control
        | Rule::diagnostics_routine
        | Rule::diagnostics_routine_start
        | Rule::diagnostics_routine_stop
        | Rule::diagnostics_routine_result
        | Rule::diagnostics_event

        // --- VHAL ---
        | Rule::vhal_vehicle_property
        | Rule::vhal_vehicle_prop_config
        | Rule::vhal_vehicle_default_value
        | Rule::vhal_vehicle_default_area_config
        | Rule::vhal_vehicle_prop_config_area_configs
        | Rule::vhal_data_source
        | Rule::vhal_area_mapping
        | Rule::vhal_area_mapping_area
        | Rule::vhal_property_area_config
        | Rule::vhal_property_area_default_value

        // --- SOME/IP: Service Mapping & Instances ---
        | Rule::some_ip_service_mapping
        | Rule::some_ip_instance
        | Rule::some_ip_sdv_service_unit_parameterization_block
        | Rule::some_ip_instance_impl
        | Rule::some_ip_offered_interface
        | Rule::some_ip_offered_interface_sdv_unit_type
        | Rule::some_ip_method
        | Rule::request_payload_mapping
        | Rule::response_payload_mapping
        | Rule::some_ip_offered_event
        | Rule::some_ip_simple_event
        | Rule::some_ip_simple_event_sdv_unit_type
        | Rule::some_ip_simple_event_type_definition
        | Rule::some_ip_group_event
        | Rule::some_ip_group_event_type_definition
        | Rule::some_ip_event_part
        | Rule::some_ip_event_part_sdv_unit_type
        | Rule::some_ip_event_part_field_name_mapping

        // --- SOME/IP: SDV Type Mapping ---
        | Rule::some_ip_sdv_type_mapping
        | Rule::some_ip_sdv_type_mapping_unit_type
        | Rule::some_ip_sdv_offered_interface
        | Rule::some_ip_sdv_method
        | Rule::some_ip_offered_topic
        | Rule::some_ip_offered_topic_type_definition
        | Rule::some_ip_simple_mapping
        | Rule::some_ip_multipub_topic_info
        | Rule::some_ip_service_unit_mapping
        | Rule::some_ip_service_unit_mapping_service_unit_id
        | Rule::some_ip_fire_and_forget_method

        // --- SOME/IP: Types & Structs ---
        | Rule::some_ip_type // Covers string/array/struct definitions
        | Rule::some_ip_array_type
        | Rule::some_ip_struct_field
        | Rule::some_ip_struct_field_type => {
            let (start_line, start_col) = pair.as_span().start_pos().line_col();
            let (end_line, end_col) = pair.as_span().end_pos().line_col();
            Some(FoldingRange {
                start_line: (max(start_line, 1) - 1) as u32,
                start_character: Some((max(start_col, 1)) as u32 + keyword_length(pair.as_str())),
                end_line: (max(end_line, 1) - 1) as u32,
                end_character: Some((max(end_col, 1) - 1) as u32),
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: Some(match collapse_text {
                    Some(ref s) => format!("{{ {} }}", s),
                    None => "{ ... }".to_string(),
                }),
            })
        }
        _ => None,
    };

    if let Some(range) = range {
        ranges.push(range);
    }

    for inner_pair in pair.into_inner() {
        extract_ranges(ranges, inner_pair);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::lsp_file_provider::LspFileProvider;
    use crate::lsp::test_utils::setup_test_catalog;
    use lsp_types::FoldingRangeKind::Region;
    use lsp_types::Url;
    use lsp_types::{
        FoldingRange, FoldingRangeParams, PartialResultParams, TextDocumentIdentifier,
        WorkDoneProgressParams,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use vsidl_v1_parser::file_provider::DiskFileProvider;

    #[test]
    fn test_folding_ranges() -> anyhow::Result<()> {
        // GIVEN a VSIDL file with foldable elements from setup_test_catalog
        let temp_dir = setup_test_catalog();
        let vsidl_file_path = temp_dir.path().join("test.vsidl");
        let vsidl_content = std::fs::read_to_string(&vsidl_file_path)?;

        let document_store = Arc::new(Mutex::new(HashMap::new()));
        document_store
            .lock()
            .unwrap()
            .insert(Url::from_file_path(&vsidl_file_path).unwrap(), vsidl_content.clone());
        let file_provider = LspFileProvider {
            document_store,
            file_hashes: Arc::new(Mutex::new(HashMap::new())),
            disk_fallback: DiskFileProvider,
        };

        let params = FoldingRangeParams {
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(vsidl_file_path).unwrap(),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        // WHEN folding ranges are requested
        let result = folding_ranges(params, &file_provider)?;

        // THEN the correct folding ranges are returned
        assert!(result.is_some());
        let ranges: Vec<FoldingRange> = serde_json::from_value(result.unwrap())?;

        eprintln!("Ranges: {:?}", ranges);
        let expected_ranges = [
            FoldingRange {
                start_line: 1,
                start_character: Some(15),
                end_line: 16,
                end_character: Some(1),
                kind: Some(Region),
                collapsed_text: Some("{ Manager }".to_string()),
            },
            FoldingRange {
                start_line: 4,
                start_character: Some(12),
                end_line: 9,
                end_character: Some(3),
                kind: Some(Region),
                collapsed_text: Some("{ TirePressure }".to_string()),
            },
            FoldingRange {
                start_line: 11,
                start_character: Some(13),
                end_line: 15,
                end_character: Some(3),
                kind: Some(Region),
                collapsed_text: Some("{ TirePressure }".to_string()),
            },
        ];

        assert_eq!(ranges.len(), expected_ranges.len());
        for (i, expected) in expected_ranges.iter().enumerate() {
            assert_eq!(ranges[i].start_line, expected.start_line);
            assert_eq!(ranges[i].start_character, expected.start_character);
            assert_eq!(ranges[i].end_line, expected.end_line);
            assert_eq!(ranges[i].end_character, expected.end_character);
            assert_eq!(ranges[i].kind, expected.kind);
            assert_eq!(ranges[i].collapsed_text, expected.collapsed_text);
        }

        Ok(())
    }
}
