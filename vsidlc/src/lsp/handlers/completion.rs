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
use crate::lsp::lsp_file_provider::LspFileProvider;
use crate::lsp::recovering_parser::parse_vsidl_with_recovery;
use crate::lsp::utils::convert_position_0_based_to_1_based_index;
use anyhow::Result;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, InsertTextFormat,
    Position,
};
use pest::iterators::Pair;
use std::collections::HashSet;
use std::path::Path;
use vsidl_v1::span::Span;
use vsidl_v1_parser::file_provider::FileProvider;
use vsidl_v1_parser::line_col_to_offset;
use vsidl_v1_parser::pest_parser::Rule;
use vsidl_v1_parser::Scope;
use vsidl_v1_parser::SymbolTable;

/// Handles a textDocument/completion request.
pub(crate) fn on_completion(
    provider: &LspFileProvider,
    context: &mut CatalogContext,
    params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    let Ok(file_path) = params.text_document_position.text_document.uri.to_file_path() else {
        // If it's not a valid file path, we stop
        // as we can't provide completions for non-file URIs.
        return Ok(None);
    };
    let position = params.text_document_position.position;

    let mut session = CompletionSession::new(context, provider, &file_path, position);
    session.run()
}

// =================================================================================================
// Data Definitions & Configuration
// =================================================================================================

/// Defines the semantic type of a property to determine how its value should be completed.
#[derive(PartialEq, Clone, Copy)]
enum PropertyType {
    String,
    Numeric,
    Boolean,
    Identifier,
}

/// Defines the type of symbols that can be suggested for a given property key.
#[derive(PartialEq, Clone, Copy, Debug)]
enum SymbolCompletionType {
    ProtoMessageOrEnum,
    ProtoRpcService,
    PubSubChannel,
    RpcChannel,
    Topic,
    None,
}

/// Configuration for a specific keyword suggestion.
struct KeywordConfig {
    /// The keyword label to suggest.
    label: &'static str,
    /// Whether this keyword starts a block (e.g., "publisher { ... }").
    is_block: bool,
    /// Optional field names to autocomplete inside a block.
    initial_fields: Vec<&'static str>,
    /// Optional detailed description for the keyword.
    detail: Option<&'static str>,
    /// Whether this keyword is deprecated.
    deprecated: bool,
}

/// Centralized configuration for VSIDL language rules and completion behaviors.
struct LanguageConfig;

impl LanguageConfig {
    /// Returns the list of valid keywords and whether they start a block for a given rule.
    fn get_keywords_for_rule(rule: Rule, uses_pdk29: bool) -> Vec<KeywordConfig> {
        let raw_keywords: Vec<(
            &'static str,
            bool,
            &'static [&'static str],
            Option<&'static str>,
            bool,
        )> = match rule {
            Rule::vsidl_file => vec![
                (
                    "package",
                    false,
                    &[],
                    Some("Specifies the package namespace for all entities within the file"),
                    false,
                ),
                (
                    "service_bundle",
                    true,
                    &["name"],
                    Some("Groups related service units intended to be deployed together"),
                    false,
                ),
                (
                    "sdv_service_bundle",
                    true,
                    &["name"],
                    Some("Deprecated: Use service_bundle instead"),
                    true,
                ),
                (
                    "some_ip_mapping",
                    true,
                    &[],
                    Some("Defines rules for mapping SDV services to SOME/IP"),
                    false,
                ),
                (
                    "vhal_mapping",
                    true,
                    &[],
                    Some("Defines rules for mapping SDV services to Android VHAL properties"),
                    false,
                ),
                (
                    "extension",
                    true,
                    &[],
                    Some("Enables custom extensions beyond the standard VSIDL model"),
                    false,
                ),
                ("import", false, &[], Some("Imports another VSIDL or proto file"), false),
            ],
            Rule::service_bundle_body | Rule::sdv_service_bundle => vec![
                (
                    "name",
                    false,
                    &[],
                    Some("Unique name of the service bundle within its package"),
                    false,
                ),
                (
                    "publisher",
                    true,
                    if uses_pdk29 {
                        &["message", "channel"]
                    } else {
                        &["message", "topic", "capacity"]
                    },
                    Some("PubSub data source providing Protobuf messages to a topic"),
                    false,
                ),
                (
                    "subscriber",
                    true,
                    if uses_pdk29 { &["message", "channel"] } else { &["message", "topic"] },
                    Some("PubSub receiver consuming Protobuf messages from topics"),
                    false,
                ),
                (
                    "server",
                    true,
                    if uses_pdk29 { &["service"] } else { &["service", "channel"] },
                    Some("RPC server that implements a Protobuf service"),
                    false,
                ),
                (
                    "client",
                    true,
                    if uses_pdk29 { &["service"] } else { &["service", "channel"] },
                    Some("RPC client that consumes a remote Protobuf service"),
                    false,
                ),
                (
                    "unit_type",
                    true,
                    &["type"],
                    Some("Deprecated. vsidlc ignores this block."),
                    true,
                ),
                (
                    "build_cfg",
                    true,
                    &["target_name"],
                    Some("Configures optional parameters for code generation and build targets"),
                    false,
                ),
                (
                    "diagnostics_declaration",
                    true,
                    &[],
                    Some("Declares diagnostic data items, routines, and events for the bundle"),
                    false,
                ),
                (
                    "register_reflection_metadata",
                    false,
                    &[],
                    Some("Enables runtime metadata for service unit discovery"),
                    false,
                ),
            ],
            Rule::publisher_body | Rule::publisher => vec![
                ("type", false, &[], Some("Deprecated: Use message instead"), true),
                ("message", false, &[], Some("Protobuf message type"), false),
                ("topic", false, &[], Some("Many-to-Many Message Bus"), false),
                ("instance", false, &[], Some("Deprecated: Use channel instead"), true),
                (
                    "channel",
                    false,
                    &[],
                    Some("Specifies specific instances or variants of the data topic"),
                    false,
                ),
                (
                    "service_unit_name",
                    false,
                    &[],
                    Some("Custom name used for service discovery registration"),
                    false,
                ),
                (
                    "capacity",
                    false,
                    &[],
                    Some("Maximum number of messages the publication queue can hold"),
                    false,
                ),
            ],
            Rule::subscriber_body | Rule::subscriber => vec![
                ("type", false, &[], Some("Deprecated: Use message instead"), true),
                ("message", false, &[], Some("Protobuf message type"), false),
                ("topic", false, &[], Some("Many-to-Many Message Bus"), false),
                ("instance", false, &[], Some("Deprecated: Use channel instead"), true),
                (
                    "channel",
                    false,
                    &[],
                    Some("Specific instances of the message to subscribe to"),
                    false,
                ),
            ],
            Rule::server_body | Rule::server => vec![
                ("interface", false, &[], Some("Deprecated: Use service instead"), true),
                ("service", false, &[], Some("Protobuf RPC service"), false),
                (
                    "channel",
                    false,
                    &[],
                    Some("Unique identifier for the RPC communication path"),
                    false,
                ),
                (
                    "service_unit_name",
                    false,
                    &[],
                    Some("Custom name used for service discovery registration"),
                    false,
                ),
            ],
            Rule::client_body | Rule::client => vec![
                ("interface", false, &[], Some("Deprecated: Use service instead"), true),
                ("service", false, &[], Some("Protobuf RPC service"), false),
                (
                    "channel",
                    false,
                    &[],
                    Some("Unique identifier for the RPC communication path"),
                    false,
                ),
            ],
            Rule::unit_type_body | Rule::unit_type => vec![(
                "type",
                false,
                &[],
                Some("Protobuf publication or service type to own"),
                false,
            )],
            Rule::build_configuration_body | Rule::build_configuration => vec![
                (
                    "target_name",
                    false,
                    &[],
                    Some("Specifies the snake_case name of the library in Android.bp"),
                    false,
                ),
                (
                    "skip_codegen",
                    false,
                    &[],
                    Some("If true, VSIDLC generates no code for this bundle"),
                    false,
                ),
            ],
            // SOME/IP Mapping
            Rule::some_ip_mapping_body | Rule::some_ip_mapping => vec![
                (
                    "someip_service_mapping",
                    true,
                    &[],
                    Some("Maps SOME/IP services to SDV units"),
                    false,
                ),
                (
                    "sdv_type_mapping",
                    true,
                    &[],
                    Some("Maps SDV Unit Types to SOME/IP rules"),
                    false,
                ),
            ],
            Rule::some_ip_service_mapping => vec![
                ("someip_service_id", false, &[], Some("SOME/IP service ID (16 bits)"), false),
                ("version_major", false, &[], Some("Major version of the service (8 bits)"), false),
                ("version_minor", false, &[], Some("Minor version of the service"), false),
                (
                    "instance",
                    true,
                    &["instance_id"],
                    Some("SOME/IP service instance mapping"),
                    false,
                ),
                (
                    "instance_impl",
                    true,
                    &[],
                    Some("Translation of SOME/IP instance to SDV service units"),
                    false,
                ),
                (
                    "sd_behavior",
                    false,
                    &[],
                    Some("Expected matching behavior when discovering services"),
                    false,
                ),
            ],
            Rule::some_ip_instance => vec![
                ("instance_id", false, &[], Some("SOME/IP Instance ID"), false),
                (
                    "sdv_service_unit_parameterization",
                    true,
                    &[],
                    Some("Service unit name parameterization"),
                    false,
                ),
            ],
            Rule::some_ip_sdv_service_unit_parameterization_block => vec![
                ("key", false, &[], Some("Parameter key (referenced in implementation)"), false),
                ("value", false, &[], Some("Service unit name value"), false),
            ],
            Rule::some_ip_instance_impl => vec![
                ("offered_interface", true, &[], Some("Mapping for offered RPC interfaces"), false),
                ("offered_event", true, &[], Some("Mapping for offered SOME/IP events"), false),
            ],
            Rule::some_ip_offered_interface => vec![
                (
                    "sdv_unit_type",
                    true,
                    &[],
                    Some("SDV Unit Type corresponding to this interface"),
                    false,
                ),
                (
                    "service_unit",
                    false,
                    &[],
                    Some("Service unit ID (references parameterization)"),
                    false,
                ),
                ("method", true, &[], Some("RPC methods available on this service"), false),
                (
                    "channel",
                    false,
                    &[],
                    Some("Unique identifier for the RPC communication path"),
                    false,
                ),
            ],
            Rule::some_ip_method | Rule::some_ip_sdv_method => vec![
                ("procedure", false, &[], Some("SDV procedure name"), false),
                ("method_id", false, &[], Some("SOME/IP method ID"), false),
                ("request_payload_mapping", true, &[], Some("Mapping rule for RPC request"), false),
                (
                    "response_payload_mapping",
                    true,
                    &[],
                    Some("Mapping rule for RPC response"),
                    false,
                ),
            ],
            Rule::some_ip_offered_event => vec![
                ("group_id", false, &[], Some("SOME/IP group ID of the event"), false),
                ("event_id", false, &[], Some("SOME/IP event ID"), false),
                (
                    "simple_event",
                    true,
                    &[],
                    Some("1:1 mapping between SOME/IP event and SDV topic"),
                    false,
                ),
                (
                    "group_event",
                    true,
                    &[],
                    Some("1:n mapping between SOME/IP event and multiple topics"),
                    false,
                ),
            ],
            Rule::some_ip_simple_event => vec![
                (
                    "sdv_unit_type",
                    true,
                    &[],
                    Some("SDV Unit Type corresponding to this event"),
                    false,
                ),
                ("service_unit", false, &[], Some("Service unit ID"), false),
                ("type_definition", true, &[], Some("SOME/IP data format definition"), false),
                (
                    "message_translation_mode",
                    false,
                    &[],
                    Some("Message translation mode (REFLECTION, INTERPRET_AS_BYTES)"),
                    false,
                ),
                ("topic", false, &[], Some("Many-to-Many Message Bus"), false),
                (
                    "capacity",
                    false,
                    &[],
                    Some("Maximum number of messages the publication queue can hold"),
                    false,
                ),
            ],
            Rule::some_ip_group_event => vec![
                ("type_definition", true, &[], Some("SOME/IP data format definition"), false),
                ("event_part", true, &[], Some("Description of the event parts"), false),
            ],
            Rule::some_ip_event_part => vec![
                ("sdv_unit_type", true, &[], Some("SDV Unit Type for this part"), false),
                ("service_unit", false, &[], Some("Service unit ID"), false),
                (
                    "field_name_mapping",
                    true,
                    &[],
                    Some("Maps SOME/IP field names to SDV field names"),
                    false,
                ),
                ("topic", false, &[], Some("Many-to-Many Message Bus"), false),
                (
                    "capacity",
                    false,
                    &[],
                    Some("Maximum number of messages the publication queue can hold"),
                    false,
                ),
            ],
            Rule::some_ip_sdv_type_mapping => vec![
                ("unit_type", true, &[], Some("Unit type being mapped"), false),
                ("someip_service_id", false, &[], Some("SOME/IP service ID"), false),
                ("version_major", false, &[], Some("Major version of the service"), false),
                ("version_minor", false, &[], Some("Minor version of the service"), false),
                ("offered_interface", true, &[], Some("Mapping for SDV RPC Unit Type"), false),
                ("offered_topic", true, &[], Some("Mapping for SDV Topic Unit Type"), false),
                ("service_unit_mapping", true, &[], Some("Mappings for SDV service units"), false),
                (
                    "service_oriented_mapping",
                    true,
                    &[],
                    Some("SOA Mappings for SDV service units"),
                    false,
                ),
            ],
            Rule::some_ip_service_oriented_mapping => vec![
                ("rpc_interface_mapping", true, &[], Some("Mapping for SDV RPC Unit Type"), false),
                ("publication_mapping", true, &[], Some("Mapping for SDV Topic Unit Type"), false),
            ],
            Rule::some_ip_rpc_interface_mapping => vec![
                ("offered_interface", true, &[], Some("Mapping for offered RPC interfaces"), false),
                (
                    "rpc_channel_mapping",
                    true,
                    &["channel", "instance_id"],
                    Some("Mapping for RPC channels"),
                    false,
                ),
            ],
            Rule::some_ip_rpc_channel_mapping => vec![
                (
                    "channel",
                    false,
                    &[],
                    Some("Unique identifier for the RPC communication path"),
                    false,
                ),
                ("instance_id", false, &[], Some("SOME/IP Instance ID"), false),
            ],
            Rule::some_ip_publication_mapping => vec![
                ("offered_topic", true, &[], Some("Mapping for offered SOME/IP topics"), false),
                (
                    "topic_mapping",
                    true,
                    &["topic", "instance_id"],
                    Some("Mapping for topics"),
                    false,
                ),
            ],
            Rule::some_ip_topic_mapping => vec![
                ("topic", false, &[], Some("Many-to-Many Message Bus"), false),
                ("instance_id", false, &[], Some("SOME/IP Instance ID"), false),
                ("fire_and_forget_method", true, &[], Some("List of FireAndForget methods"), false),
            ],
            Rule::some_ip_sdv_offered_interface => {
                vec![("method", true, &[], Some("RPC methods available on this service"), false)]
            }
            Rule::some_ip_offered_topic => vec![
                ("type_definition", true, &[], Some("SOME/IP data format definition"), false),
                ("simple_mapping", true, &[], Some("Map topic to a SOME/IP event"), false),
                (
                    "someip_ff_methods",
                    false,
                    &[],
                    Some("Broker should call fire and forget methods"),
                    false,
                ),
                ("multipub_topic_info", true, &[], Some("Interpret as multi-pub topic"), false),
            ],
            Rule::some_ip_simple_mapping => vec![
                ("group_id", false, &[], Some("SOME/IP group ID"), false),
                ("event_id", false, &[], Some("SOME/IP event ID"), false),
            ],
            Rule::some_ip_multipub_topic_info => vec![(
                "someip_instance_id",
                false,
                &[],
                Some("SOME/IP instance ID for multipub service"),
                false,
            )],
            Rule::some_ip_service_unit_mapping => vec![
                (
                    "service_unit_id",
                    true,
                    &[],
                    Some("Identifier for the service unit within SDV"),
                    false,
                ),
                ("instance_id", false, &[], Some("SOME/IP Instance ID to offer"), false),
                ("fire_and_forget_method", true, &[], Some("List of FireAndForget methods"), false),
            ],
            Rule::some_ip_service_unit_id_block => vec![
                ("vm_name", false, &[], Some("Name of the VM where the bundle lives"), false),
                ("package_name", false, &[], Some("Package name of the bundle"), false),
                ("service_bundle_name", false, &[], Some("Name of the bundle"), false),
                ("service_instance_name", false, &[], Some("Name of the bundle instance"), false),
                ("service_unit_name", false, &[], Some("Service unit name"), false),
            ],
            Rule::some_ip_fire_and_forget_method => vec![
                ("service_id", false, &[], Some("SOME/IP service ID"), false),
                ("instance_id", false, &[], Some("SOME/IP instance ID"), false),
                ("version_major", false, &[], Some("Major version of the service"), false),
                ("version_minor", false, &[], Some("Minor version of the service"), false),
                ("method_id", false, &[], Some("SOME/IP method ID"), false),
            ],
            Rule::some_ip_string_body => vec![
                ("fixed_length", false, &[], Some("Fixed length of the string in bytes"), false),
                ("length_field_size", false, &[], Some("Size of the length field in bytes"), false),
                ("string_encoding", false, &[], Some("Encoding used for the string"), false),
            ],
            Rule::some_ip_array_body => vec![
                ("fixed_length", false, &[], Some("Number of elements in the array"), false),
                ("length_field_size", false, &[], Some("Size of the length field"), false),
                ("array_type", true, &[], Some("Type of the array elements"), false),
            ],
            Rule::some_ip_struct_body => vec![
                ("length_field_size", false, &[], Some("Size of the length field in bytes"), false),
                ("field", true, &[], Some("Field of the struct type"), false),
            ],
            Rule::some_ip_struct_field => vec![
                ("data_tag", false, &[], Some("Optional data tag to identify the field"), false),
                ("field_name", false, &[], Some("Field name (matches proto)"), false),
                ("field_type", true, &[], Some("Field data type"), false),
            ],

            // Diagnostics
            Rule::diagnostics_declaration_body | Rule::diagnostics_declaration => vec![
                ("data_item", true, &[], Some("Diagnostic data item declaration"), false),
                ("io_control", true, &[], Some("Diagnostic data for input/output control"), false),
                ("routine", true, &[], Some("Diagnostic routine declaration"), false),
                (
                    "event",
                    true,
                    if uses_pdk29 { &[] } else { &["topic"] },
                    Some("Diagnostic event and fault listener declaration"),
                    false,
                ),
            ],
            Rule::diagnostics_data_item_body => vec![
                ("id", false, &[], Some("Unique ID of the data item"), false),
                (
                    "message_name",
                    false,
                    &[],
                    Some("Fully qualified name of the structure message"),
                    false,
                ),
                (
                    "is_writable",
                    false,
                    &[],
                    Some("Indicates if the item is writable by Diagnostics Manager"),
                    false,
                ),
            ],
            Rule::diagnostics_io_control_body => vec![
                ("id", false, &[], Some("Unique ID of the IO control data"), false),
                (
                    "message_name",
                    false,
                    &[],
                    Some("Fully qualified name of the structure message"),
                    false,
                ),
            ],
            Rule::diagnostics_routine_body => vec![
                ("id", false, &[], Some("Unique ID of the routine"), false),
                ("start", true, &[], Some("Information about routine start method"), false),
                ("stop", true, &[], Some("Information about routine stop method"), false),
                ("result", true, &[], Some("Information about routine result method"), false),
            ],
            Rule::diagnostics_method_body => vec![
                ("request", false, &[], Some("Fully qualified name of the request message"), false),
                (
                    "response",
                    false,
                    &[],
                    Some("Fully qualified name of the response message"),
                    false,
                ),
            ],
            Rule::diagnostics_event_body => vec![
                ("id", false, &[], Some("Unique ID of the event"), false),
                (
                    "extended_data_message_name",
                    false,
                    &[],
                    Some("Message name of event's extended data"),
                    false,
                ),
                ("fault_listener_mask", false, &[], Some("Fault status changes to notify"), false),
                (
                    "service_unit_name",
                    false,
                    &[],
                    Some("Service unit name of event publisher"),
                    false,
                ),
                (
                    "topic",
                    false,
                    &[],
                    Some("Specific topic on which the event is published"),
                    false,
                ),
            ],

            // VHAL
            Rule::vhal_mapping_body | Rule::vhal_mapping => vec![
                ("property", true, &[], Some("Defines a vehicle property mapping"), false),
                (
                    "source",
                    true,
                    &[],
                    Some("Defines the SDV source for the vehicle property"),
                    false,
                ),
                (
                    "area_mapping",
                    true,
                    &[],
                    Some("Maps vehicle property areas to SDV instances"),
                    false,
                ),
            ],
            Rule::vhal_vehicle_property => vec![
                ("prop_config", true, &[], Some("Vehicle Property configuration"), false),
                ("default_value", true, &[], Some("Default value for the property"), false),
                (
                    "default_area_config",
                    true,
                    &[],
                    Some("Default area configuration for all areas"),
                    false,
                ),
            ],
            Rule::vhal_vehicle_prop_config_body => vec![
                ("prop", false, &[], Some("Vehicle Property ID"), false),
                ("access", false, &[], Some("Property access (READ, WRITE, READ_WRITE)"), false),
                (
                    "change_mode",
                    false,
                    &[],
                    Some("Property change mode (STATIC, ON_CHANGE, CONTINUOUS)"),
                    false,
                ),
                ("area_configs", true, &[], Some("Area specific configurations"), false),
                ("config_array", false, &[], Some("Additional configuration parameters"), false),
                ("config_string", false, &[], Some("Additional configuration string"), false),
                (
                    "min_sample_rate",
                    false,
                    &[],
                    Some("Minimum sample rate for continuous properties"),
                    false,
                ),
                (
                    "max_sample_rate",
                    false,
                    &[],
                    Some("Maximum sample rate for continuous properties"),
                    false,
                ),
            ],
            Rule::vhal_data_source => vec![
                ("unit_type", false, &[], Some("FQN of the unit type protobuf message"), false),
                (
                    "action",
                    false,
                    &[],
                    Some("Action on the unit type (ACTION_SUBSCRIBE, ACTION_PUBLISH)"),
                    false,
                ),
                ("field_path", false, &[], Some("Path to the field mapped to the property"), false),
                (
                    "status_field_path",
                    false,
                    &[],
                    Some("Path to the bool field specifying status"),
                    false,
                ),
            ],
            Rule::vhal_area_mapping => vec![
                ("area", true, &[], Some("The vehicle property area definition"), false),
                (
                    "publication_instance",
                    false,
                    &[],
                    Some("SDV publication instance (e.g. FRONT_LEFT)"),
                    false,
                ),
            ],
            Rule::vhal_property_area_body => vec![
                ("area_config", true, &[], Some("Vehicle property area configuration"), false),
                ("default_value", true, &[], Some("Default value for this specific area"), false),
            ],
            Rule::vhal_default_value_body => vec![
                ("string_value", false, &[], Some("String value for STRING properties"), false),
                ("bytes_value", false, &[], Some("Bytes value for BYTES properties"), false),
                (
                    "int32_values",
                    false,
                    &[],
                    Some("Values for INT, INT_VEC, or BOOLEAN properties"),
                    false,
                ),
                (
                    "int64_values",
                    false,
                    &[],
                    Some("Values for INT64 or INT64_VEC properties"),
                    false,
                ),
                (
                    "float_values",
                    false,
                    &[],
                    Some("Values for FLOAT or FLOAT_VEC properties"),
                    false,
                ),
            ],
            Rule::vhal_vehicle_area_config_body => vec![
                ("area_id", false, &[], Some("Area ID (e.g. Constants::DOOR_1_LEFT)"), false),
                ("min_int32_value", false, &[], Some("Minimum int32 value allowed"), false),
                ("max_int32_value", false, &[], Some("Maximum int32 value allowed"), false),
                ("min_int64_value", false, &[], Some("Minimum int64 value allowed"), false),
                ("max_int64_value", false, &[], Some("Maximum int64 value allowed"), false),
                ("min_float_value", false, &[], Some("Minimum float value allowed"), false),
                ("max_float_value", false, &[], Some("Maximum float value allowed"), false),
                ("supported_enum_values", false, &[], Some("Allowed enum values"), false),
                (
                    "access",
                    false,
                    &[],
                    Some("Area-specific access (READ, WRITE, READ_WRITE)"),
                    false,
                ),
                (
                    "support_variable_update_rate",
                    false,
                    &[],
                    Some("True if variable update rate is supported"),
                    false,
                ),
            ],
            _ => vec![],
        };

        raw_keywords
            .into_iter()
            .map(|(label, is_block, initial_fields, detail, deprecated)| KeywordConfig {
                label,
                is_block,
                initial_fields: initial_fields.to_vec(),
                detail,
                deprecated,
            })
            .collect()
    }

    /// Determines the semantic type of a property key to guide snippet generation.
    fn get_property_type(key: &str) -> PropertyType {
        match key {
            "capacity"
            | "prop"
            | "area_id"
            | "someip_service_id"
            | "version_major"
            | "version_minor"
            | "instance_id"
            | "service_unit"
            | "method_id"
            | "group_id"
            | "event_id"
            | "data_tag"
            | "fixed_length"
            | "fault_listener_mask"
            | "min_int32_value"
            | "max_int32_value"
            | "min_int64_value"
            | "max_int64_value"
            | "min_float_value"
            | "max_float_value"
            | "supported_enum_values"
            | "message_count" => PropertyType::Numeric,
            "skip_codegen"
            | "register_reflection_metadata"
            | "is_writable"
            | "someip_ff_methods"
            | "support_variable_update_rate" => PropertyType::Boolean,
            "action"
            | "access"
            | "change_mode"
            | "sd_behavior"
            | "message_translation_mode"
            | "someip_primitive"
            | "length_field_size"
            | "string_encoding"
            | "model" => PropertyType::Identifier,
            _ => PropertyType::String,
        }
    }

    /// Determines what kind of symbols (if any) should be suggested for a given property key.
    fn get_symbol_completion_type(key: &str, parent: Rule) -> SymbolCompletionType {
        match key {
            "message"
            | "type"
            | "message_name"
            | "request"
            | "response"
            | "extended_data_message_name"
            | "unit_type" => SymbolCompletionType::ProtoMessageOrEnum,
            "interface" | "service" => SymbolCompletionType::ProtoRpcService,
            "topic" => SymbolCompletionType::Topic,
            "channel" | "instance" => {
                if matches!(
                    parent,
                    Rule::server_body
                        | Rule::client_body
                        | Rule::some_ip_offered_interface
                        | Rule::some_ip_rpc_channel_mapping
                ) {
                    SymbolCompletionType::RpcChannel
                } else {
                    SymbolCompletionType::PubSubChannel
                }
            }
            _ => SymbolCompletionType::None,
        }
    }

    /// Validates if a property key is allowed within a given parent rule.
    fn is_valid_parent_for_key(key: &str, parent: Rule) -> bool {
        match key {
            "message" | "type" => matches!(
                parent,
                Rule::publisher_body
                    | Rule::subscriber_body
                    | Rule::unit_type_body
                    | Rule::vsidl_file
            ),
            "message_name" => matches!(
                parent,
                Rule::diagnostics_data_item_body | Rule::diagnostics_io_control_body
            ),
            "request" | "response" => matches!(parent, Rule::diagnostics_method_body),
            "extended_data_message_name" => matches!(parent, Rule::diagnostics_event_body),
            "unit_type" => matches!(parent, Rule::vhal_data_source),
            "interface" | "service" => {
                matches!(parent, Rule::server_body | Rule::client_body | Rule::vsidl_file)
            }
            "topic" | "channel" | "instance" => matches!(
                parent,
                Rule::publisher_body
                    | Rule::subscriber_body
                    | Rule::server_body
                    | Rule::client_body
                    | Rule::diagnostics_event_body
                    | Rule::vsidl_file
                    | Rule::some_ip_offered_interface
                    | Rule::some_ip_simple_event
                    | Rule::some_ip_event_part
                    | Rule::some_ip_rpc_channel_mapping
                    | Rule::some_ip_topic_mapping
            ),
            _ => false,
        }
    }
}

// =================================================================================================
// Helper Structures
// =================================================================================================

/// Encapsulates the grammatical context of the cursor position within the current line.
struct CursorContext {
    /// Text on the current line before the cursor.
    line_prefix: String,
    /// Text on the current line after the cursor.
    _line_suffix: String,
    /// Position of the last colon on the line before the cursor, if any.
    colon_pos: Option<usize>,
    /// Search term derived from the text after the colon or the last word.
    search_term: String,
    /// Whether the cursor is currently inside an open quotation mark.
    is_inside_quote: bool,
    /// Whether there is a closing quote on the line after the cursor.
    has_close_quote: bool,
}

impl CursorContext {
    /// Analyzes the content at the given offset to build the cursor context.
    fn new(content: &str, offset: usize) -> Self {
        let prefix = &content[..offset];
        let line_start = prefix.rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line_prefix = &prefix[line_start..];

        let suffix = &content[offset..];
        let line_end = suffix.find('\n').unwrap_or(suffix.len());
        let line_suffix = &suffix[..line_end];

        let colon_pos = line_prefix.rfind(':');

        // Determine quote status
        let quote_count = line_prefix.chars().filter(|&c| c == '"').count();
        let is_inside_quote = quote_count % 2 != 0;
        let has_close_quote = line_suffix.contains('"');

        // Extract search term
        let search_term = if let Some(pos) = line_prefix.rfind(':') {
            let val_prefix = &line_prefix[pos + 1..];
            if let Some(open_quote_pos) = val_prefix.find('"') {
                &val_prefix[open_quote_pos + 1..]
            } else {
                val_prefix.trim()
            }
        } else {
            line_prefix.split_whitespace().last().unwrap_or("")
        };

        Self {
            line_prefix: line_prefix.to_string(),
            _line_suffix: line_suffix.to_string(),
            colon_pos,
            search_term: search_term.to_string(),
            is_inside_quote,
            has_close_quote,
        }
    }

    /// Returns true if the cursor is in a position where a property value is expected.
    fn is_value_position(&self) -> bool {
        self.colon_pos.is_some() || self.is_inside_quote
    }

    /// Returns the property key if the cursor is in a value position (e.g. "message" from "message: ...").
    fn get_property_key(&self) -> &str {
        if let Some(pos) = self.colon_pos {
            let key_part = self.line_prefix[..pos].trim();
            key_part.split_whitespace().last().unwrap_or("")
        } else {
            ""
        }
    }
}

/// Helper to build consistent CompletionItems.
struct CompletionBuilder;

impl CompletionBuilder {
    /// Creates a completion item for a keyword, handling snippet generation for blocks vs properties.
    fn keyword(config: KeywordConfig) -> CompletionItem {
        let mut item = CompletionItem {
            label: config.label.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: config.detail.map(|s| s.to_string()).or(Some("VSIDL Keyword".to_string())),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            deprecated: Some(config.deprecated),
            ..Default::default()
        };

        if config.is_block {
            if !config.initial_fields.is_empty() {
                let mut snippet = format!("{} {{\n", config.label);
                for (i, field) in config.initial_fields.iter().enumerate() {
                    let expected_type = LanguageConfig::get_property_type(field);
                    let index = if config.initial_fields.len() == 1 { 0 } else { i + 1 };

                    let placeholder = format!("${}", index);

                    if expected_type == PropertyType::String {
                        snippet.push_str(&format!("  {}: \"{}\"\n", field, placeholder));
                    } else {
                        snippet.push_str(&format!("  {}: {}\n", field, placeholder));
                    }
                }
                snippet.push('}');
                item.insert_text = Some(snippet);
            } else {
                item.insert_text = Some(format!("{} {{\n  $0\n}}", config.label));
            }
        } else {
            let expected_type = LanguageConfig::get_property_type(config.label);
            item.insert_text = Some(if expected_type == PropertyType::String {
                format!("{}: \"$0\"", config.label)
            } else {
                format!("{}: $0", config.label)
            });
        }
        item
    }

    /// Creates a completion item for a symbol (message, enum, service).
    fn symbol(
        name: String,
        fqn: String,
        span: Option<Span>,
        kind: CompletionItemKind,
        cursor_ctx: &CursorContext,
    ) -> CompletionItem {
        let (label, insert_text) = match (cursor_ctx.is_inside_quote, cursor_ctx.has_close_quote) {
            (false, false) => (format!("\"{}\"", name), format!("\"{}\"", name)),
            (true, false) => (format!("{}\"", name), format!("{}\"", name)),
            (true, true) => (name.clone(), name),
            (false, true) => (format!("\"{}", name), format!("\"{}", name)),
        };

        CompletionItem {
            label,
            kind: Some(kind),
            detail: span
                .as_ref()
                .and_then(|s| Path::new(&s.file_path).file_name())
                .and_then(|os_str| os_str.to_str())
                .map(String::from),
            insert_text: Some(insert_text),
            filter_text: Some(fqn),
            ..Default::default()
        }
    }
}

// =================================================================================================
// Completion Logic
// =================================================================================================

/// Manages the state and logic for a single completion request.
struct CompletionSession<'a> {
    context: &'a mut CatalogContext,
    provider: &'a LspFileProvider,
    file_path: &'a Path,
    position: Position,
}

impl<'a> CompletionSession<'a> {
    /// Creates a new completion session.
    fn new(
        context: &'a mut CatalogContext,
        provider: &'a LspFileProvider,
        file_path: &'a Path,
        position: Position,
    ) -> Self {
        Self { context, provider, file_path, position }
    }

    /// Executes the completion logic and returns the list of suggestions.
    fn run(&mut self) -> Result<Option<CompletionResponse>> {
        let content = self.provider.read_to_string(self.file_path)?;
        let position = convert_position_0_based_to_1_based_index(self.position);
        let offset = match line_col_to_offset(
            self.provider,
            self.file_path.to_str().unwrap_or_default(),
            position.line as usize,
            position.character as usize,
        ) {
            Ok(offset) => offset,
            Err(_) => {
                // If the column is out of bounds, silently fail instead of opening
                // a popup to the IDE user.
                eprintln!("offset out of bounds");
                return Ok(None);
            }
        };

        let cursor_ctx = CursorContext::new(&content, offset);

        let mut items = Vec::new();

        let mut working_content = content.clone();
        let (result, _) = parse_vsidl_with_recovery(&mut working_content);

        let (hierarchy_pairs, package) = if let Ok(mut pairs) = result {
            let pair = pairs.next().expect("Recovery parser should return at least one pair");
            let entry = vsidl_v1_parser::converter::convert_to_vsidl_entry(
                pair.clone(),
                self.file_path.to_str().unwrap_or_default(),
            );

            let mut hierarchy = Vec::new();
            self.find_rule_hierarchy_at_offset(pair, offset, &mut hierarchy);
            (hierarchy, entry.package)
        } else {
            (vec![], self.extract_package_name_fallback(&content))
        };

        let hierarchy: Vec<Rule> = hierarchy_pairs.iter().map(|p| p.as_rule()).collect();
        let parent_rule = *hierarchy.last().unwrap_or(&Rule::vsidl_file);

        // 1. Keyword Completion
        if !cursor_ctx.is_value_position() {
            self.add_keywords(parent_rule, &mut items);
        }

        // 2. Symbol Completion
        if cursor_ctx.is_value_position() {
            let enclosing_type_context =
                self.extract_enclosing_type_context(&content, &hierarchy_pairs, offset);
            self.add_symbols(
                &hierarchy_pairs,
                offset,
                &enclosing_type_context,
                &cursor_ctx,
                &package,
                &mut items,
            );
        }

        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }

    /// Simplified package name recovery.
    fn extract_package_name_fallback(&self, content: &str) -> String {
        content
            .lines()
            .find(|l| l.contains("package"))
            .and_then(|l| l.split('"').nth(1))
            .unwrap_or("")
            .to_string()
    }

    /// Extracts the message or service name from the nearest enclosing block.
    fn extract_enclosing_type_context(
        &self,
        content: &str,
        hierarchy: &[Pair<Rule>],
        offset: usize,
    ) -> Option<String> {
        // 1. Try AST
        for pair in hierarchy.iter().rev() {
            let rule = pair.as_rule();
            if matches!(
                rule,
                Rule::publisher
                    | Rule::subscriber
                    | Rule::server
                    | Rule::client
                    | Rule::some_ip_offered_interface
                    | Rule::some_ip_simple_event
                    | Rule::some_ip_event_part
                    | Rule::some_ip_sdv_type_mapping
            ) {
                // Standard blocks
                if let Some(type_pair) = pair.clone().into_inner().flatten().find(|p| {
                    matches!(
                        p.as_rule(),
                        Rule::publisher_message
                            | Rule::subscriber_message
                            | Rule::server_interface
                            | Rule::client_interface
                    )
                }) {
                    return Some(
                        type_pair.into_inner().as_str().trim().trim_matches('"').to_string(),
                    );
                }

                // SOME/IP blocks: concatenate package_name and type_name from some_ip_unit_type_ref
                if let Some(unit_ref) = pair
                    .clone()
                    .into_inner()
                    .flatten()
                    .find(|p| p.as_rule() == Rule::some_ip_unit_type_ref)
                {
                    let mut pkg = String::new();
                    let mut type_name = String::new();
                    for inner in unit_ref.into_inner() {
                        match inner.as_rule() {
                            Rule::some_ip_unit_type_ref_package_name => {
                                pkg = inner
                                    .into_inner()
                                    .next()
                                    .unwrap()
                                    .as_str()
                                    .trim_matches('"')
                                    .to_string();
                            }
                            Rule::some_ip_unit_type_ref_type_name => {
                                type_name = inner
                                    .into_inner()
                                    .next()
                                    .unwrap()
                                    .as_str()
                                    .trim_matches('"')
                                    .to_string();
                            }
                            _ => {}
                        }
                    }
                    if !pkg.is_empty() && !type_name.is_empty() {
                        return Some(format!("{}.{}", pkg, type_name));
                    } else if !type_name.is_empty() {
                        return Some(type_name);
                    }
                }
            }
        }

        // 2. Simple string scan backward
        let prefix = &content[..offset];
        let last_type = prefix
            .rfind("message:")
            .or_else(|| prefix.rfind("type:"))
            .or_else(|| prefix.rfind("service:"))
            .or_else(|| prefix.rfind("interface:"));
        if let Some(pos) = last_type {
            return prefix[pos..].split('"').nth(1).map(|s| s.to_string());
        }

        None
    }

    /// Adds valid keywords for the current grammatical context to the completion items.
    fn add_keywords(&self, parent: Rule, items: &mut Vec<CompletionItem>) {
        let table =
            self.context.last_valid_symbol_table.as_ref().unwrap_or(&self.context.symbol_table);
        let valid_keywords = LanguageConfig::get_keywords_for_rule(parent, table.uses_pdk29());
        for config in valid_keywords {
            items.push(CompletionBuilder::keyword(config));
        }
    }

    /// Adds valid symbols (messages, enums, services) based on the property key being typed.
    #[allow(clippy::too_many_arguments)]
    fn add_symbols(
        &self,
        hierarchy_pairs: &[Pair<Rule>],
        offset: usize,
        type_context: &Option<String>,
        cursor_ctx: &CursorContext,
        current_package: &str,
        items: &mut Vec<CompletionItem>,
    ) {
        let key = cursor_ctx.get_property_key();
        let hierarchy: Vec<Rule> = hierarchy_pairs.iter().map(|p| p.as_rule()).collect();

        let parent_rule = match hierarchy
            .iter()
            .rev()
            .find(|r| LanguageConfig::is_valid_parent_for_key(key, **r))
        {
            Some(r) => *r,
            None => return,
        };

        let completion_type = LanguageConfig::get_symbol_completion_type(key, parent_rule);
        if completion_type == SymbolCompletionType::None {
            return;
        }

        self.suggest_symbols_for_type(
            completion_type,
            offset,
            hierarchy_pairs,
            current_package,
            type_context,
            cursor_ctx,
            items,
        );
    }

    /// Finds the innermost scope for name resolution at a given offset.
    ///
    /// It first tries a direct lookup in the symbol table. If the offset is not indexed
    /// (e.g., brand new content at the end of the file), it uses the fresh AST hierarchy
    /// to anchor the lookup using the starting positions of parent blocks.
    fn get_enclosing_scope(
        &self,
        table: &SymbolTable,
        offset: usize,
        hierarchy: &[Pair<Rule>],
    ) -> Option<Scope> {
        // 1. Try direct spatial lookup (most accurate if table is fresh)
        if let Some(sym) = table.get_symbol_at_location(self.file_path, offset) {
            return Some(sym.enclosing_scope(table));
        }

        // 2. Fallback: Use the fresh AST to find an indexed anchor
        for pair in hierarchy.iter().rev() {
            let anchor = pair.as_span().start();
            if let Some(sym) = table.get_symbol_at_location(self.file_path, anchor) {
                // Return the enclosing scope of the found parent symbol.
                return Some(sym.enclosing_scope(table));
            }
        }

        // 3. Fallback: Use the global scope as last resort
        Some(table.get_global_scope().clone())
    }

    /// Retrieves and filters symbols from the symbol table based on the expected type.
    #[allow(clippy::too_many_arguments)]
    fn suggest_symbols_for_type(
        &self,
        completion_type: SymbolCompletionType,
        offset: usize,
        hierarchy: &[Pair<Rule>],
        current_package: &str,
        type_context: &Option<String>,
        cursor_ctx: &CursorContext,
        items: &mut Vec<CompletionItem>,
    ) {
        let table =
            self.context.last_valid_symbol_table.as_ref().unwrap_or(&self.context.symbol_table);
        let mut candidates = Vec::new();

        match completion_type {
            SymbolCompletionType::ProtoMessageOrEnum => {
                for s in table.get_all_proto_message_symbols() {
                    candidates.push((
                        s.full_name(table),
                        s.package(table),
                        s.name(),
                        s.symbol.span.clone(),
                        CompletionItemKind::STRUCT,
                    ));
                }
                for s in table.get_all_proto_enum_symbols() {
                    candidates.push((
                        s.full_name(table),
                        s.package(table),
                        s.name(),
                        s.symbol.span.clone(),
                        CompletionItemKind::ENUM,
                    ));
                }
            }
            SymbolCompletionType::ProtoRpcService => {
                for s in table.get_all_proto_rpc_service_symbols() {
                    candidates.push((
                        s.full_name(table),
                        s.package(table),
                        s.name(),
                        s.symbol.span.clone(),
                        CompletionItemKind::INTERFACE,
                    ));
                }
            }
            SymbolCompletionType::PubSubChannel => {
                if !table.uses_pdk29() {
                    // Channels are not available after PDK29
                    eprintln!("Using Latest, ignoring request to complete PubSub channels");
                    return;
                }
                if let Some(name) = type_context {
                    // Get symbol at location to identify enclosing scope
                    if let Some(enclosing_scope) =
                        self.get_enclosing_scope(table, offset, hierarchy)
                    {
                        // 2. Resolve message name from that scope
                        if let Some(message) =
                            enclosing_scope.resolve_proto_message(table, name.clone()).first()
                        {
                            let maybe_publication = message
                                .publishers(table)
                                .first()
                                .map(|p| p.publication(table))
                                .or_else(|| {
                                    message.subscribers(table).first().map(|s| s.publication(table))
                                });

                            if let Some(publication) = maybe_publication {
                                let span = publication.symbol.span.clone();
                                for c_name in publication.channels(table) {
                                    candidates.push((
                                        c_name.clone(),
                                        "".to_string(),
                                        c_name,
                                        span.clone(),
                                        CompletionItemKind::ENUM_MEMBER,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            SymbolCompletionType::Topic => {
                if table.uses_pdk29() {
                    // Topics are not available in PDK29
                    eprintln!("Using PDK29, ignoring request to complete topics");
                    return;
                }
                if let Some(name) = type_context {
                    // Get symbol at location to identify enclosing scope
                    if let Some(enclosing_scope) =
                        self.get_enclosing_scope(table, offset, hierarchy)
                    {
                        // 2. Resolve message name from that scope
                        if let Some(message) =
                            enclosing_scope.resolve_proto_message(table, name.clone()).first()
                        {
                            for topic in message.topics(table) {
                                let c_name = topic.symbol.name.clone();
                                candidates.push((
                                    c_name.clone(),
                                    "".to_string(),
                                    c_name,
                                    topic.symbol.span.clone(),
                                    CompletionItemKind::ENUM_MEMBER,
                                ));
                            }
                        }
                    }
                }
            }
            SymbolCompletionType::RpcChannel => {
                if table.uses_pdk29() {
                    // RPC channels are not available in PDK29
                    eprintln!("Using PDK29, ignoring request to complete RPC channels");
                    return;
                }
                let mut seen_channels = HashSet::new();
                if let Some(name) = type_context {
                    // Get symbol at location to identify enclosing scope
                    if let Some(enclosing_scope) =
                        self.get_enclosing_scope(table, offset, hierarchy)
                    {
                        // 2. Resolve service name from that scope
                        if let Some(service) =
                            enclosing_scope.resolve_proto_rpc_service(table, name.clone()).first()
                        {
                            for server in service.servers(table) {
                                let channel = server.channel(table);
                                let c_name = channel.symbol.name.clone();
                                if seen_channels.insert(c_name.clone()) {
                                    candidates.push((
                                        c_name.clone(),
                                        "".to_string(),
                                        c_name,
                                        channel.symbol.span.clone(),
                                        CompletionItemKind::ENUM_MEMBER,
                                    ));
                                }
                            }
                            for client in service.clients(table) {
                                let channel = client.channel(table);
                                let c_name = channel.symbol.name.clone();
                                if seen_channels.insert(c_name.clone()) {
                                    candidates.push((
                                        c_name.clone(),
                                        "".to_string(),
                                        c_name,
                                        channel.symbol.span.clone(),
                                        CompletionItemKind::ENUM_MEMBER,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            SymbolCompletionType::None => return,
        }

        for (fqn, pkg, name, span, kind) in candidates {
            let display_name = if pkg == current_package { name } else { fqn.clone() };
            if cursor_ctx.search_term.is_empty()
                || fqn.contains(&cursor_ctx.search_term)
                || display_name.contains(&cursor_ctx.search_term)
            {
                items.push(CompletionBuilder::symbol(display_name, fqn, span, kind, cursor_ctx));
            }
        }
    }

    /// Recursively finds the hierarchy of Pest rules at a given byte offset.
    fn find_rule_hierarchy_at_offset<'b>(
        &self,
        pair: Pair<'b, Rule>,
        offset: usize,
        hierarchy: &mut Vec<Pair<'b, Rule>>,
    ) {
        hierarchy.push(pair.clone());
        for inner in pair.into_inner() {
            let span = inner.as_span();
            if offset >= span.start() && offset <= span.end() {
                if inner.as_rule() != Rule::WHITESPACE {
                    self.find_rule_hierarchy_at_offset(inner, offset, hierarchy);
                }
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::lsp_file_provider::LspFileProvider;
    use crate::lsp::test_utils::{setup_mock_cli, setup_test_catalog};
    use lsp_types::{
        CompletionParams, CompletionResponse, PartialResultParams, Position,
        TextDocumentIdentifier, TextDocumentPositionParams, Url, WorkDoneProgressParams,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use vsidl_v1_parser::file_provider::DiskFileProvider;
    use vsidl_v1_parser::Catalog;
    use vsidlc_cli::VsidlcCommonCli;

    fn setup_test_state() -> (LspFileProvider, CatalogContext, TempDir) {
        let temp_dir = setup_test_catalog();
        let cli = setup_mock_cli(&temp_dir);
        let common_cli = VsidlcCommonCli::from(cli.clone());

        let document_store = Arc::new(Mutex::new(HashMap::new()));
        let file_provider = LspFileProvider {
            document_store: Arc::clone(&document_store),
            file_hashes: Arc::new(Mutex::new(HashMap::new())),
            disk_fallback: DiskFileProvider,
        };

        let symbol_table = common_cli
            .create_symbol_table(Some(".".into()))
            .expect("Failed to create symbol table");

        let context = CatalogContext {
            cli: cli.clone(),
            symbol_table: symbol_table.clone(),
            catalog: Catalog::new(),
            last_valid_symbol_table: Some(symbol_table),
            recovered_syntax_errors: HashMap::new(),
            active_findings: HashMap::new(),
            shadow_workspace: None,
        };

        (file_provider, context, temp_dir)
    }

    use tempfile::TempDir;

    #[test]
    fn test_completion_keywords_top_level() -> anyhow::Result<()> {
        let (file_provider, mut context, temp_dir) = setup_test_state();
        let vsidl_file_path = temp_dir.path().join("test_minimal.vsidl");

        // GIVEN an almost empty file
        let content = "\n";
        std::fs::write(&vsidl_file_path, content)?;

        // WHEN requesting completion at the beginning of the file
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(&vsidl_file_path).unwrap(),
                },
                position: Position { line: 0, character: 0 },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        // THEN top-level keywords like 'service_bundle' and 'package' are suggested
        let result = on_completion(&file_provider, &mut context, params)?;
        assert!(result.is_some(), "Result should be Some for top-level completion");

        if let CompletionResponse::Array(items) = result.unwrap() {
            let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
            assert!(labels.contains(&"service_bundle".to_string()));
            assert!(labels.contains(&"package".to_string()));
        } else {
            panic!("Expected array response");
        }
        Ok(())
    }

    #[test]
    fn test_completion_block_snippet() -> anyhow::Result<()> {
        let (file_provider, mut context, temp_dir) = setup_test_state();
        let vsidl_file_path = temp_dir.path().join("test.vsidl");

        // GIVEN a standard VSIDL file from the test catalog
        // WHEN requesting completion inside a 'service_bundle' block
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(vsidl_file_path).unwrap(),
                },
                position: Position { line: 3, character: 0 },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        // THEN block keywords like 'publisher' are suggested with their corresponding block snippets including mandatory fields
        let result = on_completion(&file_provider, &mut context, params)?;
        assert!(result.is_some(), "Result should be Some for block completion");
        let items = match result.unwrap() {
            CompletionResponse::Array(i) => i,
            _ => panic!("Expected array"),
        };

        let publisher =
            items.iter().find(|i| i.label == "publisher").expect("Should find 'publisher' keyword");
        assert_eq!(
            publisher.insert_text,
            Some("publisher {\n  message: \"$1\"\n  channel: \"$2\"\n}".to_string())
        );
        assert_eq!(publisher.insert_text_format, Some(InsertTextFormat::SNIPPET));
        assert_eq!(
            publisher.detail,
            Some("PubSub data source providing Protobuf messages to a topic".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_completion_service_bundle_snippet() -> anyhow::Result<()> {
        let (file_provider, mut context, temp_dir) = setup_test_state();
        let vsidl_file_path = temp_dir.path().join("test_sb.vsidl");

        // GIVEN an empty VSIDL file
        let content = "\n";
        std::fs::write(&vsidl_file_path, content)?;

        // WHEN requesting completion at the top level
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(&vsidl_file_path).unwrap(),
                },
                position: Position { line: 0, character: 0 },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        // THEN 'service_bundle' is suggested with the 'name' field autocompleted
        let result = on_completion(&file_provider, &mut context, params)?;
        let items = match result.unwrap() {
            CompletionResponse::Array(i) => i,
            _ => panic!("Expected array"),
        };

        let sb = items
            .iter()
            .find(|i| i.label == "service_bundle")
            .expect("Should find 'service_bundle' keyword");
        assert_eq!(sb.insert_text, Some("service_bundle {\n  name: \"$0\"\n}".to_string()));
        assert_eq!(
            sb.detail,
            Some("Groups related service units intended to be deployed together".to_string())
        );

        let sdv_sb = items
            .iter()
            .find(|i| i.label == "sdv_service_bundle")
            .expect("Should find 'sdv_service_bundle' keyword");
        assert_eq!(sdv_sb.deprecated, Some(true));
        Ok(())
    }

    #[test]
    fn test_completion_property_snippet() -> anyhow::Result<()> {
        let (file_provider, mut context, temp_dir) = setup_test_state();
        let vsidl_file_path = temp_dir.path().join("test_prop.vsidl");

        // GIVEN a VSIDL file with an existing publisher
        let content = "service_bundle {\n  publisher {\n    message: \"Tire\"\n  }\n}";
        std::fs::write(&vsidl_file_path, content)?;

        // WHEN requesting completion inside a 'publisher' block
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(&vsidl_file_path).unwrap(),
                },
                position: Position { line: 2, character: 2 },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        // THEN property keywords like 'channel' are suggested with their corresponding property snippets
        let result = on_completion(&file_provider, &mut context, params)?;
        assert!(result.is_some(), "Result should be Some for property completion");
        let items = match result.unwrap() {
            CompletionResponse::Array(i) => i,
            _ => panic!("Expected array"),
        };

        let channel =
            items.iter().find(|i| i.label == "channel").expect("Should find 'channel' keyword");
        assert_eq!(channel.insert_text, Some("channel: \"$0\"".to_string()));
        Ok(())
    }

    #[test]
    fn test_completion_suppress_keywords_in_value_position() -> anyhow::Result<()> {
        let (file_provider, mut context, temp_dir) = setup_test_state();
        let vsidl_file_path = temp_dir.path().join("test_suppress.vsidl");

        // GIVEN a file ending with a property key and a colon
        let content = "package: ";
        std::fs::write(&vsidl_file_path, content)?;

        // WHEN requesting completion at the value position following the colon
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(&vsidl_file_path).unwrap(),
                },
                position: Position { line: 0, character: 9 },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        // THEN structural keywords are suppressed to favor value-specific suggestions
        let result = on_completion(&file_provider, &mut context, params)?;

        if let Some(CompletionResponse::Array(items)) = result {
            let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
            // Should NOT contain keywords like 'service_bundle' or 'package'
            assert!(!labels.contains(&"service_bundle".to_string()));
            assert!(!labels.contains(&"package".to_string()));
        }
        Ok(())
    }

    #[test]
    fn test_completion_symbols_with_quotes() -> anyhow::Result<()> {
        let (file_provider, mut context, temp_dir) = setup_test_state();
        let vsidl_file_path = temp_dir.path().join("test.vsidl");

        // GIVEN a VSIDL file with a property expecting a symbol and no existing quotes
        let content =
            "package: \"com.sdv.tm\"\nservice_bundle {\n  publisher {\n    message: \n  }\n}";
        std::fs::write(&vsidl_file_path, content)?;

        // WHEN requesting completion for the symbol value
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(vsidl_file_path).unwrap(),
                },
                position: Position { line: 3, character: 12 },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        // THEN symbols are suggested with wrapping quotes for a complete insertion
        let result = on_completion(&file_provider, &mut context, params)?;
        let items = match result.unwrap() {
            CompletionResponse::Array(i) => i,
            _ => panic!("Expected array"),
        };

        let tire_pressure = items.iter().find(|i| i.label.contains("TirePressure")).unwrap();
        // Since it's the same package 'com.sdv.tm', it should suggest simple name with quotes
        assert_eq!(tire_pressure.label, "\"TirePressure\"");
        assert_eq!(tire_pressure.insert_text, Some("\"TirePressure\"".to_string()));
        Ok(())
    }

    #[test]
    fn test_completion_symbols_inside_quotes() -> anyhow::Result<()> {
        let (file_provider, mut context, temp_dir) = setup_test_state();
        let vsidl_file_path = temp_dir.path().join("test.vsidl");

        // GIVEN a VSIDL file with a property value already containing empty quotes
        let content =
            "package: \"com.sdv.tm\"\nservice_bundle {\n  publisher {\n    message: \"\"\n  }\n}";
        std::fs::write(&vsidl_file_path, content)?;

        // WHEN requesting completion inside the existing quotes
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(vsidl_file_path).unwrap(),
                },
                position: Position { line: 3, character: 14 },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        // THEN symbols are suggested without quotes to avoid doubling them
        let result = on_completion(&file_provider, &mut context, params)?;
        let items = match result.unwrap() {
            CompletionResponse::Array(i) => i,
            _ => panic!("Expected array"),
        };

        let tire_pressure = items.iter().find(|i| i.label.contains("TirePressure")).unwrap();
        // Inside quotes, suggest simple name without quotes
        assert_eq!(tire_pressure.label, "TirePressure");
        assert_eq!(tire_pressure.insert_text, Some("TirePressure".to_string()));
        Ok(())
    }

    #[test]
    fn test_completion_rpc_channel() -> anyhow::Result<()> {
        let temp_dir = setup_test_catalog();

        // Add a service bundle with an RPC server and channel to the catalog
        let vsidl_path = temp_dir.path().join("test_rpc_catalog.vsidl");
        let catalog_content = r#"package: "com.sdv.tm"
service_bundle {
  name: "RpcBundle"
  server {
    service: "TestService"
    channel: "existing-channel"
  }
}"#;
        std::fs::write(&vsidl_path, catalog_content)?;

        let mut cli = setup_mock_cli(&temp_dir);
        cli.target_api = "latest".to_string();
        let common_cli = VsidlcCommonCli::from(cli.clone());
        let symbol_table = common_cli
            .create_symbol_table(Some(".".into()))
            .expect("Failed to create symbol table");

        let document_store = Arc::new(Mutex::new(HashMap::new()));
        let file_provider = LspFileProvider {
            document_store: Arc::clone(&document_store),
            file_hashes: Arc::new(Mutex::new(HashMap::new())),
            disk_fallback: DiskFileProvider,
        };

        let mut context = CatalogContext {
            cli: cli.clone(),
            symbol_table: symbol_table.clone(),
            catalog: Catalog::new(),
            last_valid_symbol_table: Some(symbol_table),
            recovered_syntax_errors: HashMap::new(),
            active_findings: HashMap::new(),
            shadow_workspace: None,
        };

        // Now request completion in a new file
        let test_file_path = temp_dir.path().join("test.vsidl");
        let content =
            "package: \"com.sdv.tm\"\nservice_bundle {\n  server {\n    service: \"TestService\"\n    channel: \"\"\n  }\n}";
        std::fs::write(&test_file_path, content)?;

        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(&test_file_path).unwrap(),
                },
                position: Position { line: 4, character: 14 },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        let result = on_completion(&file_provider, &mut context, params)?;
        assert!(result.is_some(), "Result should be Some for RPC channel completion");
        let items = match result.clone().unwrap() {
            CompletionResponse::Array(i) => i,
            _ => panic!("Expected array"),
        };

        // Should find existing-channel
        assert!(items.iter().any(|i| i.label.contains("existing-channel")));
        Ok(())
    }

    #[test]
    fn test_completion_utf8_safe() -> anyhow::Result<()> {
        let (file_provider, mut context, temp_dir) = setup_test_state();
        let vsidl_file_path = temp_dir.path().join("test.vsidl");

        // GIVEN a VSIDL file containing multi-byte UTF-8 characters
        let content = "package: \"ø\"\nservice_bundle {  }";
        std::fs::write(&vsidl_file_path, content)?;

        // WHEN requesting completion at a position following multi-byte characters
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(vsidl_file_path).unwrap(),
                },
                position: Position { line: 1, character: 17 },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        // THEN the character offset calculation remains accurate and the request completes successfully
        let result = on_completion(&file_provider, &mut context, params);
        assert!(result.is_ok());
        Ok(())
    }
}
