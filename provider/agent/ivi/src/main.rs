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

use log::info;
use log::warn;
use sdv_comms::id::ServiceFqin;
use sdv_comms::rpc::{Server, ServerOptions, UnaryRpcMethod};
use sdv_comms::sd::UnitRegistrationArgs;
use sdv_comms::ContextRef;
use sdv_status_rs::SdvResult;
use sdv_status_rs::SdvStatus;
use sdv_status_rs::SdvStatusCode;
use std::collections::HashMap;
use std::future;
use std::sync::Arc;
use vsidl_provider_agent_api::{self as api, *};
use vsidl_provider_agent_protos::messages::*;

// Sets up the RPC Server
async fn create_server(ctx: ContextRef) -> SdvResult<Server> {
    let mut unit_registration_args = UnitRegistrationArgs::new();
    unit_registration_args.set_unit_name(api::SERVICE_UNIT_NAME);
    unit_registration_args.set_unit_type(&api::UNIT_TYPE);
    let get_diagnostics_declaration = GetDiagnosticsDeclaration::create_method_handler(Arc::new(
        move |_caller_fqin: ServiceFqin, _request: GetDiagnosticsDeclarationRequest| {
            Box::pin(async move { Err(SdvStatus::new(SdvStatusCode::Unimplemented)) })
        },
    ));

    let get_message_descriptor = GetMessageDescriptor::create_method_handler(Arc::new(
        move |_caller_fqin: ServiceFqin, _request: GetMessageDescriptorRequest| {
            Box::pin(async move { Err(SdvStatus::new(SdvStatusCode::Unimplemented)) })
        },
    ));

    let get_publication_descriptor = GetPublicationDescriptor::create_method_handler(Arc::new(
        move |_caller_fqin: ServiceFqin, _request: GetPublicationDescriptorRequest| {
            Box::pin(async move { Err(SdvStatus::new(SdvStatusCode::Unimplemented)) })
        },
    ));

    let get_rpc_method_descriptor = GetRpcMethodDescriptor::create_method_handler(Arc::new(
        move |_caller_fqin: ServiceFqin, _request: GetRpcMethodDescriptorRequest| {
            Box::pin(async move { Err(SdvStatus::new(SdvStatusCode::Unimplemented)) })
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

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    sdv_log::init_logger("sdv_vsidl_provider_agent")
        .map_err(|e| anyhow::anyhow!("sdv_log::init_logger - Error {e:?}"))?;

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

    // Sets up the RPC Server.
    let _server = match create_server(ctx).await {
        Ok(_server) => _server,
        Err(e) => {
            warn!("Failed to create server for vsidl provider agent: {e:?}");
            return Err(anyhow::anyhow!("Failed to create server for vsidl provider agent"));
        }
    };

    log::debug!("sdv_vsidl_provider_agent has started successfully");
    tracing::event!(tracing::Level::DEBUG, "sdv_vsidl_provider_agent has started successfully");

    future::pending::<()>().await;
    Ok(())
}
