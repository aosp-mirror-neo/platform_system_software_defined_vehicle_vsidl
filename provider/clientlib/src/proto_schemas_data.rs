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

use crate::cache::VsidlFdCache;
use crate::{file_utils::read_config_file, vsidl_provider_api::*};
use dashmap::DashMap;
use log::{error, warn};
use protobuf::descriptor::FileDescriptorProto;
use protobuf::reflect::FileDescriptor;
use protobuf::reflect::MessageDescriptor;
use sdv_comms::sd::UnitType;
use sdv_vsidl_common::{WELLKNOWN_PROTOS_TO_DESCRIPTOR, WELLKNOWN_TYPES_TO_DESCRIPTOR};
use std::collections::HashMap;
use std::path::PathBuf;
use vsidl_v1::runtime_config::vsidl_runtime_config::unit_type_descriptor_mapping::Archetype_metadata;
use vsidl_v1::runtime_config::vsidl_runtime_config::{RpcMetadata, TopicMetadata};
use vsidl_v1::runtime_config::VsidlRuntimeConfig;

const VSIDL_SYNTAX_PROTO: &str = "sdv/vsidl/v1/syntax.proto";
const VSIDL_ANNOTATIONS_PROTO: &str = "sdv/vsidl/v1/annotations.proto";
const DIAG_SYNTAX_PROTO: &str = "sdv/diagnostics/v1/diagnostics_syntax.proto";
const DIAG_ANNOTATIONS_PROTO: &str = "sdv/diagnostics/v1/diagnostics.proto";

/// Proto schemas data.
#[derive(Debug, Default)]
pub(crate) struct ProtoSchemasData {
    pub topic_unit_type_to_proto_message: HashMap<UnitType, (String, SizeMetadata)>,
    pub rpc_unit_type_to_proto_messages: HashMap<UnitType, HashMap<String, (String, String)>>,
    pub proto_message_to_file_name: HashMap<String, String>,
    pub file_name_to_file_descriptor: HashMap<String, FileDescriptorProto>,
    pub file_descriptor_cache: DashMap<String, FileDescriptor>,
}

impl ProtoSchemasData {
    pub fn new(input_files: impl Iterator<Item = PathBuf>) -> Self {
        let mut result = Self {
            topic_unit_type_to_proto_message: HashMap::new(),
            rpc_unit_type_to_proto_messages: HashMap::new(),
            proto_message_to_file_name: HashMap::new(),
            file_name_to_file_descriptor: HashMap::new(),
            file_descriptor_cache: DashMap::new(),
        };

        let mut vsidl_runtime_config = VsidlRuntimeConfig::default();

        for input_file in input_files {
            let config = read_config_file::<VsidlRuntimeConfig, _>(&input_file);
            let Ok(mut config) = config else {
                warn!("Unable to read config file {input_file:?}: {config:?}");
                continue;
            };

            vsidl_runtime_config.unit_type_mapping.append(&mut config.unit_type_mapping);
            vsidl_runtime_config.descriptor_mapping.append(&mut config.descriptor_mapping);
            vsidl_runtime_config.file_descriptor_proto.append(&mut config.file_descriptor_proto);
        }

        for file_descriptor_proto in &vsidl_runtime_config.file_descriptor_proto {
            let Some(file_name) = file_descriptor_proto.name.clone() else {
                warn!("Name is not found for file {file_descriptor_proto:?}.");
                continue;
            };

            result.file_name_to_file_descriptor.insert(file_name, file_descriptor_proto.clone());
        }

        for unit_type_mapping in &vsidl_runtime_config.unit_type_mapping {
            let Ok(unit_type) = UnitType::builder()
                .set_sdv_package_name(&unit_type_mapping.package_name)
                .set_service_bundle_name(&unit_type_mapping.service_bundle_name)
                .set_unit_type_name(&unit_type_mapping.type_name)
                .build()
            else {
                log::error!("Unable to build Unit Type from unit type mapping! Package: {}, Service Bundle Name: {}, Unit Type Name: {}", &unit_type_mapping.package_name, &unit_type_mapping.service_bundle_name, &unit_type_mapping.type_name);
                continue;
            };

            match &unit_type_mapping.archetype_metadata {
                Some(Archetype_metadata::TopicMetadata(topic_metadata)) => {
                    result.topic_unit_type_to_proto_message.insert(
                        unit_type,
                        (
                            format!(".{}", topic_metadata.message_name.clone()),
                            get_size_metadata(topic_metadata),
                        ),
                    );
                }
                Some(Archetype_metadata::RpcMetadata(rpc_metadata)) => {
                    result
                        .rpc_unit_type_to_proto_messages
                        .insert(unit_type, get_unit_type_rpc_map(rpc_metadata));
                }
                _ => {
                    // log::error!("Unknown metadata for unit type {unit_type:?}");
                }
            }
        }

        for message in &vsidl_runtime_config.descriptor_mapping {
            if result.file_name_to_file_descriptor.contains_key(&message.file_name) {
                result.proto_message_to_file_name.insert(
                    format!(".{}", message.message_name.clone()),
                    message.file_name.clone(),
                );
            } else {
                warn!(
                    "One of the proto messages does not have a valid descriptor proto {:?}",
                    message.message_name.clone()
                );
            }
        }

        result
    }

    pub fn get_publication_descriptor(
        &self,
        unit_type: &UnitType,
    ) -> Option<PublicationDescriptor> {
        let (message, size_metadata) = self.topic_unit_type_to_proto_message.get(unit_type)?;

        let message_descriptor = self.get_message_descriptor(message)?;
        Some(PublicationDescriptor { message_descriptor, size_metadata: size_metadata.clone() })
    }

    pub fn get_rpc_method_descriptor(
        &self,
        unit_type: &UnitType,
        method_name: &str,
    ) -> Option<RpcMethodDescriptor> {
        let rpc_unit_type_to_proto_messages = &self.rpc_unit_type_to_proto_messages;

        let Some(method_map) = rpc_unit_type_to_proto_messages.get(unit_type) else {
            log::error!("Could not find message for RPC interface {unit_type}");
            return None;
        };

        let Some((input_message_name, output_message_name)) = method_map.get(method_name) else {
            log::error!("Could not find method name {method_name} in {unit_type}");
            return None;
        };
        let request_message = self.get_message_descriptor(input_message_name);
        let response_message = self.get_message_descriptor(output_message_name);
        Some(RpcMethodDescriptor { request_message, response_message })
    }

    pub fn get_message_descriptor(&self, message: &str) -> Option<MessageDescriptor> {
        let file_descriptor = if let Some(file_name) = self.proto_message_to_file_name.get(message)
        {
            self.get_file_descriptor(file_name)?
        } else {
            // don't immediately return error, could be a well-known type
            WELLKNOWN_TYPES_TO_DESCRIPTOR.get(message)?.clone()
        };

        file_descriptor.message_by_full_name(message)
    }

    /// Gets a file descriptor for the given file_name, or creates if it does not exist in the cache.
    /// NOTE: A race condition may occur when a request to a non-cached FileDescriptor is made from
    /// multiple threads. This may result in each thread creating its own FileDescriptor object, but
    /// subsequent calls to `get_file_descriptor` will always return the same object.
    /// Since FileDescriptor objects are checked for equality using pointer comparison, one should not
    /// rely on MessageDescriptors created from different threads to be equal (However, each MessageDescriptor will always be
    /// valid for reflective access to proto messages)
    fn get_file_descriptor(&self, file_name: &str) -> Option<FileDescriptor> {
        if !self.file_descriptor_cache.contains_file_descriptor_with_key(file_name) {
            let Some(file_descriptor_proto) = self.file_name_to_file_descriptor.get(file_name)
            else {
                error!("Could not find file descriptor proto for file {file_name:?}");
                return None;
            };
            //TODO try to add a match with if guards instead of this
            let mut fd_dependencies: Vec<FileDescriptor> = Vec::new();
            for dependency in &file_descriptor_proto.dependency {
                if dependency == VSIDL_SYNTAX_PROTO {
                    fd_dependencies.push(vsidl_v1::syntax::file_descriptor().clone());
                } else if dependency == VSIDL_ANNOTATIONS_PROTO {
                    fd_dependencies.push(vsidl_v1::annotations::file_descriptor().clone());
                } else if dependency == DIAG_SYNTAX_PROTO {
                    fd_dependencies.push(vsidl_v1::diagnostics_syntax::file_descriptor().clone());
                } else if dependency == DIAG_ANNOTATIONS_PROTO {
                    fd_dependencies.push(vsidl_v1::diagnostics::file_descriptor().clone());
                } else if let Some(file_descriptor) = WELLKNOWN_PROTOS_TO_DESCRIPTOR.get(dependency)
                {
                    fd_dependencies.push(file_descriptor.clone())
                } else {
                    fd_dependencies.push(self.get_file_descriptor(dependency)?);
                }
            }
            let Ok(file_descriptor) =
                FileDescriptor::new_dynamic(file_descriptor_proto.clone(), &fd_dependencies)
            else {
                error!("Unable to create file descriptor for file {file_name:?}");
                return None;
            };
            self.file_descriptor_cache.insert_into_cache(file_name.to_string(), file_descriptor);
        }

        self.file_descriptor_cache.get_file_descriptor(file_name).ok()
    }

    /// Returns true if this data object contains no schema mappings.
    /// This typically happens if the factory was initialized without a VSIDL file.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.topic_unit_type_to_proto_message.is_empty()
            && self.rpc_unit_type_to_proto_messages.is_empty()
            && self.proto_message_to_file_name.is_empty()
            && self.file_name_to_file_descriptor.is_empty()
    }
}

fn get_unit_type_rpc_map(rpc_metadata: &RpcMetadata) -> HashMap<String, (String, String)> {
    let mut unit_type_rpc_map = HashMap::new();
    for rpc_method_mapping in &rpc_metadata.method_mapping {
        unit_type_rpc_map.insert(
            rpc_method_mapping.method_name.clone(),
            (
                format!(".{}", rpc_method_mapping.argument_message_name.clone()),
                format!(".{}", rpc_method_mapping.return_message_name.clone()),
            ),
        );
    }
    unit_type_rpc_map
}

fn get_size_metadata(topic_metadata: &TopicMetadata) -> SizeMetadata {
    let mut field_size_constraints = HashMap::new();
    for (field_name, field_size_constraint) in &topic_metadata.field_size_constraint {
        let mut fzc = FieldSizeConstraint::default();
        if field_size_constraint.repeated_max_count != 0 {
            fzc.repeated_max_count = Some(field_size_constraint.repeated_max_count);
        }
        if field_size_constraint.variable_type_max_size != 0 {
            fzc.variable_type_max_size = Some(field_size_constraint.variable_type_max_size);
        }
        if fzc.repeated_max_count.is_some() || fzc.variable_type_max_size.is_some() {
            field_size_constraints.insert(field_name.clone(), fzc);
        }
    }

    SizeMetadata {
        message_size: topic_metadata.message_size,
        message_count: topic_metadata.message_count,
        field_size_constraints,
    }
}
