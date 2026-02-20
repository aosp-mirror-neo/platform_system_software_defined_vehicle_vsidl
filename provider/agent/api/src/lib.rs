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

//! Vsidl provider API declarations.

use lazy_static::lazy_static;
use log::warn;
use protobuf::descriptor::FileDescriptorProto;
use protobuf::reflect::{FileDescriptor, MessageDescriptor};
use protobuf::{Message, MessageFull};
use sdv_comms::id::ServiceFqin;
use sdv_comms::rpc::Client;
use sdv_comms::rpc::UnaryRpcMethod;
use sdv_comms::sd::UnitType;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use sdv_vsidl_common::{WELLKNOWN_PROTOS_TO_DESCRIPTOR, WELLKNOWN_TYPES_TO_DESCRIPTOR};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use vsidl_provider_agent_protos::messages::*;

/// Package name of VSIDL provider agent.
pub const PACKAGE_NAME: &str = "com.android.sdv.vsidl_provider";

/// Service bundle name of VSIDL provider agent.
pub const BUNDLE_NAME: &str = "Agent";

/// Service bundle instance name of VSIDL provider agent.
pub const INSTANCE_NAME: &str = "default";

/// Unit type name of VSIDL provider RPC server.
pub const UNIT_TYPE_NAME: &str = "VsidlProviderService";

/// Service unit name of VSIDL provider RPC server.
pub const SERVICE_UNIT_NAME: &str = "vsidl-provider-server";

lazy_static! {
    /// Unit type of VSIDL provider RPC server.
    pub static ref UNIT_TYPE: UnitType =
    UnitType::builder()
        .set_sdv_package_name(PACKAGE_NAME)
        .set_service_bundle_name(BUNDLE_NAME)
        .set_unit_type_name(UNIT_TYPE_NAME)
        .build().expect("Expect Valid UnitType");
}

/// Type information about RPC server methods.
#[async_trait::async_trait]
pub trait TypedRpcMethod {
    /// RPC method name.
    const NAME: &'static str;
    /// Request proto message.
    type Request: MessageFull;
    /// Response proto message.
    type Response: MessageFull;

    /// Performs unary call.
    async fn unary_call(client: &Client, request: Self::Request) -> SdvResult<Self::Response> {
        let response_bytes = client
            .unary_call(
                Self::NAME,
                &request
                    .write_to_bytes()
                    .map_err(|e| SdvStatus::with_message(SdvStatusCode::Internal, e.to_string()))?,
            )
            .await?;

        Self::Response::parse_from_bytes(&response_bytes)
            .map_err(|e| SdvStatus::with_message(SdvStatusCode::Internal, e.to_string()))
    }

    /// Create sdv_comms::rpc::UnaryRpcMethod from strictly typed callback.
    fn create_method_handler(
        typed_handler: Arc<
            dyn Fn(
                    ServiceFqin,
                    Self::Request,
                )
                    -> Pin<Box<dyn Future<Output = SdvResult<Self::Response>> + Send>>
                + Send
                + Sync
                + 'static,
        >,
    ) -> UnaryRpcMethod {
        let method: UnaryRpcMethod = Arc::new(move |caller_fqin: ServiceFqin, x: &[u8]| {
            let request = Self::Request::parse_from_bytes(x);

            let Ok(request) = request else {
                return Box::pin(async move {
                    Err(SdvStatus::with_message(
                        SdvStatusCode::Internal,
                        format!("Failed to parse request message for {}", Self::NAME),
                    ))
                });
            };

            Box::pin({
                let typed_handler = typed_handler.clone();
                async move {
                    let response = match typed_handler(caller_fqin, request).await {
                        Ok(response) => response,
                        Err(e) => {
                            warn!("Failed typed_handler {e:?}");
                            return Err(e);
                        }
                    };

                    response.write_to_bytes().map_err(|e| {
                        SdvStatus::with_message(
                            SdvStatusCode::Internal,
                            format!("Failed to encode response message for {}: {e}", Self::NAME),
                        )
                    })
                }
            })
        });
        method
    }
}

/// Type information about get_diagnsotics_declaration RPC method.
pub struct GetDiagnosticsDeclaration;
impl TypedRpcMethod for GetDiagnosticsDeclaration {
    const NAME: &'static str = "get_diagnostics_declaration";
    type Request = GetDiagnosticsDeclarationRequest;
    type Response = GetDiagnosticsDeclarationResponse;
}

/// Type information about get_publication_descriptor RPC method.
pub struct GetPublicationDescriptor;
impl TypedRpcMethod for GetPublicationDescriptor {
    const NAME: &'static str = "get_publication_descriptor";
    type Request = GetPublicationDescriptorRequest;
    type Response = GetPublicationDescriptorResponse;
}

/// Type information about get_subscription_descriptor RPC method.
pub struct GetRpcMethodDescriptor;
impl TypedRpcMethod for GetRpcMethodDescriptor {
    const NAME: &'static str = "get_rpc_method_descriptor";
    type Request = GetRpcMethodDescriptorRequest;
    type Response = GetRpcMethodDescriptorResponse;
}

/// Type information about get_message_descriptor RPC method.
pub struct GetMessageDescriptor;
impl TypedRpcMethod for GetMessageDescriptor {
    const NAME: &'static str = "get_message_descriptor";
    type Request = GetMessageDescriptorRequest;
    type Response = GetMessageDescriptorResponse;
}

/// Serialize MessageDescriptor.
pub fn message_descriptor_to_proto(descriptor: MessageDescriptor) -> MessageDescriptorProto {
    let mut descriptor_proto = MessageDescriptorProto::default();
    let mut file_descriptor_protos = HashMap::new();

    push_with_recursive_dependencies(&mut file_descriptor_protos, descriptor.file_descriptor());

    descriptor_proto.file_descriptors = file_descriptor_protos.into_values().collect::<Vec<_>>();
    descriptor_proto.message_name = format!(".{}", descriptor.full_name());
    descriptor_proto
}

fn push_with_recursive_dependencies(
    file_descriptor_set: &mut HashMap<String, FileDescriptorProto>,
    descriptor: &FileDescriptor,
) {
    // Skip well known descriptors to avoid unnecessary data exchange.
    if WELLKNOWN_PROTOS_TO_DESCRIPTOR.contains_key(descriptor.name()) {
        return;
    }

    let old_value =
        file_descriptor_set.insert(descriptor.name().to_string(), descriptor.proto().clone());

    if old_value.is_some() {
        return;
    }

    for dependency in descriptor.deps() {
        push_with_recursive_dependencies(file_descriptor_set, dependency);
    }
}

/// Deserialize MessageDescriptor.
pub fn message_descriptor_from_proto(
    proto: &MessageDescriptorProto,
) -> SdvResult<MessageDescriptor> {
    let file_descriptors = &proto.file_descriptors;

    if let Some(file_descriptor) = WELLKNOWN_TYPES_TO_DESCRIPTOR.get(&proto.message_name) {
        return file_descriptor.message_by_full_name(&proto.message_name).ok_or_else(|| {
            SdvStatus::with_message(
                SdvStatusCode::NotFound,
                format!(
                    "Failed to retrieve message descriptor from file descriptor for {}",
                    proto.message_name
                ),
            )
        });
    }

    let fds = FileDescriptor::new_dynamic_fds(
        file_descriptors.clone(),
        &WELLKNOWN_PROTOS_TO_DESCRIPTOR.values().cloned().collect::<Vec<_>>(),
    )
    .map_err(|e| {
        SdvStatus::with_message(SdvStatusCode::NotFound, format!("Failed new_dynamic_fds: {e:?}",))
    })?;

    for file_descriptor in fds {
        if let Some(message_descriptor) = file_descriptor.message_by_full_name(&proto.message_name)
        {
            return Ok(message_descriptor);
        }
    }

    Err(SdvStatus::with_message(
        SdvStatusCode::NotFound,
        format!("Message descriptor not found for message name {}", proto.message_name,),
    ))
}

/// Deserialize UnitType.
pub fn unit_type_from_proto(proto: &UnitTypeProto) -> SdvResult<UnitType> {
    UnitType::builder()
        .set_sdv_package_name(proto.package.clone())
        .set_service_bundle_name(proto.bundle.clone())
        .set_unit_type_name(proto.type_name.clone())
        .build()
}

/// Serialize UnitType.
pub fn unit_type_to_proto(unit_type: &UnitType) -> UnitTypeProto {
    UnitTypeProto {
        package: unit_type.get_sdv_package_name().to_owned(),
        bundle: unit_type.get_service_bundle_name().to_owned(),
        type_name: unit_type.get_unit_type_name().to_owned(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::assert_that;
    use googletest::matchers::eq;
    use protobuf::well_known_types::timestamp::Timestamp;
    use protobuf::MessageDyn;
    use vsidl_provider_agent_api_test_protos::message::TestMessage;
    use vsidl_provider_agent_api_test_protos::message_with_deps::TestMessageWithDeps;
    use vsidl_provider_agent_api_test_protos::message_with_more_deps::{
        self, TestMessageWithMoreDeps,
    };

    #[googletest::test]
    fn test_wellknown_message_conversion() {
        let message_descriptor_proto =
            message_descriptor_to_proto(Timestamp::default().descriptor_dyn());

        assert_that!((message_descriptor_proto).file_descriptors.len(), eq(0));

        let message_descriptor = message_descriptor_from_proto(&message_descriptor_proto)
            .expect("MessageDescriptor for google.protobuf.Timestamp");

        assert_that!(message_descriptor.full_name(), eq("google.protobuf.Timestamp"));
        assert_that!(message_descriptor.proto(), eq(Timestamp::default().descriptor_dyn().proto()));
    }

    #[googletest::test]
    fn test_custom_message_conversion() {
        let message_descriptor_proto =
            message_descriptor_to_proto(TestMessage::default().descriptor_dyn());

        assert_that!((message_descriptor_proto).file_descriptors.len(), eq(1));

        let message_descriptor = message_descriptor_from_proto(&message_descriptor_proto)
            .expect("MessageDescriptor for TestMessage");

        assert_that!(
            message_descriptor.full_name(),
            eq("com.vsidl_provider_agent_api_test.TestMessage")
        );
        assert_that!(
            message_descriptor.proto(),
            eq(TestMessage::default().descriptor_dyn().proto())
        );
    }

    #[googletest::test]
    fn test_message_with_dependencies_conversion() {
        let message_descriptor_proto =
            message_descriptor_to_proto(TestMessageWithDeps::default().descriptor_dyn());

        assert_that!((message_descriptor_proto).file_descriptors.len(), eq(1));

        let message_descriptor = message_descriptor_from_proto(&message_descriptor_proto)
            .expect("MessageDescriptor for TestMessageWithDeps");

        assert_that!(
            message_descriptor.full_name(),
            eq("com.vsidl_provider_agent_api_test.TestMessageWithDeps")
        );
        assert_that!(
            message_descriptor.proto(),
            eq(TestMessageWithDeps::default().descriptor_dyn().proto())
        );
    }

    #[googletest::test]
    fn test_message_with_overlapping_dependencies_conversion() {
        let message_descriptor_proto =
            message_descriptor_to_proto(TestMessageWithMoreDeps::default().descriptor_dyn());

        assert_that!((message_descriptor_proto).file_descriptors.len(), eq(3));

        let message_descriptor = message_descriptor_from_proto(&message_descriptor_proto)
            .expect("MessageDescriptor for TestMessageWithMoreDeps");

        assert_that!(
            message_descriptor.full_name(),
            eq("com.vsidl_provider_agent_api_test.TestMessageWithMoreDeps")
        );
        assert_that!(
            message_descriptor.proto(),
            eq(TestMessageWithMoreDeps::default().descriptor_dyn().proto())
        );
    }

    #[googletest::test]
    fn test_malformed_conversion() {
        let mut message_descriptor_proto =
            message_descriptor_to_proto(TestMessageWithDeps::default().descriptor_dyn());
        message_descriptor_proto.file_descriptors.clear();

        let message_descriptor = message_descriptor_from_proto(&message_descriptor_proto);

        assert!(message_descriptor.is_err());
    }

    #[googletest::test]
    fn test_shallow_conversion_for_message_with_dependencies() {
        let mut message_descriptor_proto =
            message_descriptor_to_proto(TestMessageWithMoreDeps::default().descriptor_dyn());

        message_descriptor_proto.file_descriptors =
            vec![message_with_more_deps::file_descriptor().proto().clone()];

        let message_descriptor = message_descriptor_from_proto(&message_descriptor_proto);

        assert!(message_descriptor.is_err());
    }

    #[googletest::test]
    fn test_unit_type_to_proto() {
        let unit_type = UnitType::builder()
            .set_sdv_package_name("com.sdv.package")
            .set_service_bundle_name("Bundle")
            .set_unit_type_name("Type")
            .build()
            .unwrap();

        let expected_proto = UnitTypeProto {
            package: "com.sdv.package".to_string(),
            bundle: "".to_string(), // service bundle as part of unit types is deprecated
            type_name: "Type".to_string(),
            ..Default::default()
        };

        assert_that!(unit_type_to_proto(&unit_type), eq(&expected_proto));
    }

    #[googletest::test]
    fn test_unit_type_from_proto() {
        let unit_type_proto = UnitTypeProto {
            package: "com.sdv.package".to_string(),
            bundle: "".to_string(),
            type_name: "Type".to_string(),
            ..Default::default()
        };

        let expected_unit_type = UnitType::builder()
            .set_sdv_package_name("com.sdv.package")
            .set_unit_type_name("Type")
            .build()
            .unwrap();

        assert_that!(unit_type_from_proto(&unit_type_proto).unwrap(), eq(&expected_unit_type));
    }
}
