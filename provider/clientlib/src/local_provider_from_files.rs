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
use crate::file_utils::directory_files;
use crate::proto_schemas_data::ProtoSchemasData;
use crate::types::DataType;
use crate::vsidl_provider_api::*;
use log::warn;
use protobuf::reflect::MessageDescriptor;
use sdv_comms::id::ServiceFqin;
use sdv_comms::sd::UnitType;
use std::collections::HashSet;
use std::fmt;
use std::fmt::Display;
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;
use vsidl_v1::someip::SomeIpMapping;

pub(crate) struct LocalProviderFromFiles {
    pub(crate) proto_schemas: ProtoSchemasData,
}

impl LocalProviderFromFiles {
    pub fn new(vsidl_directory: &str, data_types: HashSet<DataType>) -> Self {
        Self {
            proto_schemas: if data_types.contains(&DataType::VsidlSchemas) {
                ProtoSchemasData::new(directory_files(vsidl_directory))
            } else {
                ProtoSchemasData::new(std::iter::empty())
            },
        }
    }
}

impl Display for LocalProviderFromFiles {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Proto Schemas:")?;
        writeln!(
            f,
            "  Topics: {} unit types",
            self.proto_schemas.topic_unit_type_to_proto_message.len()
        )?;
        for (unit_type, (message_name, size_metadata)) in
            &self.proto_schemas.topic_unit_type_to_proto_message
        {
            writeln!(
          f,
          "    - Package: {}, Bundle: {}, Type: {} → {}, size {}, count: {}, constraints: {:?}",
          unit_type.get_sdv_package_name(),
          unit_type.get_service_bundle_name(),
          unit_type.get_unit_type_name(),
          message_name,
          size_metadata.message_size,
          size_metadata.message_count,
          size_metadata.field_size_constraints
      )?;
        }
        writeln!(
            f,
            "  RPCs: {} unit types",
            self.proto_schemas.rpc_unit_type_to_proto_messages.len()
        )?;
        for (unit_type, methods) in &self.proto_schemas.rpc_unit_type_to_proto_messages {
            writeln!(
                f,
                "    - Package: {}, Bundle: {}, Type: {} ({} methods)",
                unit_type.get_sdv_package_name(),
                unit_type.get_service_bundle_name(),
                unit_type.get_unit_type_name(),
                methods.len()
            )?;
            for (method_name, (request_msg, response_msg)) in methods {
                writeln!(
                    f,
                    "      - {} (request: {}, response: {})",
                    method_name, request_msg, response_msg
                )?;
            }
        }
        writeln!(f, "  proto messages: {}", self.proto_schemas.proto_message_to_file_name.len())?;
        for (message_name, file_name) in &self.proto_schemas.proto_message_to_file_name {
            writeln!(f, "    - {} → {}", message_name, file_name)?;
        }
        writeln!(f, "  File names: {}", self.proto_schemas.file_name_to_file_descriptor.len())?;
        for (file_name, file_descriptor_proto) in &self.proto_schemas.file_name_to_file_descriptor {
            writeln!(f, "    - File: {}", file_name)?;
            writeln!(f, "      Package: {:?}", file_descriptor_proto.package)?;
            writeln!(f, "      Syntax: {:?}", file_descriptor_proto.syntax)?;
            writeln!(f, "      Dependencies: {}", file_descriptor_proto.dependency.len())?;
            for dep in &file_descriptor_proto.dependency {
                writeln!(f, "        - {}", dep)?;
            }
            writeln!(
                f,
                "      Public dependencies: {:?}",
                file_descriptor_proto.public_dependency
            )?;
            writeln!(f, "      Weak dependencies: {:?}", file_descriptor_proto.weak_dependency)?;
            writeln!(f, "      Message types: {}", file_descriptor_proto.message_type.len())?;
            for msg in &file_descriptor_proto.message_type {
                writeln!(f, "        - Message: {}", msg.name())?;
                writeln!(f, "          Fields: {}", msg.field.len())?;
                for field in &msg.field {
                    writeln!(
                        f,
                        "            - {} (#{}, type: {:?}, label: {:?})",
                        field.name(),
                        field.number(),
                        field.type_(),
                        field.label()
                    )?;
                }
                writeln!(f, "          Nested types: {}", msg.nested_type.len())?;
                for nested in &msg.nested_type {
                    writeln!(f, "            - {}", nested.name())?;
                }
                writeln!(f, "          Nested enums: {}", msg.enum_type.len())?;
                for nested_enum in &msg.enum_type {
                    writeln!(f, "            - {}", nested_enum.name())?;
                }
                writeln!(f, "          Oneofs: {}", msg.oneof_decl.len())?;
                for oneof in &msg.oneof_decl {
                    writeln!(f, "            - {}", oneof.name())?;
                }
                writeln!(f, "          Extensions: {}", msg.extension.len())?;
                writeln!(f, "          Extension ranges: {}", msg.extension_range.len())?;
                writeln!(f, "          Reserved ranges: {}", msg.reserved_range.len())?;
                writeln!(f, "          Reserved names: {:?}", msg.reserved_name)?;
            }
            writeln!(f, "      Enum types: {}", file_descriptor_proto.enum_type.len())?;
            for enum_type in &file_descriptor_proto.enum_type {
                writeln!(f, "        - {}", enum_type.name())?;
            }
            writeln!(f, "      Services: {}", file_descriptor_proto.service.len())?;
            for service in &file_descriptor_proto.service {
                writeln!(f, "        - {}", service.name())?;
            }
            writeln!(f, "      Extensions: {}", file_descriptor_proto.extension.len())?;
        }

        writeln!(f, "  File Descriptors: {}", self.proto_schemas.file_descriptor_cache.len())?;
        for ref_multi in &self.proto_schemas.file_descriptor_cache {
            writeln!(
                f,
                "    - Name: {}, Package: {}, Services: {}, Syntax: {:?}",
                ref_multi.key(),
                ref_multi.value().package(),
                ref_multi.value().services().count(),
                ref_multi.value().syntax()
            )?;
            writeln!(f, "      Messages:")?;
            for message in ref_multi.value().messages() {
                writeln!(f, "        - {}", message.full_name())?;
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl VsidlConfigProviderV1 for LocalProviderFromFiles {
    async fn get_publication_descriptor(
        &self,
        _source_fqin: &ServiceFqin,
        unit_type: &UnitType,
    ) -> Option<PublicationDescriptor> {
        self.proto_schemas.get_publication_descriptor(unit_type).ok()
    }

    async fn get_rpc_method_descriptor(
        &self,
        _source_fqin: &ServiceFqin,
        unit_type: &UnitType,
        method_name: &str,
    ) -> Option<RpcMethodDescriptor> {
        self.proto_schemas.get_rpc_method_descriptor(unit_type, method_name).ok()
    }

    async fn get_message_descriptor(
        &self,
        _source_fqin: &ServiceFqin,
        message_name: &str,
    ) -> Option<MessageDescriptor> {
        let full_message_name = &format!(".{message_name}");

        self.proto_schemas.get_message_descriptor(full_message_name).ok()
    }

    async fn get_someip_mappings(&self, _source_fqin: &ServiceFqin) -> Option<Vec<SomeIpMapping>> {
        warn!("Unexpected call to get someip mappings from file source");
        None
    }

    async fn get_diagnostics_declaration(
        &self,
        _fqin: &ServiceFqin,
    ) -> Option<DiagnosticsDeclaration> {
        warn!("Unexpected call to get diagnostics declaration from file source");
        None
    }
}

#[cfg(test)]
mod test {
    use super::LocalProviderFromFiles;
    use sdv_comms::ContextRef;
    use std::collections::HashSet;
    use std::future::Future;

    use crate::test_utils::test_fixture_vsidl_init_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_api_fails;
    use crate::test_utils::test_fixture_vsidl_provider_diagnostics_api_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_message_descriptor_api_fails;
    use crate::test_utils::test_fixture_vsidl_provider_message_descriptor_api_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_pubsub_metadata_api_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_rpc_descriptor_api_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_with_import_catalog_dependencies_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_with_import_well_known_dependencies_succeeds;
    use crate::types::DataType;

    async fn run_test<F, Fut>(init_dir: &str, test_function: F)
    where
        F: Fn(LocalProviderFromFiles, Option<ContextRef>, HashSet<DataType>) -> Fut,
        Fut: Future<Output = ()>,
    {
        // Only VsidlSchemas DataType can be provided from files
        let data_types: Vec<HashSet<DataType>> =
            vec![HashSet::from([DataType::VsidlSchemas]), HashSet::new()];

        for data_type in data_types {
            //WHEN provider is called created with folder containing both valid and invalid runtime config files.
            let provider = LocalProviderFromFiles::new(init_dir, data_type.clone());

            // THEN test function assertions are satisfied.
            test_function(provider, None, data_type).await;
        }
    }

    #[tokio::test]
    async fn test_vsidl_provider_pubsub_metadata_api_succeeds() {
        let init_dir = ".";
        run_test(init_dir, test_fixture_vsidl_provider_pubsub_metadata_api_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_rpc_descriptor_api_succeeds() {
        let init_dir = ".";
        run_test(init_dir, test_fixture_vsidl_provider_rpc_descriptor_api_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_api_fails() {
        let init_dir = ".";
        run_test(init_dir, test_fixture_vsidl_provider_api_fails).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_with_import_catalog_dependencies_succeeds() {
        let init_dir = ".";
        run_test(init_dir, test_fixture_vsidl_provider_with_import_catalog_dependencies_succeeds)
            .await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_with_import_well_known_dependencies_succeeds() {
        let init_dir = ".";
        run_test(
            init_dir,
            test_fixture_vsidl_provider_with_import_well_known_dependencies_succeeds,
        )
        .await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_message_descriptor_api_succeeds() {
        let init_dir = ".";
        run_test(init_dir, test_fixture_vsidl_provider_message_descriptor_api_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_message_descriptor_api_fails() {
        let init_dir = ".";
        run_test(init_dir, test_fixture_vsidl_provider_message_descriptor_api_fails).await;
    }

    #[tokio::test]
    async fn test_vsidl_init_succeeds() {
        let init_dir = ".";
        run_test(init_dir, test_fixture_vsidl_init_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_diagnostics_api_succeeds() {
        let init_dir = ".";
        run_test(init_dir, test_fixture_vsidl_provider_diagnostics_api_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_init_fails_to_parse_proto() {
        let init_dir = "test_data/v1/invalid";
        run_test(init_dir, test_fixture_vsidl_provider_empty_proto_schemas).await;
    }

    async fn test_fixture_vsidl_provider_empty_proto_schemas(
        provider: LocalProviderFromFiles,
        _ctx: Option<ContextRef>,
        _data_types: HashSet<DataType>,
    ) {
        assert!(provider.proto_schemas.topic_unit_type_to_proto_message.is_empty());
        assert!(provider.proto_schemas.rpc_unit_type_to_proto_messages.is_empty());
        assert!(provider.proto_schemas.proto_message_to_file_name.is_empty());
        assert!(provider.proto_schemas.file_name_to_file_descriptor.is_empty());
        assert!(provider.proto_schemas.file_descriptor_cache.is_empty());
    }
}
