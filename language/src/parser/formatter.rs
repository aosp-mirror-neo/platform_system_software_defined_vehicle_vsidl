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
use crate::pest_parser::Rule;
use pest::iterators::{Pair, Pairs};

/// Formats the given Pest pair into a string with configurable indentation.
///
/// # Arguments
///
/// * `pair` - The parsed rule pair to format.
/// * `indent_width` - The number of spaces to use for each level of indentation.
pub fn format(pair: Pair<Rule>, indent_width: usize) -> String {
    let mut result = String::new();
    let mut context = FormatContext::new(indent_width);
    format_pair(&mut result, pair, &mut context);
    // Ensure single trailing newline
    result.trim().to_string() + "\n"
}

struct FormatContext {
    indent_level: usize,
    indent_width: usize,
    has_seen_code: bool,
}

impl FormatContext {
    fn new(indent_width: usize) -> Self {
        Self { indent_level: 0, indent_width, has_seen_code: false }
    }

    fn indent(&mut self) {
        self.indent_level += 1;
    }

    fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    fn get_indent(&self) -> String {
        " ".repeat(self.indent_level * self.indent_width)
    }
}

fn get_first_word(s: &str) -> &str {
    s.trim_start()
        .split(|c: char| c.is_whitespace() || c == ':' || c == '{' || c == '[')
        .next()
        .unwrap_or("")
}

/// Helpers to reduce boilerplate
fn format_block(
    result: &mut String,
    keyword: &str,
    inner: Pairs<Rule>,
    context: &mut FormatContext,
) {
    let indent = context.get_indent();
    result.push_str(&indent);
    result.push_str(keyword);
    result.push_str(" {\n");
    context.indent();
    format_children(result, inner, context);
    context.dedent();
    result.push_str(&indent);
    result.push_str("}\n");
}

fn format_property(result: &mut String, keyword: &str, pair: Pair<Rule>, context: &FormatContext) {
    let indent = context.get_indent();
    // We expect the property rule (e.g., some_ip_service_id) to contain exactly
    // one meaningful child (the value) or be the value itself depending on grammar.
    // Based on the grammar, property rules wrap the value (e.g. "id" ~ ":" ~ string).
    // The inner child is the value.
    let value = if let Some(inner) = pair.clone().into_inner().next() {
        inner.as_str().trim()
    } else {
        pair.as_str().trim()
    };

    result.push_str(&indent);
    result.push_str(keyword);
    result.push_str(": ");
    result.push_str(value);
    result.push('\n');
}

fn format_children(result: &mut String, pairs: Pairs<Rule>, context: &mut FormatContext) {
    for pair in pairs {
        format_pair(result, pair, context);
    }
}

fn ensure_newline(result: &mut String, context: &FormatContext, is_comment: bool) {
    if result.is_empty() || result.ends_with("\n\n") {
        return;
    }

    let last_line = result.trim_end().split('\n').next_back().unwrap_or("");
    let last_was_comment = last_line.trim().starts_with('#');

    // If we haven't seen any code yet, and we are at top level,
    // we want a blank line after the initial comment block (e.g. copyright)
    // ONLY when we are about to start a code block.
    let is_starting_code_after_initial_comments =
        !context.has_seen_code && context.indent_level == 0 && last_was_comment && !is_comment;

    if !last_was_comment || is_starting_code_after_initial_comments {
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push('\n');
    }
}

fn format_pair(result: &mut String, pair: Pair<Rule>, context: &mut FormatContext) {
    // Filter out pure whitespace nodes, but keep comments
    if pair.as_rule() == Rule::WHITESPACE {
        return;
    }

    match pair.as_rule() {
        // =========================================================================
        // File & Root
        // =========================================================================
        Rule::vsidl_file => {
            format_children(result, pair.into_inner(), context);
        }
        Rule::package => {
            ensure_newline(result, context, false);
            context.has_seen_code = true;
            let indent = context.get_indent();
            let value = pair.into_inner().next().unwrap().as_str().trim();
            result.push_str(&indent);
            result.push_str("package: ");
            result.push_str(value);
            result.push('\n');
        }
        Rule::COMMENT => {
            ensure_newline(result, context, true);
            let indent = context.get_indent();
            result.push_str(&indent);
            result.push_str(pair.as_str().trim());
            result.push('\n');
        }
        Rule::EOI => {}

        // =========================================================================
        // High Level Blocks
        // =========================================================================
        Rule::sdv_service_bundle => {
            ensure_newline(result, context, false); // Add spacing before main blocks
            context.has_seen_code = true;
            format_block(result, "service_bundle", pair.into_inner(), context);
        }
        Rule::some_ip_mapping => {
            ensure_newline(result, context, false); // Add spacing before main blocks
            context.has_seen_code = true;
            format_block(result, "some_ip_mapping", pair.into_inner(), context);
        }
        Rule::vhal_mapping => {
            ensure_newline(result, context, false); // Add spacing before main blocks
            context.has_seen_code = true;
            format_block(result, "vhal_mapping", pair.into_inner(), context);
        }
        Rule::extension => {
            let mut inner = pair.into_inner();
            let name = inner.next().unwrap().as_str(); // qualified_name
            ensure_newline(result, context, false);
            context.has_seen_code = true;
            let indent = context.get_indent();
            result.push_str(&indent);
            result.push('[');
            result.push_str(name);
            result.push_str("] {\n");
            context.indent();
            // message_body
            for child in inner {
                format_pair(result, child, context);
            }
            context.dedent();
            result.push_str(&indent);
            result.push_str("}\n");
        }

        // =========================================================================
        // Service Bundle Components
        // =========================================================================
        // Blocks
        Rule::publisher
        | Rule::subscriber
        | Rule::server
        | Rule::client
        | Rule::unit_type
        | Rule::build_configuration
        | Rule::diagnostics_declaration => {
            ensure_newline(result, context, false);
            let keyword = get_first_word(pair.as_str());
            format_block(result, keyword, pair.into_inner(), context)
        }

        // Blocks
        Rule::diagnostics_data_item
        | Rule::diagnostics_io_control
        | Rule::diagnostics_routine
        | Rule::diagnostics_event
        | Rule::diagnostics_routine_start
        | Rule::diagnostics_routine_stop
        | Rule::diagnostics_routine_result
        | Rule::vhal_vehicle_property
        | Rule::vhal_vehicle_prop_config
        | Rule::vhal_vehicle_default_value
        | Rule::vhal_property_area_default_value
        | Rule::vhal_vehicle_default_area_config
        | Rule::vhal_vehicle_prop_config_area_configs
        | Rule::vhal_data_source
        | Rule::vhal_area_mapping
        | Rule::vhal_area_mapping_area
        | Rule::vhal_property_area_config
        | Rule::some_ip_service_mapping
        | Rule::some_ip_instance
        | Rule::some_ip_sdv_service_unit_parameterization_block
        | Rule::some_ip_instance_impl
        | Rule::some_ip_offered_interface
        | Rule::some_ip_sdv_offered_interface
        | Rule::some_ip_offered_interface_sdv_unit_type
        | Rule::some_ip_simple_event_sdv_unit_type
        | Rule::some_ip_event_part_sdv_unit_type
        | Rule::some_ip_sdv_type_mapping_unit_type
        | Rule::some_ip_method
        | Rule::some_ip_sdv_method
        | Rule::request_payload_mapping
        | Rule::response_payload_mapping
        | Rule::some_ip_offered_event
        | Rule::some_ip_simple_event
        | Rule::some_ip_simple_event_type_definition
        | Rule::some_ip_group_event_type_definition
        | Rule::some_ip_offered_topic_type_definition
        | Rule::some_ip_group_event
        | Rule::some_ip_event_part
        | Rule::some_ip_event_part_field_name_mapping
        | Rule::some_ip_sdv_type_mapping
        | Rule::some_ip_offered_topic
        | Rule::some_ip_simple_mapping
        | Rule::some_ip_multipub_topic_info
        | Rule::some_ip_service_unit_mapping
        | Rule::some_ip_service_unit_mapping_service_unit_id
        | Rule::some_ip_fire_and_forget_method
        | Rule::some_ip_array_type
        | Rule::some_ip_struct_field
        | Rule::some_ip_struct_field_type => {
            let keyword = get_first_word(pair.as_str());
            format_block(result, keyword, pair.into_inner(), context)
        }

        // Type System Blocks
        Rule::some_ip_string_body => format_children(result, pair.into_inner(), context),
        Rule::some_ip_array_body => format_children(result, pair.into_inner(), context),
        Rule::some_ip_struct_body => format_children(result, pair.into_inner(), context),

        // Properties with deprecated keyword replacement
        Rule::publisher_message | Rule::subscriber_message => {
            format_property(result, "message", pair, context)
        }
        Rule::server_interface | Rule::client_interface => {
            format_property(result, "service", pair, context)
        }

        // Properties
        Rule::service_bundle_name_prop
        | Rule::service_bundle_register_reflection_metadata
        | Rule::publisher_topic
        | Rule::subscriber_topic
        | Rule::publisher_channel
        | Rule::subscriber_channel
        | Rule::publisher_service_unit_name
        | Rule::server_service_unit_name
        | Rule::publisher_capacity
        | Rule::server_channel
        | Rule::client_channel
        | Rule::unit_type_type
        | Rule::build_configuration_target_name
        | Rule::build_configuration_skip_codegen
        | Rule::diagnostics_data_item_id
        | Rule::diagnostics_io_control_id
        | Rule::diagnostics_routine_id
        | Rule::diagnostics_event_id
        | Rule::diagnostics_data_item_message_name
        | Rule::diagnostics_io_control_message_name
        | Rule::diagnostics_data_item_is_writable
        | Rule::diagnostics_method_request
        | Rule::diagnostics_method_response
        | Rule::diagnostics_event_extended_data_message_name
        | Rule::diagnostics_event_fault_listener_mask
        | Rule::diagnostics_event_service_unit_name
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
        | Rule::vhal_vehicle_area_config_access
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
        | Rule::vhal_vehicle_area_config_support_variable_update_rate
        | Rule::some_ip_service_id
        | Rule::some_ip_sdv_type_mapping_service_id
        | Rule::some_ip_fire_and_forget_method_service_id
        | Rule::some_ip_version_major
        | Rule::some_ip_sdv_type_mapping_version_major
        | Rule::some_ip_fire_and_forget_method_version_major
        | Rule::some_ip_version_minor
        | Rule::some_ip_sdv_type_mapping_version_minor
        | Rule::some_ip_fire_and_forget_method_version_minor
        | Rule::some_ip_sd_behavior
        | Rule::some_ip_instance_id
        | Rule::some_ip_service_unit_mapping_instance_id
        | Rule::some_ip_fire_and_forget_method_instance_id
        | Rule::some_ip_multipub_topic_info_someip_instance_id
        | Rule::some_ip_sdv_service_unit_parameterization_block_key
        | Rule::some_ip_map_entry_key
        | Rule::some_ip_sdv_service_unit_parameterization_block_value
        | Rule::some_ip_map_entry_value
        | Rule::some_ip_service_unit
        | Rule::some_ip_simple_event_service_unit
        | Rule::some_ip_event_part_service_unit
        | Rule::some_ip_unit_type_ref_package_name
        | Rule::some_ip_service_unit_id_package_name
        | Rule::some_ip_unit_type_ref_service_bundle_name
        | Rule::some_ip_service_unit_id_service_bundle_name
        | Rule::some_ip_unit_type_ref_type_name
        | Rule::some_ip_method_procedure
        | Rule::some_ip_sdv_method_procedure
        | Rule::some_ip_method_method_id
        | Rule::some_ip_sdv_method_method_id
        | Rule::some_ip_fire_and_forget_method_method_id
        | Rule::some_ip_offered_event_group_id
        | Rule::some_ip_simple_mapping_group_id
        | Rule::some_ip_offered_event_event_id
        | Rule::some_ip_simple_mapping_event_id
        | Rule::some_ip_simple_event_message_translation_mode
        | Rule::some_ip_offered_topic_someip_ff_methods
        | Rule::some_ip_service_unit_id_vm_name
        | Rule::some_ip_service_unit_id_service_instance_name
        | Rule::some_ip_service_unit_id_service_unit_name
        | Rule::some_ip_string_body_fixed_length
        | Rule::some_ip_array_body_fixed_length
        | Rule::some_ip_string_body_length_field_size
        | Rule::some_ip_array_body_length_field_size
        | Rule::some_ip_struct_body_length_field_size
        | Rule::some_ip_string_body_string_encoding
        | Rule::some_ip_struct_field_data_tag
        | Rule::some_ip_struct_field_field_name => {
            let keyword = get_first_word(pair.as_str());
            format_property(result, keyword, pair, context)
        }

        // Type Properties
        Rule::some_ip_type => {
            // handle primitive/string/array/struct wrapper
            let inner = pair.into_inner().next().unwrap();
            let keyword = get_first_word(inner.as_str());
            match inner.as_rule() {
                Rule::some_ip_type_someip_primitive => {
                    format_property(result, keyword, inner, context)
                }
                Rule::some_ip_string_body
                | Rule::some_ip_array_body
                | Rule::some_ip_struct_body => {
                    format_block(result, keyword, inner.into_inner(), context)
                }
                _ => {}
            }
        }

        // =========================================================================
        // Textproto / Generic Fields
        // =========================================================================
        Rule::field => {
            let mut inner = pair.into_inner();
            let name = inner.next().unwrap().as_str();
            let value = inner.next().unwrap().as_str().trim();
            let indent = context.get_indent();
            result.push_str(&indent);
            result.push_str(name);
            result.push_str(": ");
            result.push_str(value);
            result.push('\n');
        }
        Rule::message_field => {
            let mut inner = pair.into_inner();
            let name = inner.next().unwrap().as_str();
            format_block(result, name, inner, context);
        }

        // Pass through map entries which are handled inside their block container
        Rule::some_ip_map_entry_str_str => {
            format_children(result, pair.into_inner(), context);
        }

        _ => {
            // Fallback: If it's a container rule not explicitly matched but contains children, recurse
            let inner: Vec<Pair<Rule>> = pair.clone().into_inner().collect();
            if !inner.is_empty() {
                format_children(result, pair.into_inner(), context);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pest_parser::{Rule, VsidlParser};
    use pest::Parser;

    #[test]
    fn test_format_service_bundle_after_comment() {
        let input = r#"# Copyright (C) 2026 The Android Open Source Project
service_bundle {
  name: "MyBundle"
}
"#;
        let pairs = VsidlParser::parse(Rule::vsidl_file, input).unwrap();
        let formatted = format(pairs.into_iter().next().unwrap(), 2);

        assert_eq!(
            formatted,
            "# Copyright (C) 2026 The Android Open Source Project\n\nservice_bundle {\n  name: \"MyBundle\"\n}\n"
        );
    }

    #[test]
    fn test_format_package_after_comment() {
        let input = r#"# Copyright (C) 2026 The Android Open Source Project
package: "sdv.test"
"#;
        let pairs = VsidlParser::parse(Rule::vsidl_file, input).unwrap();
        let formatted = format(pairs.into_iter().next().unwrap(), 2);

        assert_eq!(
            formatted,
            "# Copyright (C) 2026 The Android Open Source Project\n\npackage: \"sdv.test\"\n"
        );
    }

    #[test]
    fn test_format_comments_and_newlines() {
        let input = r#"# Copyright (C) 2026 The Android Open Source Project
service_bundle {
  name: "FullBundleStable"

  server {
    service: "TireStatus"

    # Use a custom service unit name for the server
    # service_unit_name: "custom-server-name"
  }

  # Regular PubSub publisher
  publisher {
    message: "TirePressure"
  }
}
"#;
        let pairs = VsidlParser::parse(Rule::vsidl_file, input).unwrap();
        let formatted = format(pairs.into_iter().next().unwrap(), 2);

        // The expected output should have the comments preserved and proper newlines.
        assert!(formatted.contains("# Copyright (C) 2026 The Android Open Source Project"));
        assert!(formatted.contains("# Use a custom service unit name for the server"));
        assert!(formatted.contains("# service_unit_name: \"custom-server-name\""));
        assert!(formatted.contains("# Regular PubSub publisher"));

        // Check if comments are inside the block
        let server_block = formatted.split("server {").nth(1).unwrap().split('}').next().unwrap();
        assert!(server_block.contains("# Use a custom service unit name for the server"));

        // Check for proper spacing before publisher
        assert!(formatted.contains("\n  # Regular PubSub publisher\n  publisher {"));
    }

    #[test]
    fn test_format_trailing_comment_in_block() {
        let input = r#"# Copyright (C) 2026 The Android Open Source Project
service_bundle {
  name: "TestBundle"
  # End of the bundle definition
}
"#;
        let pairs = VsidlParser::parse(Rule::vsidl_file, input).unwrap();
        let formatted = format(pairs.into_iter().next().unwrap(), 2);

        assert!(formatted.contains("# Copyright (C) 2026 The Android Open Source Project"));
        assert!(formatted.contains("# End of the bundle definition"));
        assert!(formatted.contains("name: \"TestBundle\"\n\n  # End of the bundle definition\n}"));
    }

    #[test]
    fn test_format_alternative_keywords() {
        let input = r#"sdv_service_bundle {
  name: "MyBundle"
  publisher {
    type: "MyType"
    instance: "MyInstance"
  }
  server {
    interface: "MyInterface"
  }
}
"#;
        let pairs = VsidlParser::parse(Rule::vsidl_file, input).unwrap();
        let formatted = format(pairs.into_iter().next().unwrap(), 2);

        assert_eq!(
            formatted,
            "service_bundle {\n  name: \"MyBundle\"\n\n  publisher {\n    message: \"MyType\"\n    instance: \"MyInstance\"\n  }\n\n  server {\n    service: \"MyInterface\"\n  }\n}\n"
        );
    }

    #[test]
    fn test_format_all_ast_node_types() {
        // GIVEN a perfectly formatted file containing every possible AST node
        let input = include_str!("../tests_vsidl_v1/vsidl/all_ast_node_types.vsidl");
        let pairs = VsidlParser::parse(Rule::vsidl_file, input).unwrap();

        // WHEN formatting the file
        let formatted = format(pairs.into_iter().next().unwrap(), 2);

        // THEN the input shall match the formatted output
        assert_eq!(input, formatted, "Input file should already be perfectly formatted");
    }
}
