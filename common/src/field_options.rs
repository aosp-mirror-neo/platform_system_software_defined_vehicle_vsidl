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

//! Provides utility functions to retrieve field options defined by VSIDL.
use protobuf::descriptor::{
    field_descriptor_proto::{Label, Type},
    FieldDescriptorProto,
};
use protobuf::reflect::FieldDescriptor;
use protobuf::UnknownValueRef::{self, *};
use std::num::TryFromIntError;
use std::result::Result;

/// Trait that allows to retrieve protobuf field options.
pub trait FieldOptions {
    /// Retrieve sdv.vsidl.repeated_field_max_count option.
    fn repeated_field_max_count(&self) -> Result<Option<usize>, TryFromIntError>;

    /// Retrieve sdv.vsidl.variable_type_max_size option.
    fn variable_type_max_size(&self) -> Result<Option<usize>, TryFromIntError>;
}

/// Helper function for retrieving `usize` value of a field option by its id.
fn get_size_option(
    field_descriptor: &FieldDescriptorProto,
    option_id: u32,
) -> Result<Option<usize>, TryFromIntError> {
    if let Some(value) = get_varint_option(field_descriptor, option_id) {
        value.try_into().map(Some)
    } else {
        Ok(None)
    }
}

/// Helper function for retrieving `u64` value of a field option by its id.
fn get_varint_option(field_descriptor: &FieldDescriptorProto, option_id: u32) -> Option<u64> {
    get_option_by_id(field_descriptor, option_id).and_then(|unknown_value| {
        if let Varint(value) = unknown_value {
            Some(value)
        } else {
            None
        }
    })
}

/// Helper function for retrieving option value as an `UnknownValueRef`.
fn get_option_by_id(
    field_descriptor: &FieldDescriptorProto,
    option_id: u32,
) -> Option<UnknownValueRef> {
    field_descriptor.options.special_fields.unknown_fields().get(option_id)
}

impl FieldOptions for FieldDescriptor {
    fn repeated_field_max_count(&self) -> Result<Option<usize>, TryFromIntError> {
        self.proto().repeated_field_max_count()
    }

    fn variable_type_max_size(&self) -> Result<Option<usize>, TryFromIntError> {
        self.proto().variable_type_max_size()
    }
}

impl FieldOptions for FieldDescriptorProto {
    fn repeated_field_max_count(&self) -> Result<Option<usize>, TryFromIntError> {
        let field_label = self.label.and_then(|l| l.enum_value().ok());

        if let Some(Label::LABEL_REPEATED) = field_label {
            // id of sdv.vsidl.repeated_field_max_count field option
            const REPEATED_FIELD_MAX_COUNT: u32 = 50050;
            get_size_option(self, REPEATED_FIELD_MAX_COUNT)
        } else {
            Ok(None)
        }
    }

    fn variable_type_max_size(&self) -> Result<Option<usize>, TryFromIntError> {
        let field_type = self.type_.and_then(|l| l.enum_value().ok());

        if let Some(Type::TYPE_STRING) | Some(Type::TYPE_BYTES) | Some(Type::TYPE_MESSAGE) =
            field_type
        {
            // id of sdv.vsidl.variable_type_max_size field option
            const VARIABLE_TYPE_MAX_SIZE: u32 = 50051;
            get_size_option(self, VARIABLE_TYPE_MAX_SIZE)
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protobuf::MessageFull;
    use sdv_vsidl_common_test_protos::test_messages::{WithAnnotations, WithoutAnnotations};

    #[test]
    fn test_repeated_field_max_count() {
        // Annotated Repeated Field
        let field_descriptor = WithAnnotations::descriptor().field_by_name("numbers").unwrap();
        assert_eq!(field_descriptor.repeated_field_max_count().unwrap(), Some(319));

        // Unannotated Repeated Field
        let field_descriptor = WithoutAnnotations::descriptor().field_by_name("numbers").unwrap();
        assert_eq!(field_descriptor.repeated_field_max_count().unwrap(), None);

        // Annotated Non-Repeated Field
        let field_descriptor = WithAnnotations::descriptor().field_by_name("single_num").unwrap();
        assert_eq!(field_descriptor.repeated_field_max_count().unwrap(), None);

        // Unannotated Non-Repeated Field
        let field_descriptor =
            WithoutAnnotations::descriptor().field_by_name("single_num").unwrap();
        assert_eq!(field_descriptor.repeated_field_max_count().unwrap(), None);

        // Wrongly Annotated Repeated Field
        let field_descriptor =
            WithAnnotations::descriptor().field_by_name("numbers_wrong_annotation").unwrap();
        assert_eq!(field_descriptor.repeated_field_max_count().unwrap(), None);
    }

    #[test]
    fn test_variable_type_max_size() {
        // Annotated Variable Size Field
        let field_descriptor = WithAnnotations::descriptor().field_by_name("data").unwrap();
        assert_eq!(field_descriptor.variable_type_max_size().unwrap(), Some(952));

        // Unannotated Variable Size Field
        let field_descriptor = WithoutAnnotations::descriptor().field_by_name("data").unwrap();
        assert_eq!(field_descriptor.variable_type_max_size().unwrap(), None);

        // Annotated Non-Variable Size Field
        let field_descriptor = WithAnnotations::descriptor().field_by_name("single_num").unwrap();
        assert_eq!(field_descriptor.variable_type_max_size().unwrap(), None);

        // Unannotated Non-Variable Size Field
        let field_descriptor =
            WithoutAnnotations::descriptor().field_by_name("single_num").unwrap();
        assert_eq!(field_descriptor.variable_type_max_size().unwrap(), None);

        // Wrongly Annotated Variable Size Field
        let field_descriptor =
            WithAnnotations::descriptor().field_by_name("data_wrong_annotation").unwrap();
        assert_eq!(field_descriptor.variable_type_max_size().unwrap(), None);
    }
}
