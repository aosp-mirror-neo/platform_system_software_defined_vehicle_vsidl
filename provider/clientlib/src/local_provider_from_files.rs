// Copyright 2024 Google LLC
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
        write!(f, "proto_schemas: {:?}", self.proto_schemas)
    }
}

#[async_trait::async_trait]
impl VsidlConfigProviderV1 for LocalProviderFromFiles {
    async fn get_publication_descriptor(
        &self,
        _source_fqin: &ServiceFqin,
        unit_type: &UnitType,
    ) -> Option<PublicationDescriptor> {
        self.proto_schemas.get_publication_descriptor(unit_type)
    }

    async fn get_rpc_method_descriptor(
        &self,
        _source_fqin: &ServiceFqin,
        unit_type: &UnitType,
        method_name: &str,
    ) -> Option<RpcMethodDescriptor> {
        self.proto_schemas.get_rpc_method_descriptor(unit_type, method_name)
    }

    async fn get_message_descriptor(
        &self,
        _source_fqin: &ServiceFqin,
        message_name: &str,
    ) -> Option<MessageDescriptor> {
        let full_message_name = &format!(".{message_name}");

        self.proto_schemas.get_message_descriptor(full_message_name)
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
