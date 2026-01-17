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
//! Contains logic which extracts size and constraint for protobuf messages

use anyhow::{anyhow, bail};
use indexmap::IndexMap;
use protobuf::descriptor::field_descriptor_proto::{Label, Type};
use protobuf::descriptor::{DescriptorProto, FieldDescriptorProto, FileDescriptorProto};
use protobuf::EnumOrUnknown;
use sdv_vsidl_common::{
    field_options::FieldOptions, proto_size_utils::*, WELLKNOWN_TYPES_TO_DESCRIPTOR,
};

/// Represents the computed size and constraints of a Protobuf message.
#[derive(Debug, Default, Clone)]
pub struct MessageSize {
    /// The maximum possible size of the message in bytes, considering constraints.
    pub size: usize,
    /// A map of field names to their specific size constraints.
    pub size_constraints: IndexMap<String, FieldSizeConstraint>,
}

/// Holds metadata extracted from a Protobuf message definition, used for size calculation.
#[derive(Debug, Default)]
pub struct MessageSizeMetadata {
    /// The name of the Protobuf message.
    #[allow(dead_code)]
    pub name: String,
    /// A list of metadata for each field within the message.
    pub fields: Vec<FieldSizeMetadata>,
}

/// A collection of [MessageSizeMetadata] for multiple Protobuf messages, keyed by message name.
#[derive(Debug, Default)]
pub struct MessageSizeMetadataSet {
    /// An ordered map where keys are fully qualified message names and values are their metadata.
    pub messages: IndexMap<MessageName, MessageSizeMetadata>,
}

/// Holds metadata extracted from a Protobuf field definition, used for size calculation.
#[derive(Debug, Default)]
pub struct FieldSizeMetadata {
    /// The name of the Protobuf field.
    field_name: String,
    /// The tag number assigned to the field in the Protobuf definition.
    tag_id: i32,
    /// The data type of the field (e.g., TYPE_INT32, TYPE_STRING).
    type_: Option<EnumOrUnknown<Type>>,
    /// The label of the field (e.g., LABEL_OPTIONAL, LABEL_REPEATED).
    label: Option<EnumOrUnknown<Label>>,
    /// For message or enum types, the fully qualified name of the type.
    type_name: Option<String>,
    /// Specific size constraints applied to this field via options.
    size_constraint: Option<FieldSizeConstraint>,
}

/// Defines size constraints for a Protobuf field, typically specified via custom options.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FieldSizeConstraint {
    /// The maximum number of elements allowed if the field is repeated.
    pub repeated_max_count: Option<usize>,
    /// The maximum size in bytes allowed for variable-sized types like string or bytes.
    pub variable_type_max_size: Option<usize>,
}

/// Type alias for a fully qualified Protobuf message name (e.g., "package.Message").
pub type MessageName = String;

/// Computes the maximum size of a Protobuf message based on its metadata and constraints.
///
/// This function recursively calculates the size, utilizing memoization to avoid redundant
/// computations for nested messages. It relies on size constraints defined in Protobuf options
/// (like `repeated_field_max_count` and `variable_type_max_size`) to determine the maximum
/// possible size.
///
/// # Parameters
///
/// * `message_full_name` - The fully qualified name of the message to compute the size for.
/// * `messages_computed_sizes` - A mutable map used for memoization, storing already computed sizes.
/// * `messages_metadata_set` - The set containing metadata for all relevant messages.
///
/// # Returns
///
/// The computed [MessageSize].
///
/// # Errors
///
/// Returns an error if:
///   - Metadata for the message is not found.
///   - Required size constraints are missing for repeated or variable-length fields.
///   - An invalid Protobuf type (like TYPE_GROUP) is encountered.
///   - A referenced message name is not fully qualified.
pub fn compute_message_size(
    message_full_name: &str,
    messages_computed_sizes: &mut IndexMap<String, MessageSize>,
    messages_metadata_set: &MessageSizeMetadataSet,
) -> anyhow::Result<MessageSize> {
    if let Some(message_size) = messages_computed_sizes.get(message_full_name) {
        // we computed size of the message already
        return Ok(message_size.clone());
    }

    let message_size_metadata = messages_metadata_set.messages.get(message_full_name).ok_or(anyhow!(format!("Failed to find message size metadata for {:?} message. Something went wrong during parsing.", message_full_name)))?;
    let mut message_size: usize = 0;
    for field in &message_size_metadata.fields {
        message_size += compute_field_size(
            field,
            messages_computed_sizes,
            messages_metadata_set,
            message_full_name,
        )?;
    }

    // copy size constraints we collected during parsing, we will need them later to show proper error messages
    let size_constraints: IndexMap<String, FieldSizeConstraint> = message_size_metadata
        .fields
        .iter()
        .filter_map(|f| f.size_constraint.clone().map(|sc| (f.field_name.clone(), sc.clone())))
        .collect();

    let message_computed_size = MessageSize { size: message_size, size_constraints };
    messages_computed_sizes.insert(message_full_name.to_string(), message_computed_size.clone());
    Ok(message_computed_size)
}

/// Computes the maximum size contribution of a single Protobuf field.
///
/// Calculates the size based on the field's type, tag, label (repeated), and any
/// specified size constraints. For message types, it recursively calls [compute_message_size].
///
/// # Parameters
///
/// * `field_size_metadata` - Metadata for the field whose size is being computed.
/// * `messages_computed_sizes` - A mutable map used for memoization when calculating nested message sizes.
/// * `messages_metadata_set` - The set containing metadata for all relevant messages.
/// * `message_name` - The fully qualified name of the message containing this field (used for error reporting).
///
/// # Returns
///
/// The computed maximum size in bytes for the field.
///
/// # Errors
///
/// Returns an error if:
///   - Required size constraints (`repeated_field_max_count` for repeated fields,
///     `variable_type_max_size` for string/bytes/message fields without explicit size) are missing.
///   - The field type is TYPE_GROUP (deprecated).
///   - The `type_name` is missing for a message field.
///   - A referenced message name (`type_name`) is not fully qualified (doesn't start with '.').
///   - Recursive size computation for a nested message fails.
fn compute_field_size(
    field_size_metadata: &FieldSizeMetadata,
    messages_computed_sizes: &mut IndexMap<String, MessageSize>,
    messages_metadata_set: &MessageSizeMetadataSet,
    message_name: &str,
) -> anyhow::Result<usize> {
    let mut field_size: usize = 0;
    field_size += compute_tag_size(field_size_metadata.tag_id);
    field_size += match field_size_metadata.type_.and_then(|l| l.enum_value().ok()) {
        // please see https://protobuf.dev/programming-guides/encoding/ for the details
        Some(Type::TYPE_FLOAT) | Some(Type::TYPE_FIXED32) | Some(Type::TYPE_SFIXED32) => 4,
        Some(Type::TYPE_BOOL) => 1,
        Some(Type::TYPE_ENUM) | Some(Type::TYPE_UINT32) | Some(Type::TYPE_SINT32) => 5,
        Some(Type::TYPE_INT32)
        | Some(Type::TYPE_INT64)
        | Some(Type::TYPE_UINT64)
        | Some(Type::TYPE_SINT64) => 10,
        Some(Type::TYPE_DOUBLE) | Some(Type::TYPE_FIXED64) | Some(Type::TYPE_SFIXED64) => 8,
        Some(Type::TYPE_STRING) | Some(Type::TYPE_BYTES) => {
            let size: usize = get_variable_type_max_size(field_size_metadata, message_name)?;
            size + compute_length_size(size)
        }
        Some(Type::TYPE_MESSAGE) => {
            let message_full_name = field_size_metadata
                .type_name
                .clone()
                .ok_or(anyhow!("Type name is not set for the field of the message type"))?;
            // Fully qualified names should start with '.'
            if !message_full_name.starts_with('.') {
                anyhow::bail!(format!(
                    "Referenced message name is not fully qualified. Message name: {}",
                    &message_full_name
                ));
            }

            if let Ok(size) = get_variable_type_max_size(field_size_metadata, message_name) {
                size + compute_length_size(size)
            } else {
                let message_full_name_without_dot = &message_full_name[1..];
                let message_size = compute_message_size(
                    message_full_name_without_dot,
                    messages_computed_sizes,
                    messages_metadata_set,
                )?;
                message_size.size + compute_length_size(message_size.size)
            }
        }
        Some(Type::TYPE_GROUP) => {
            bail!("Groups are deprecated and not supported. Use nested messages instead.")
        }
        None => 0,
    };

    let repeated = get_repeated_count_for_field(field_size_metadata, message_name)?;
    field_size *= repeated;

    Ok(field_size)
}

/// Extracts the `variable_type_max_size` constraint for a field.
///
/// # Parameters
///
/// * `field_description` - Metadata for the field.
/// * `message_name` - The name of the message containing the field (for error reporting).
///
/// # Returns
///
/// The maximum size in bytes.
///
/// # Errors
///
/// Returns an error if the `variable_type_max_size` option is missing for a field
/// that requires it (string, bytes, or message without explicit size).
fn get_variable_type_max_size(
    field_description: &FieldSizeMetadata,
    message_name: &str,
) -> anyhow::Result<usize> {
    if let Some(variable_type_max_size_value) =
        field_description.size_constraint.clone().and_then(|c| c.variable_type_max_size)
    {
        Ok(variable_type_max_size_value)
    } else {
        anyhow::bail!(format!("sdv.vsidl.variable_type_max_size required option is missing on the variable length {:?} field in the following message {:?}", field_description.field_name, message_name));
    }
}

/// Extracts the `repeated_field_max_count` constraint for a field or defaults to 1.
///
/// # Parameters
///
/// * `field_description` - Metadata for the field.
/// * `message_name` - The name of the message containing the field (for error reporting).
///
/// # Returns
///
/// The maximum repetition count (1 if not repeated).
///
/// # Errors
///
/// Returns an error if the field is marked as `LABEL_REPEATED` but the
/// `repeated_field_max_count` option is missing.
fn get_repeated_count_for_field(
    field_description: &FieldSizeMetadata,
    message_name: &str,
) -> anyhow::Result<usize> {
    let field_label = field_description.label.and_then(|l| l.enum_value().ok());
    if let Some(Label::LABEL_REPEATED) = field_label {
        if let Some(repeated_max_count_value) =
            field_description.size_constraint.clone().and_then(|c| c.repeated_max_count)
        {
            return Ok(repeated_max_count_value);
        } else {
            anyhow::bail!(format!("sdv.vsidl.repeated_field_max_count required option is missing on the {:?} field, which is declared repeated in the following message {:?}", field_description.field_name, message_name));
        }
    }

    Ok(1)
}

/// Parses multiple [FileDescriptorProto] instances to build a [MessageSizeMetadataSet].
///
/// Iterates through all messages defined in the provided file descriptors, including nested messages,
/// and extracts the necessary metadata for size computation. It also handles parsing
/// referenced Protobuf well-known types.
///
/// # Parameters
///
/// * `proto_file_descriptors` - A vector of file descriptors to parse.
///
/// # Returns
///
/// The populated [MessageSizeMetadataSet].
///
/// # Errors
///
/// Returns an error if:
///   - A file descriptor is missing its package name.
///   - A message descriptor is missing its name.
///   - Parsing metadata for any field fails (see [parse_field_size_metadata]).
///   - Parsing a well-known type fails (see [parse_protobuf_well_known_type]).
pub fn get_message_size_metadata(
    proto_file_descriptors: &Vec<FileDescriptorProto>,
) -> anyhow::Result<MessageSizeMetadataSet> {
    let mut message_size_metadata = MessageSizeMetadataSet { messages: IndexMap::new() };
    for file in proto_file_descriptors {
        let package = file
            .package
            .clone()
            .ok_or(anyhow!(format!("Package is not set for {}", file.name())))?;
        for message in &file.message_type {
            let mut fully_qualified_name = vec![package.clone()];
            parse_message(message, &mut message_size_metadata, &mut fully_qualified_name)?;
        }
    }

    Ok(message_size_metadata)
}

/// Recursively parses a [DescriptorProto] and its nested messages to populate the [MessageSizeMetadataSet].
///
/// # Parameters
///
/// * `message_proto_descriptor` - The descriptor of the message to parse.
/// * `message_size_metadata_set` - The mutable set to add the parsed metadata to.
/// * `message_full_name` - A mutable vector representing the parts of the fully qualified name, used during recursion.
///
/// # Returns
///
/// An `Ok(())` on success.
///
/// # Errors
///
/// Returns an error if:
///   - The message descriptor is missing its name.
///   - Parsing metadata for any field fails (see [parse_field_size_metadata]).
///   - Parsing a well-known type referenced by a field fails (see [parse_protobuf_well_known_type]).
fn parse_message(
    message_proto_descriptor: &DescriptorProto,
    message_size_metadata_set: &mut MessageSizeMetadataSet,
    message_full_name: &mut Vec<String>,
) -> anyhow::Result<()> {
    let message_name =
        message_proto_descriptor.name.as_ref().ok_or(anyhow!("Message name is not defined"))?;
    message_full_name.push(message_name.to_string());
    let message_full_name_formatted = message_full_name.join(".");

    let mut fields = Vec::new();
    for field in &message_proto_descriptor.field {
        let field_metadata = parse_field_size_metadata(field)?;
        fields.push(field_metadata);
        parse_protobuf_well_known_type(field.type_name(), message_size_metadata_set)?;
    }

    let message_size_metadata = MessageSizeMetadata { name: message_name.to_string(), fields };
    message_size_metadata_set.messages.insert(message_full_name_formatted, message_size_metadata);

    for nested_message in &message_proto_descriptor.nested_type {
        parse_message(nested_message, message_size_metadata_set, message_full_name)?;
    }

    message_full_name.pop();

    Ok(())
}

/// Parses metadata for Protobuf well-known types if referenced.
///
/// Checks if the given `full_qualified_message_name` corresponds to a known well-known type.
/// If it is, and its metadata hasn't been parsed yet, it retrieves the descriptor and calls
/// [parse_message] to add its metadata to the set. This is necessary because descriptors for
/// well-known types might not be directly included in the input `proto_file_descriptors`.
///
/// # Parameters
///
/// * `full_qualified_message_name` - The fully qualified name (starting with '.') of the potential well-known type.
/// * `message_size_metadata_set` - The mutable set to potentially add the parsed metadata to.
///
/// # Returns
///
/// An `Ok(())` on success.
///
/// # Errors
///
/// Returns an error if the name corresponds to a well-known type but its descriptor cannot be found
/// or if parsing the well-known type's message descriptor fails.
fn parse_protobuf_well_known_type(
    full_qualified_message_name: &str,
    message_size_metadata_set: &mut MessageSizeMetadataSet,
) -> anyhow::Result<()> {
    if let Some(file_descriptor) = WELLKNOWN_TYPES_TO_DESCRIPTOR.get(full_qualified_message_name) {
        const PROTOBUF_VALUE: &str = ".google.protobuf.Value";
        const PROTOBUF_LISTVALUE: &str = ".google.protobuf.ListValue";
        // Skips when the type has already been parsed in which case the same type is used for multiple times.
        if message_size_metadata_set
            .messages
            .contains_key(full_qualified_message_name.get(1..).unwrap_or_default())
        {
            return Ok(());
        }

        // Skip Value and ListValue parsing since they are designed as recursive struct
        if full_qualified_message_name == PROTOBUF_LISTVALUE
            || full_qualified_message_name == PROTOBUF_VALUE
        {
            return Ok(());
        }

        if let Some(message_descriptor) =
            file_descriptor.message_by_full_name(full_qualified_message_name)
        {
            let mut package_name_vec = vec![file_descriptor.package().to_string()];
            return parse_message(
                message_descriptor.proto(),
                message_size_metadata_set,
                &mut package_name_vec,
            );
        } else {
            bail!("Failed to find message descriptor for {full_qualified_message_name}");
        }
    }

    Ok(())
}

/// Parses a single [FieldDescriptorProto] to extract [FieldSizeMetadata].
///
/// Extracts basic information (name, tag, type, label) and calls [get_size_constraint_for_field]
/// to parse any relevant size constraint options.
///
/// # Parameters
///
/// * `field` - The field descriptor to parse.
///
/// # Returns
///
/// A `Result` containing the extracted [FieldSizeMetadata] on success.
///
/// # Errors
///
/// Returns an error if the field is missing its name or tag number, or if parsing
/// the size constraints fails.
fn parse_field_size_metadata(field: &FieldDescriptorProto) -> anyhow::Result<FieldSizeMetadata> {
    let size_constraint = get_size_constraint_for_field(field)?;
    let field_size_description = FieldSizeMetadata {
        field_name: field.name.clone().ok_or(anyhow!("Field has no name"))?,
        tag_id: field.number.ok_or(anyhow!("Field has no tag"))?,
        type_: field.type_,
        label: field.label,
        type_name: field.type_name.clone(),
        size_constraint,
    };

    Ok(field_size_description)
}

/// Extracts size constraint options ([FieldOptions::repeated_field_max_count] and
/// [FieldOptions::variable_type_max_size]) from a field descriptor.
///
/// # Parameters
///
/// * `field` - The field descriptor to parse options from.
///
/// # Returns
///
/// An `Option<FieldSizeConstraint>`. It returns `Some` if at least
/// one of the relevant size constraint options is present, and `None` otherwise.
///
/// # Errors
///
/// Returns an error if accessing the field options fails (though this is unlikely with valid descriptors).
fn get_size_constraint_for_field(
    field: &FieldDescriptorProto,
) -> anyhow::Result<Option<FieldSizeConstraint>> {
    let repeated_max_count = field.repeated_field_max_count()?;
    let variable_type_max_size = field.variable_type_max_size()?;

    if repeated_max_count.is_some() || variable_type_max_size.is_some() {
        Ok(Some(FieldSizeConstraint { repeated_max_count, variable_type_max_size }))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use indexmap::IndexMap;
    use protobuf::descriptor::field_descriptor_proto::{Label, Type};
    use protobuf::descriptor::{
        DescriptorProto, FieldDescriptorProto, FieldOptions, FileDescriptorProto,
    };
    use protobuf::UnknownValue;
    use protobuf::{EnumOrUnknown, MessageField, SpecialFields};

    const FIELD_UNDER_TEST: &str = "field_under_test";
    const MESSAGE_UNDER_TEST: &str = "MessageUnderTest";
    const MESSAGE_FULL_NAME: &str = "p.MessageUnderTest";
    const NESTED_MESSAGE: &str = "NestedMessage";
    const NESTED_MESSAGE_FULL_NAME: &str = "p.MessageUnderTest.NestedMessage";
    const REFERENCED_MESSAGE: &str = "ReferencedMessage";
    const REFERENCED_MESSAGE_FULL_NAME: &str = "p.ReferencedMessage";
    const WELLKNOWN_COMPUTABLE: &str = ".google.protobuf.Empty";
    const WELLKNOWN_NON_COMPUTABLE: &str = ".google.protobuf.Any";
    const WELLKNOWN_PROTOBUF_VALUE: &str = ".google.protobuf.Value";
    const WELLKNOWN_PROTOBUF_LISTVALUE: &str = ".google.protobuf.ListValue";
    const MAX_SIZE_PROTOBUF_VALUE: usize = 37;
    const MAX_SIZE_PROTOBUF_LISTVALUE: usize = 56;
    const MAX_REPEATED: usize = 6;
    const FIELD_REPEATED: u32 = 50050;
    const MAX_BYTES: usize = 9;
    const FIELD_BYTES: u32 = 50051;

    #[test]
    fn test_get_message_size_metadata_empty_descriptor_succeeds() {
        let res = get_message_size_metadata(&vec![]);

        assert!(res.is_ok());
        assert!(res.unwrap().messages.is_empty());
    }

    #[test]
    fn test_get_message_size_metadata_int_no_limit_succeeds() {
        let int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, int_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert!(&res
            .messages
            .get(MESSAGE_FULL_NAME)
            .unwrap()
            .fields
            .first()
            .unwrap()
            .size_constraint
            .is_none());
    }

    #[test]
    fn test_get_message_size_metadata_int_with_limit_succeeds() {
        let int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let int_field = add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), int_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, int_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert!(&res
            .messages
            .get(MESSAGE_FULL_NAME)
            .unwrap()
            .fields
            .first()
            .unwrap()
            .size_constraint
            .is_none());
    }

    #[test]
    fn test_get_message_size_metadata_repeated_int_no_limit_succeeds() {
        let int_field = build_field_descriptor(Type::TYPE_INT32, true);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, int_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert!(&res
            .messages
            .get(MESSAGE_FULL_NAME)
            .unwrap()
            .fields
            .first()
            .unwrap()
            .size_constraint
            .is_none());
        assert_eq!(
            res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().label.unwrap(),
            EnumOrUnknown::new(Label::LABEL_REPEATED)
        );
    }

    #[test]
    fn test_get_message_size_metadata_repeated_int_count_limit_succeeds() {
        let int_field = build_field_descriptor(Type::TYPE_INT32, true);
        let int_field = add_size_limit_to_field(Some(MAX_REPEATED), None, int_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, int_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert_eq!(
            res.messages
                .get(MESSAGE_FULL_NAME)
                .unwrap()
                .fields
                .first()
                .unwrap()
                .size_constraint
                .as_ref()
                .unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: Some(MAX_REPEATED),
                variable_type_max_size: None
            }
        );
    }

    #[test]
    fn test_get_message_size_metadata_string_no_limit_succeeds() {
        let string_field = build_field_descriptor(Type::TYPE_STRING, false);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, string_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert!(&res
            .messages
            .get(MESSAGE_FULL_NAME)
            .unwrap()
            .fields
            .first()
            .unwrap()
            .size_constraint
            .is_none());
    }

    #[test]
    fn test_get_message_size_metadata_repeated_string_no_limit_succeeds() {
        let string_field = build_field_descriptor(Type::TYPE_STRING, true);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, string_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert!(&res
            .messages
            .get(MESSAGE_FULL_NAME)
            .unwrap()
            .fields
            .first()
            .unwrap()
            .size_constraint
            .is_none());
        assert_eq!(
            res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().label.unwrap(),
            EnumOrUnknown::new(Label::LABEL_REPEATED)
        );
    }

    #[test]
    fn test_get_message_size_metadata_string_byte_limit_succeeds() {
        let string_field = build_field_descriptor(Type::TYPE_STRING, false);
        let string_field = add_size_limit_to_field(None, Some(MAX_BYTES), string_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, string_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert_eq!(
            res.messages
                .get(MESSAGE_FULL_NAME)
                .unwrap()
                .fields
                .first()
                .unwrap()
                .size_constraint
                .as_ref()
                .unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_get_message_size_metadata_repeated_string_both_limits_succeeds() {
        let string_field = build_field_descriptor(Type::TYPE_STRING, true);
        let string_field =
            add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), string_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, string_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert_eq!(
            res.messages
                .get(MESSAGE_FULL_NAME)
                .unwrap()
                .fields
                .first()
                .unwrap()
                .size_constraint
                .as_ref()
                .unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: Some(MAX_REPEATED),
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_get_message_size_metadata_bytes_no_limit_succeeds() {
        let bytes_field = build_field_descriptor(Type::TYPE_BYTES, false);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, bytes_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert!(&res
            .messages
            .get(MESSAGE_FULL_NAME)
            .unwrap()
            .fields
            .first()
            .unwrap()
            .size_constraint
            .is_none());
    }

    #[test]
    fn test_get_message_size_metadata_repeated_bytes_no_limit_succeeds() {
        let bytes_field = build_field_descriptor(Type::TYPE_BYTES, true);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, bytes_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert!(&res
            .messages
            .get(MESSAGE_FULL_NAME)
            .unwrap()
            .fields
            .first()
            .unwrap()
            .size_constraint
            .is_none());
        assert_eq!(
            res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().label.unwrap(),
            EnumOrUnknown::new(Label::LABEL_REPEATED)
        );
    }

    #[test]
    fn test_get_message_size_metadata_bytes_byte_limit_succeeds() {
        let bytes_field = build_field_descriptor(Type::TYPE_BYTES, false);
        let bytes_field = add_size_limit_to_field(None, Some(MAX_BYTES), bytes_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, bytes_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert_eq!(
            res.messages
                .get(MESSAGE_FULL_NAME)
                .unwrap()
                .fields
                .first()
                .unwrap()
                .size_constraint
                .as_ref()
                .unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_get_message_size_metadata_repeated_bytes_both_limits_succeeds() {
        let bytes_field = build_field_descriptor(Type::TYPE_BYTES, true);
        let bytes_field = add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), bytes_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, bytes_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 1);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert_eq!(
            res.messages
                .get(MESSAGE_FULL_NAME)
                .unwrap()
                .fields
                .first()
                .unwrap()
                .size_constraint
                .as_ref()
                .unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: Some(MAX_REPEATED),
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_get_message_size_metadata_nested_message_no_limit_succeeds() {
        let child_int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let child_message = build_message_descriptor(NESTED_MESSAGE, child_int_field);
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(".") + NESTED_MESSAGE_FULL_NAME),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let mut message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        message.nested_type.push(child_message);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert!(&res.messages.contains_key(NESTED_MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert!(res
            .messages
            .get(MESSAGE_FULL_NAME)
            .unwrap()
            .fields
            .first()
            .unwrap()
            .size_constraint
            .as_ref()
            .is_none());
    }

    #[test]
    fn test_get_message_size_metadata_nested_message_with_limit_succeeds() {
        let child_int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let child_message = build_message_descriptor(NESTED_MESSAGE, child_int_field);
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(".") + NESTED_MESSAGE_FULL_NAME),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field =
            add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), message_field);
        let mut message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        message.nested_type.push(child_message);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert!(&res.messages.contains_key(NESTED_MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert_eq!(
            res.messages
                .get(MESSAGE_FULL_NAME)
                .unwrap()
                .fields
                .first()
                .unwrap()
                .size_constraint
                .as_ref()
                .unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_get_message_size_metadata_referenced_message_no_limit_succeeds() {
        let referenced_int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let referenced_message = build_message_descriptor(REFERENCED_MESSAGE, referenced_int_field);
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(".") + REFERENCED_MESSAGE_FULL_NAME),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![FileDescriptorProto {
            name: Some(String::from("f")),
            package: Some(String::from("p")),
            message_type: vec![message, referenced_message],
            ..Default::default()
        }];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert!(&res.messages.contains_key(REFERENCED_MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert!(res
            .messages
            .get(MESSAGE_FULL_NAME)
            .unwrap()
            .fields
            .first()
            .unwrap()
            .size_constraint
            .as_ref()
            .is_none());
    }

    #[test]
    fn test_get_message_size_metadata_referenced_message_with_limit_succeeds() {
        let referenced_int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let referenced_message = build_message_descriptor(REFERENCED_MESSAGE, referenced_int_field);
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(".") + REFERENCED_MESSAGE_FULL_NAME),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field =
            add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), message_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![FileDescriptorProto {
            name: Some(String::from("f")),
            package: Some(String::from("p")),
            message_type: vec![message, referenced_message],
            ..Default::default()
        }];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), 2);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert!(&res.messages.contains_key(REFERENCED_MESSAGE_FULL_NAME));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        assert_eq!(
            res.messages
                .get(MESSAGE_FULL_NAME)
                .unwrap()
                .fields
                .first()
                .unwrap()
                .size_constraint
                .as_ref()
                .unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_get_message_size_metadata_wellknown_non_computable_type_no_limit_succeeds() {
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(WELLKNOWN_NON_COMPUTABLE)),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];
        const EXPECTED_MESSAGES_COUNT: usize = 2;

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), EXPECTED_MESSAGES_COUNT);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert!(&res.messages.contains_key(WELLKNOWN_NON_COMPUTABLE.get(1..).unwrap()));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        // Asserts that there is no size constraint.
        assert!(res
            .messages
            .get(MESSAGE_FULL_NAME)
            .unwrap()
            .fields
            .first()
            .unwrap()
            .size_constraint
            .as_ref()
            .is_none());
    }

    #[test]
    fn test_get_message_size_metadata_wellknown_non_computable_type_with_limit_succeeds() {
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(WELLKNOWN_NON_COMPUTABLE)),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field =
            add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), message_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];
        const EXPECTED_MESSAGES_COUNT: usize = 2;

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), EXPECTED_MESSAGES_COUNT);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert!(&res.messages.contains_key(WELLKNOWN_NON_COMPUTABLE.get(1..).unwrap()));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        // Asserts that there is size constraint on the max bytes count.
        assert_eq!(
            res.messages
                .get(MESSAGE_FULL_NAME)
                .unwrap()
                .fields
                .first()
                .unwrap()
                .size_constraint
                .as_ref()
                .unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_get_message_size_metadata_wellknown_computable_type_no_limit_succeeds() {
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(WELLKNOWN_COMPUTABLE)),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];
        const EXPECTED_MESSAGES_COUNT: usize = 2;

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), EXPECTED_MESSAGES_COUNT);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert!(&res.messages.contains_key(WELLKNOWN_COMPUTABLE.get(1..).unwrap()));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
    }

    #[test]
    fn test_get_message_size_metadata_wellknown_computable_type_with_limit_succeeds() {
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(WELLKNOWN_COMPUTABLE)),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field =
            add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), message_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];
        const EXPECTED_MESSAGES_COUNT: usize = 2;

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.messages.len(), EXPECTED_MESSAGES_COUNT);
        assert!(&res.messages.contains_key(MESSAGE_FULL_NAME));
        assert!(&res.messages.contains_key(WELLKNOWN_COMPUTABLE.get(1..).unwrap()));
        assert_eq!(res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.len(), 1);
        assert_eq!(
            &res.messages.get(MESSAGE_FULL_NAME).unwrap().fields.first().unwrap().field_name,
            FIELD_UNDER_TEST
        );
        // Asserts that there is size constraint on the max bytes count.
        assert_eq!(
            res.messages
                .get(MESSAGE_FULL_NAME)
                .unwrap()
                .fields
                .first()
                .unwrap()
                .size_constraint
                .as_ref()
                .unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_get_message_size_metadata_none_message_name_fails() {
        let int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let message_without_name = DescriptorProto {
            name: None,
            ..build_message_descriptor(MESSAGE_UNDER_TEST, int_field)
        };
        let file_descriptor_set = vec![build_file_descriptor(message_without_name)];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_err());
        assert_eq!(format!("{}", res.unwrap_err()), "Message name is not defined");
    }

    #[test]
    fn test_get_message_size_metadata_empty_package_fails() {
        let file_descriptor_set = vec![build_file_descriptor_without_package()];

        let res = get_message_size_metadata(&file_descriptor_set);

        assert!(res.is_err());
        assert_eq!(format!("{}", res.unwrap_err()), "Package is not set for f");
    }

    #[test]
    fn test_compute_message_size_bool_no_limit_succeeds() {
        let bool_field = build_field_descriptor(Type::TYPE_BOOL, false);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, bool_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert!(&res.size_constraints.is_empty());
        assert_eq!(res.size, 2);
    }

    #[test]
    fn test_compute_message_size_uint32_no_limit_succeeds() {
        let bool_field = build_field_descriptor(Type::TYPE_UINT32, false);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, bool_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert!(&res.size_constraints.is_empty());
        assert_eq!(res.size, 6);
    }

    #[test]
    fn test_compute_message_size_sint32_no_limit_succeeds() {
        let bool_field = build_field_descriptor(Type::TYPE_SINT32, false);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, bool_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert!(&res.size_constraints.is_empty());
        assert_eq!(res.size, 6);
    }

    #[test]
    fn test_compute_message_size_int_no_limit_succeeds() {
        let int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, int_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert!(&res.size_constraints.is_empty());
        assert_eq!(res.size, 11);
    }

    #[test]
    fn test_compute_message_size_int_with_limit_succeeds_limit_ignored() {
        let int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let int_field = add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), int_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, int_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert!(&res.size_constraints.is_empty());
        assert_eq!(res.size, 11);
    }

    #[test]
    fn test_compute_message_size_repeated_int_with_limit_succeeds() {
        let int_field = build_field_descriptor(Type::TYPE_INT32, true);
        let int_field = add_size_limit_to_field(Some(MAX_REPEATED), None, int_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, int_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        let constraints = res.clone().size_constraints;
        assert_eq!(res.size, 11 * MAX_REPEATED);
        assert!(constraints.contains_key(FIELD_UNDER_TEST));
        assert_eq!(
            constraints.get(FIELD_UNDER_TEST).unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: Some(MAX_REPEATED),
                variable_type_max_size: None
            }
        );
    }

    #[test]
    fn test_compute_message_size_string_with_limit_succeeds() {
        let string_field = build_field_descriptor(Type::TYPE_STRING, false);
        let string_field = add_size_limit_to_field(None, Some(MAX_BYTES), string_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, string_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        let constraints = res.clone().size_constraints;
        assert_eq!(res.size, MAX_BYTES + 2);
        assert!(constraints.contains_key(FIELD_UNDER_TEST));
        assert_eq!(
            constraints.get(FIELD_UNDER_TEST).unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_compute_message_size_repeated_string_with_limits_succeeds() {
        let string_field = build_field_descriptor(Type::TYPE_STRING, true);
        let string_field =
            add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), string_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, string_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        let constraints = res.clone().size_constraints;
        assert_eq!(res.size, (MAX_BYTES + 2) * MAX_REPEATED);
        assert!(constraints.contains_key(FIELD_UNDER_TEST));
        assert_eq!(
            constraints.get(FIELD_UNDER_TEST).unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: Some(MAX_REPEATED),
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_compute_message_size_bytes_with_limit_succeeds() {
        let bytes_field = build_field_descriptor(Type::TYPE_BYTES, false);
        let bytes_field = add_size_limit_to_field(None, Some(MAX_BYTES), bytes_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, bytes_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        let constraints = res.clone().size_constraints;
        assert_eq!(res.size, MAX_BYTES + 2);
        assert!(constraints.contains_key(FIELD_UNDER_TEST));
        assert_eq!(
            constraints.get(FIELD_UNDER_TEST).unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_compute_message_size_repeated_bytes_with_limits_succeeds() {
        let bytes_field = build_field_descriptor(Type::TYPE_BYTES, true);
        let bytes_field = add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), bytes_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, bytes_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        let constraints = res.clone().size_constraints;
        assert_eq!(res.size, (MAX_BYTES + 2) * MAX_REPEATED);
        assert!(constraints.contains_key(FIELD_UNDER_TEST));
        assert_eq!(
            constraints.get(FIELD_UNDER_TEST).unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: Some(MAX_REPEATED),
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_compute_message_size_multiple_fields_succeeds() {
        let int_field = FieldDescriptorProto {
            name: Some(String::from("field1")),
            ..build_field_descriptor(Type::TYPE_INT32, false)
        };
        let bytes_field = FieldDescriptorProto {
            name: Some(String::from("field2")),
            ..build_field_descriptor(Type::TYPE_BYTES, false)
        };
        let bytes_field = add_size_limit_to_field(None, Some(MAX_BYTES), bytes_field);
        let bool_field = FieldDescriptorProto {
            name: Some(String::from("field3")),
            ..build_field_descriptor(Type::TYPE_BOOL, false)
        };
        let message = DescriptorProto {
            name: Some(String::from(MESSAGE_UNDER_TEST)),
            field: vec![int_field, bytes_field, bool_field],
            ..Default::default()
        };
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        assert_eq!(res.unwrap().size, 24);
    }

    #[test]
    fn test_compute_message_size_nested_message_no_limit_succeeds() {
        let child_int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let child_message = build_message_descriptor(NESTED_MESSAGE, child_int_field);
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(".") + NESTED_MESSAGE_FULL_NAME),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let mut message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        message.nested_type.push(child_message);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert!(&res.size_constraints.is_empty());
        assert_eq!(res.size, 13);
    }

    #[test]
    fn test_compute_message_size_nested_message_with_limit_succeeds() {
        let child_int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let child_message = build_message_descriptor(NESTED_MESSAGE, child_int_field);
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(".") + NESTED_MESSAGE_FULL_NAME),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field =
            add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), message_field);
        let mut message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        message.nested_type.push(child_message);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        let constraints = res.clone().size_constraints;
        assert_eq!(res.size, MAX_BYTES + 2);
        assert!(constraints.contains_key(FIELD_UNDER_TEST));
        assert_eq!(
            constraints.get(FIELD_UNDER_TEST).unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_compute_message_size_referenced_message_no_limit_succeeds() {
        let referenced_int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let referenced_message = build_message_descriptor(REFERENCED_MESSAGE, referenced_int_field);
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(".") + REFERENCED_MESSAGE_FULL_NAME),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![FileDescriptorProto {
            name: Some(String::from("f")),
            package: Some(String::from("p")),
            message_type: vec![message, referenced_message],
            ..Default::default()
        }];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert!(&res.size_constraints.is_empty());
        assert_eq!(res.size, 13);
    }

    #[test]
    fn test_compute_message_size_referenced_message_with_limit_succeeds() {
        let referenced_int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let referenced_message = build_message_descriptor(REFERENCED_MESSAGE, referenced_int_field);
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(".") + REFERENCED_MESSAGE_FULL_NAME),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field =
            add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), message_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![FileDescriptorProto {
            name: Some(String::from("f")),
            package: Some(String::from("p")),
            message_type: vec![message, referenced_message],
            ..Default::default()
        }];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        let constraints = res.clone().size_constraints;
        assert_eq!(res.size, MAX_BYTES + 2);
        assert!(constraints.contains_key(FIELD_UNDER_TEST));
        assert_eq!(
            constraints.get(FIELD_UNDER_TEST).unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_compute_message_size_wellknown_computable_type_no_limit_succeeds() {
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(WELLKNOWN_COMPUTABLE)),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];
        const EXPECTED_MESSAGE_SIZE: usize = 2;

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert!(&res.size_constraints.is_empty());
        assert_eq!(res.size, EXPECTED_MESSAGE_SIZE);
    }

    #[test]
    fn test_compute_message_size_wellknown_computable_type_with_limit_succeeds() {
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(WELLKNOWN_COMPUTABLE)),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field = add_size_limit_to_field(None, Some(MAX_BYTES), message_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        let constraints = res.clone().size_constraints;
        assert_eq!(res.size, MAX_BYTES + 2);
        assert!(constraints.contains_key(FIELD_UNDER_TEST));
        assert_eq!(
            constraints.get(FIELD_UNDER_TEST).unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_compute_message_size_wellknown_non_computable_type_with_limit_succeeds() {
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(WELLKNOWN_NON_COMPUTABLE)),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field = add_size_limit_to_field(None, Some(MAX_BYTES), message_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];
        const EXPECTED_MESSAGE_SIZE: usize = MAX_BYTES + 2;

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        let constraints = res.clone().size_constraints;
        assert_eq!(res.size, EXPECTED_MESSAGE_SIZE);
        assert!(constraints.contains_key(FIELD_UNDER_TEST));
        // Asserts that there is size constraint on max bytes count.
        assert_eq!(
            constraints.get(FIELD_UNDER_TEST).unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: None,
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_compute_message_size_wellknown_type_value_with_limit_succeeds() {
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(WELLKNOWN_PROTOBUF_VALUE)),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field =
            add_size_limit_to_field(None, Some(MAX_SIZE_PROTOBUF_VALUE), message_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];
        const EXPECTED_MESSAGE_SIZE: usize =
            MAX_SIZE_PROTOBUF_VALUE + /* header of message, which include length and tag size */ 2;

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.size, EXPECTED_MESSAGE_SIZE);
    }

    #[test]
    fn test_compute_message_size_wellknown_type_listvalue_with_limit_succeeds() {
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(WELLKNOWN_PROTOBUF_LISTVALUE)),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field =
            add_size_limit_to_field(None, Some(MAX_SIZE_PROTOBUF_LISTVALUE), message_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];
        const EXPECTED_MESSAGE_SIZE: usize = MAX_SIZE_PROTOBUF_LISTVALUE + /* header of message, which include length and tag size */ 2;

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.size, EXPECTED_MESSAGE_SIZE);
    }

    #[test]
    fn test_compute_message_size_repeated_wellknown_non_computable_type_with_limit_succeeds() {
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(WELLKNOWN_NON_COMPUTABLE)),
            ..build_field_descriptor(Type::TYPE_MESSAGE, true)
        };
        let message_field =
            add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), message_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];
        const EXPECTED_MESSAGE_SIZE: usize = (MAX_BYTES + 2) * MAX_REPEATED;

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_ok());
        let res = res.unwrap();
        let constraints = res.clone().size_constraints;
        assert_eq!(res.size, EXPECTED_MESSAGE_SIZE);
        assert!(constraints.contains_key(FIELD_UNDER_TEST));
        // Asserts that there is size constraint on both max repeated and bytes count.
        assert_eq!(
            constraints.get(FIELD_UNDER_TEST).unwrap(),
            &FieldSizeConstraint {
                repeated_max_count: Some(MAX_REPEATED),
                variable_type_max_size: Some(MAX_BYTES)
            }
        );
    }

    #[test]
    fn test_compute_message_size_message_not_calculated_fails() {
        let res = call_compute("m", &vec![]);

        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err())
            .contains("Failed to find message size metadata for \"m\" message"));
    }

    #[test]
    fn test_compute_message_size_repeated_int_no_limit_fails() {
        let int_field = build_field_descriptor(Type::TYPE_INT32, true);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, int_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains(
            "repeated_field_max_count required option is missing on the \"field_under_test\" field"
        ));
    }

    #[test]
    fn test_compute_message_size_string_no_limit_fails() {
        let string_field = build_field_descriptor(Type::TYPE_STRING, false);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, string_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains(
          "variable_type_max_size required option is missing on the variable length \"field_under_test\" field"
        ));
    }

    #[test]
    fn test_compute_message_size_repeated_string_no_limit_fails() {
        let string_field = build_field_descriptor(Type::TYPE_STRING, true);
        let string_field = add_size_limit_to_field(None, None, string_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, string_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains("variable_type_max_size required option is missing on the variable length \"field_under_test\""));
    }

    #[test]
    fn test_compute_message_size_bytes_no_limit_fails() {
        let bytes_field = build_field_descriptor(Type::TYPE_BYTES, false);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, bytes_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains(
            "variable_type_max_size required option is missing on the variable length \"field_under_test\" field"
        ));
    }

    #[test]
    fn test_compute_message_size_repeated_bytes_no_limit_fails() {
        let bytes_field = build_field_descriptor(Type::TYPE_BYTES, true);
        let bytes_field = add_size_limit_to_field(None, None, bytes_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, bytes_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains("variable_type_max_size required option is missing on the variable length \"field_under_test\""));
    }

    #[test]
    fn test_compute_message_size_referenced_message_not_specified_fails() {
        let referenced_int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let referenced_message = build_message_descriptor(REFERENCED_MESSAGE, referenced_int_field);
        let message_field = FieldDescriptorProto {
            type_name: None,
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field =
            add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), message_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![FileDescriptorProto {
            name: Some(String::from("f")),
            package: Some(String::from("p")),
            message_type: vec![message, referenced_message],
            ..Default::default()
        }];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains("Type name is not set"));
    }

    #[test]
    fn test_compute_message_size_referenced_message_invalid_fails() {
        let referenced_int_field = build_field_descriptor(Type::TYPE_INT32, false);
        let referenced_message = build_message_descriptor(REFERENCED_MESSAGE, referenced_int_field);
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from("without_dot_prefix")),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message_field =
            add_size_limit_to_field(Some(MAX_REPEATED), Some(MAX_BYTES), message_field);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![FileDescriptorProto {
            name: Some(String::from("f")),
            package: Some(String::from("p")),
            message_type: vec![message, referenced_message],
            ..Default::default()
        }];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err())
            .contains("Referenced message name is not fully qualified"));
    }

    #[test]
    fn test_compute_message_size_invalid_field_type_fails() {
        let group_field = build_field_descriptor(Type::TYPE_GROUP, true);
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, group_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains("Groups are deprecated"));
    }

    #[test]
    fn test_compute_message_size_wellknown_non_computable_type_no_limit_fails() {
        let message_field = FieldDescriptorProto {
            type_name: Some(String::from(WELLKNOWN_NON_COMPUTABLE)),
            ..build_field_descriptor(Type::TYPE_MESSAGE, false)
        };
        let message = build_message_descriptor(MESSAGE_UNDER_TEST, message_field);
        let file_descriptor_set = vec![build_file_descriptor(message)];

        let res = call_compute(MESSAGE_FULL_NAME, &file_descriptor_set);

        assert!(res.is_err());
        let err_msg = format!("{}", res.unwrap_err());
        assert!(err_msg.contains("variable_type_max_size required option is missing"));
        assert!(err_msg.contains("message \"google.protobuf.Any\""));
    }

    fn call_compute(
        message_name: &str,
        file_descriptor_set: &Vec<FileDescriptorProto>,
    ) -> anyhow::Result<MessageSize> {
        let message_size_metadata = get_message_size_metadata(file_descriptor_set);
        assert!(message_size_metadata.is_ok());

        let mut computed_sizes: IndexMap<String, MessageSize> = IndexMap::new();
        compute_message_size(message_name, &mut computed_sizes, &message_size_metadata.unwrap())
    }

    fn build_file_descriptor_without_package() -> FileDescriptorProto {
        FileDescriptorProto { name: Some(String::from("f")), ..Default::default() }
    }

    fn build_file_descriptor(message: DescriptorProto) -> FileDescriptorProto {
        FileDescriptorProto {
            name: Some(String::from("f")),
            package: Some(String::from("p")),
            message_type: vec![message],
            ..Default::default()
        }
    }

    fn build_message_descriptor(
        message_name: &str,
        field_descriptor: FieldDescriptorProto,
    ) -> DescriptorProto {
        DescriptorProto {
            name: Some(String::from(message_name)),
            field: vec![field_descriptor],
            ..Default::default()
        }
    }

    fn build_field_descriptor(field_type: Type, is_repeated: bool) -> FieldDescriptorProto {
        FieldDescriptorProto {
            name: Some(String::from(FIELD_UNDER_TEST)),
            number: Some(1),
            label: if is_repeated { Some(EnumOrUnknown::new(Label::LABEL_REPEATED)) } else { None },
            type_: Some(EnumOrUnknown::new(field_type)),
            ..Default::default()
        }
    }

    fn add_size_limit_to_field(
        repeated_max: Option<usize>,
        bytes_max: Option<usize>,
        original: FieldDescriptorProto,
    ) -> FieldDescriptorProto {
        let mut special_fields = SpecialFields::new();
        let unknown_fields = special_fields.mut_unknown_fields();
        if let Some(repeated) = repeated_max {
            unknown_fields
                .add_value(FIELD_REPEATED, UnknownValue::int32(repeated.try_into().unwrap()));
        }
        if let Some(bytes) = bytes_max {
            unknown_fields.add_value(FIELD_BYTES, UnknownValue::int32(bytes.try_into().unwrap()));
        }
        FieldDescriptorProto {
            options: MessageField::some(FieldOptions { special_fields, ..Default::default() }),
            ..original
        }
    }
}
