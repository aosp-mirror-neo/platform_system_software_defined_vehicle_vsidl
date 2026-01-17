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
//! This crate houses all behavior and/or structs that should be shared between VSIDL and
//! other automotive components

use lazy_static::lazy_static;
use protobuf::reflect::FileDescriptor;
use std::collections::HashMap;

pub mod field_options;
pub mod proto_size_utils;

lazy_static! {
    /// Protobuf well-known types are the pre-defined types by Google, see https://protobuf.dev/reference/protobuf/google.protobuf/.
    /// The values in the map are used by Rust only and correspond to the generated structs by `rust-protobuf` from `google/protobuf/`. See https://github.com/stepancheg/rust-protobuf/tree/7ad294f48110d530f341ec5f70f1cc8e56f20462/protobuf/src/well_known_types for the generated Rust structs.
    pub static ref WELLKNOWN_TYPES_TO_DESCRIPTOR: HashMap<String, FileDescriptor> = HashMap::from([
        (String::from(".google.protobuf.Any"), protobuf::well_known_types::any::file_descriptor().clone()),
        (String::from(".google.protobuf.Api"), protobuf::well_known_types::api::file_descriptor().clone()),
        (String::from(".google.protobuf.Method"), protobuf::well_known_types::api::file_descriptor().clone()),
        (String::from(".google.protobuf.Mixin"), protobuf::well_known_types::api::file_descriptor().clone()),
        (String::from(".google.protobuf.Duration"), protobuf::well_known_types::duration::file_descriptor().clone()),
        (String::from(".google.protobuf.Empty"), protobuf::well_known_types::empty::file_descriptor().clone()),
        (String::from(".google.protobuf.FieldMask"), protobuf::well_known_types::field_mask::file_descriptor().clone()),
        (String::from(".google.protobuf.SourceContext"), protobuf::well_known_types::source_context::file_descriptor().clone()),
        (String::from(".google.protobuf.Struct"), protobuf::well_known_types::struct_::file_descriptor().clone()),
        (String::from(".google.protobuf.Value"), protobuf::well_known_types::struct_::file_descriptor().clone()),
        (String::from(".google.protobuf.ListValue"), protobuf::well_known_types::struct_::file_descriptor().clone()),
        (String::from(".google.protobuf.Timestamp"), protobuf::well_known_types::timestamp::file_descriptor().clone()),
        (String::from(".google.protobuf.Type"), protobuf::well_known_types::type_::file_descriptor().clone()),
        (String::from(".google.protobuf.Field"), protobuf::well_known_types::type_::file_descriptor().clone()),
        (String::from(".google.protobuf.Enum"), protobuf::well_known_types::type_::file_descriptor().clone()),
        (String::from(".google.protobuf.EnumValue"), protobuf::well_known_types::type_::file_descriptor().clone()),
        (String::from(".google.protobuf.Option"), protobuf::well_known_types::type_::file_descriptor().clone()),
        (String::from(".google.protobuf.DoubleValue"), protobuf::well_known_types::wrappers::file_descriptor().clone()),
        (String::from(".google.protobuf.FloatValue"), protobuf::well_known_types::wrappers::file_descriptor().clone()),
        (String::from(".google.protobuf.Int64Value"), protobuf::well_known_types::wrappers::file_descriptor().clone()),
        (String::from(".google.protobuf.UInt64Value"), protobuf::well_known_types::wrappers::file_descriptor().clone()),
        (String::from(".google.protobuf.Int32Value"), protobuf::well_known_types::wrappers::file_descriptor().clone()),
        (String::from(".google.protobuf.UInt32Value"), protobuf::well_known_types::wrappers::file_descriptor().clone()),
        (String::from(".google.protobuf.BoolValue"), protobuf::well_known_types::wrappers::file_descriptor().clone()),
        (String::from(".google.protobuf.StringValue"), protobuf::well_known_types::wrappers::file_descriptor().clone()),
        (String::from(".google.protobuf.BytesValue"), protobuf::well_known_types::wrappers::file_descriptor().clone()),
        ]);

    /// Index the file descriptors above by their proto name
    pub static ref WELLKNOWN_PROTOS_TO_DESCRIPTOR: HashMap<String, FileDescriptor> = {
        let mut proto_to_fds = HashMap::new();
        for file_descriptor in WELLKNOWN_TYPES_TO_DESCRIPTOR.values() {
            proto_to_fds.insert(file_descriptor.name().to_string(), file_descriptor.clone());
        }

        // In addition to the MW supported protos, add `descriptor.proto`
        proto_to_fds.insert(String::from("google/protobuf/descriptor.proto"), protobuf::descriptor::file_descriptor().clone());
        proto_to_fds
    };
}
