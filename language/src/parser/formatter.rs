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
}

impl FormatContext {
    fn new(indent_width: usize) -> Self {
        Self { indent_level: 0, indent_width }
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
            // Special handling for package to add double newline
            let indent = context.get_indent();
            let value = pair.into_inner().next().unwrap().as_str().trim();
            result.push_str(&indent);
            result.push_str("package: ");
            result.push_str(value);
            result.push('\n');
        }
        Rule::COMMENT => {
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
            result.push('\n'); // Add spacing before main blocks
            let inner =
                pair.into_inner().find(|p| p.as_rule() == Rule::service_bundle_body).unwrap();
            format_block(result, "service_bundle", inner.into_inner(), context);
        }
        Rule::some_ip_mapping => {
            result.push('\n');
            let inner =
                pair.into_inner().find(|p| p.as_rule() == Rule::some_ip_mapping_body).unwrap();
            format_block(result, "some_ip_mapping", inner.into_inner(), context);
        }
        Rule::vhal_mapping => {
            result.push('\n');
            let inner = pair.into_inner().find(|p| p.as_rule() == Rule::vhal_mapping_body).unwrap();
            format_block(result, "vhal_mapping", inner.into_inner(), context);
        }
        Rule::extension => {
            let mut inner = pair.into_inner();
            let name = inner.next().unwrap().as_str(); // qualified_name
            let indent = context.get_indent();
            result.push('\n');
            result.push_str(&indent);
            result.push('[');
            result.push_str(name);
            result.push_str("] {\n");
            context.indent();
            // message_body
            if let Some(body) = inner.next() {
                format_children(result, body.into_inner(), context);
            }
            context.dedent();
            result.push_str(&indent);
            result.push_str("}\n");
        }

        // =========================================================================
        // Service Bundle Components
        // =========================================================================
        // Blocks
        Rule::publisher => {
            result.push('\n');
            format_block(
                result,
                "publisher",
                pair.into_inner().next().unwrap().into_inner(),
                context,
            )
        }
        Rule::subscriber => {
            result.push('\n');
            format_block(
                result,
                "subscriber",
                pair.into_inner().next().unwrap().into_inner(),
                context,
            )
        }
        Rule::server => {
            result.push('\n');
            format_block(result, "server", pair.into_inner().next().unwrap().into_inner(), context)
        }
        Rule::client => {
            result.push('\n');
            format_block(result, "client", pair.into_inner().next().unwrap().into_inner(), context)
        }
        Rule::unit_type => {
            result.push('\n');
            format_block(
                result,
                "unit_type",
                pair.into_inner().next().unwrap().into_inner(),
                context,
            )
        }
        Rule::build_configuration => {
            result.push('\n');
            format_block(
                result,
                "build_cfg",
                pair.into_inner().next().unwrap().into_inner(),
                context,
            )
        }
        Rule::diagnostics_declaration => {
            result.push('\n');
            format_block(
                result,
                "diagnostics_declaration",
                pair.into_inner().next().unwrap().into_inner(),
                context,
            )
        }
        // Properties
        Rule::service_bundle_name_prop => format_property(result, "name", pair, context),
        Rule::service_bundle_register_reflection_metadata => {
            format_property(result, "register_reflection_metadata", pair, context)
        }

        // Inner Properties of Service Components
        Rule::publisher_message | Rule::subscriber_message => {
            format_property(result, "message", pair, context)
        }
        Rule::publisher_topic | Rule::subscriber_topic => {
            format_property(result, "topic", pair, context)
        }
        Rule::publisher_channel | Rule::subscriber_channel => {
            format_property(result, "channel", pair, context)
        }
        Rule::publisher_service_unit_name | Rule::server_service_unit_name => {
            format_property(result, "service_unit_name", pair, context)
        }
        Rule::server_interface | Rule::client_interface => {
            format_property(result, "service", pair, context)
        }
        Rule::server_channel | Rule::client_channel => {
            format_property(result, "channel", pair, context)
        }
        Rule::unit_type_type => format_property(result, "type", pair, context),
        Rule::build_configuration_target_name => {
            format_property(result, "target_name", pair, context)
        }
        Rule::build_configuration_skip_codegen => {
            format_property(result, "skip_codegen", pair, context)
        }

        // =========================================================================
        // Diagnostics
        // =========================================================================
        // Blocks
        Rule::diagnostics_data_item => format_block(
            result,
            "data_item",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),
        Rule::diagnostics_io_control => format_block(
            result,
            "io_control",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),
        Rule::diagnostics_routine => {
            format_block(result, "routine", pair.into_inner().next().unwrap().into_inner(), context)
        }
        Rule::diagnostics_event => {
            format_block(result, "event", pair.into_inner().next().unwrap().into_inner(), context)
        }

        // Nested Routine Blocks
        Rule::diagnostics_routine_start => {
            format_block(result, "start", pair.into_inner().next().unwrap().into_inner(), context)
        }
        Rule::diagnostics_routine_stop => {
            format_block(result, "stop", pair.into_inner().next().unwrap().into_inner(), context)
        }
        Rule::diagnostics_routine_result => {
            format_block(result, "result", pair.into_inner().next().unwrap().into_inner(), context)
        }

        // Properties
        Rule::diagnostics_data_item_id
        | Rule::diagnostics_io_control_id
        | Rule::diagnostics_routine_id
        | Rule::diagnostics_event_id => format_property(result, "id", pair, context),
        Rule::diagnostics_data_item_message_name | Rule::diagnostics_io_control_message_name => {
            format_property(result, "message_name", pair, context)
        }
        Rule::diagnostics_data_item_is_writable => {
            format_property(result, "is_writable", pair, context)
        }
        Rule::diagnostics_method_request => format_property(result, "request", pair, context),
        Rule::diagnostics_method_response => format_property(result, "response", pair, context),
        Rule::diagnostics_event_extended_data_message_name => {
            format_property(result, "extended_data_message_name", pair, context)
        }
        Rule::diagnostics_event_fault_listener_mask => {
            format_property(result, "fault_listener_mask", pair, context)
        }
        Rule::diagnostics_event_service_unit_name => {
            format_property(result, "service_unit_name", pair, context)
        }

        // =========================================================================
        // VHAL
        // =========================================================================
        // Blocks
        Rule::vhal_vehicle_property => format_block(result, "property", pair.into_inner(), context),
        Rule::vhal_vehicle_prop_config => format_block(
            result,
            "prop_config",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),
        Rule::vhal_vehicle_default_value | Rule::vhal_property_area_default_value => format_block(
            result,
            "default_value",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),
        Rule::vhal_vehicle_default_area_config => format_block(
            result,
            "default_area_config",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),
        Rule::vhal_vehicle_prop_config_area_configs => format_block(
            result,
            "area_configs",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),
        Rule::vhal_data_source => format_block(result, "source", pair.into_inner(), context),
        Rule::vhal_area_mapping => format_block(result, "area_mapping", pair.into_inner(), context),
        Rule::vhal_area_mapping_area => {
            format_block(result, "area", pair.into_inner().next().unwrap().into_inner(), context)
        }
        Rule::vhal_property_area_config => format_block(
            result,
            "area_config",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),

        // Properties
        Rule::vhal_data_source_unit_type => format_property(result, "unit_type", pair, context),
        Rule::vhal_data_source_action => format_property(result, "action", pair, context),
        Rule::vhal_data_source_field_path => format_property(result, "field_path", pair, context),
        Rule::vhal_data_source_status_field_path => {
            format_property(result, "status_field_path", pair, context)
        }
        Rule::vhal_area_mapping_publication_instance => {
            format_property(result, "publication_instance", pair, context)
        }

        Rule::vhal_default_value_string_value => {
            format_property(result, "string_value", pair, context)
        }
        Rule::vhal_default_value_bytes_value => {
            format_property(result, "bytes_value", pair, context)
        }
        Rule::vhal_default_value_int32_values => {
            format_property(result, "int32_values", pair, context)
        }
        Rule::vhal_default_value_int64_values => {
            format_property(result, "int64_values", pair, context)
        }
        Rule::vhal_default_value_float_values => {
            format_property(result, "float_values", pair, context)
        }

        Rule::vhal_vehicle_prop_config_prop => format_property(result, "prop", pair, context),
        Rule::vhal_vehicle_prop_config_access | Rule::vhal_vehicle_area_config_access => {
            format_property(result, "access", pair, context)
        }
        Rule::vhal_vehicle_prop_config_change_mode => {
            format_property(result, "change_mode", pair, context)
        }
        Rule::vhal_vehicle_prop_config_config_array => {
            format_property(result, "config_array", pair, context)
        }
        Rule::vhal_vehicle_prop_config_config_string => {
            format_property(result, "config_string", pair, context)
        }
        Rule::vhal_vehicle_prop_config_min_sample_rate => {
            format_property(result, "min_sample_rate", pair, context)
        }
        Rule::vhal_vehicle_prop_config_max_sample_rate => {
            format_property(result, "max_sample_rate", pair, context)
        }

        Rule::vhal_vehicle_area_config_area_id => format_property(result, "area_id", pair, context),
        Rule::vhal_vehicle_area_config_min_int32_value => {
            format_property(result, "min_int32_value", pair, context)
        }
        Rule::vhal_vehicle_area_config_max_int32_value => {
            format_property(result, "max_int32_value", pair, context)
        }
        Rule::vhal_vehicle_area_config_min_int64_value => {
            format_property(result, "min_int64_value", pair, context)
        }
        Rule::vhal_vehicle_area_config_max_int64_value => {
            format_property(result, "max_int64_value", pair, context)
        }
        Rule::vhal_vehicle_area_config_min_float_value => {
            format_property(result, "min_float_value", pair, context)
        }
        Rule::vhal_vehicle_area_config_max_float_value => {
            format_property(result, "max_float_value", pair, context)
        }
        Rule::vhal_vehicle_area_config_supported_enum_values => {
            format_property(result, "supported_enum_values", pair, context)
        }
        Rule::vhal_vehicle_area_config_support_variable_update_rate => {
            format_property(result, "support_variable_update_rate", pair, context)
        }

        // =========================================================================
        // SOME/IP
        // =========================================================================
        // Blocks
        Rule::some_ip_service_mapping => {
            format_block(result, "someip_service_mapping", pair.into_inner(), context)
        }
        Rule::some_ip_instance => format_block(result, "instance", pair.into_inner(), context),
        Rule::some_ip_sdv_service_unit_parameterization_block => {
            format_block(result, "sdv_service_unit_parameterization", pair.into_inner(), context)
        }
        Rule::some_ip_instance_impl => {
            format_block(result, "instance_impl", pair.into_inner(), context)
        }
        Rule::some_ip_offered_interface | Rule::some_ip_sdv_offered_interface => {
            format_block(result, "offered_interface", pair.into_inner(), context)
        }
        Rule::some_ip_offered_interface_sdv_unit_type
        | Rule::some_ip_simple_event_sdv_unit_type
        | Rule::some_ip_event_part_sdv_unit_type
        | Rule::some_ip_sdv_type_mapping_unit_type => format_block(
            result,
            "sdv_unit_type",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),

        Rule::some_ip_method | Rule::some_ip_sdv_method => {
            format_block(result, "method", pair.into_inner(), context)
        }
        Rule::request_payload_mapping => format_block(
            result,
            "request_payload_mapping",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),
        Rule::response_payload_mapping => format_block(
            result,
            "response_payload_mapping",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),

        Rule::some_ip_offered_event => {
            format_block(result, "offered_event", pair.into_inner(), context)
        }
        Rule::some_ip_simple_event => {
            format_block(result, "simple_event", pair.into_inner(), context)
        }
        Rule::some_ip_simple_event_type_definition
        | Rule::some_ip_group_event_type_definition
        | Rule::some_ip_offered_topic_type_definition => format_block(
            result,
            "type_definition",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),

        Rule::some_ip_group_event => {
            format_block(result, "group_event", pair.into_inner(), context)
        }
        Rule::some_ip_event_part => format_block(result, "event_part", pair.into_inner(), context),
        Rule::some_ip_event_part_field_name_mapping => {
            format_block(result, "field_name_mapping", pair.into_inner(), context)
        }

        Rule::some_ip_sdv_type_mapping => {
            format_block(result, "sdv_type_mapping", pair.into_inner(), context)
        }
        Rule::some_ip_offered_topic => {
            format_block(result, "offered_topic", pair.into_inner(), context)
        }
        Rule::some_ip_simple_mapping => {
            format_block(result, "simple_mapping", pair.into_inner(), context)
        }
        Rule::some_ip_multipub_topic_info => {
            format_block(result, "multipub_topic_info", pair.into_inner(), context)
        }

        Rule::some_ip_service_unit_mapping => {
            format_block(result, "service_unit_mapping", pair.into_inner(), context)
        }
        Rule::some_ip_service_unit_mapping_service_unit_id => format_block(
            result,
            "service_unit_id",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),
        Rule::some_ip_fire_and_forget_method => {
            format_block(result, "fire_and_forget_method", pair.into_inner(), context)
        }

        // Type System Blocks
        Rule::some_ip_string_body => format_children(result, pair.into_inner(), context),
        Rule::some_ip_array_body => format_children(result, pair.into_inner(), context),
        Rule::some_ip_struct_body => format_children(result, pair.into_inner(), context),
        Rule::some_ip_array_type => format_block(
            result,
            "array_type",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),
        Rule::some_ip_struct_field => format_block(result, "field", pair.into_inner(), context),
        Rule::some_ip_struct_field_type => format_block(
            result,
            "field_type",
            pair.into_inner().next().unwrap().into_inner(),
            context,
        ),

        // Properties
        Rule::some_ip_service_id
        | Rule::some_ip_sdv_type_mapping_service_id
        | Rule::some_ip_fire_and_forget_method_service_id => {
            format_property(result, "someip_service_id", pair, context)
        }
        Rule::some_ip_version_major
        | Rule::some_ip_sdv_type_mapping_version_major
        | Rule::some_ip_fire_and_forget_method_version_major => {
            format_property(result, "version_major", pair, context)
        }
        Rule::some_ip_version_minor
        | Rule::some_ip_sdv_type_mapping_version_minor
        | Rule::some_ip_fire_and_forget_method_version_minor => {
            format_property(result, "version_minor", pair, context)
        }
        Rule::some_ip_sd_behavior => format_property(result, "sd_behavior", pair, context),
        Rule::some_ip_instance_id
        | Rule::some_ip_multipub_topic_info_someip_instance_id
        | Rule::some_ip_service_unit_mapping_instance_id
        | Rule::some_ip_fire_and_forget_method_instance_id => {
            format_property(result, "instance_id", pair, context)
        }

        Rule::some_ip_sdv_service_unit_parameterization_block_key | Rule::some_ip_map_entry_key => {
            format_property(result, "key", pair, context)
        }
        Rule::some_ip_sdv_service_unit_parameterization_block_value
        | Rule::some_ip_map_entry_value => format_property(result, "value", pair, context),

        Rule::some_ip_service_unit
        | Rule::some_ip_simple_event_service_unit
        | Rule::some_ip_event_part_service_unit => {
            format_property(result, "service_unit", pair, context)
        }

        Rule::some_ip_unit_type_ref_package_name | Rule::some_ip_service_unit_id_package_name => {
            format_property(result, "package_name", pair, context)
        }
        Rule::some_ip_unit_type_ref_service_bundle_name
        | Rule::some_ip_service_unit_id_service_bundle_name => {
            format_property(result, "service_bundle_name", pair, context)
        }
        Rule::some_ip_unit_type_ref_type_name => {
            format_property(result, "type_name", pair, context)
        }

        Rule::some_ip_method_procedure | Rule::some_ip_sdv_method_procedure => {
            format_property(result, "procedure", pair, context)
        }
        Rule::some_ip_method_method_id
        | Rule::some_ip_sdv_method_method_id
        | Rule::some_ip_fire_and_forget_method_method_id => {
            format_property(result, "method_id", pair, context)
        }

        Rule::some_ip_offered_event_group_id | Rule::some_ip_simple_mapping_group_id => {
            format_property(result, "group_id", pair, context)
        }
        Rule::some_ip_offered_event_event_id | Rule::some_ip_simple_mapping_event_id => {
            format_property(result, "event_id", pair, context)
        }
        Rule::some_ip_simple_event_message_translation_mode => {
            format_property(result, "message_translation_mode", pair, context)
        }

        Rule::some_ip_offered_topic_someip_ff_methods => {
            format_property(result, "someip_ff_methods", pair, context)
        }

        Rule::some_ip_service_unit_id_vm_name => format_property(result, "vm_name", pair, context),
        Rule::some_ip_service_unit_id_service_instance_name => {
            format_property(result, "service_instance_name", pair, context)
        }
        Rule::some_ip_service_unit_id_service_unit_name => {
            format_property(result, "service_unit_name", pair, context)
        }

        // Type Properties
        Rule::some_ip_type => {
            // handle primitive/string/array/struct wrapper
            let inner = pair.into_inner().next().unwrap();
            match inner.as_rule() {
                Rule::some_ip_type_someip_primitive => {
                    format_property(result, "someip_primitive", inner, context)
                }
                Rule::some_ip_string_body => {
                    format_block(result, "someip_string", inner.into_inner(), context)
                }
                Rule::some_ip_array_body => {
                    format_block(result, "someip_array", inner.into_inner(), context)
                }
                Rule::some_ip_struct_body => {
                    format_block(result, "someip_struct", inner.into_inner(), context)
                }
                _ => {}
            }
        }
        Rule::some_ip_string_body_fixed_length | Rule::some_ip_array_body_fixed_length => {
            format_property(result, "fixed_length", pair, context)
        }
        Rule::some_ip_string_body_length_field_size
        | Rule::some_ip_array_body_length_field_size
        | Rule::some_ip_struct_body_length_field_size => {
            format_property(result, "length_field_size", pair, context)
        }
        Rule::some_ip_string_body_string_encoding => {
            format_property(result, "string_encoding", pair, context)
        }
        Rule::some_ip_struct_field_data_tag => format_property(result, "data_tag", pair, context),
        Rule::some_ip_struct_field_field_name => {
            format_property(result, "field_name", pair, context)
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
