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

use crate::impl_context_condition;
use crate::{FindingCollector, FindingId};
use lazy_static::lazy_static;
use protobuf::descriptor::{
    field_descriptor_proto::{Label, Type},
    DescriptorProto, FieldDescriptorProto,
};
use std::collections::{HashMap, HashSet};
use vsidl_v1::annotations::{
    exts::{repeated_field_max_count, variable_type_max_size},
    SdvPublication,
};
use vsidl_v1_common::visitors::visitor::{ProtoFile, VsidlVisitor};

#[derive(Debug, Default)]
pub struct ProtoMessageRequireAnnotationType {
    finding_collector: FindingCollector,
    protofile_name: String,
    package_name: String,
    /// all unvisited message in current catalog
    /// HashMap<full_message_name, (file_name, message)>
    unvisited_message: HashMap<String, (String, DescriptorProto)>,
    /// record the visit path chain for loop detecting
    /// HashSet<full_message_name>
    visit_path: HashSet<String>,
}
impl_context_condition!(ProtoMessageRequireAnnotationType);

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

impl ProtoMessageRequireAnnotationType {
    fn check_variable_type_max_size(
        &mut self,
        field_descriptor: &FieldDescriptorProto,
        file_name: String,
    ) {
        let option_value = variable_type_max_size.get(&field_descriptor.options);
        if option_value.is_none() {
            self.finding_collector.error(
                FindingId::ProtoMessageRequireAnnotationType,
                format!(
                    "File name: {}, variable_type_max_size annotation for '{}' is missing",
                    file_name.clone(),
                    field_descriptor.name()
                ),
                Vec::new(),
            );
        }
    }

    fn check_repeated_field_max_count(
        &mut self,
        field_descriptor: &FieldDescriptorProto,
        file_name: String,
    ) {
        let option_value = repeated_field_max_count.get(&field_descriptor.options);
        if option_value.is_none() {
            self.finding_collector.error(
                FindingId::ProtoMessageRequireAnnotationType,
                format!(
                    "File name: {}, repeated_field_max_count annotation for '{}' is missing",
                    file_name.clone(),
                    field_descriptor.name()
                ),
                Vec::new(),
            );
        }
    }

    /// check if fields require annotations, and visit sub messages
    fn check_field_annotation_and_loop(
        &mut self,
        message_field: &FieldDescriptorProto,
        file_name: String,
    ) {
        // check repeated_field_max_count annotation
        if message_field.label() == Label::LABEL_REPEATED {
            self.check_repeated_field_max_count(message_field, file_name.clone());
        }

        // check variable_type_max_size annotation
        let datatype: Type = message_field.type_();
        match datatype {
            Type::TYPE_STRING | Type::TYPE_BYTES => {
                self.check_variable_type_max_size(message_field, file_name.clone())
            }
            Type::TYPE_MESSAGE => {
                let datatype_name = message_field.type_name();
                if ANNOTATION_DATATYPE.contains(datatype_name) {
                    self.check_variable_type_max_size(message_field, file_name.clone());
                } else if let Some(message) = self.unvisited_message.get(datatype_name) {
                    let message_object = message.1.clone();
                    let current_file_name = message.0.clone();
                    self.check_message(
                        &message_object,
                        datatype_name.to_string(),
                        current_file_name,
                    );
                }
            }
            _ => { /* annotation checking is not necessary for rest of the datatype */ }
        }
    }

    fn check_message(
        &mut self,
        message: &DescriptorProto,
        message_full_name: String,
        file_name: String,
    ) {
        // check and update path
        if self.visit_path.contains(&message_full_name) {
            self.finding_collector.error(
                FindingId::ProtoMessageLoop,
                format!(
                    "File name: {}, detect a loop in message '{}', please resolve it",
                    file_name.clone(),
                    message.name()
                ),
                Vec::new(),
            );
            return;
        }
        self.visit_path.insert(message_full_name.clone());

        // check field
        for field in &message.field {
            self.check_field_annotation_and_loop(field, file_name.clone());
        }

        // check nested message
        for nested_message in &message.nested_type {
            self.check_message(
                nested_message,
                message_full_name.to_owned() + "." + nested_message.name(),
                file_name.clone(),
            );
        }

        // update path before leaving this message
        self.visit_path.remove(&message_full_name);
        self.unvisited_message.remove(&message_full_name);
    }

    fn get_message(&mut self, message: &DescriptorProto, file_name: String, prefix: String) {
        let full_message_name = prefix + "." + message.name();
        self.unvisited_message
            .insert(full_message_name.clone(), (file_name.clone(), message.clone()));
        for nested_message in &message.nested_type {
            self.get_message(nested_message, file_name.clone(), full_message_name.clone());
        }
    }
}

/// Issues an error if annotations are missing for certain fields in proto messages
impl VsidlVisitor for ProtoMessageRequireAnnotationType {
    fn start_visit_catalog(&mut self, catalog: &vsidl_v1_common::Catalog) {
        self.unvisited_message.clear();
        for (_, file) in &catalog.proto_descriptors {
            // CoCo only check internal file, so it should ignore google/protobuf/*.proto (external file)
            if !FILE_NO_CHECK.contains(file.name()) {
                for message in &file.message_type {
                    let prefix = ".".to_owned() + file.package();
                    self.get_message(message, file.name().to_string(), prefix);
                }
            }
        }
    }

    fn start_visit_proto_file(&mut self, node: &ProtoFile) {
        self.protofile_name = node.1.name().to_string();
        self.package_name = node.1.package().to_string();
    }

    fn start_visit_proto_single_pub_message(
        &mut self,
        message: &DescriptorProto,
        _publication: &SdvPublication,
    ) {
        let message_full_name = ".".to_owned() + &self.package_name + "." + message.name();
        self.check_message(message, message_full_name, self.protofile_name.clone());
    }

    fn start_visit_proto_multi_pub_message(
        &mut self,
        message: &DescriptorProto,
        _publication: &SdvPublication,
    ) {
        let message_full_name = ".".to_owned() + &self.package_name + "." + message.name();
        self.check_message(message, message_full_name, self.protofile_name.clone());
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::pdk29::ProtoMessageRequireAnnotationType;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_message_no_annotation_single_pub_error() {
        // GIVEN a message without annotation
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_WITHOUT_ANNOTATION.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_SAME_TARGET.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_DIFF_TARGET.clone(),
        ]);

        // WHEN executing context condition
        let mut check = ProtoMessageRequireAnnotationType::default();
        check.check_catalog(&catalog);

        // THEN error message should be found
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "repeated_int_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "datatype_string_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "datatype_bytes_single"));

        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_any_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_api_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_bytesvalue_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_enum_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_enumvalue_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_field_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_fieldmask_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_method_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_mixin_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_option_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_sourcecontext_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_stringvalue_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_struct_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_type_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_value_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "protobuf_listvalue_single"));

        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "repeated_field_max_count annotation for 'repeated_string_single' is missing"
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "variable_type_max_size annotation for 'repeated_string_single' is missing"
        ));

        assert!(check.any_finding_contains(FindingSeverity::Error, "nested_repeated_int_single"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "nested_string_single"));

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
    fn test_message_no_annotation_multi_pub_error() {
        // GIVEN a message without annotation
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_WITHOUT_ANNOTATION.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_SAME_TARGET.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_DIFF_TARGET.clone(),
        ]);

        // WHEN executing context condition
        let mut check = ProtoMessageRequireAnnotationType::default();
        check.check_catalog(&catalog);

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

        // WHEN executing context condition
        let mut check = ProtoMessageRequireAnnotationType::default();
        check.check_catalog(&catalog);

        // THEN check should be OK
        assert!(check.is_ok());
    }

    #[test]
    fn test_message_with_loop_single_pub_error() {
        // GIVEN message with loop
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_LOOP.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_SAME_TARGET.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_DIFF_TARGET.clone(),
        ]);

        // WHEN executing context condition
        let mut check = ProtoMessageRequireAnnotationType::default();
        check.check_catalog(&catalog);

        // THEN error message should be found
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: proto_message_loop.proto, detect a loop in message 'LoopSinglePubMessageA', please resolve it"
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "detect a loop in message 'LoopSinglePubMessageB', please resolve it"
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "detect a loop in message 'LoopSinglePubMessageC', please resolve it"
        ));
    }

    #[test]
    fn test_message_with_loop_multi_pub_error() {
        // GIVEN message with loop
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_LOOP.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_SAME_TARGET.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_SUBMESSAGE_DIFF_TARGET.clone(),
        ]);

        // WHEN executing context condition
        let mut check = ProtoMessageRequireAnnotationType::default();
        check.check_catalog(&catalog);

        // THEN error message should be found
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: proto_message_submessage_same_target.proto, detect a loop in message 'SameTargetLoopMessage', please resolve it"
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: proto_message_submessage_diff_target.proto, detect a loop in message 'DiffTargetLoopMessage', please resolve it"
        ));
    }
}
