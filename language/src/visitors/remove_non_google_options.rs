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
//! Removes options from Protobuf AST that are not defined by Google

use crate::impl_traverse_catalog_mut;
use crate::visitors::visitor_mut::{AstNodeMut, VsidlVisitorMut};
use protobuf::descriptor::{
    EnumOptions, EnumValueOptions, FieldOptions, FileDescriptorProto, MessageOptions,
    MethodOptions, OneofOptions, ServiceOptions,
};
use protobuf::MessageField;
use protobuf::UnknownFields;

/// A mutating visitor that traverses the Protobuf AST and removes any custom options
/// that are not within the Google-reserved range (50000-59999).
///
/// This is useful for ensuring that only standard or SDV-specific options are
/// processed or generated, potentially stripping out OEM-specific options before
/// further processing.
///
/// It utilizes the [VsidlVisitorMut] trait to hook into the traversal process
/// for various Protobuf options structures.
#[derive(Default)]
pub struct RemoveNonGoogleOptions {}
impl_traverse_catalog_mut!(RemoveNonGoogleOptions);

/// Implements the [VsidlVisitorMut] trait for [RemoveNonGoogleOptions].
///
/// Overrides `start_visit_*` methods for various Protobuf options types
/// ([FileDescriptorProto], [MessageOptions], [FieldOptions], etc.) to
/// apply the option filtering logic using [remove_non_google_options].
impl VsidlVisitorMut for RemoveNonGoogleOptions {
    /// Called before visiting the children of a mutable [FileDescriptorProto] node.
    /// Removes non-Google options from the file options.
    ///
    /// # Parameters
    /// * `node`: A mutable reference to the [FileDescriptorProto] being visited.
    fn start_visit_file_descriptor_proto(&mut self, node: &mut FileDescriptorProto) {
        if let MessageField(Some(options)) = &mut node.options {
            remove_non_google_options(options.special_fields.mut_unknown_fields());
        }
    }

    /// Called before visiting the children of a mutable [MessageOptions] node.
    /// Removes non-Google options from the message options.
    ///
    /// # Parameters
    /// * `node`: A mutable reference to the [MessageOptions] being visited.
    fn start_visit_proto_message_options(&mut self, node: &mut MessageOptions) {
        remove_non_google_options(node.special_fields.mut_unknown_fields());
    }

    /// Called before visiting the children of a mutable [FieldOptions] node.
    /// Removes non-Google options from the field options.
    ///
    /// # Parameters
    /// * `node`: A mutable reference to the [FieldOptions] being visited.
    fn start_visit_proto_field_options(&mut self, node: &mut FieldOptions) {
        remove_non_google_options(node.special_fields.mut_unknown_fields());
    }

    /// Called before visiting the children of a mutable [ServiceOptions] node.
    /// Removes non-Google options from the service options.
    ///
    /// # Parameters
    /// * `node`: A mutable reference to the [ServiceOptions] being visited.
    fn start_visit_proto_service_options(&mut self, node: &mut ServiceOptions) {
        remove_non_google_options(node.special_fields.mut_unknown_fields());
    }

    /// Called before visiting the children of a mutable [MethodOptions] node.
    /// Removes non-Google options from the method options.
    ///
    /// # Parameters
    /// * `node`: A mutable reference to the [MethodOptions] being visited.
    fn start_visit_proto_method_options(&mut self, node: &mut MethodOptions) {
        remove_non_google_options(node.special_fields.mut_unknown_fields());
    }

    /// Called before visiting the children of a mutable [EnumOptions] node.
    /// Removes non-Google options from the enum options.
    ///
    /// # Parameters
    /// * `node`: A mutable reference to the [EnumOptions] being visited.
    fn start_visit_proto_enum_options(&mut self, node: &mut EnumOptions) {
        remove_non_google_options(node.special_fields.mut_unknown_fields());
    }

    /// Called before visiting the children of a mutable [EnumValueOptions] node.
    /// Removes non-Google options from the enum value options.
    ///
    /// # Parameters
    /// * `node`: A mutable reference to the [EnumValueOptions] being visited.
    fn start_visit_proto_enum_value_options(&mut self, node: &mut EnumValueOptions) {
        remove_non_google_options(node.special_fields.mut_unknown_fields());
    }

    /// Called before visiting the children of a mutable [OneofOptions] node.
    /// Removes non-Google options from the oneof options.
    ///
    /// # Parameters
    /// * `node`: A mutable reference to the [OneofOptions] being visited.
    fn start_visit_proto_oneof_options(&mut self, node: &mut OneofOptions) {
        remove_non_google_options(node.special_fields.mut_unknown_fields());
    }
}

/// Removes options from the given [UnknownFields] that are not defined by Google.
///
/// Iterates through the unknown fields, identifies field numbers outside the
/// Google-reserved range (50000-59999) using [is_google_defined_option],
/// and removes them.
///
/// # Parameters
/// * `fields`: A mutable reference to the [UnknownFields] to modify.
fn remove_non_google_options(fields: &mut UnknownFields) {
    // Find fields not defined by Google
    let mut to_remove = Vec::new();
    for (field_number, _) in fields.iter() {
        if !is_google_defined_option(field_number) {
            to_remove.push(field_number);
        }
    }

    // Remove those fields
    for field_number in to_remove.into_iter().rev() {
        fields.remove(field_number);
    }
}

/// Returns `true` if the given Protobuf custom option field number falls within the
/// range reserved for Google/SDV (50000-59999).
///
/// According to `annotations.proto`, custom options index values 50000-59999
/// are reserved for SDV. OEMs may introduce their own custom options with indexes
/// 60000-99999. Field numbers outside these ranges are typically standard Protobuf
/// options or potentially invalid.
///
/// # Parameters
/// * `field_number`: The field number of the custom option to check.
///
/// # Returns
/// `true` if the field number is within the Google/SDV reserved range, `false` otherwise.
fn is_google_defined_option(field_number: u32) -> bool {
    // See vsidl/language/src/protos/sdv/vsidl/v1/annotations.proto
    // Custom options index values 50000-59999 are reserved for SDV.
    // OEMs may introduce own custom options with indexes 60000-99999
    (50000..=59999).contains(&field_number)
}

#[cfg(test)]
mod test {
    use crate::remove_non_google_options::is_google_defined_option;
    use crate::remove_non_google_options::RemoveNonGoogleOptions;
    use crate::test_data;
    use crate::Catalog;
    use protobuf::MessageField;
    use protobuf::SpecialFields;

    /// Checks if a specific option with a given field number exists in a message's options.
    ///
    /// # Parameters
    /// * `catalog`: The [Catalog] containing the Protobuf descriptors.
    /// * `message_name`: The name of the message to check.
    /// * `option_field_number`: The field number of the option to look for.
    ///
    /// # Returns
    /// `true` if the option exists in the specified message's options, `false` otherwise.
    fn has_message_option(catalog: &Catalog, message_name: &str, option_field_number: u32) -> bool {
        for (_, file_descriptor) in &catalog.proto_descriptors {
            for message_descriptor in &file_descriptor.message_type {
                if message_descriptor.name() == message_name {
                    if let MessageField(Some(options)) = &message_descriptor.options {
                        return options
                            .special_fields
                            .unknown_fields()
                            .iter()
                            .any(|(field_number, _)| field_number == option_field_number);
                    }
                }
            }
        }
        false
    }

    /// Checks if a specific option with a given field number exists in a field's options
    /// within a specific message.
    ///
    /// # Parameters
    /// * `catalog`: The [Catalog] containing the Protobuf descriptors.
    /// * `message_name`: The name of the message containing the field.
    /// * `field_name`: The name of the field to check.
    /// * `option_field_number`: The field number of the option to look for.
    ///
    /// # Returns
    /// `true` if the option exists in the specified field's options, `false` otherwise.
    fn has_field_option(
        catalog: &Catalog,
        message_name: &str,
        field_name: &str,
        option_field_number: u32,
    ) -> bool {
        for (_, file_descriptor) in &catalog.proto_descriptors {
            for message_descriptor in &file_descriptor.message_type {
                if message_descriptor.name() == message_name {
                    for field_descriptor in &message_descriptor.field {
                        if field_descriptor.name() == field_name {
                            if let MessageField(Some(options)) = &field_descriptor.options {
                                return options
                                    .special_fields
                                    .unknown_fields()
                                    .iter()
                                    .any(|(field_number, _)| field_number == option_field_number);
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Checks if a specific option with a given field number exists in an enum's options.
    ///
    /// # Parameters
    /// * `catalog`: The [Catalog] containing the Protobuf descriptors.
    /// * `enum_name`: The name of the enum to check.
    /// * `option_field_number`: The field number of the option to look for.
    ///
    /// # Returns
    /// `true` if the option exists in the specified enum's options, `false` otherwise.
    fn has_enum_option(catalog: &Catalog, enum_name: &str, option_field_number: u32) -> bool {
        for (_, file_descriptor) in &catalog.proto_descriptors {
            for enum_descriptor in &file_descriptor.enum_type {
                if enum_descriptor.name() == enum_name {
                    if let MessageField(Some(options)) = &enum_descriptor.options {
                        return options
                            .special_fields
                            .unknown_fields()
                            .iter()
                            .any(|(field_number, _)| field_number == option_field_number);
                    }
                }
            }
        }
        false
    }

    /// Checks if the given [SpecialFields] contain any options outside the Google-reserved range.
    ///
    /// # Parameters
    /// * `special_fields`: The [SpecialFields] to examine.
    ///
    /// # Returns
    /// `true` if any non-Google options are found, `false` otherwise.
    fn special_fields_has_non_google_options(special_fields: SpecialFields) -> bool {
        special_fields
            .unknown_fields()
            .iter()
            .any(|(field_number, _)| !is_google_defined_option(field_number))
    }

    /// Checks if any Protobuf definition within the catalog contains options outside the
    /// Google-reserved range.
    ///
    /// This function iterates through file, message, field, oneof, enum, enum value,
    /// service, and method options.
    ///
    /// # Parameters
    /// * `catalog`: The [Catalog] to examine.
    ///
    /// # Returns
    /// `true` if any non-Google options are found anywhere in the catalog's Protobuf definitions,
    /// `false` otherwise.
    fn has_non_google_options(catalog: &Catalog) -> bool {
        let mut options_found = false;

        for (_, file_descriptor) in &catalog.proto_descriptors {
            // File options
            if let MessageField(Some(options)) = &file_descriptor.options {
                options_found |=
                    special_fields_has_non_google_options(options.special_fields.clone());
            }

            // Messages
            for message_descriptor in &file_descriptor.message_type {
                if let MessageField(Some(options)) = &message_descriptor.options {
                    options_found |=
                        special_fields_has_non_google_options(options.special_fields.clone());
                }

                for field_descriptor in &message_descriptor.field {
                    if let MessageField(Some(options)) = &field_descriptor.options {
                        options_found |=
                            special_fields_has_non_google_options(options.special_fields.clone());
                    }
                }

                for oneof_descriptor in &message_descriptor.oneof_decl {
                    if let MessageField(Some(options)) = &oneof_descriptor.options {
                        options_found |=
                            special_fields_has_non_google_options(options.special_fields.clone());
                    }
                }
            }

            // Enum
            for enum_descriptor in &file_descriptor.enum_type {
                if let MessageField(Some(options)) = &enum_descriptor.options {
                    options_found |=
                        special_fields_has_non_google_options(options.special_fields.clone());
                }

                for value_descriptor in &enum_descriptor.value {
                    if let MessageField(Some(options)) = &value_descriptor.options {
                        options_found |=
                            special_fields_has_non_google_options(options.special_fields.clone());
                    }
                }
            }

            // RPC
            for service_descriptor in &file_descriptor.service {
                if let MessageField(Some(options)) = &service_descriptor.options {
                    options_found |=
                        special_fields_has_non_google_options(options.special_fields.clone());
                }

                for method_descriptor in &service_descriptor.method {
                    if let MessageField(Some(options)) = &method_descriptor.options {
                        options_found |=
                            special_fields_has_non_google_options(options.special_fields.clone());
                    }
                }
            }
        }

        options_found
    }

    /// Test helper: Returns true if the `BatteryLevel` message in the given catalog contains the
    /// non-Google placeholder option (field number 69999).
    ///
    /// # Parameters
    /// * `catalog`: The [Catalog] to examine.
    ///
    /// # Returns
    /// `true` if the option is found, `false` otherwise.
    pub fn has_non_google_message_option(catalog: &Catalog) -> bool {
        has_message_option(catalog, "BatteryLevel", 69999)
    }

    /// Test helper: Returns true if the `level2` field in the `BatteryLevel` message contains
    /// the non-Google `repeated_field_max_count` option (field number 60050).
    ///
    /// # Parameters
    /// * `catalog`: The [Catalog] to examine.
    ///
    /// # Returns
    /// `true` if the option is found, `false` otherwise.
    pub fn has_non_google_field_option(catalog: &Catalog) -> bool {
        has_field_option(catalog, "BatteryLevel", "level2", 60050)
    }

    /// Test helper: Returns true if the `BatteryChannels` enum contains the non-Google
    /// `my_enum_option` (field number 67890).
    ///
    /// # Parameters
    /// * `catalog`: The [Catalog] to examine.
    ///
    /// # Returns
    /// `true` if the option is found, `false` otherwise.
    pub fn has_non_google_enum_option(catalog: &Catalog) -> bool {
        has_enum_option(catalog, "BatteryChannels", 67890)
    }

    /// Test helper: Returns true if the `BatteryLevel` message contains the Google-defined
    /// `SdvPublication` option (field number 50001).
    ///
    /// # Parameters
    /// * `catalog`: The [Catalog] to examine.
    ///
    /// # Returns
    /// `true` if the option is found, `false` otherwise.
    pub fn has_google_message_option(catalog: &Catalog) -> bool {
        has_message_option(catalog, "BatteryLevel", 50001)
    }

    /// Test helper: Returns true if the `level1` field in the `BatteryLevel` message contains
    /// the Google-defined `repeated_field_max_count` option (field number 50050).
    ///
    /// # Parameters
    /// * `catalog`: The [Catalog] to examine.
    ///
    /// # Returns
    /// `true` if the option is found, `false` otherwise.
    pub fn has_google_field_option(catalog: &Catalog) -> bool {
        has_field_option(catalog, "BatteryLevel", "level1", 50050)
    }

    /// Test helper: Returns true if the `BatteryChannels` enum contains the Google-defined
    /// `enum_byte_size` option (field number 50200).
    ///
    /// # Parameters
    /// * `catalog`: The [Catalog] to examine.
    ///
    /// # Returns
    /// `true` if the option is found, `false` otherwise.
    pub fn has_google_enum_option(catalog: &Catalog) -> bool {
        has_enum_option(catalog, "BatteryChannels", 50200)
    }

    #[test]
    fn test_should_remove_non_google_options() {
        // GIVEN A catalog with messages that contain options not defined by Google
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([test_data::DESCRIPTOR_UNKNOWN_OPTIONS.clone()]);

        // THEN has non-Google fields before stripping them
        assert!(has_non_google_message_option(&catalog));
        assert!(has_non_google_field_option(&catalog));
        assert!(has_non_google_enum_option(&catalog));

        // THEN catalog contains Google options
        assert!(has_google_message_option(&catalog));
        assert!(has_google_field_option(&catalog));
        assert!(has_google_enum_option(&catalog));

        // WHEN Stripping the non-Google options
        RemoveNonGoogleOptions::traverse(&mut catalog);

        // THEN non-Google options shall be removed
        assert!(!has_non_google_message_option(&catalog));
        assert!(!has_non_google_field_option(&catalog));
        assert!(!has_non_google_enum_option(&catalog));

        // THEN Google options are still there
        assert!(has_google_message_option(&catalog));
        assert!(has_google_field_option(&catalog));
        assert!(has_google_enum_option(&catalog));
    }

    #[test]
    fn test_should_remove_all_types_of_options() {
        // GIVEN A catalog with messages that contain options not defined by Google
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([test_data::ALL_AST_NODE_TYPES_PROTO.clone()]);

        // WHEN Stripping the non-Google options
        RemoveNonGoogleOptions::traverse(&mut catalog);

        // THEN no non-Google options shall be left
        assert!(!has_non_google_options(&catalog));
    }
}
