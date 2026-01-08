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

//! Vsidl provider agent implementation.

use crate::publication_descriptor_proto::size_metadata_proto::FieldSizeConstraintProto;
use crate::publication_descriptor_proto::SizeMetadataProto;
use log::info;
use log::warn;
use protobuf::MessageField;
use sdv_comms::id::ServiceFqin;
use sdv_comms::rpc::{Server, ServerOptions, UnaryRpcMethod};
use sdv_comms::sd::UnitRegistrationArgs;
use sdv_comms::ContextRef;
use sdv_config_provider::vsidl_provider_api::PublicationDescriptor;
use sdv_config_provider::{vsidl_provider, vsidl_provider_api::VsidlConfigProviderV1};
use sdv_dump::DumpBinderConnection;
use sdv_status_rs::SdvResult;
use sdv_status_rs::SdvStatus;
use sdv_status_rs::SdvStatusCode;
use std::collections::HashMap;
use std::future;
use std::sync::Arc;
use vsidl_provider_agent_api::{self as api, *};
use vsidl_provider_agent_protos::messages::*;

/// Serialize PublicationDescriptor.
pub fn publication_descriptor_to_proto(
    descriptor: PublicationDescriptor,
) -> PublicationDescriptorProto {
    PublicationDescriptorProto {
        message_descriptor: MessageField::some(message_descriptor_to_proto(
            descriptor.message_descriptor,
        )),
        size_metadata: MessageField::some(SizeMetadataProto {
            message_size: descriptor.size_metadata.message_size,
            message_count: descriptor.size_metadata.message_count,
            field_size_constraints: descriptor
                .size_metadata
                .field_size_constraints
                .into_iter()
                .map(|(key, constraint)| {
                    (
                        key,
                        FieldSizeConstraintProto {
                            repeated_max_count: constraint.repeated_max_count,
                            variable_type_max_size: constraint.variable_type_max_size,
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// Sets up the RPC Server
async fn create_server(
    ctx: ContextRef,
    vsidl_provider: Arc<dyn VsidlConfigProviderV1>,
) -> SdvResult<Server> {
    let mut unit_registration_args = UnitRegistrationArgs::new();
    unit_registration_args.set_unit_name(api::SERVICE_UNIT_NAME);
    unit_registration_args.set_unit_type(&api::UNIT_TYPE);

    let cloned_vsidl_provider = vsidl_provider.clone();
    let get_diagnostics_declaration = GetDiagnosticsDeclaration::create_method_handler(Arc::new(
        move |_caller_fqin: ServiceFqin, request: GetDiagnosticsDeclarationRequest| {
            let Some(ServiceFqbn { package, bundle, .. }) = request.fqbn.into_option() else {
                return Box::pin(async move {
                    Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        "Failed to get ServiceFqbn from request".to_string(),
                    ))
                });
            };

            let Ok(fqin) = ServiceFqin::builder()
                .sdv_vm_name(ctx.get_self_fqin().get_sdv_vm_name())
                .sdv_package_name(package)
                .service_bundle_name(bundle)
                .service_instance_name("ignored")
                .build()
            else {
                return Box::pin(async move {
                    Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        "Failed to create ServiceFqin from request".to_string(),
                    ))
                });
            };

            let cloned_vsidl_provider = cloned_vsidl_provider.clone();
            Box::pin(async move {
                let vsidl_provider = cloned_vsidl_provider;
                let Some(diag) = vsidl_provider.get_diagnostics_declaration(&fqin).await else {
                    return Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        "No Diagnostics found",
                    ));
                };

                Ok(GetDiagnosticsDeclarationResponse {
                    declaration: MessageField::some(diag),
                    ..Default::default()
                })
            })
        },
    ));

    let cloned_vsidl_provider = vsidl_provider.clone();
    let get_message_descriptor = GetMessageDescriptor::create_method_handler(Arc::new(
        move |_caller_fqin: ServiceFqin, request: GetMessageDescriptorRequest| {
            let Some(ServiceFqbn { package, bundle, .. }) = request.source_fqbn.into_option()
            else {
                return Box::pin(async move {
                    Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        "Failed to get ServiceFqbn from request".to_string(),
                    ))
                });
            };

            let message_name = request.message_name.clone();

            let Ok(source_fqin) = ServiceFqin::builder()
                .sdv_vm_name(ctx.get_self_fqin().get_sdv_vm_name())
                .sdv_package_name(package)
                .service_bundle_name(bundle)
                .service_instance_name("ignored")
                .build()
            else {
                return Box::pin(async move {
                    Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        "Failed to create ServiceFqin from request".to_string(),
                    ))
                });
            };

            let cloned_vsidl_provider = cloned_vsidl_provider.clone();
            Box::pin(async move {
                let vsidl_provider = cloned_vsidl_provider;

                let Some(message_descriptor) =
                    vsidl_provider.get_message_descriptor(&source_fqin, &message_name).await
                else {
                    return Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        format!("No message descriptor for {source_fqin:#?} found"),
                    ));
                };

                Ok(GetMessageDescriptorResponse {
                    message_descriptor: MessageField::some(api::message_descriptor_to_proto(
                        message_descriptor,
                    )),
                    ..Default::default()
                })
            })
        },
    ));

    let cloned_vsidl_provider = vsidl_provider.clone();
    let get_publication_descriptor = GetPublicationDescriptor::create_method_handler(Arc::new(
        move |_caller_fqin: ServiceFqin, request: GetPublicationDescriptorRequest| {
            let Some(ServiceFqbn { package, bundle, .. }) = request.source_fqbn.into_option()
            else {
                return Box::pin(async move {
                    Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        "Failed to get ServiceFqbn from request".to_string(),
                    ))
                });
            };

            let Ok(unit_type) = unit_type_from_proto(&request.unit_type) else {
                return Box::pin(async move {
                    Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        "Failed to get UnitType from request".to_string(),
                    ))
                });
            };

            let Ok(source_fqin) = ServiceFqin::builder()
                .sdv_vm_name(ctx.get_self_fqin().get_sdv_vm_name())
                .sdv_package_name(package)
                .service_bundle_name(bundle)
                .service_instance_name("ignored")
                .build()
            else {
                return Box::pin(async move {
                    Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        "Failed to create ServiceFqin from request".to_string(),
                    ))
                });
            };

            let cloned_vsidl_provider = cloned_vsidl_provider.clone();
            Box::pin(async move {
                let vsidl_provider = cloned_vsidl_provider;

                let Some(publication_descriptor) =
                    vsidl_provider.get_publication_descriptor(&source_fqin, &unit_type).await
                else {
                    return Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        format!("No publication descriptor for {source_fqin:#?} found"),
                    ));
                };

                Ok(GetPublicationDescriptorResponse {
                    publication_descriptor: MessageField::some(publication_descriptor_to_proto(
                        publication_descriptor,
                    )),
                    ..Default::default()
                })
            })
        },
    ));

    let cloned_vsidl_provider = vsidl_provider.clone();
    let get_rpc_method_descriptor = GetRpcMethodDescriptor::create_method_handler(Arc::new(
        move |_caller_fqin: ServiceFqin, request: GetRpcMethodDescriptorRequest| {
            let Some(ServiceFqbn { package, bundle, .. }) = request.source_fqbn.into_option()
            else {
                return Box::pin(async move {
                    Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        "Failed to get ServiceFqbn from request".to_string(),
                    ))
                });
            };

            let Ok(unit_type) = unit_type_from_proto(&request.unit_type) else {
                return Box::pin(async move {
                    Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        "Failed to get UnitType from request".to_string(),
                    ))
                });
            };

            let Ok(source_fqin) = ServiceFqin::builder()
                .sdv_vm_name(ctx.get_self_fqin().get_sdv_vm_name())
                .sdv_package_name(package)
                .service_bundle_name(bundle)
                .service_instance_name("ignored")
                .build()
            else {
                return Box::pin(async move {
                    Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        "Failed to create ServiceFqin from request".to_string(),
                    ))
                });
            };

            let cloned_vsidl_provider = cloned_vsidl_provider.clone();
            Box::pin(async move {
                let vsidl_provider = cloned_vsidl_provider;

                let Some(rpc_method_descriptor) = vsidl_provider
                    .get_rpc_method_descriptor(&source_fqin, &unit_type, &request.method_name)
                    .await
                else {
                    return Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        format!("No rpc method descriptor for {source_fqin:#?} found"),
                    ));
                };

                Ok(GetRpcMethodDescriptorResponse {
                    request_message_descriptor: MessageField::from_option(
                        rpc_method_descriptor.request_message.map(api::message_descriptor_to_proto),
                    ),
                    response_message_descriptor: MessageField::from_option(
                        rpc_method_descriptor
                            .response_message
                            .map(api::message_descriptor_to_proto),
                    ),
                    ..Default::default()
                })
            })
        },
    ));

    let methods_map: HashMap<String, UnaryRpcMethod> = HashMap::from([
        (
            <GetDiagnosticsDeclaration as TypedRpcMethod>::NAME.to_string(),
            get_diagnostics_declaration,
        ),
        (<GetMessageDescriptor as TypedRpcMethod>::NAME.to_string(), get_message_descriptor),
        (
            <GetPublicationDescriptor as TypedRpcMethod>::NAME.to_string(),
            get_publication_descriptor,
        ),
        (<GetRpcMethodDescriptor as TypedRpcMethod>::NAME.to_string(), get_rpc_method_descriptor),
    ]);

    let server =
        Server::create(ctx, unit_registration_args, methods_map, &ServerOptions::default()).await?;

    Ok(server)
}

static BINDER_TAG: &str = "vsidl_provider";
static DUMP_REPORT_TITLE: &str = "SDV Agent dump - VSIDL Provider";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    sdv_log::init_logger("sdv_vsidl_provider_agent")
        .map_err(|e| anyhow::anyhow!("sdv_log::init_logger - Error {e:?}"))?;
    let dm = DumpBinderConnection::initialize(DUMP_REPORT_TITLE, BINDER_TAG);

    sdv_tracing::init_tracing();
    binder::ProcessState::start_thread_pool();

    info!("Starting vsidl provider agent");

    sdv_comms_ctx::wait_for_agents()
        .map_err(|e| anyhow::anyhow!("Error waiting for Comm Stack agents - Error {e:?}"))?;

    let fqin_builder = ServiceFqin::builder();
    let vsidl_provider_fqin = fqin_builder
        .sdv_vm_name("ignored")
        .sdv_package_name(api::PACKAGE_NAME)
        .service_bundle_name(api::BUNDLE_NAME)
        .service_instance_name(api::INSTANCE_NAME)
        .build()
        .map_err(|e| anyhow::anyhow!("VSIDL provider should have a valid FQIN - Error {e:?}"))?;

    let ctx: ContextRef = ContextRef::create(vsidl_provider_fqin.clone());

    let self_fqin = ctx.get_self_fqin();
    dm.set_self_fqin(&self_fqin);

    let vsidl_provider = Arc::new(vsidl_provider::new_for_vsidl_provider_agent());
    dm.add_reporter("VSIDL Provider", &vsidl_provider);

    // Sets up the RPC Server.
    let _server = match create_server(ctx, vsidl_provider).await {
        Ok(_server) => _server,
        Err(e) => {
            warn!("Failed to create server for vsidl provider agent: {e:?}");
            return Err(anyhow::anyhow!("Failed to create server for vsidl provider agent"));
        }
    };

    log::debug!("sdv_vsidl_provider_agent has started successfully");
    dm.set_agent_state("sdv_vsidl_provider_agent has started successfully");
    tracing::event!(tracing::Level::DEBUG, "sdv_vsidl_provider_agent has started successfully");

    future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::assert_that;
    use googletest::prelude::eq;
    use protobuf::MessageDyn;
    use sdv_config_provider::vsidl_provider_api::FieldSizeConstraint;
    use sdv_config_provider::vsidl_provider_api::SizeMetadata;
    use vsidl_provider_agent_api_test_protos::message::TestMessage;

    #[googletest::test]
    fn test_publication_descriptor_to_proto_conversion() {
        let size_metadata = SizeMetadata {
            message_size: 10,
            message_count: 11,
            field_size_constraints: HashMap::from([
                (
                    "Aa".to_owned(),
                    FieldSizeConstraint {
                        repeated_max_count: Some(5),
                        variable_type_max_size: Some(6),
                    },
                ),
                (
                    "Bb".to_owned(),
                    FieldSizeConstraint {
                        repeated_max_count: Some(7),
                        variable_type_max_size: None,
                    },
                ),
                (
                    "Cc".to_owned(),
                    FieldSizeConstraint {
                        repeated_max_count: None,
                        variable_type_max_size: Some(8),
                    },
                ),
                (
                    "Dd".to_owned(),
                    FieldSizeConstraint { repeated_max_count: None, variable_type_max_size: None },
                ),
            ]),
        };

        let publication_descriptor = PublicationDescriptor {
            message_descriptor: TestMessage::default().descriptor_dyn(),
            size_metadata: size_metadata.clone(),
        };

        let publication_descriptor_proto = publication_descriptor_to_proto(publication_descriptor);

        let message_descriptor =
            message_descriptor_from_proto(&publication_descriptor_proto.message_descriptor)
                .expect("MessageDescriptor for TestMessage");

        assert_that!(
            message_descriptor.full_name(),
            eq("com.vsidl_provider_agent_api_test.TestMessage")
        );
        assert_that!(
            message_descriptor.proto(),
            eq(TestMessage::default().descriptor_dyn().proto())
        );

        assert_that!(
            publication_descriptor_proto.size_metadata.message_size,
            eq(size_metadata.message_size)
        );
        assert_that!(
            publication_descriptor_proto.size_metadata.message_count,
            eq(size_metadata.message_count)
        );

        for (key, value) in &publication_descriptor_proto.size_metadata.field_size_constraints {
            assert_that!(
                value.repeated_max_count,
                eq(size_metadata.field_size_constraints.get(key).unwrap().repeated_max_count)
            );
            assert_that!(
                value.variable_type_max_size,
                eq(size_metadata.field_size_constraints.get(key).unwrap().variable_type_max_size)
            );
        }
    }
}
