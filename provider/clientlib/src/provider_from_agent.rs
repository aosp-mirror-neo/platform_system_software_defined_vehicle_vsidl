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

use crate::types::DataType;
use crate::vsidl_provider_api::*;
use dashmap::DashMap;
use futures::Future;
use futures::StreamExt;
use log::{info, warn};
use protobuf::reflect::MessageDescriptor;
use protobuf::MessageField;
use rustutils::android::system_properties::read;
use sdv_comms::id::ServiceFqin;
use sdv_comms::rpc::{Client, ClientOptions};
use sdv_comms::sd::ServiceDiscoveryManager;
use sdv_comms::sd::ServiceUnitChangeEvent;
use sdv_comms::sd::UnitNameDiscoveryArgs;
use sdv_comms::sd::UnitType;
use sdv_comms::ContextRef;
use sdv_status_rs::SdvResult;
use sdv_status_rs::SdvStatus;
use sdv_status_rs::SdvStatusCode;
use std::collections::HashSet;
use std::fmt;
use std::fmt::Display;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use vsidl_provider_agent_api as api;
use vsidl_provider_agent_api::TypedRpcMethod;
use vsidl_provider_agent_protos::messages::*;
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;
use vsidl_v1::someip::SomeIpMapping;

pub(crate) struct ProviderFromAgent {
    ctx: ContextRef,
    local_vm_name: String,
    use_for_local_vm: bool,
    client_cache: Arc<DashMap<String, Arc<Client>>>,
    data_types: HashSet<DataType>,
}

const VSIDL_PROVIDER_CREATE_RPC_CLIENT_TIMEOUT_MS_PROP: &str =
    "persist.sdv.vsidl_provider.create_rpc_client_timeout_ms";
#[cfg(not(test))]
const DEFAULT_CREATE_RPC_CLIENT_TIMEOUT: Duration = Duration::from_secs(10);
/// For tests, use a shorter timeout to fail faster.
#[cfg(test)]
const DEFAULT_CREATE_RPC_CLIENT_TIMEOUT: Duration = Duration::from_secs(1);

impl Display for ProviderFromAgent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut client_list = "".to_owned();
        for client_vm in self.client_cache.deref().iter().map(|ref_multi| ref_multi.key().clone()) {
            if !client_list.is_empty() {
                client_list.push(';');
            }
            client_list.push_str(client_vm.as_str());
        }
        write!(f, "use_for_local_vm: {} client_cache list: {}", self.use_for_local_vm, client_list)
    }
}

impl ProviderFromAgent {
    pub fn new_from_remote_agent(ctx: ContextRef, data_types: HashSet<DataType>) -> Self {
        let local_vm_name = ctx.get_self_fqin().get_sdv_vm_name().to_string();
        Self {
            ctx,
            local_vm_name,
            use_for_local_vm: false,
            client_cache: Arc::new(DashMap::new()),
            data_types,
        }
    }

    pub fn new_from_agent(ctx: ContextRef, data_types: HashSet<DataType>) -> Self {
        let local_vm_name = ctx.get_self_fqin().get_sdv_vm_name().to_string();
        Self {
            ctx,
            local_vm_name,
            use_for_local_vm: true,
            client_cache: Arc::new(DashMap::new()),
            data_types,
        }
    }

    // Sets up the RPC Client
    async fn create_client(&self, server_vm_name: &str) -> SdvResult<Client> {
        let creation_future = async {
            let mut discovery_stream = ServiceDiscoveryManager::new(self.ctx)
                .subscribe_service_unit_change_by_name(
                    &UnitNameDiscoveryArgs::new_builder()
                        .set_sdv_vm_name(server_vm_name)
                        .set_sdv_package_name(api::PACKAGE_NAME)
                        .set_service_bundle_name(api::BUNDLE_NAME)
                        .set_service_unit_name(api::SERVICE_UNIT_NAME)
                        .build()
                        .unwrap(),
                )
                .await?;

            let discovery_result = loop {
                let status = discovery_stream.next().await;
                if let Some(ServiceUnitChangeEvent::Registered(unit)) = status {
                    break unit;
                }
            };

            let server_identifier = discovery_result.get_unit_identifier();

            Client::create(self.ctx, server_identifier, &ClientOptions::default()).await
        };

        let timeout_duration = get_create_rpc_client_timeout(read);
        tokio::time::timeout(timeout_duration, creation_future).await.map_err(|_| {
            SdvStatus::with_message(
                SdvStatusCode::DeadlineExceeded,
                format!(
                    "Timed out creating RPC client for {} after {} seconds",
                    server_vm_name,
                    timeout_duration.as_secs()
                ),
            )
        })?
    }

    fn is_same_vm(&self, fqin: &ServiceFqin) -> bool {
        self.local_vm_name == fqin.get_sdv_vm_name()
    }

    fn use_for_fqin(&self, fqin: &ServiceFqin) -> bool {
        self.use_for_local_vm || !self.is_same_vm(fqin)
    }

    async fn try_get_data<T, F>(&self, source_fqin: &ServiceFqin, do_try_get_data: F) -> Option<T>
    where
        for<'a> F:
            Fn(&'a Client) -> Pin<Box<dyn Future<Output = SdvResult<Option<T>>> + Send + 'a>>,
    {
        if !self.use_for_fqin(source_fqin) {
            return None;
        }

        let server_vm_name = source_fqin.get_sdv_vm_name();

        // try with a cached client first
        if let Some(client) =
            self.client_cache.get(server_vm_name).map(|client_ref| Arc::clone(client_ref.value()))
        {
            if let Ok(result) = do_try_get_data(&client).await {
                info!("On vm:{}, reused Client for vm:{}", self.local_vm_name, server_vm_name);
                return result;
            }
        }

        // try with a new client and insert if successful
        let client = self.create_client(server_vm_name).await.ok()?;
        if let Ok(result) = do_try_get_data(&client).await {
            let old_client = self.client_cache.insert(server_vm_name.to_owned(), Arc::new(client));

            if old_client.is_some() {
                info!("On vm:{}, replaced Client for vm:{}", self.local_vm_name, server_vm_name);
            }
            return result;
        }

        None
    }
}

fn get_create_rpc_client_timeout(
    reader: fn(&str) -> rustutils::android::system_properties::Result<Option<String>>,
) -> Duration {
    match reader(VSIDL_PROVIDER_CREATE_RPC_CLIENT_TIMEOUT_MS_PROP) {
        Ok(Some(timeout_str)) => {
            timeout_str.parse::<u64>().map(Duration::from_millis).unwrap_or_else(|_| {
                warn!(
                    "Failed to parse timeout value '{}' for property '{}', using default.",
                    timeout_str, VSIDL_PROVIDER_CREATE_RPC_CLIENT_TIMEOUT_MS_PROP
                );
                DEFAULT_CREATE_RPC_CLIENT_TIMEOUT
            })
        }
        _ => DEFAULT_CREATE_RPC_CLIENT_TIMEOUT,
    }
}

/// Deserialize PublicationDescriptor.
pub fn publication_descriptor_from_proto(
    proto: PublicationDescriptorProto,
) -> SdvResult<PublicationDescriptor> {
    Ok(PublicationDescriptor {
        message_descriptor: api::message_descriptor_from_proto(
            proto.message_descriptor.into_option().as_ref().ok_or_else(|| {
                SdvStatus::with_message(
                    SdvStatusCode::NotFound,
                    "No message_descriptor found in PublicationDescriptorProto",
                )
            })?,
        )?,
        size_metadata: SizeMetadata {
            message_size: proto.size_metadata.message_size,
            message_count: proto.size_metadata.message_count,
            // TODO get rid of clone
            field_size_constraints: proto
                .size_metadata
                .field_size_constraints
                .clone()
                .into_iter()
                .map(|(key, constraint)| {
                    (
                        key,
                        FieldSizeConstraint {
                            repeated_max_count: constraint.repeated_max_count,
                            variable_type_max_size: constraint.variable_type_max_size,
                        },
                    )
                })
                .collect(),
        },
    })
}

async fn try_get_publication_descriptor_from_client(
    client: &Client,
    source_fqin: ServiceFqin,
    unit_type: UnitType,
) -> SdvResult<Option<PublicationDescriptor>> {
    let response = match api::GetPublicationDescriptor::unary_call(
        client,
        GetPublicationDescriptorRequest {
            source_fqbn: MessageField::some(ServiceFqbn {
                package: source_fqin.get_sdv_package_name().to_string(),
                bundle: source_fqin.get_service_bundle_name().to_string(),
                ..Default::default()
            }),
            unit_type: MessageField::some(api::unit_type_to_proto(&unit_type)),
            ..Default::default()
        },
    )
    .await
    {
        Ok(response) => response,
        Err(e) => {
            warn!("GetPublicationDescriptor Error {e:?}");
            return Err(e);
        }
    };

    let Some(publication_descriptor_proto) = response.publication_descriptor.into_option() else {
        warn!("No publication descriptor received");
        return Ok(None);
    };

    Ok(publication_descriptor_from_proto(publication_descriptor_proto).ok())
}

async fn try_get_rpc_method_descriptor_from_client(
    client: &Client,
    source_fqin: ServiceFqin,
    unit_type: UnitType,
    method_name: String,
) -> SdvResult<Option<RpcMethodDescriptor>> {
    let response = match api::GetRpcMethodDescriptor::unary_call(
        client,
        GetRpcMethodDescriptorRequest {
            source_fqbn: MessageField::some(ServiceFqbn {
                package: source_fqin.get_sdv_package_name().to_string(),
                bundle: source_fqin.get_service_bundle_name().to_string(),
                ..Default::default()
            }),
            unit_type: MessageField::some(api::unit_type_to_proto(&unit_type)),
            method_name,
            ..Default::default()
        },
    )
    .await
    {
        Ok(response) => response,
        Err(e) => {
            warn!("GetRpcMethodDescriptor Error {e:?}");
            return Err(e);
        }
    };

    Ok(Some(RpcMethodDescriptor {
        request_message: response
            .request_message_descriptor
            .into_option()
            .as_ref()
            .and_then(|p| api::message_descriptor_from_proto(p).ok()),
        response_message: response
            .response_message_descriptor
            .into_option()
            .as_ref()
            .and_then(|p| api::message_descriptor_from_proto(p).ok()),
    }))
}

async fn try_get_message_descriptor_from_client(
    client: &Client,
    source_fqin: ServiceFqin,
    message_name: String,
) -> SdvResult<Option<MessageDescriptor>> {
    match api::GetMessageDescriptor::unary_call(
        client,
        GetMessageDescriptorRequest {
            source_fqbn: MessageField::some(ServiceFqbn {
                package: source_fqin.get_sdv_package_name().to_string(),
                bundle: source_fqin.get_service_bundle_name().to_string(),
                ..Default::default()
            }),
            message_name,
            ..Default::default()
        },
    )
    .await
    {
        Ok(response) => Ok(response
            .message_descriptor
            .into_option()
            .as_ref()
            .and_then(|p| api::message_descriptor_from_proto(p).ok())),
        Err(e) => {
            warn!("GetMessageDescriptorRequest Error {e:?}");
            Err(e)
        }
    }
}

async fn try_get_diagnostics_declaration_from_client(
    client: &Client,
    fqin: ServiceFqin,
) -> SdvResult<Option<DiagnosticsDeclaration>> {
    match api::GetDiagnosticsDeclaration::unary_call(
        client,
        GetDiagnosticsDeclarationRequest {
            fqbn: MessageField::some(ServiceFqbn {
                package: fqin.get_sdv_package_name().to_string(),
                bundle: fqin.get_service_bundle_name().to_string(),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await
    {
        Ok(response) => Ok(response.declaration.into_option()),
        Err(e) => {
            warn!("GetDiagnosticsDeclarationRequest Error {e:?}");
            Err(e)
        }
    }
}

#[async_trait::async_trait]
impl VsidlConfigProviderV1 for ProviderFromAgent {
    async fn get_publication_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
    ) -> Option<PublicationDescriptor> {
        if !self.data_types.contains(&DataType::VsidlSchemas) {
            return None;
        };

        self.try_get_data(source_fqin, |client| {
            Box::pin(try_get_publication_descriptor_from_client(
                client,
                source_fqin.clone(),
                unit_type.clone(),
            ))
        })
        .await
    }

    async fn get_rpc_method_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
        method_name: &str,
    ) -> Option<RpcMethodDescriptor> {
        if !self.data_types.contains(&DataType::VsidlSchemas) {
            return None;
        };

        self.try_get_data(source_fqin, |client| {
            Box::pin(try_get_rpc_method_descriptor_from_client(
                client,
                source_fqin.clone(),
                unit_type.clone(),
                method_name.to_owned(),
            ))
        })
        .await
    }

    async fn get_message_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        message_name: &str,
    ) -> Option<MessageDescriptor> {
        if !self.data_types.contains(&DataType::VsidlSchemas) {
            return None;
        };

        self.try_get_data(source_fqin, |client| {
            Box::pin(try_get_message_descriptor_from_client(
                client,
                source_fqin.clone(),
                message_name.to_owned(),
            ))
        })
        .await
    }

    async fn get_someip_mappings(&self, _source_fqin: &ServiceFqin) -> Option<Vec<SomeIpMapping>> {
        if !self.data_types.contains(&DataType::SomeIp) {
            return None;
        };

        warn!("Unexpected call to get someip mappings from remote VM");
        None
    }

    async fn get_diagnostics_declaration(
        &self,
        fqin: &ServiceFqin,
    ) -> Option<DiagnosticsDeclaration> {
        if !self.data_types.contains(&DataType::Diagnostics) {
            return None;
        };
        self.try_get_data(fqin, |client| {
            Box::pin(try_get_diagnostics_declaration_from_client(client, fqin.clone()))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_from_agent::publication_descriptor_proto::size_metadata_proto::FieldSizeConstraintProto;
    use crate::provider_from_agent::publication_descriptor_proto::SizeMetadataProto;
    use crate::test_utils::*;
    use googletest::assert_that;
    use googletest::prelude::eq;
    use protobuf::MessageDyn;
    use rustutils::android::system_properties::PropertyWatcherError;
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::time::Duration;
    use vsidl_provider_agent_api::message_descriptor_to_proto;
    use vsidl_provider_agent_api_test_protos::message::TestMessage;

    async fn ignore_client(_client: &Client) -> SdvResult<Option<String>> {
        Ok(Some("closure is called".to_owned()))
    }

    #[tokio::test]
    async fn test_try_get_data_from_remote_agent() {
        // setup Binder thread pool to enable use of Comms "listener" API.
        sdv_dump::ProcessState::start_thread_pool();

        let data_types =
            HashSet::from([DataType::SomeIp, DataType::Diagnostics, DataType::VsidlSchemas]);

        let fqin_str =
            "ignored:com.sdv.google.sample.test_try_get_data_from_remote_agent.Client/ignored";
        let remote_provider_from_agent = ProviderFromAgent::new_from_remote_agent(
            ContextRef::create(ServiceFqin::from_str(fqin_str).expect("Valid ServiceFqin")),
            data_types,
        );

        let fqin1 = ServiceFqin::from_str("instance1:com.sdv.google.vsidl_provider.Agent/default")
            .expect("Valid FQIN");

        // do not create a cache entry if we are on the same vm
        let res = remote_provider_from_agent
            .try_get_data(&fqin1, |client| {
                Box::pin(try_get_diagnostics_declaration_from_client(client, fqin1.clone()))
            })
            .await;
        assert!(res.is_none());

        let fqin2 = ServiceFqin::from_str("instance2:com.sdv.google.vsidl_provider.Agent/default")
            .expect("Valid FQIN");

        // the passed closure should not be called because:
        // - the cache entry doesn't exist
        // - new client can't be created, because "instance2" vm doesn't exist in the test setup
        let res = remote_provider_from_agent
            .try_get_data(&fqin2, |client| Box::pin(ignore_client(client)))
            .await;
        assert!(res.is_none());

        // create an entry for a remote client (note, it doesn't matter that the actual client is for instance1 for this test)
        remote_provider_from_agent.client_cache.insert(
            fqin2.get_sdv_vm_name().to_owned(),
            Arc::new(remote_provider_from_agent.create_client("instance1").await.unwrap()),
        );
        let res = remote_provider_from_agent.client_cache.get(fqin2.get_sdv_vm_name());
        assert!(res.is_some());

        // now that the cached entry exists, the closure should be called
        let res = remote_provider_from_agent
            .try_get_data(&fqin2, |client| Box::pin(ignore_client(client)))
            .await;
        assert!(res.is_some());
    }

    #[tokio::test]
    async fn test_try_get_data_from_agent() {
        // setup Binder thread pool to enable use of Comms "listener" API.
        sdv_dump::ProcessState::start_thread_pool();

        let data_types =
            HashSet::from([DataType::SomeIp, DataType::Diagnostics, DataType::VsidlSchemas]);

        let fqin_str = "ignored:com.sdv.google.sample.test_try_get_data_from_agent.Client/ignored";
        let provider_from_agent = ProviderFromAgent::new_from_agent(
            ContextRef::create(ServiceFqin::from_str(fqin_str).expect("Valid ServiceFqin")),
            data_types,
        );

        let fqin1 = ServiceFqin::from_str("instance1:com.sdv.google.vsidl_provider.Agent/default")
            .expect("Valid FQIN");

        // creates a cache entry, even if we are on the same vm
        let res = provider_from_agent
            .try_get_data(&fqin1, |client| Box::pin(ignore_client(client)))
            .await;
        assert!(res.is_some());

        let fqin2 = ServiceFqin::from_str("instance2:com.sdv.google.vsidl_provider.Agent/default")
            .expect("Valid FQIN");

        // the passed closure should not be called because:
        // - the cache entry doesn't exist
        // - new client can't be created, because "instance2" vm doesn't exist in the test setup
        let res = provider_from_agent
            .try_get_data(&fqin2, |client| Box::pin(ignore_client(client)))
            .await;
        assert!(res.is_none());

        // create an entry for a remote client (note, it doesn't matter that the actual client is for instance1 for this test)
        provider_from_agent.client_cache.insert(
            fqin2.get_sdv_vm_name().to_owned(),
            Arc::new(provider_from_agent.create_client("instance1").await.unwrap()),
        );
        let res = provider_from_agent.client_cache.get(fqin2.get_sdv_vm_name());
        assert!(res.is_some());

        // now that the cached entry exists, the closure should be called
        let res = provider_from_agent
            .try_get_data(&fqin2, |client| Box::pin(ignore_client(client)))
            .await;
        assert!(res.is_some());
    }

    #[googletest::test]
    fn test_publication_descriptor_from_proto_conversion() {
        let size_metadata_proto = SizeMetadataProto {
            message_size: 10,
            message_count: 11,
            field_size_constraints: HashMap::from([
                (
                    "Aa".to_owned(),
                    FieldSizeConstraintProto {
                        repeated_max_count: Some(5),
                        variable_type_max_size: Some(6),
                        ..Default::default()
                    },
                ),
                (
                    "Bb".to_owned(),
                    FieldSizeConstraintProto {
                        repeated_max_count: Some(7),
                        variable_type_max_size: None,
                        ..Default::default()
                    },
                ),
                (
                    "Cc".to_owned(),
                    FieldSizeConstraintProto {
                        repeated_max_count: None,
                        variable_type_max_size: Some(8),
                        ..Default::default()
                    },
                ),
                (
                    "Dd".to_owned(),
                    FieldSizeConstraintProto {
                        repeated_max_count: None,
                        variable_type_max_size: None,
                        ..Default::default()
                    },
                ),
            ]),
            ..Default::default()
        };

        let publication_descriptor_proto = PublicationDescriptorProto {
            message_descriptor: MessageField::some(message_descriptor_to_proto(
                TestMessage::default().descriptor_dyn(),
            )),
            size_metadata: MessageField::some(size_metadata_proto.clone()),
            ..Default::default()
        };

        let publication_descriptor =
            publication_descriptor_from_proto(publication_descriptor_proto).unwrap();

        assert_that!(
            publication_descriptor.message_descriptor.full_name(),
            eq("com.vsidl_provider_agent_api_test.TestMessage")
        );
        assert_that!(
            publication_descriptor.message_descriptor.proto(),
            eq(TestMessage::default().descriptor_dyn().proto())
        );

        assert_that!(
            publication_descriptor.size_metadata.message_size,
            eq(size_metadata_proto.message_size)
        );
        assert_that!(
            publication_descriptor.size_metadata.message_count,
            eq(size_metadata_proto.message_count)
        );

        for (key, value) in &publication_descriptor.size_metadata.field_size_constraints {
            assert_that!(
                value.repeated_max_count,
                eq(size_metadata_proto.field_size_constraints.get(key).unwrap().repeated_max_count)
            );
            assert_that!(
                value.variable_type_max_size,
                eq(size_metadata_proto
                    .field_size_constraints
                    .get(key)
                    .unwrap()
                    .variable_type_max_size)
            );
        }
    }

    async fn run_test<F, Fut>(test_name: &str, test_function: F)
    where
        F: Fn(ProviderFromAgent, Option<ContextRef>, HashSet<DataType>) -> Fut,
        Fut: Future<Output = ()>,
    {
        // setup Binder thread pool to enable use of Comms "listener" API.
        sdv_dump::ProcessState::start_thread_pool();

        let fqin_str = format!("ignored:com.sdv.google.sample.{test_name}.Client/ignored");
        let ctx = ContextRef::create(ServiceFqin::from_str(&fqin_str).expect("Valid ServiceFqin"));

        let data_types: Vec<HashSet<DataType>> = vec![
            HashSet::from([DataType::Diagnostics, DataType::VsidlSchemas]),
            HashSet::from([DataType::Diagnostics]),
            HashSet::from([DataType::VsidlSchemas]),
            HashSet::new(),
        ];

        for data_type in data_types {
            //WHEN provider from agent is created.
            let provider = ProviderFromAgent::new_from_agent(ctx, data_type.clone());

            // THEN test function assertions are satisfied.
            test_function(provider, Some(ctx), data_type.clone()).await;
        }
    }

    #[tokio::test]
    async fn test_vsidl_init_succeeds() {
        run_test("test_fixture_vsidl_init_succeeds", test_fixture_vsidl_init_succeeds).await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_diagnostics_api_succeeds() {
        run_test(
            "test_fixture_vsidl_provider_diagnostics_api_succeeds",
            test_fixture_vsidl_provider_diagnostics_api_succeeds,
        )
        .await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_pubsub_metadata_api_succeeds() {
        run_test(
            "test_fixture_vsidl_provider_pubsub_metadata_api_succeeds",
            test_fixture_vsidl_provider_pubsub_metadata_api_succeeds,
        )
        .await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_rpc_descriptor_api_succeeds() {
        run_test(
            "test_fixture_vsidl_provider_rpc_descriptor_api_succeeds",
            test_fixture_vsidl_provider_rpc_descriptor_api_succeeds,
        )
        .await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_api_fails() {
        run_test("test_fixture_vsidl_provider_api_fails", test_fixture_vsidl_provider_api_fails)
            .await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_with_import_catalog_dependencies_succeeds() {
        run_test(
            "test_fixture_vsidl_provider_with_import_catalog_dependencies_succeeds",
            test_fixture_vsidl_provider_with_import_catalog_dependencies_succeeds,
        )
        .await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_with_import_well_known_dependencies_succeeds() {
        run_test(
            "test_fixture_vsidl_provider_with_import_well_known_dependencies_succeeds",
            test_fixture_vsidl_provider_with_import_well_known_dependencies_succeeds,
        )
        .await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_message_descriptor_api_succeeds() {
        run_test(
            "test_fixture_vsidl_provider_message_descriptor_api_succeeds",
            test_fixture_vsidl_provider_message_descriptor_api_succeeds,
        )
        .await;
    }

    #[tokio::test]
    async fn test_vsidl_provider_message_descriptor_api_fails() {
        run_test(
            "test_fixture_vsidl_provider_message_descriptor_api_fails",
            test_fixture_vsidl_provider_message_descriptor_api_fails,
        )
        .await;
    }

    #[test]
    fn test_get_create_rpc_client_timeout() {
        // Test with a valid integer value.
        let duration = get_create_rpc_client_timeout(|_| Ok(Some("5000".to_string())));
        assert_eq!(duration, Duration::from_millis(5000));

        // Test with an invalid integer value, should fall back to default.
        let duration = get_create_rpc_client_timeout(|_| Ok(Some("not-a-number".to_string())));
        assert_eq!(duration, DEFAULT_CREATE_RPC_CLIENT_TIMEOUT);

        // Test with no property set, should fall back to default.
        let duration = get_create_rpc_client_timeout(|_| Ok(None));
        assert_eq!(duration, DEFAULT_CREATE_RPC_CLIENT_TIMEOUT);

        // Test with an error reading the property, should fall back to default.
        let duration = get_create_rpc_client_timeout(|_| Err(PropertyWatcherError::WaitFailed));
        assert_eq!(duration, DEFAULT_CREATE_RPC_CLIENT_TIMEOUT);
    }
}
