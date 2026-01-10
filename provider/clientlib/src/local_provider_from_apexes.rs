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

use crate::bundle_metadata::{BundleMetadata, BundleMetadataFactory};
use crate::types::DataType;
use crate::types::ServiceFqbn;
use crate::vsidl_provider_api::*;
use google_sdv_service_bundles_registry::aidl::google::sdv::service_bundles_registry::IRegistry::{
    BpRegistry as BpServiceBundlesRegistry, IRegistry as IServiceBundlesRegistry,
};
use google_sdv_service_bundles_registry::binder;
use log::warn;
use protobuf::reflect::MessageDescriptor;
use sdv::status::SdvResult;
use sdv_comms::id::ServiceFqin;
use sdv_comms::sd::UnitType;
use sdv_status_rs::SdvStatus;
use sdv_status_rs::SdvStatusCode;
use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::fmt;
use std::fmt::Display;
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;
use vsidl_v1::someip::SomeIpMapping;

pub struct LocalProviderFromApexes {
    bundle_metadata: HashMap<ServiceFqbn<'static>, BundleMetadata>,
}

impl LocalProviderFromApexes {
    pub fn new(data_types: HashSet<DataType>) -> SdvResult<Self> {
        Ok(Self { bundle_metadata: Self::get_service_bundles_registry_metadata(data_types)? })
    }

    /// Gets service bundles registry handler.
    fn get_service_bundles_registry() -> SdvResult<binder::Strong<dyn IServiceBundlesRegistry>> {
        let descriptor = <BpServiceBundlesRegistry as IServiceBundlesRegistry>::get_descriptor()
            .to_owned()
            + "/default";
        binder::wait_for_interface::<dyn IServiceBundlesRegistry>(&descriptor).map_err(|_| {
            SdvStatus::with_message(
                SdvStatusCode::DeadlineExceeded,
                "Failed to find interface ServiceBundlesRegistry",
            )
        })
    }

    /// Scans the service bundles registry and returns file paths in the apexes that contain service bundles for somip, diagnostics and vsidl configs respectively.
    fn get_service_bundles_registry_metadata(
        data_types: HashSet<DataType>,
    ) -> SdvResult<HashMap<ServiceFqbn<'static>, BundleMetadata>> {
        let registry = Self::get_service_bundles_registry()?;
        let available_service_bundles = registry.getAvailableServiceBundlesMetadata()?;
        let mut bundle_metadata_factory = BundleMetadataFactory::new();

        let bundle_config_files = available_service_bundles
            .iter()
            .filter_map(|metadata| {
                let source_fqbn = match ServiceFqbn::from_metadata(metadata) {
                    Ok(source_fqbn) => source_fqbn,
                    Err(e) => {warn!(
                        "Failed to instantiate ServiceFqbn from service bundle metadata of {} {}: {e:?}",
                        metadata.packageName, metadata.name
                    );
                    return None},
                };

                Some((
                    source_fqbn,
                    bundle_metadata_factory.create_bundle_metadata(
                        metadata.diagnosticsConfigPath.as_ref().filter(|_| data_types.contains(&DataType::Diagnostics)),
                        metadata.externalProtocolMappingPath.as_ref().filter(|_| data_types.contains(&DataType::SomeIp)),
                        metadata.vsidlSchemasPath.as_ref().filter(|_| data_types.contains(&DataType::VsidlSchemas)),
                    ),
                ))
            })
            .collect::<Vec<_>>();

        let mut result = HashMap::new();
        bundle_config_files
            .into_iter()
            .for_each(|(fqbn, metadata)| {
                match result.entry(fqbn) {
                    Entry::Vacant(e) => {
                        e.insert(metadata);
                    },
                    Entry::Occupied(e) => warn!("Multiple configuration files for the same FQBN: {:#?}, old value {:#?}, new value {:#?}", e.key(), metadata, e.get())
                };
            });

        Ok(result)
    }
}

impl Display for LocalProviderFromApexes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "bundle_metadata: {:?}", self.bundle_metadata)
    }
}

#[async_trait::async_trait]
impl VsidlConfigProviderV1 for LocalProviderFromApexes {
    async fn get_publication_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
    ) -> Option<PublicationDescriptor> {
        self.bundle_metadata
            .get(&source_fqin.into())
            .and_then(|data| data.proto_schemas.get_publication_descriptor(unit_type))
    }

    async fn get_rpc_method_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
        method_name: &str,
    ) -> Option<RpcMethodDescriptor> {
        self.bundle_metadata
            .get(&source_fqin.into())
            .and_then(|data| data.proto_schemas.get_rpc_method_descriptor(unit_type, method_name))
    }

    async fn get_message_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        message_name: &str,
    ) -> Option<MessageDescriptor> {
        let full_message_name = &format!(".{message_name}");

        self.bundle_metadata
            .get(&source_fqin.into())
            .and_then(|data| data.proto_schemas.get_message_descriptor(full_message_name))
    }

    async fn get_someip_mappings(&self, source_fqin: &ServiceFqin) -> Option<Vec<SomeIpMapping>> {
        self.bundle_metadata.get(&source_fqin.into()).and_then(|data| data.someip_mappings.clone())
    }

    async fn get_diagnostics_declaration(
        &self,
        fqin: &ServiceFqin,
    ) -> Option<DiagnosticsDeclaration> {
        self.bundle_metadata.get(&fqin.into()).and_then(|data| data.diagnostics_declaration.clone())
    }
}

#[cfg(test)]
mod test {
    use super::LocalProviderFromApexes;
    use sdv_comms::ContextRef;
    use std::collections::HashSet;
    use std::future::Future;
    use std::sync::Arc;

    use crate::test_utils::test_fixture_vsidl_init_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_api_fails;
    use crate::test_utils::test_fixture_vsidl_provider_diagnostics_api_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_message_descriptor_api_fails;
    use crate::test_utils::test_fixture_vsidl_provider_message_descriptor_api_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_pubsub_metadata_api_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_rpc_descriptor_api_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_with_import_catalog_dependencies_succeeds;
    use crate::test_utils::test_fixture_vsidl_provider_with_import_well_known_dependencies_succeeds;
    use crate::test_utils::Subpackage;
    use crate::types::DataType;

    async fn run_test<F, Fut>(test_function: F)
    where
        F: Fn(LocalProviderFromApexes, Option<ContextRef>, HashSet<DataType>) -> Fut,
        Fut: Future<Output = ()>,
    {
        let data_types: Vec<HashSet<DataType>> = vec![
            HashSet::from([DataType::SomeIp, DataType::Diagnostics, DataType::VsidlSchemas]),
            HashSet::from([DataType::SomeIp]),
            HashSet::from([DataType::Diagnostics]),
            HashSet::from([DataType::VsidlSchemas]),
            HashSet::new(),
        ];

        for data_type in data_types {
            //WHEN provider is called with two example protos for someip and vsidl runtime config.
            let provider = LocalProviderFromApexes::new(data_type.clone()).unwrap();

            // THEN test function assertions are satisfied.
            test_function(provider, None, data_type.clone()).await;
        }
    }

    #[tokio::test]
    async fn test_vsidl_init_succeeds() {
        run_test(test_fixture_vsidl_init_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_diagnostics_api_succeeds() {
        run_test(test_fixture_vsidl_provider_diagnostics_api_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_pubsub_metadata_api_succeeds() {
        run_test(test_fixture_vsidl_provider_pubsub_metadata_api_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_rpc_descriptor_api_succeeds() {
        run_test(test_fixture_vsidl_provider_rpc_descriptor_api_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_api_fails() {
        run_test(test_fixture_vsidl_provider_api_fails).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_with_import_catalog_dependencies_succeeds() {
        run_test(test_fixture_vsidl_provider_with_import_catalog_dependencies_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_with_import_well_known_dependencies_succeeds() {
        run_test(test_fixture_vsidl_provider_with_import_well_known_dependencies_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_message_descriptor_api_succeeds() {
        run_test(test_fixture_vsidl_provider_message_descriptor_api_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_message_descriptor_api_fails() {
        run_test(test_fixture_vsidl_provider_message_descriptor_api_fails).await;
    }

    #[tokio::test]
    async fn test_arc_reference_count_succeeds() {
        let data_types_combinations: Vec<HashSet<DataType>> = vec![
            HashSet::from([DataType::SomeIp, DataType::Diagnostics, DataType::VsidlSchemas]),
            HashSet::from([DataType::VsidlSchemas]),
        ];

        for data_types in data_types_combinations {
            // WHEN the provider is initialized.
            let provider = LocalProviderFromApexes::new(data_types.clone()).unwrap();

            // THEN the Arc reference counts for proto_schemas are correct.
            for subpackage in Subpackage::all() {
                let package_name = format!("com.sdv.google.sample.vsidl_provider.{subpackage}");

                let relevant_metadata: Vec<_> = provider
                    .bundle_metadata
                    .iter()
                    .filter(|(fqbn, _)| fqbn.to_string().starts_with(&package_name))
                    .map(|(_, metadata)| metadata)
                    .collect();

                assert!(!relevant_metadata.is_empty(), "Should find metadata for {}", package_name);

                let expected_count = match subpackage {
                    Subpackage::PerBundle => 1,
                    Subpackage::Shared => 6,
                };

                for metadata in relevant_metadata {
                    assert_eq!(
                        Arc::strong_count(&metadata.proto_schemas),
                        expected_count,
                        "Failed for package {} with FQBNs: {:?}",
                        package_name,
                        provider.bundle_metadata.keys()
                    );
                }
            }
        }
    }
}
