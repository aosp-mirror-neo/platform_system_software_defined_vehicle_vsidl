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

use lsp_types::{SemanticToken, SemanticTokenType, SemanticTokens};
use vsidl_v1_parser::pest_parser::Rule;

/// The semantic token types that are supported by the language server.
///
/// This list is sent to the client during initialization to inform it about the
/// types of tokens the server can provide.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::STRING,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::NUMBER,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::COMMENT,
];

/// A builder for creating a list of semantic tokens for a VSIDL file.
///
/// It processes Pest parser pairs, converting them into `SemanticToken`s
/// that can be sent to an LSP client.
pub struct TokenBuilder {
    tokens: Vec<SemanticToken>,
    last_line: u32,
    last_char: u32,
}

impl TokenBuilder {
    /// Creates a new `TokenBuilder`.
    pub fn new() -> Self {
        Self { tokens: Vec::new(), last_line: 0, last_char: 0 }
    }

    /// Recursively traverses a Pest parser pair and pushes semantic tokens
    /// to the builder's internal `tokens` vector.
    ///
    /// For each pair that corresponds to a token type, it calculates the token's
    /// position relative to the previous token and adds it to the list.
    pub fn push(&mut self, pair: pest::iterators::Pair<Rule>) {
        let token_type_index = get_token_type_index(&pair);

        if let Some(token_type) = token_type_index {
            let (line, col) = pair.line_col();
            let line: u32 = (line - 1) as u32;
            let col = (col - 1) as u32;
            let length = if token_type == 0 || token_type == 1 {
                keyword_length(pair.as_str())
            } else {
                pair.as_str().chars().count() as u32
            };

            let delta_line = line - self.last_line;
            let delta_start = if delta_line == 0 { col - self.last_char } else { col };

            self.tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: 0,
            });

            self.last_line = line;
            self.last_char = col;
        }
        if token_type_index != Some(2) {
            for inner_pair in pair.into_inner() {
                self.push(inner_pair);
            }
        }
    }

    /// Consumes the builder and returns the final `SemanticTokens`.
    pub fn build(self) -> SemanticTokens {
        SemanticTokens { result_id: None, data: self.tokens }
    }
}

/// Maps a Pest parser `Rule` to a semantic token type index.
///
/// The index corresponds to the position of the `SemanticTokenType` in the `TOKEN_TYPES` array.
fn get_token_type_index(pair: &pest::iterators::Pair<Rule>) -> Option<u32> {
    let token_type = match pair.as_rule() {
        // =========================================================================
        // KEYWORDS (followed by `{`)
        // =========================================================================

        // --- High Level Blocks ---
        Rule::sdv_service_bundle
        | Rule::some_ip_mapping
        | Rule::vhal_mapping
        | Rule::publisher
        | Rule::subscriber
        | Rule::server
        | Rule::client
        | Rule::unit_type
        | Rule::build_configuration
        | Rule::diagnostics_declaration

        // --- Diagnostics Blocks ---
        | Rule::diagnostics_data_item
        | Rule::diagnostics_io_control
        | Rule::diagnostics_routine
        | Rule::diagnostics_routine_start
        | Rule::diagnostics_routine_stop
        | Rule::diagnostics_routine_result
        | Rule::diagnostics_event

        // --- VHAL Blocks ---
        | Rule::vhal_vehicle_property
        | Rule::vhal_vehicle_prop_config
        | Rule::vhal_vehicle_default_value
        | Rule::vhal_vehicle_default_area_config
        | Rule::vhal_data_source
        | Rule::vhal_area_mapping
        | Rule::vhal_area_mapping_area
        | Rule::vhal_property_area_config
        | Rule::vhal_property_area_default_value
        | Rule::vhal_vehicle_prop_config_area_configs

        // --- SOME/IP Blocks ---
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
        | Rule::some_ip_type
        | Rule::some_ip_array_type
        | Rule::some_ip_struct_field
        | Rule::some_ip_struct_field_type
        | Rule::some_ip_service_oriented_mapping
        | Rule::some_ip_rpc_interface_mapping
        | Rule::some_ip_rpc_channel_mapping
        | Rule::some_ip_publication_mapping
        | Rule::some_ip_topic_mapping

        // --- Literals (True/False) ---
        | Rule::boolean => Some(SemanticTokenType::KEYWORD),

        // =========================================================================
        // PROPERTIES (followed by `:`)
        // =========================================================================

        // --- High Level Properties ---
        Rule::package
        | Rule::register_reflection_metadata
        | Rule::service_bundle_name_prop
        | Rule::service_bundle_register_reflection_metadata
        | Rule::publisher_message
        | Rule::publisher_topic
        | Rule::publisher_channel
        | Rule::publisher_capacity
        | Rule::publisher_service_unit_name
        | Rule::subscriber_message
        | Rule::subscriber_topic
        | Rule::subscriber_channel
        | Rule::server_interface
        | Rule::server_service_unit_name
        | Rule::server_channel
        | Rule::client_interface
        | Rule::client_channel
        | Rule::unit_type_type
        | Rule::build_configuration_target_name
        | Rule::build_configuration_skip_codegen
        | Rule::field // Textproto field

        // --- Diagnostics Properties ---
        | Rule::diagnostics_data_item_id
        | Rule::diagnostics_data_item_message_name
        | Rule::diagnostics_data_item_is_writable
        | Rule::diagnostics_io_control_id
        | Rule::diagnostics_io_control_message_name
        | Rule::diagnostics_routine_id
        | Rule::diagnostics_method_request
        | Rule::diagnostics_method_response
        | Rule::diagnostics_event_id
        | Rule::diagnostics_event_extended_data_message_name
        | Rule::diagnostics_event_fault_listener_mask
        | Rule::diagnostics_event_service_unit_name
        | Rule::diagnostics_event_topic

        // --- VHAL Properties ---
        | Rule::vhal_data_source_unit_type
        | Rule::vhal_data_source_action
        | Rule::vhal_data_source_field_path
        | Rule::vhal_data_source_status_field_path
        | Rule::vhal_area_mapping_publication_instance
        | Rule::vhal_default_value_string_value
        | Rule::vhal_default_value_bytes_value
        | Rule::vhal_default_value_int32_values
        | Rule::vhal_default_value_int64_values
        | Rule::vhal_default_value_float_values
        | Rule::vhal_vehicle_prop_config_prop
        | Rule::vhal_vehicle_prop_config_access
        | Rule::vhal_vehicle_prop_config_change_mode
        | Rule::vhal_vehicle_prop_config_config_array
        | Rule::vhal_vehicle_prop_config_config_string
        | Rule::vhal_vehicle_prop_config_min_sample_rate
        | Rule::vhal_vehicle_prop_config_max_sample_rate
        | Rule::vhal_vehicle_area_config_area_id
        | Rule::vhal_vehicle_area_config_min_int32_value
        | Rule::vhal_vehicle_area_config_max_int32_value
        | Rule::vhal_vehicle_area_config_min_int64_value
        | Rule::vhal_vehicle_area_config_max_int64_value
        | Rule::vhal_vehicle_area_config_min_float_value
        | Rule::vhal_vehicle_area_config_max_float_value
        | Rule::vhal_vehicle_area_config_supported_enum_values
        | Rule::vhal_vehicle_area_config_access
        | Rule::vhal_vehicle_area_config_support_variable_update_rate

        // --- SOME/IP Properties ---
        | Rule::some_ip_service_id
        | Rule::some_ip_version_major
        | Rule::some_ip_version_minor
        | Rule::some_ip_sd_behavior
        | Rule::some_ip_instance_id
        | Rule::some_ip_sdv_service_unit_parameterization_block_key
        | Rule::some_ip_sdv_service_unit_parameterization_block_value
        | Rule::some_ip_service_unit
        | Rule::some_ip_unit_type_ref_package_name
        | Rule::some_ip_unit_type_ref_service_bundle_name
        | Rule::some_ip_unit_type_ref_type_name
        | Rule::some_ip_method_procedure
        | Rule::some_ip_method_method_id
        | Rule::some_ip_offered_event_group_id
        | Rule::some_ip_offered_event_event_id
        | Rule::some_ip_simple_event_service_unit
        | Rule::some_ip_simple_event_message_translation_mode
        | Rule::some_ip_event_part_service_unit
        | Rule::some_ip_map_entry_key
        | Rule::some_ip_map_entry_value
        | Rule::some_ip_sdv_type_mapping_service_id
        | Rule::some_ip_sdv_type_mapping_version_major
        | Rule::some_ip_sdv_type_mapping_version_minor
        | Rule::some_ip_sdv_method_procedure
        | Rule::some_ip_sdv_method_method_id
        | Rule::some_ip_offered_topic_someip_ff_methods
        | Rule::some_ip_simple_mapping_group_id
        | Rule::some_ip_simple_mapping_event_id
        | Rule::some_ip_multipub_topic_info_someip_instance_id
        | Rule::some_ip_service_unit_mapping_instance_id
        | Rule::some_ip_service_unit_id_vm_name
        | Rule::some_ip_service_unit_id_package_name
        | Rule::some_ip_service_unit_id_service_bundle_name
        | Rule::some_ip_service_unit_id_service_instance_name
        | Rule::some_ip_service_unit_id_service_unit_name
        | Rule::some_ip_fire_and_forget_method_service_id
        | Rule::some_ip_fire_and_forget_method_instance_id
        | Rule::some_ip_fire_and_forget_method_version_major
        | Rule::some_ip_fire_and_forget_method_version_minor
        | Rule::some_ip_fire_and_forget_method_method_id
        | Rule::some_ip_type_someip_primitive
        | Rule::some_ip_string_body_fixed_length
        | Rule::some_ip_string_body_length_field_size
        | Rule::some_ip_string_body_string_encoding
        | Rule::some_ip_array_body_fixed_length
        | Rule::some_ip_array_body_length_field_size
        | Rule::some_ip_struct_body_length_field_size
        | Rule::some_ip_struct_field_data_tag
        | Rule::some_ip_struct_field_field_name
        | Rule::some_ip_channel
        | Rule::some_ip_simple_event_topic
        | Rule::some_ip_simple_event_capacity
        | Rule::some_ip_event_part_topic
        | Rule::some_ip_event_part_capacity
        | Rule::some_ip_rpc_channel_mapping_channel
        | Rule::some_ip_rpc_channel_mapping_instance_id
        | Rule::some_ip_topic_mapping_topic
        | Rule::some_ip_topic_mapping_instance_id => Some(SemanticTokenType::PROPERTY),

        // =========================================================================
        // OTHER
        // =========================================================================

        Rule::qualified_name | Rule::service_bundle_name => {
            Some(SemanticTokenType::STRING)
        }
        Rule::integer | Rule::number | Rule::float => Some(SemanticTokenType::NUMBER),
        Rule::string
        | Rule::identifier_str
        | Rule::service_bundle_name_str
        | Rule::service_unit_name_str
        | Rule::qualified_name_str => Some(SemanticTokenType::STRING),

        Rule::COMMENT => Some(SemanticTokenType::COMMENT),
        Rule::IDENTIFIER => Some(SemanticTokenType::ENUM_MEMBER),

        _ => None,
    };

    token_type.and_then(|t| TOKEN_TYPES.iter().position(|leg_t| leg_t == &t).map(|i| i as u32))
}

/// Calculates the length of a keyword from a string slice.
///
/// This is necessary because the `pair.as_str()` for a rule might contain more
/// than just the keyword. This function finds the matching keyword at the start
/// and returns its length.
pub fn keyword_length(s: &str) -> u32 {
    // Keywords start with A-Z, a-z, or _
    // and are followed by A-Z, a-z, _, or 0-9.

    s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        // If a non-identifier character is found, its index is the length.
        // If the entire string is an identifier, find() returns None,
        // so we use the string's total length.
        .unwrap_or(s.len()) as u32
}
