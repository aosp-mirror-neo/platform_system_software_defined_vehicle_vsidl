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

use super::{FindingCollector, FindingId};
use crate::impl_context_condition_symtab;
use lazy_static::lazy_static;
use protobuf::descriptor::{
    field_descriptor_proto::{Label, Type},
    DescriptorProto, FieldDescriptorProto,
};
use protobuf::reflect::ProtobufValue;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use vsidl_v1::annotations::exts::{repeated_field_max_count, variable_type_max_size};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::{
    proto_message_symbol::ProtoMessageSymbol, symbol_table::SymbolTable,
    symbol_table_checker::SymbolTableChecker,
};

lazy_static! {
    static ref FILE_NO_CHECK: HashSet<&'static str> = {
        HashSet::from([
            "google/protobuf/api.proto",
            "google/protobuf/empty.proto",
            "google/protobuf/source_context.proto",
            "google/protobuf/timestamp.proto",
            "google/protobuf/wrappers.proto",
            "google/protobuf/any.proto",
            "google/protobuf/duration.proto",
            "google/protobuf/field_mask.proto",
            "google/protobuf/struct.proto",
            "google/protobuf/type.proto",
            "google/protobuf/descriptor.proto",
        ])
    };
    static ref ANNOTATION_DATATYPE: HashSet<&'static str> = {
        HashSet::from([
            ".google.protobuf.Any",
            ".google.protobuf.Api",
            ".google.protobuf.BytesValue",
            ".google.protobuf.Enum",
            ".google.protobuf.EnumValue",
            ".google.protobuf.Field",
            ".google.protobuf.FieldMask",
            ".google.protobuf.Method",
            ".google.protobuf.Mixin",
            ".google.protobuf.Option",
            ".google.protobuf.SourceContext",
            ".google.protobuf.StringValue",
            ".google.protobuf.Struct",
            ".google.protobuf.Type",
            ".google.protobuf.Value",
            ".google.protobuf.ListValue",
        ])
    };
}

#[derive(Debug, Default)]
pub struct ProtoMessageRequireAnnotationType {
    finding_collector: FindingCollector,

    // Lookup Table: Created once, read-only during check.
    // Map<FullMessageName, (FileName, MessageDefinition)>
    message_lookup: HashMap<String, (String, DescriptorProto)>,

    // Recursion Stack: Tracks current path to find loops.
    recursion_stack: Vec<String>,

    // Memoization: Tracks messages that have already passed validation.
    // Optimization to avoid re-checking the same common sub-messages.
    validated_messages: HashSet<String>,
}

impl_context_condition_symtab!(ProtoMessageRequireAnnotationType);
impl VsidlVisitor for ProtoMessageRequireAnnotationType {}

impl ProtoMessageRequireAnnotationType {
    /// Helper to check if a generic option exists
    fn ensure_annotation<T>(
        &mut self,
        field: &FieldDescriptorProto,
        file_name: &str,
        option: &protobuf::ext::ExtFieldOptional<protobuf::descriptor::FieldOptions, T>,
        annotation_name: &str,
    ) where
        T: ProtobufValue,
    {
        if let Some(options) = field.options.as_ref() {
            if option.get(options).is_none() {
                self.finding_collector.error(
                    FindingId::ProtoMessageRequireAnnotationType,
                    format!(
                        "File name: {}, {} annotation for '{}' is missing",
                        file_name,
                        annotation_name,
                        field.name()
                    ),
                    Vec::new(),
                );
            }
        } else {
            self.finding_collector.error(
                FindingId::ProtoMessageRequireAnnotationType,
                format!(
                    "File name: {}, {} annotation for '{}' is missing",
                    file_name,
                    annotation_name,
                    field.name()
                ),
                Vec::new(),
            );
        }
    }

    /// Helper to report loop findings
    fn report_loop(&mut self, file_name: &str) {
        let loop_path = self.recursion_stack.join(" -> ");
        self.finding_collector.error(
            FindingId::ProtoMessageLoop,
            format!(
                "File name: {}, detect a loop in message path: {}. Please resolve the circular dependency.",
                file_name, loop_path
            ),
            Vec::new(),
        );
    }

    /// The core recursive validator
    fn validate_message(&mut self, message: &DescriptorProto, full_name: &str, file_name: &str) {
        // 1. Memoization Check: If we already approved this message, skip it.
        if self.validated_messages.contains(full_name) {
            return;
        }

        // 2. Push to Stack
        self.recursion_stack.push(full_name.to_string());

        // 3. Validate Fields
        for field in &message.field {
            self.validate_field(field, file_name);
        }

        // 4. Recursion into Nested Definitions
        // (This handles checking nested types defined inside this message)
        for nested in &message.nested_type {
            let nested_name = format!("{}.{}", full_name, nested.name());

            // Loop Detection: Check before recursing
            if self.recursion_stack.contains(&nested_name) {
                self.report_loop(file_name);
            } else {
                self.validate_message(nested, &nested_name, file_name);
            }
        }

        // 5. Pop from Stack & Mark Validated
        self.recursion_stack.pop();
        self.validated_messages.insert(full_name.to_string());
    }

    fn validate_field(&mut self, field: &FieldDescriptorProto, file_name: &str) {
        // Check 1: Repeated Fields
        if field.label() == Label::LABEL_REPEATED {
            self.ensure_annotation(
                field,
                file_name,
                &repeated_field_max_count,
                "repeated_field_max_count",
            );
        }

        // Check 2: Variable Types (Strings, Bytes, Wrapper Types)
        match field.type_() {
            Type::TYPE_STRING | Type::TYPE_BYTES => {
                self.ensure_annotation(
                    field,
                    file_name,
                    &variable_type_max_size,
                    "variable_type_max_size",
                );
            }
            Type::TYPE_MESSAGE => {
                let type_name = field.type_name(); // e.g. ".google.protobuf.StringValue" or ".my.package.MyMsg"

                if ANNOTATION_DATATYPE.contains(type_name) {
                    // It's a Google wrapper (like StringValue), treat as variable type
                    self.ensure_annotation(
                        field,
                        file_name,
                        &variable_type_max_size,
                        "variable_type_max_size",
                    );
                } else if let Some((def_file, def_msg)) =
                    self.message_lookup.get(type_name).cloned()
                {
                    // Loop Detection: Check before recursing into custom message
                    if self.recursion_stack.contains(&type_name.to_string()) {
                        self.report_loop(file_name);
                    } else {
                        // It's a custom message: Recurse into it to check *its* fields
                        self.validate_message(&def_msg, type_name, &def_file);
                    }
                }
            }
            _ => {} // Other types are safe
        }
    }

    /// Helper to populate the lookup table (Recursive)
    fn index_message(&mut self, message: &DescriptorProto, file_name: &str, prefix: &str) {
        let full_name = format!("{}.{}", prefix, message.name());
        self.message_lookup.insert(full_name.clone(), (file_name.to_string(), message.clone()));

        for nested in &message.nested_type {
            self.index_message(nested, file_name, &full_name);
        }
    }
}

impl SymbolTableChecker for ProtoMessageRequireAnnotationType {
    fn before_checks(&mut self, symbol_table: &SymbolTable) {
        self.message_lookup.clear();
        self.validated_messages.clear();
        self.recursion_stack.clear();

        // Build the index first
        for message_symbol in symbol_table.get_all_proto_message_symbols() {
            let file_name = message_symbol.symbol.span.as_ref().map_or("", |s| {
                Path::new(&s.file_path).file_name().and_then(|os_str| os_str.to_str()).unwrap_or("")
            });
            if !FILE_NO_CHECK.contains(file_name) {
                let package_prefix = format!(".{}", message_symbol.package(symbol_table));
                self.index_message(&message_symbol.ast_node, file_name, &package_prefix);
            }
        }
    }

    fn check_proto_message(
        &mut self,
        symbol_table: &SymbolTable,
        message_symbol: &ProtoMessageSymbol,
    ) {
        if message_symbol.publishers(symbol_table).is_empty()
            && message_symbol.subscribers(symbol_table).is_empty()
        {
            return; // Skip messages not used by any publisher or subscriber
        }

        let full_name = message_symbol.full_name(symbol_table);
        let file_name = message_symbol.symbol.span.as_ref().map_or("", |s| &s.file_path);
        if !FILE_NO_CHECK.contains(file_name) {
            self.validate_message(&message_symbol.ast_node, &full_name, file_name);
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::proto_message_datatype_require_annotation::ProtoMessageRequireAnnotationType;
    use crate::FindingSeverity;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;

    #[test]
    fn test_message_no_annotation_error() {
        // GIVEN a message without annotation
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_WITHOUT_ANNOTATION.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_SAME_TARGET.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_DIFF_TARGET.clone(),
        ]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_DESCRIPTOR_MESSAGE_WITHOUT_ANNOTATION.clone());

        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = ProtoMessageRequireAnnotationType::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN error message should be found
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "repeated_int_multi"));

        assert!(check.any_finding_contains(FindingSeverity::Error, "datatype_string_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "datatype_bytes_multi"));

        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_any_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_api_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_bytesvalue_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_enum_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_enumvalue_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_field_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_fieldmask_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_method_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_mixin_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_option_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_sourcecontext_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_stringvalue_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_struct_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_type_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_value_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_listvalue_multi"));

        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "repeated_field_max_count annotation for 'repeated_string_multi' is missing"
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "variable_type_max_size annotation for 'repeated_string_multi' is missing"
        ));

        assert!(check.any_finding_contains(FindingSeverity::Error, "nested_repeated_int_multi"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "nested_string_multi"));

        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: proto_message_without_annotation.proto, repeated_field_max_count annotation for 'samefile' is missing"
        ));

        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: proto_message_submessage_same_target.proto, repeated_field_max_count annotation for 'sametargetsubmessage' is missing"
        ));

        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: proto_message_submessage_diff_target.proto, repeated_field_max_count annotation for 'differenttargetmessage' is missing"
        ));
    }

    #[test]
    fn test_message_with_annotation_ok() {
        // GIVEN a message with annotation
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_WITH_ANNOTATION.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_SAME_TARGET.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_DIFF_TARGET.clone(),
        ]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_DESCRIPTOR_MESSAGE_WITH_ANNOTATION.clone());

        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = ProtoMessageRequireAnnotationType::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN check should be OK
        assert!(check.is_ok());
    }

    #[test]
    fn test_message_with_loop_error() {
        // GIVEN message with loop
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_LOOP.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_SAME_TARGET.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_DIFF_TARGET.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_DESCRIPTOR_MESSAGE_LOOP.clone());

        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = ProtoMessageRequireAnnotationType::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN error message should be found
        eprintln!("Findings: {:?}", check.finding_collector.findings());
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: proto_message_loop.proto, detect a loop in message path: sdv.proto.test.message.annotation.LoopSinglePubMessageA -> .sdv.proto.test.message.annotation.LoopSinglePubMessageA. Please resolve the circular dependency."
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: proto_message_loop.proto, detect a loop in message path: sdv.proto.test.message.annotation.LoopSinglePubMessageB -> .sdv.proto.test.message.annotation.LoopSinglePubMessageB.Nested -> .sdv.proto.test.message.annotation.LoopSinglePubMessageB. Please resolve the circular dependency."
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: proto_message_loop.proto, detect a loop in message path: sdv.proto.test.message.annotation.LoopSinglePubMessageC -> .sdv.proto.test.message.annotation.LoopSubMessage -> .sdv.proto.test.message.annotation.LoopSinglePubMessageC. Please resolve the circular dependency."
        ));
    }
}
