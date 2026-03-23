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

#[derive(pest_derive::Parser)]
#[grammar_inline = r##"
//=================================================================
// Basics
//=================================================================

WHITESPACE = _{ " " | "\t" | "\r" | "\n" }
NEWLINE = { "\r" | "\n" }
COMMENT = { "#" ~ (!NEWLINE ~ ANY)* }
IDENTIFIER = ${ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }

//=================================================================
// VSIDL
//=================================================================

// Root of the grammar
vsidl_file = {
    SOI ~
    (COMMENT | WHITESPACE)* ~
    (package | sdv_service_bundle | some_ip_mapping | vhal_mapping | extension)* ~
    EOI
}

// Top-level VSIDL constructs
package = { "package" ~ ":" ~ qualified_name_str }
sdv_service_bundle = { ("sdv_service_bundle" | "service_bundle" ) ~ "{" ~ service_bundle_body ~ "}" }
some_ip_mapping = { "some_ip_mapping" ~ "{" ~ some_ip_mapping_body ~ "}" }
vhal_mapping = { "vhal_mapping" ~ "{" ~ vhal_mapping_body ~ "}" }
extension = { "[" ~ qualified_name ~ "]" ~ "{" ~ message_body ~ "}" }

// ServiceBundle Body
service_bundle_body = {
    (
        service_bundle_name_prop |
        publisher |
        subscriber |
        server |
        client |
        unit_type |
        build_configuration |
        (extension) |
        diagnostics_declaration |
        service_bundle_register_reflection_metadata
    )*
}

service_bundle_name_prop = { "name" ~ ":" ~ service_bundle_name_str }
service_bundle_register_reflection_metadata = { "register_reflection_metadata" ~ ":" ~ register_reflection_metadata }

// Publisher
publisher = { "publisher" ~ "{" ~ publisher_body ~ "}" }

publisher_body = {
    (
        publisher_message |
        publisher_topic |
        publisher_channel |
        publisher_capacity |
        publisher_service_unit_name
    )*
}

publisher_message = { ( "type" | "message" ) ~ ":" ~ message_str }
publisher_topic = { ( "topic" ) ~ ":" ~ topic_str }
publisher_channel = { ( "instance" | "channel" ) ~ ":" ~ channel_str }
publisher_capacity = { ( "capacity" ) ~ ":" ~ integer }
publisher_service_unit_name = { "service_unit_name" ~ ":" ~ service_unit_name_str }

// Subscriber
subscriber = { "subscriber" ~ "{" ~ subscriber_body ~ "}" }

subscriber_body = {
    (
        subscriber_message |
        subscriber_topic |
        subscriber_channel
    )*
}

subscriber_message = { ( "type" | "message" ) ~ ":" ~ message_str }
subscriber_topic = { ( "topic" ) ~ ":" ~ topic_str }
subscriber_channel = { ( "instance" | "channel" ) ~ ":" ~ channel_str }

// Server
server = { "server" ~ "{" ~ server_body ~ "}" }

server_body = {
    (
        server_interface |
        server_service_unit_name |
        server_channel
    )*
}

server_interface = { ("interface" | "service" ) ~ ":" ~ service_str }
server_service_unit_name = { "service_unit_name" ~ ":" ~ service_unit_name_str }
server_channel = { "channel" ~ ":" ~ channel_str }

// Client
client = { "client" ~ "{" ~ client_body ~ "}" }

client_body = {
    (
        client_interface |
        client_channel
    )*
}

client_interface = { ("interface" | "service" ) ~ ":" ~ service_str }
client_channel = { "channel" ~ ":" ~ channel_str }

// UnitType
unit_type = { "unit_type" ~ "{" ~ unit_type_body ~ "}" }

unit_type_body = {
    (unit_type_type)*
}

unit_type_type = { "type" ~ ":" ~ qualified_name_str }

// BuildConfiguration
build_configuration = { "build_cfg" ~ "{" ~ build_configuration_body ~ "}" }

build_configuration_body = {
    (
        build_configuration_target_name |
        build_configuration_skip_codegen
    )*
}

build_configuration_target_name = { "target_name" ~ ":" ~ target_name_str }
build_configuration_skip_codegen = { "skip_codegen" ~ ":" ~ skip_codegen }

// String wrappers for textproto-like syntax
service_bundle_name_str = ${ string }
service_unit_name_str = ${ string }
qualified_name_str = ${ "\"" ~ qualified_name ~ "\"" }
identifier_str = ${ "\"" ~ IDENTIFIER ~ "\"" }

// The following rules just tag existing rules to make them distinguishable
// The Pest Tag feature is not enabled in Android
message_str = ${ string }
topic_str = ${ string }
channel_str = ${ string }
service_str = ${ string }
target_name_str = ${ string }
skip_codegen = ${ boolean }
register_reflection_metadata = ${ boolean }

//=================================================================
// Textproto
//=================================================================

message_body = { (field | message_field | extension)* }
field = { IDENTIFIER ~ ":" ~ value }
value = { string | boolean | number | qualified_name_str | IDENTIFIER }
message_field = { IDENTIFIER ~ "{" ~ message_body ~ "}" }

//=================================================================
// Literals
//=================================================================

qualified_name = @{ "."? ~ (IDENTIFIER ~ ".")* ~ IDENTIFIER }
service_bundle_name = @{ XID_START ~ XID_CONTINUE* }
signed_identifier = { "-" ~ IDENTIFIER }
service_unit_name = @{
    // Starts with a lowercase letter
    'a'..'z' ~
    // Followed by zero or more valid characters (lowercase, digit, or hyphen)
    (
        // The core of the rule matches a valid character.
        // If a hyphen is matched, it must not be followed by another hyphen.
        (
            "-" ~ (!"-" ~ ('a'..'z' | '0'..'9'))
        )
        | ('a'..'z' | '0'..'9')
    )*
}

boolean = @{ "true" | "false" }

string = { single_string | double_string }
single_string = { "'" ~ ((!("'" | NEWLINE | "\\") ~ ANY))* ~ "'" }
double_string = { "\"" ~ ((!("\"" | NEWLINE | "\\") ~ ANY))* ~ "\"" }

// The basic building blocks of numbers
sign = @{ "-" }
digit = @{ '0'..'9' }
dec_unsigned_integer = @{ "0" | '1'..'9' ~ digit* }

// Hexadecimal and Octal Literals
hex_digit = @{ digit | 'a'..'f' | 'A'..'F' }
hex_unsigned_integer = @{ ("0x" | "0X") ~ hex_digit+ }
oct_digit = @{ '0'..'7' }
oct_unsigned_integer = @{ "0" ~ oct_digit+ }

// Signed Integer
integer = { sign? ~ (hex_unsigned_integer | oct_unsigned_integer | dec_unsigned_integer) }

// Floating-point literals
exponent = { ("e" | "E") ~ sign? ~ digit+ }
float = { sign? ~ (
    digit* ~ "." ~ digit+ ~ exponent?
    | dec_unsigned_integer ~ exponent
)}

// A generic number rule
number = { float | integer }

//=================================================================
// Diagnostics
//=================================================================

diagnostics_declaration = { "diagnostics_declaration" ~ "{" ~ diagnostics_declaration_body ~ "}" }
diagnostics_declaration_body = { (
    diagnostics_data_item |
    diagnostics_io_control |
    diagnostics_routine |
    diagnostics_event
)* }

diagnostics_data_item = { "data_item" ~ "{" ~ diagnostics_data_item_body ~ "}" }
diagnostics_data_item_body = {
    (
        diagnostics_data_item_id |
        diagnostics_data_item_message_name |
        diagnostics_data_item_is_writable
    )*
}

diagnostics_data_item_id = { "id" ~ ":" ~ string }
diagnostics_data_item_message_name = { "message_name" ~ ":" ~ string }
diagnostics_data_item_is_writable = { "is_writable" ~ ":" ~ boolean }

diagnostics_io_control = { "io_control" ~ "{" ~ diagnostics_io_control_body ~ "}" }
diagnostics_io_control_body = {
    (
        diagnostics_io_control_id |
        diagnostics_io_control_message_name
    )*
}

diagnostics_io_control_id = { "id" ~ ":" ~ string }
diagnostics_io_control_message_name = { "message_name" ~ ":" ~ string }

diagnostics_routine = { "routine" ~ "{" ~ diagnostics_routine_body ~ "}" }
diagnostics_routine_body = {
    (
        diagnostics_routine_id |
        diagnostics_routine_start |
        diagnostics_routine_stop |
        diagnostics_routine_result
    )*
}

diagnostics_routine_id = { "id" ~ ":" ~ string }
diagnostics_routine_start = { "start" ~ "{" ~ diagnostics_method_body ~ "}" }
diagnostics_routine_stop = { "stop" ~ "{" ~ diagnostics_method_body ~ "}" }
diagnostics_routine_result = { "result" ~ "{" ~ diagnostics_method_body ~ "}" }

diagnostics_method_body = {
    (
        diagnostics_method_request |
        diagnostics_method_response
    )*
}

diagnostics_method_request = { "request" ~ ":" ~ string }
diagnostics_method_response = { "response" ~ ":" ~ string }

diagnostics_event = { "event" ~ "{" ~ diagnostics_event_body ~ "}" }
diagnostics_event_body = {
    (
        diagnostics_event_id |
        diagnostics_event_extended_data_message_name |
        diagnostics_event_fault_listener_mask |
        diagnostics_event_service_unit_name
    )*
}

diagnostics_event_id = { "id" ~ ":" ~ string }
diagnostics_event_extended_data_message_name = { "extended_data_message_name" ~ ":" ~ string }
diagnostics_event_fault_listener_mask = { "fault_listener_mask" ~ ":" ~ number }
diagnostics_event_service_unit_name = { "service_unit_name" ~ ":" ~ string }

//=================================================================
// VHAL
//=================================================================

vhal_mapping_body = { (vhal_vehicle_property | vhal_data_source | vhal_area_mapping)* }

vhal_vehicle_property = { "property" ~ "{" ~ (
    vhal_vehicle_prop_config |
    vhal_vehicle_default_value |
    vhal_vehicle_default_area_config
)* ~ "}"}

vhal_vehicle_prop_config = { "prop_config" ~ "{" ~ vhal_vehicle_prop_config_body ~ "}" }
vhal_vehicle_default_value = { "default_value" ~ "{" ~ vhal_default_value_body ~ "}" }
vhal_vehicle_default_area_config = { "default_area_config" ~ "{" ~ vhal_vehicle_area_config_body ~ "}" }

vhal_data_source = { "source" ~ "{" ~ (
    vhal_data_source_unit_type |
    vhal_data_source_action |
    vhal_data_source_field_path |
    vhal_data_source_status_field_path
)* ~ "}" }

vhal_data_source_unit_type = { "unit_type" ~ ":" ~ string }
vhal_data_source_action = { "action" ~ ":" ~ IDENTIFIER }
vhal_data_source_field_path = { "field_path" ~ ":" ~ string }
vhal_data_source_status_field_path = { "status_field_path" ~ ":" ~ string }

vhal_area_mapping = { "area_mapping" ~ "{" ~ (
    vhal_area_mapping_area |
    vhal_area_mapping_publication_instance
)* ~ "}"}

vhal_area_mapping_area = { "area" ~ "{" ~ vhal_property_area_body ~ "}" }
vhal_area_mapping_publication_instance = { "publication_instance" ~ ":" ~ string }

vhal_property_area_body = { (
    vhal_property_area_config |
    vhal_property_area_default_value
)* }

vhal_property_area_config = { "area_config" ~ "{" ~ vhal_vehicle_area_config_body ~ "}" }
vhal_property_area_default_value = { "default_value" ~ "{" ~ vhal_default_value_body ~ "}" }

vhal_default_value_body = { (
    vhal_default_value_string_value |
    vhal_default_value_bytes_value |
    vhal_default_value_int32_values |
    vhal_default_value_int64_values |
    vhal_default_value_float_values
)* }

vhal_default_value_string_value = { "string_value" ~ ":" ~ string }
vhal_default_value_bytes_value = { "bytes_value" ~ ":" ~ string }
vhal_default_value_int32_values = { "int32_values" ~ ":" ~ value }
vhal_default_value_int64_values = { "int64_values" ~ ":" ~ value }
vhal_default_value_float_values = { "float_values" ~ ":" ~ value }

vhal_vehicle_prop_config_body = { (
    vhal_vehicle_prop_config_prop |
    vhal_vehicle_prop_config_access |
    vhal_vehicle_prop_config_change_mode |
    vhal_vehicle_prop_config_area_configs |
    vhal_vehicle_prop_config_config_array |
    vhal_vehicle_prop_config_config_string |
    vhal_vehicle_prop_config_min_sample_rate |
    vhal_vehicle_prop_config_max_sample_rate
)* }

vhal_vehicle_prop_config_prop = { "prop" ~ ":" ~ integer }
vhal_vehicle_prop_config_access = { "access" ~ ":" ~ IDENTIFIER }
vhal_vehicle_prop_config_change_mode = { "change_mode" ~ ":" ~ IDENTIFIER }
vhal_vehicle_prop_config_area_configs = { "area_configs" ~ "{" ~ vhal_vehicle_area_config_body ~ "}" }
vhal_vehicle_prop_config_config_array = { "config_array" ~ ":" ~ value }
vhal_vehicle_prop_config_config_string = { "config_string" ~ ":" ~ string }
vhal_vehicle_prop_config_min_sample_rate = { "min_sample_rate" ~ ":" ~ number }
vhal_vehicle_prop_config_max_sample_rate = { "max_sample_rate" ~ ":" ~ number }

vhal_vehicle_area_config_body = { (
    vhal_vehicle_area_config_area_id |
    vhal_vehicle_area_config_min_int32_value |
    vhal_vehicle_area_config_max_int32_value |
    vhal_vehicle_area_config_min_int64_value |
    vhal_vehicle_area_config_max_int64_value |
    vhal_vehicle_area_config_min_float_value |
    vhal_vehicle_area_config_max_float_value |
    vhal_vehicle_area_config_supported_enum_values |
    vhal_vehicle_area_config_access |
    vhal_vehicle_area_config_support_variable_update_rate
)* }

vhal_vehicle_area_config_area_id = { "area_id" ~ ":" ~ integer }
vhal_vehicle_area_config_min_int32_value = { "min_int32_value" ~ ":" ~ integer }
vhal_vehicle_area_config_max_int32_value = { "max_int32_value" ~ ":" ~ integer }
vhal_vehicle_area_config_min_int64_value = { "min_int64_value" ~ ":" ~ integer }
vhal_vehicle_area_config_max_int64_value = { "max_int64_value" ~ ":" ~ integer }
vhal_vehicle_area_config_min_float_value = { "min_float_value" ~ ":" ~ float }
vhal_vehicle_area_config_max_float_value = { "max_float_value" ~ ":" ~ float }
vhal_vehicle_area_config_supported_enum_values = { "supported_enum_values" ~ ":" ~ value }
vhal_vehicle_area_config_access = { "access" ~ ":" ~ IDENTIFIER }
vhal_vehicle_area_config_support_variable_update_rate = { "support_variable_update_rate" ~ ":" ~ boolean }

//=================================================================
// SOME/IP
//=================================================================

some_ip_mapping_body = { (some_ip_service_mapping | some_ip_sdv_type_mapping)* }

some_ip_service_mapping = { "someip_service_mapping" ~ "{" ~ (
    some_ip_service_id |
    some_ip_version_major |
    some_ip_version_minor |
    some_ip_instance |
    some_ip_instance_impl |
    some_ip_sd_behavior
)* ~ "}" }

some_ip_service_id = { "someip_service_id" ~ ":" ~ integer }
some_ip_version_major = { "version_major" ~ ":" ~ integer }
some_ip_version_minor = { "version_minor" ~ ":" ~ integer }
some_ip_sd_behavior = { "sd_behavior" ~ ":" ~ IDENTIFIER }

some_ip_instance = { "instance" ~ "{" ~ (
    some_ip_instance_id |
    some_ip_sdv_service_unit_parameterization_block |
    some_ip_sdv_topic_parameterization_block
)* ~ "}" }

some_ip_instance_id = { "instance_id" ~ ":" ~ integer }

some_ip_sdv_service_unit_parameterization_block = { "sdv_service_unit_parameterization" ~ "{" ~ (
    some_ip_sdv_service_unit_parameterization_block_key |
    some_ip_sdv_service_unit_parameterization_block_value
)* ~ "}" }

some_ip_sdv_service_unit_parameterization_block_key = { "key" ~ ":" ~ integer }
some_ip_sdv_service_unit_parameterization_block_value = { "value" ~ ":" ~ string }

some_ip_sdv_topic_parameterization_block = { "sdv_topic_parameterization" ~ "{" ~ (
    some_ip_sdv_topic_parameterization_block_key |
    some_ip_sdv_topic_parameterization_block_value
)* ~ "}" }

some_ip_sdv_topic_parameterization_block_key = { "key" ~ ":" ~ integer }
some_ip_sdv_topic_parameterization_block_value = { "value" ~ "{" ~ sdv_topic_parameters_ref ~ "}" }

sdv_topic_parameters_ref = { (
    some_ip_sdv_topic |
    some_ip_capacity
)* }

some_ip_sdv_topic = { "topic" ~ ":" ~ string }
some_ip_capacity = { "capacity" ~ ":" ~ integer }

some_ip_instance_impl = { "instance_impl" ~ "{" ~ (
    some_ip_offered_interface |
    some_ip_offered_event
)* ~ "}" }

some_ip_offered_interface = { "offered_interface" ~ "{" ~ (
    some_ip_offered_interface_sdv_unit_type |
    some_ip_service_unit |
    some_ip_method
)* ~ "}" }

some_ip_offered_interface_sdv_unit_type = { "sdv_unit_type" ~ "{" ~ some_ip_unit_type_ref ~ "}" }
some_ip_service_unit = { "service_unit" ~ ":" ~ integer }

some_ip_unit_type_ref = { (
    some_ip_unit_type_ref_package_name |
    some_ip_unit_type_ref_service_bundle_name |
    some_ip_unit_type_ref_type_name
)* }

some_ip_unit_type_ref_package_name = { "package_name" ~ ":" ~ string }
some_ip_unit_type_ref_service_bundle_name = { "service_bundle_name" ~ ":" ~ string }
some_ip_unit_type_ref_type_name = { "type_name" ~ ":" ~ string }

some_ip_method = { "method" ~ "{" ~ (
    some_ip_method_procedure |
    some_ip_method_method_id |
    request_payload_mapping |
    response_payload_mapping
)* ~ "}" }

some_ip_method_procedure = { "procedure" ~ ":" ~ string }
some_ip_method_method_id = { "method_id" ~ ":" ~ integer }

request_payload_mapping = { "request_payload_mapping" ~ "{" ~ some_ip_struct_body ~ "}" }
response_payload_mapping = { "response_payload_mapping" ~ "{" ~ some_ip_struct_body ~ "}" }

some_ip_offered_event = { "offered_event" ~ "{" ~ (
    some_ip_offered_event_group_id |
    some_ip_offered_event_event_id |
    some_ip_simple_event |
    some_ip_group_event
)* ~ "}" }

some_ip_offered_event_group_id = { "group_id" ~ ":" ~ integer }
some_ip_offered_event_event_id = { "event_id" ~ ":" ~ integer }

some_ip_simple_event = { "simple_event" ~ "{" ~ (
    some_ip_simple_event_sdv_unit_type |
    some_ip_simple_event_service_unit |
    some_ip_simple_event_type_definition |
    some_ip_simple_event_message_translation_mode |
    some_ip_simple_event_topic
)* ~ "}" }

some_ip_simple_event_sdv_unit_type = { "sdv_unit_type" ~ "{" ~ some_ip_unit_type_ref ~ "}" }
some_ip_simple_event_service_unit = { "service_unit" ~ ":" ~ integer }
some_ip_simple_event_type_definition = { "type_definition" ~ "{" ~ some_ip_struct_body ~ "}" }
some_ip_simple_event_message_translation_mode = { "message_translation_mode" ~ ":" ~ IDENTIFIER }
some_ip_simple_event_topic = { "topic" ~ ":" ~ integer }

some_ip_group_event = { "group_event" ~ "{" ~ (
    some_ip_group_event_type_definition |
    some_ip_event_part
)* ~ "}" }

some_ip_group_event_type_definition = { "type_definition" ~ "{" ~ some_ip_struct_body ~ "}" }

some_ip_event_part = { "event_part" ~ "{" ~ (
    some_ip_event_part_sdv_unit_type |
    some_ip_event_part_service_unit |
    some_ip_event_part_field_name_mapping |
    some_ip_event_part_topic
)* ~ "}" }

some_ip_event_part_sdv_unit_type = { "sdv_unit_type" ~ "{" ~ some_ip_unit_type_ref ~ "}" }
some_ip_event_part_service_unit = { "service_unit" ~ ":" ~ integer }
some_ip_event_part_field_name_mapping = { "field_name_mapping" ~ "{" ~ some_ip_map_entry_str_str* ~ "}" }
some_ip_event_part_topic = { "topic" ~ ":" ~ integer }

some_ip_map_entry_str_str = { some_ip_map_entry_key | some_ip_map_entry_value }

some_ip_map_entry_key = { "key" ~ ":" ~ string }
some_ip_map_entry_value = { "value" ~ ":" ~ string }

some_ip_sdv_type_mapping = { "sdv_type_mapping" ~ "{" ~ (
    some_ip_sdv_type_mapping_unit_type |
    some_ip_sdv_type_mapping_service_id |
    some_ip_sdv_type_mapping_version_major |
    some_ip_sdv_type_mapping_version_minor |
    some_ip_sdv_offered_interface |
    some_ip_offered_topic |
    some_ip_service_unit_mapping |
    some_ip_service_oriented_mapping
)* ~ "}" }

some_ip_sdv_type_mapping_unit_type = { "unit_type" ~ "{" ~ some_ip_unit_type_ref ~ "}" }
some_ip_sdv_type_mapping_service_id = { "someip_service_id" ~ ":" ~ integer }
some_ip_sdv_type_mapping_version_major = { "version_major" ~ ":" ~ integer }
some_ip_sdv_type_mapping_version_minor = { "version_minor" ~ ":" ~ integer }

some_ip_sdv_offered_interface = { "offered_interface" ~ "{" ~ (
    some_ip_sdv_method
)* ~ "}" }

some_ip_sdv_method = { "method" ~ "{" ~ (
    some_ip_sdv_method_procedure |
    some_ip_sdv_method_method_id |
    request_payload_mapping |
    response_payload_mapping
)* ~ "}" }

some_ip_sdv_method_procedure = { "procedure" ~ ":" ~ string }
some_ip_sdv_method_method_id = { "method_id" ~ ":" ~ integer }

some_ip_service_oriented_mapping = { "service_oriented_mapping" ~ "{" ~ (
    some_ip_rpc_interface_mapping |
    some_ip_publication_mapping
)* ~ "}" }

some_ip_rpc_interface_mapping = { "rpc_interface_mapping" ~ "{" ~ (
    some_ip_offered_interface |
    some_ip_rpc_channel_mapping
)* ~ "}" }

some_ip_rpc_channel_mapping = { "rpc_channel_mapping" ~ "{" ~ (
    some_ip_rpc_channel_mapping_channel |
    some_ip_rpc_channel_mapping_instance_id
)* ~ "}" }

some_ip_rpc_channel_mapping_channel = { "channel" ~ ":" ~ channel_str }
some_ip_rpc_channel_mapping_instance_id = { "instance_id" ~ ":" ~ integer }

some_ip_publication_mapping = { "publication_mapping" ~ "{" ~ (
    some_ip_offered_topic |
    some_ip_topic_mapping
)* ~ "}" }

some_ip_topic_mapping = { "topic_mapping" ~ "{" ~ (
    some_ip_topic_mapping_topic |
    some_ip_topic_mapping_instance_id |
    some_ip_fire_and_forget_method
)* ~ "}" }

some_ip_topic_mapping_topic = { "topic" ~ ":" ~ topic_str }
some_ip_topic_mapping_instance_id = { "instance_id" ~ ":" ~ integer }

some_ip_offered_topic = { "offered_topic" ~ "{" ~ (
    some_ip_offered_topic_type_definition |
    some_ip_simple_mapping |
    some_ip_offered_topic_someip_ff_methods |
    some_ip_multipub_topic_info
)* ~ "}" }

some_ip_offered_topic_type_definition = { "type_definition" ~ "{" ~ some_ip_struct_body ~ "}" }
some_ip_offered_topic_someip_ff_methods = { "someip_ff_methods" ~ ":" ~ boolean }

some_ip_simple_mapping = { "simple_mapping" ~ "{" ~ (
    some_ip_simple_mapping_group_id |
    some_ip_simple_mapping_event_id
)* ~ "}" }

some_ip_simple_mapping_group_id = { "group_id" ~ ":" ~ integer }
some_ip_simple_mapping_event_id = { "event_id" ~ ":" ~ integer }

some_ip_multipub_topic_info = { "multipub_topic_info" ~ "{" ~ (
    some_ip_multipub_topic_info_someip_instance_id
)* ~ "}" }

some_ip_multipub_topic_info_someip_instance_id = { "someip_instance_id" ~ ":" ~ integer }

some_ip_service_unit_mapping = { "service_unit_mapping" ~ "{" ~ (
    some_ip_service_unit_mapping_service_unit_id |
    some_ip_service_unit_mapping_instance_id |
    some_ip_fire_and_forget_method
)* ~ "}" }

some_ip_service_unit_mapping_service_unit_id = { "service_unit_id" ~ "{" ~ some_ip_service_unit_id_block ~ "}" }
some_ip_service_unit_mapping_instance_id = { "instance_id" ~ ":" ~ integer }

some_ip_service_unit_id_block = { (
    some_ip_service_unit_id_vm_name |
    some_ip_service_unit_id_package_name |
    some_ip_service_unit_id_service_bundle_name |
    some_ip_service_unit_id_service_instance_name |
    some_ip_service_unit_id_service_unit_name
)* }

some_ip_service_unit_id_vm_name = { "vm_name" ~ ":" ~ string }
some_ip_service_unit_id_package_name = { "package_name" ~ ":" ~ string }
some_ip_service_unit_id_service_bundle_name = { "service_bundle_name" ~ ":" ~ string }
some_ip_service_unit_id_service_instance_name = { "service_instance_name" ~ ":" ~ string }
some_ip_service_unit_id_service_unit_name = { "service_unit_name" ~ ":" ~ string }

some_ip_fire_and_forget_method = { "fire_and_forget_method" ~ "{" ~ (
    some_ip_fire_and_forget_method_service_id |
    some_ip_fire_and_forget_method_instance_id |
    some_ip_fire_and_forget_method_version_major |
    some_ip_fire_and_forget_method_version_minor |
    some_ip_fire_and_forget_method_method_id
)* ~ "}" }

some_ip_fire_and_forget_method_service_id = { "service_id" ~ ":" ~ integer }
some_ip_fire_and_forget_method_instance_id = { "instance_id" ~ ":" ~ integer }
some_ip_fire_and_forget_method_version_major = { "version_major" ~ ":" ~ integer }
some_ip_fire_and_forget_method_version_minor = { "version_minor" ~ ":" ~ integer }
some_ip_fire_and_forget_method_method_id = { "method_id" ~ ":" ~ integer }

some_ip_type = { some_ip_type_someip_primitive |
              ( "someip_string" ~ "{" ~ some_ip_string_body ~ "}" ) |
              ( "someip_array" ~ "{" ~ some_ip_array_body ~ "}" ) |
              ( "someip_struct" ~ "{" ~ some_ip_struct_body ~ "}" ) }

some_ip_type_someip_primitive = { "someip_primitive" ~ ":" ~ IDENTIFIER }

some_ip_string_body = { (
    some_ip_string_body_fixed_length |
    some_ip_string_body_length_field_size |
    some_ip_string_body_string_encoding
)* }

some_ip_string_body_fixed_length = { "fixed_length" ~ ":" ~ integer }
some_ip_string_body_length_field_size = { "length_field_size" ~ ":" ~ IDENTIFIER }
some_ip_string_body_string_encoding = { "string_encoding" ~ ":" ~ IDENTIFIER }

some_ip_array_body = { (
    some_ip_array_body_fixed_length |
    some_ip_array_body_length_field_size |
    some_ip_array_type
)* }

some_ip_array_body_fixed_length = { "fixed_length" ~ ":" ~ integer }
some_ip_array_body_length_field_size = { "length_field_size" ~ ":" ~ IDENTIFIER }
some_ip_array_type = { "array_type" ~ "{" ~ some_ip_type ~ "}" }

some_ip_struct_body = { (
    some_ip_struct_body_length_field_size |
    some_ip_struct_field
)* }

some_ip_struct_body_length_field_size = { "length_field_size" ~ ":" ~ IDENTIFIER }

some_ip_struct_field = { "field" ~ "{" ~ (
    some_ip_struct_field_data_tag |
    some_ip_struct_field_field_name |
    some_ip_struct_field_type
)* ~ "}" }

some_ip_struct_field_data_tag = { "data_tag" ~ ":" ~ integer }
some_ip_struct_field_field_name = { "field_name" ~ ":" ~ string }
some_ip_struct_field_type = { "field_type" ~ "{" ~ some_ip_type ~ "}" }
"##]
pub struct VsidlParser;

use pest::error::Error as PestError;
use pest::error::ErrorVariant;
use std::collections::BTreeSet;

impl Rule {
    /// Returns a human-friendly name for the rule, used in error messages.
    pub fn to_friendly_string(&self) -> &str {
        match self {
            Rule::EOI => "end of file",
            Rule::NEWLINE => "newline",
            Rule::COMMENT => "comment",
            Rule::IDENTIFIER => "identifier",

            Rule::vsidl_file => "VSIDL content",
            Rule::package => "'package:'",
            Rule::sdv_service_bundle => "'service_bundle {'",
            Rule::some_ip_mapping => "'some_ip_mapping {'",
            Rule::vhal_mapping => "'vhal_mapping {'",
            Rule::extension => "'[...] {'",

            Rule::service_bundle_name_prop => "'name:'",
            Rule::publisher => "'publisher {'",
            Rule::subscriber => "'subscriber {'",
            Rule::server => "'server {'",
            Rule::client => "'client {'",
            Rule::unit_type => "'unit_type {'",
            Rule::build_configuration => "'build_cfg {'",

            Rule::some_ip_service_oriented_mapping => "'service_oriented_mapping {'",
            Rule::some_ip_rpc_interface_mapping => "'rpc_interface_mapping {'",
            Rule::some_ip_rpc_channel_mapping => "'rpc_channel_mapping {'",
            Rule::some_ip_publication_mapping => "'publication_mapping {'",
            Rule::some_ip_topic_mapping => "'topic_mapping {'",

            Rule::publisher_message | Rule::subscriber_message => "'message:' or 'type:'",
            Rule::publisher_channel
            | Rule::subscriber_channel
            | Rule::server_channel
            | Rule::client_channel => "'channel:' or 'instance:'",
            Rule::publisher_capacity => "'capacity:'",
            Rule::publisher_service_unit_name | Rule::server_service_unit_name => {
                "'service_unit_name:'"
            }

            Rule::some_ip_rpc_channel_mapping_channel
            | Rule::some_ip_simple_event_topic
            | Rule::some_ip_event_part_topic
            | Rule::some_ip_topic_mapping_topic => "'channel:' or 'topic:'",

            Rule::some_ip_rpc_channel_mapping_instance_id
            | Rule::some_ip_topic_mapping_instance_id => "'instance_id:'",

            Rule::server_interface | Rule::client_interface => "'interface:' or 'service:'",
            Rule::unit_type_type => "'type:'",
            Rule::build_configuration_target_name => "'target_name:'",
            Rule::build_configuration_skip_codegen => "'skip_codegen:'",

            Rule::qualified_name_str | Rule::message_str | Rule::service_str => {
                "quoted name (e.g. \"pkg.Type\")"
            }
            Rule::service_bundle_name_str => "quoted name (e.g. \"MyBundle\")",
            Rule::service_unit_name_str => "quoted name (e.g. \"my-unit\")",
            Rule::identifier_str | Rule::channel_str => "quoted identifier (e.g. \"instance1\")",
            Rule::target_name_str => "quoted target name",

            Rule::string => "string literal",
            Rule::boolean => "boolean (true/false)",
            Rule::number => "number",
            Rule::integer => "integer",
            Rule::float => "float",

            _ => "valid VSIDL construct",
        }
    }
}

/// Formats a Pest error into a human-friendly single-line string.
pub fn format_pest_error(e: &PestError<Rule>) -> String {
    match &e.variant {
        ErrorVariant::ParsingError { positives, .. } => {
            let friendly_positives: BTreeSet<&str> =
                positives.iter().map(|r| r.to_friendly_string()).collect();

            if friendly_positives.is_empty() {
                return e.to_string();
            }

            let mut v: Vec<_> = friendly_positives.into_iter().collect();
            // Filter out generic "valid VSIDL construct" if we have more specific ones
            if v.len() > 1 {
                v.retain(|&s| s != "valid VSIDL construct");
            }

            let expected = if v.len() > 1 {
                let last = v.pop().unwrap();
                format!("{} or {}", v.join(", "), last)
            } else {
                v[0].to_string()
            };
            format!("Expected {}", expected)
        }
        ErrorVariant::CustomError { message } => message.clone(),
    }
}
