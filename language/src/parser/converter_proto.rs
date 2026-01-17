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

// Enables consistent conversion code even for AST nodes with only one rule
#![allow(clippy::single_match)]

use crate::proto_parser::Rule;
use pest::iterators::Pair;
// PROTOBUF
use protobuf::descriptor::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FileDescriptorProto, MessageOptions, MethodDescriptorProto, OneofDescriptorProto,
    ServiceDescriptorProto,
};
use protobuf::MessageField;

// A simple helper function to unquote string literals.
fn unquote_str(pair: Pair<Rule>) -> String {
    pair.as_str().trim_matches(|c| c == '\'' || c == '\"').to_string()
}

// Main conversion function
pub fn convert_to_file_descriptor(pair: Pair<Rule>, file_path: &str) -> FileDescriptorProto {
    assert_eq!(pair.as_rule(), Rule::proto);
    let mut file_descriptor = FileDescriptorProto::new();
    file_descriptor.set_name(file_path.to_string());

    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::syntax => {
                let syntax_value = item.into_inner().next().unwrap();
                file_descriptor.set_syntax(unquote_str(syntax_value));
            }
            Rule::package => {
                let package_name = item.into_inner().next().unwrap();
                file_descriptor.set_package(package_name.as_str().to_string());
            }
            Rule::import => {
                let mut is_public = false;
                let mut is_weak = false;
                let mut import_path = None;

                for import_item in item.into_inner() {
                    match import_item.as_rule() {
                        Rule::public => is_public = true,
                        Rule::weak => is_weak = true,
                        Rule::strLit => import_path = Some(unquote_str(import_item)),
                        rule => eprintln!("Unexpected rule in import: {:?}", rule),
                    }
                }
                if let Some(path) = import_path {
                    file_descriptor.dependency.push(path);
                    if is_public {
                        file_descriptor
                            .public_dependency
                            .push((file_descriptor.dependency.len() - 1) as i32);
                    }
                    if is_weak {
                        file_descriptor
                            .weak_dependency
                            .push((file_descriptor.dependency.len() - 1) as i32);
                    }
                }
            }
            Rule::topLevelDef => {
                let inner_pair = item.into_inner().next().unwrap();
                match inner_pair.as_rule() {
                    Rule::message => {
                        file_descriptor.message_type.push(convert_to_message(inner_pair))
                    }
                    Rule::enum_ => file_descriptor.enum_type.push(convert_to_enum(inner_pair)),
                    Rule::extend => {
                        file_descriptor.extension.push(convert_to_extend_field(inner_pair))
                    }
                    Rule::service => file_descriptor.service.push(convert_to_service(inner_pair)),
                    rule => eprintln!("Unexpected rule in topLevelDef: {:?}", rule),
                }
            }
            Rule::option => {
                // TODO: Options need to be handled within FileOptions struct.
                // This requires a more complex mapping from generic `option` to specific `FileOptions` fields.
                // For now, we ignore file-level options in the converted struct.
            }
            Rule::emptyStatement => { /* Ignore empty statements */ }
            rule => eprintln!("Unexpected rule in convert_to_file_descriptor: {:?}", rule),
        }
    }

    file_descriptor
}

fn convert_to_message(pair: Pair<Rule>) -> DescriptorProto {
    assert_eq!(pair.as_rule(), Rule::message);
    let mut message = DescriptorProto::new();

    let mut pairs = pair.into_inner();
    let name = pairs.next().unwrap();
    assert_eq!(name.as_rule(), Rule::messageName);
    message.set_name(name.as_str().to_string());

    let body = pairs.next().unwrap();
    assert_eq!(body.as_rule(), Rule::messageBody);

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::message => message.nested_type.push(convert_to_message(item)),
            Rule::enum_ => message.enum_type.push(convert_to_enum(item)),
            Rule::field => message.field.push(convert_to_field(item, None)),
            Rule::oneof => message.oneof_decl.push(convert_to_oneof(item)),
            Rule::mapField => {
                // Map fields are stored as a synthetic message with a `map_entry` option.
                let map_message = convert_to_map_entry(item);
                message.nested_type.push(map_message);
            }
            Rule::reserved => {
                let reserved_inner = item.into_inner().next().unwrap();
                match reserved_inner.as_rule() {
                    Rule::ranges => {
                        for range_pair in reserved_inner.into_inner() {
                            message.reserved_range.push(convert_to_reserved_range(range_pair));
                        }
                    }
                    Rule::strFieldNames => {
                        for str_pair in reserved_inner.into_inner() {
                            message
                                .reserved_name
                                .push(unquote_str(str_pair.into_inner().next().unwrap()));
                        }
                    }
                    Rule::idents => {
                        for ident_pair in reserved_inner.into_inner() {
                            message.reserved_name.push(ident_pair.as_str().to_string());
                        }
                    }
                    rule => eprintln!("Unexpected rule in message reserved: {:?}", rule),
                }
            }
            Rule::extensions => {
                for range_pair in item
                    .into_inner()
                    .filter(|p| p.as_rule() == Rule::ranges)
                    .flat_map(|p| p.into_inner())
                {
                    message.extension_range.push(convert_to_extension_range(range_pair));
                }
            }
            rule => eprintln!("Unexpected rule in messageBody: {:?}", rule),
        }
    }
    message
}

fn convert_to_enum(pair: Pair<Rule>) -> EnumDescriptorProto {
    assert_eq!(pair.as_rule(), Rule::enum_);
    let mut enum_proto = EnumDescriptorProto::new();

    let mut pairs = pair.into_inner();
    let name = pairs.next().unwrap();
    assert_eq!(name.as_rule(), Rule::enumName);
    enum_proto.set_name(name.as_str().to_string());

    let body = pairs.next().unwrap();
    assert_eq!(body.as_rule(), Rule::enumBody);

    for item in body.into_inner() {
        match item.as_rule() {
            Rule::enumField => enum_proto.value.push(convert_to_enum_value(item)),
            Rule::reserved => {
                let reserved_inner = item.into_inner().next().unwrap();
                match reserved_inner.as_rule() {
                    Rule::ranges => {
                        for range_pair in reserved_inner.into_inner() {
                            enum_proto
                                .reserved_range
                                .push(convert_to_enum_reserved_range(range_pair));
                        }
                    }
                    Rule::idents => {
                        for ident_pair in reserved_inner.into_inner() {
                            enum_proto.reserved_name.push(ident_pair.as_str().to_string());
                        }
                    }
                    rule => eprintln!("Unexpected rule in enum reserved: {:?}", rule),
                }
            }
            rule => eprintln!("Unexpected rule in enumBody: {:?}", rule),
        }
    }
    enum_proto
}

fn convert_to_enum_value(pair: Pair<Rule>) -> EnumValueDescriptorProto {
    assert_eq!(pair.as_rule(), Rule::enumField);
    let mut enum_value = EnumValueDescriptorProto::new();

    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::ident => enum_value.set_name(item.as_str().to_string()),
            Rule::intLit => {
                let number: i32 = item.as_str().parse().unwrap_or_default();
                enum_value.set_number(number);
            }
            Rule::enumValueOption => {
                // TODO: Handle options on enum values
            }
            rule => eprintln!("Unexpected rule in enumField: {:?}", rule),
        }
    }
    enum_value
}

fn convert_to_service(pair: Pair<Rule>) -> ServiceDescriptorProto {
    assert_eq!(pair.as_rule(), Rule::service);
    let mut service = ServiceDescriptorProto::new();

    let mut pairs = pair.into_inner();
    let name = pairs.next().unwrap();
    assert_eq!(name.as_rule(), Rule::serviceName);
    service.set_name(name.as_str().to_string());

    if let Some(body) = pairs.next() {
        //assert_eq!(body.as_rule(), Rule::COMMENT | Rule::emptyStatement | Rule::option | Rule::rpc)

        for item in body.into_inner() {
            match item.as_rule() {
                Rule::rpc => service.method.push(convert_to_method(item)),
                rule => eprintln!("Unexpected rule in serviceBody: {:?}", rule),
            }
        }
    }

    service
}

fn convert_to_method(pair: Pair<Rule>) -> MethodDescriptorProto {
    assert_eq!(pair.as_rule(), Rule::rpc);
    let mut method = MethodDescriptorProto::new();
    let mut is_client_stream = false;
    let mut is_server_stream = false;

    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::rpcName => method.set_name(item.as_str().to_string()),
            Rule::messageType => {
                if method.input_type.is_none() {
                    method.set_input_type(item.as_str().to_string());
                } else {
                    method.set_output_type(item.as_str().to_string());
                }
            }
            Rule::client_stream => is_client_stream = true,
            Rule::server_stream => is_server_stream = true,
            rule => eprintln!("Unexpected rule in rpc: {:?}", rule),
        }
    }

    if is_client_stream {
        method.set_client_streaming(true);
    }
    if is_server_stream {
        method.set_server_streaming(true);
    }
    method
}

fn convert_to_field(pair: Pair<Rule>, oneof_index: Option<i32>) -> FieldDescriptorProto {
    assert_eq!(pair.as_rule(), Rule::field);
    let mut field = FieldDescriptorProto::new();
    if let Some(oneof_index) = oneof_index {
        field.set_oneof_index(oneof_index);
    }

    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::label => field.set_label(label_from_str(item.as_str())),
            Rule::type_ => {
                let type_str = item.as_str();
                match type_str {
                    "double" | "float" | "int32" | "int64" | "uint32" | "uint64" | "sint32"
                    | "sint64" | "fixed32" | "fixed64" | "sfixed32" | "sfixed64" | "bool"
                    | "string" | "bytes" => {
                        field.set_type(type_from_str(type_str));
                    }
                    _ => {
                        // This is a messageType or enumType
                        field.set_type(
                            protobuf::descriptor::field_descriptor_proto::Type::TYPE_MESSAGE,
                        );
                        field.set_type_name(type_str.to_string());
                    }
                }
            }
            Rule::fieldName => field.set_name(item.as_str().to_string()),
            Rule::fieldNumber => {
                let number: i32 = item.as_str().parse().unwrap_or_default();
                field.set_number(number);
            }
            Rule::fieldOptions => {
                // TODO: Handle field options
            }
            rule => eprintln!("Unexpected rule in field: {:?}", rule),
        }
    }
    field
}

fn convert_to_oneof(pair: Pair<Rule>) -> OneofDescriptorProto {
    assert_eq!(pair.as_rule(), Rule::oneof);
    let mut oneof_proto = OneofDescriptorProto::new();

    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::oneofName => oneof_proto.set_name(item.as_str().to_string()),
            Rule::oneofField => {
                // Oneof fields are handled as regular fields but associated with the oneof.
                // The `DescriptorProto` parent will need to know about the index.
            }
            rule => eprintln!("Unexpected rule in oneof: {:?}", rule),
        }
    }
    oneof_proto
}

fn convert_to_map_entry(pair: Pair<Rule>) -> DescriptorProto {
    assert_eq!(pair.as_rule(), Rule::mapField);
    let mut map_entry = DescriptorProto::new();
    let mut options = MessageOptions::new();
    options.set_map_entry(true);
    map_entry.options = MessageField::some(options);

    let mut pairs = pair.into_inner();
    let key_type = pairs.next().unwrap();
    let value_type = pairs.next().unwrap();
    let map_name = pairs.next().unwrap();
    let _field_number = pairs.next().unwrap();

    // Map fields are a special case, handled as a nested message.
    map_entry.set_name(
        format!(
            "{}Entry",
            map_name
                .as_str()
                .to_string()
                .chars()
                .next()
                .unwrap()
                .to_uppercase()
                .collect::<String>()
        ) + &map_name.as_str()[1..],
    );

    // Add key field
    let mut key_field = FieldDescriptorProto::new();
    key_field.set_name("key".to_string());
    key_field.set_number(1);
    key_field.set_label(protobuf::descriptor::field_descriptor_proto::Label::LABEL_OPTIONAL);
    key_field.set_type(type_from_str(key_type.as_str()));
    map_entry.field.push(key_field);

    // Add value field
    let mut value_field = FieldDescriptorProto::new();
    value_field.set_name("value".to_string());
    value_field.set_number(2);
    value_field.set_label(protobuf::descriptor::field_descriptor_proto::Label::LABEL_OPTIONAL);
    value_field.set_type(type_from_str(value_type.as_str()));
    map_entry.field.push(value_field);

    map_entry
}

fn convert_to_reserved_range(
    pair: Pair<Rule>,
) -> protobuf::descriptor::descriptor_proto::ReservedRange {
    assert_eq!(pair.as_rule(), Rule::range_);
    let mut range = protobuf::descriptor::descriptor_proto::ReservedRange::new();
    let mut pairs = pair.into_inner();

    if let Some(start) = pairs.next() {
        range.set_start(start.as_str().parse().unwrap_or_default());
    }
    if let Some(end) = pairs.next() {
        if end.as_str() != "max" {
            range.set_end(end.as_str().parse().unwrap_or_default());
        }
    }
    range
}

fn convert_to_extension_range(
    pair: Pair<Rule>,
) -> protobuf::descriptor::descriptor_proto::ExtensionRange {
    assert_eq!(pair.as_rule(), Rule::range_);
    let mut range = protobuf::descriptor::descriptor_proto::ExtensionRange::new();
    let mut pairs = pair.into_inner();

    if let Some(start) = pairs.next() {
        range.set_start(start.as_str().parse().unwrap_or_default());
    }
    if let Some(end) = pairs.next() {
        if end.as_str() != "max" {
            range.set_end(end.as_str().parse().unwrap_or_default());
        }
    }
    range
}

fn convert_to_enum_reserved_range(
    pair: Pair<Rule>,
) -> protobuf::descriptor::enum_descriptor_proto::EnumReservedRange {
    assert_eq!(pair.as_rule(), Rule::range_);
    let mut range = protobuf::descriptor::enum_descriptor_proto::EnumReservedRange::new();
    let mut pairs = pair.into_inner();

    if let Some(start) = pairs.next() {
        range.set_start(start.as_str().parse().unwrap_or_default());
    }
    if let Some(end) = pairs.next() {
        if end.as_str() != "max" {
            range.set_end(end.as_str().parse().unwrap_or_default());
        }
    }
    range
}

fn convert_to_extend_field(pair: Pair<Rule>) -> FieldDescriptorProto {
    assert_eq!(pair.as_rule(), Rule::extend);
    let mut field = FieldDescriptorProto::new();

    let mut pairs = pair.into_inner();
    let extendee = pairs.next().unwrap();
    field.set_extendee(extendee.as_str().to_string());

    // We expect only one field per extend block in this minimal implementation
    if let Some(field_pair) = pairs.find(|p| p.as_rule() == Rule::field) {
        field = convert_to_field(field_pair, None);
        field.set_extendee(extendee.as_str().to_string());
    }

    field
}

fn type_from_str(s: &str) -> protobuf::descriptor::field_descriptor_proto::Type {
    match s {
        "double" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_DOUBLE,
        "float" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_FLOAT,
        "int32" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_INT32,
        "int64" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_INT64,
        "uint32" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_UINT32,
        "uint64" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_UINT64,
        "sint32" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_SINT32,
        "sint64" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_SINT64,
        "fixed32" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_FIXED32,
        "fixed64" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_FIXED64,
        "sfixed32" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_SFIXED32,
        "sfixed64" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_SFIXED64,
        "bool" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_BOOL,
        "string" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_STRING,
        "bytes" => protobuf::descriptor::field_descriptor_proto::Type::TYPE_BYTES,
        _ => protobuf::descriptor::field_descriptor_proto::Type::TYPE_MESSAGE, // Default for user-defined types (message or enum)
    }
}

fn label_from_str(s: &str) -> protobuf::descriptor::field_descriptor_proto::Label {
    match s {
        "required" => protobuf::descriptor::field_descriptor_proto::Label::LABEL_REQUIRED,
        "optional" => protobuf::descriptor::field_descriptor_proto::Label::LABEL_OPTIONAL,
        "repeated" => protobuf::descriptor::field_descriptor_proto::Label::LABEL_REPEATED,
        _ => protobuf::descriptor::field_descriptor_proto::Label::LABEL_OPTIONAL,
    }
}

#[cfg(test)]
mod test {
    use crate::converter_proto;
    use crate::proto_parser::{ProtoParser, Rule};
    use pest::Parser;

    const FILE_PATH: &str = "test.proto";

    #[test]
    fn test_proto_file_is_correctly_converted() {
        // GIVEN a Proto file with all AST node types
        let content =
            std::fs::read_to_string("src/tests_vsidl_v1/protos/all_ast_node_types.proto").unwrap();

        // WHEN parsing the file
        let parsed_file = ProtoParser::parse(Rule::proto, &content)
            .unwrap_or_else(|e| panic!("failed to parse proto file: {}", e));
        let file_descriptor = converter_proto::convert_to_file_descriptor(
            parsed_file.into_iter().next().unwrap(),
            FILE_PATH,
        );

        // THEN the package is correct
        assert_eq!(file_descriptor.package, Some("sdv.test".to_string()));
    }
}
