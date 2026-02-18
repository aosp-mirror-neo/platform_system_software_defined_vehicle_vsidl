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
//! API declarations for VSIDL configuration provider.

use async_trait::async_trait;
use futures::Stream;
use log::error;
use mockall::mock;
use protobuf::reflect::MessageDescriptor;
use sdv::status::SdvResult;
use sdv_comms::id::ServiceFqin;
use sdv_comms::sd::UnitType;
use std::collections::HashMap;
use std::fmt::Display;
use std::marker::{Send, Sync};
use std::pin::Pin;
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;
use vsidl_v1::someip::SomeIpMapping;

/// Trait for VSIDL config provider.
///
/// Allows to retrieve metadata from VSIDL catalog, as well as SOME/IP and Diagnostics-specific metadata.
/// Deprecated in SDV 2.0, use VsidlConfigProvider trait instead for better error handling.
#[async_trait]
#[deprecated(since = "2.0.0", note = "Use VsidlConfigProvider trait instead.")]
pub trait VsidlConfigProviderV1: Sync + Send + Display {
    /// Returns metadata for a publication, provided by source [ServiceFqin].
    async fn get_publication_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
    ) -> Option<PublicationDescriptor>;

    /// Returns metadata for a RPC server method, provided by source [ServiceFqin].
    async fn get_rpc_method_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
        method_name: &str,
    ) -> Option<RpcMethodDescriptor>;

    /// Returns [MessageDescriptor] for a message, provided by source [ServiceFqin].
    async fn get_message_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        message_name: &str,
    ) -> Option<MessageDescriptor>;

    /// Returns a list of SOME/IP mappings available on a local VM, provided by source [ServiceFqin].
    async fn get_someip_mappings(&self, source_fqin: &ServiceFqin) -> Option<Vec<SomeIpMapping>>;

    /// Returns [DiagnosticsDeclaration] for the provided [ServiceFqin].
    async fn get_diagnostics_declaration(
        &self,
        fqin: &ServiceFqin,
    ) -> Option<DiagnosticsDeclaration>;
}

/// Event indicating a change in availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailabilityChangeEvent {
    /// An Event fired when a new Service Unit was registered.
    /// This corresponds to the ServiceUnitChangeEvent::Registered event from SDV Comms.
    Available,
    /// An Event fired when a new Service Unit was unregistered.
    /// This corresponds to the ServiceUnitChangeEvent::Unregistered event from SDV Comms.
    Unavailable,
}

/// Type alias for availability stream.
pub type AvailabilityStream = Pin<Box<dyn Stream<Item = AvailabilityChangeEvent> + Send>>;

/// Trait for VSIDL config provider.
///
/// Allows to retrieve metadata from VSIDL catalog, as well as SOME/IP and Diagnostics-specific metadata.
#[async_trait]
pub trait VsidlConfigProvider: Sync + Send + Display {
    /// Returns an availability stream for the specified virtual machine.
    ///
    /// This method serves as a fallback mechanism. It is intended to be utilized
    /// in scenarios where VSIDL Provider API calls are unsuccessful, specifically
    /// when those calls return `SdvStatusCode::Unavailable`.
    ///
    /// The stream will immediately emmit the current availability.
    /// The stream behaves the same way as subscribe_service_unit_change_by_name of ServiceDiscoveryManager
    async fn subscribe_availability_change_by_vm(
        &self,
        vm_name: &str,
    ) -> SdvResult<AvailabilityStream>;

    /// Returns metadata for a publication, provided by source [ServiceFqin].
    async fn get_publication_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
    ) -> SdvResult<PublicationDescriptor>;

    /// Returns metadata for a RPC server method, provided by source [ServiceFqin].
    async fn get_rpc_method_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
        method_name: &str,
    ) -> SdvResult<RpcMethodDescriptor>;

    /// Returns [MessageDescriptor] for a message, provided by source [ServiceFqin].
    async fn get_message_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        message_name: &str,
    ) -> SdvResult<MessageDescriptor>;

    /// Returns a list of SOME/IP mappings available on a local VM, provided by source [ServiceFqin].
    async fn get_someip_mappings(&self, source_fqin: &ServiceFqin)
        -> SdvResult<Vec<SomeIpMapping>>;

    /// Returns [DiagnosticsDeclaration] for the provided [ServiceFqin].
    async fn get_diagnostics_declaration(
        &self,
        fqin: &ServiceFqin,
    ) -> SdvResult<DiagnosticsDeclaration>;
}

/// Enables backward compatibility with old trait, while still allowing us to only have a single implementation for the new trait.
#[async_trait]
impl<T> VsidlConfigProviderV1 for T
where
    T: VsidlConfigProvider,
{
    async fn get_publication_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
    ) -> Option<PublicationDescriptor> {
        match VsidlConfigProvider::get_publication_descriptor(self, source_fqin, unit_type).await {
            Ok(result) => Some(result),
            Err(e) => {
                error!("Failed to get publication descriptor: {e}");
                None
            }
        }
    }

    async fn get_rpc_method_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
        method_name: &str,
    ) -> Option<RpcMethodDescriptor> {
        match VsidlConfigProvider::get_rpc_method_descriptor(
            self,
            source_fqin,
            unit_type,
            method_name,
        )
        .await
        {
            Ok(result) => Some(result),
            Err(e) => {
                error!("Failed to get rpc method descriptor: {e}");
                None
            }
        }
    }

    async fn get_message_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        message_name: &str,
    ) -> Option<MessageDescriptor> {
        match VsidlConfigProvider::get_message_descriptor(self, source_fqin, message_name).await {
            Ok(result) => Some(result),
            Err(e) => {
                error!("Failed to get message descriptor: {e}");
                None
            }
        }
    }

    async fn get_someip_mappings(&self, source_fqin: &ServiceFqin) -> Option<Vec<SomeIpMapping>> {
        match VsidlConfigProvider::get_someip_mappings(self, source_fqin).await {
            Ok(result) => Some(result),
            Err(e) => {
                error!("Failed to get someip mappings: {e}");
                None
            }
        }
    }

    async fn get_diagnostics_declaration(
        &self,
        fqin: &ServiceFqin,
    ) -> Option<DiagnosticsDeclaration> {
        match VsidlConfigProvider::get_diagnostics_declaration(self, fqin).await {
            Ok(result) => Some(result),
            Err(e) => {
                error!("Failed to get diagnostics declaration: {e}");
                None
            }
        }
    }
}

mock! {
    pub VsidlConfigProviderV1 {}

    #[async_trait]
    impl VsidlConfigProviderV1 for VsidlConfigProviderV1 {
        /// Returns metadata for a publication, provided by source [ServiceFqin].
        async fn get_publication_descriptor(
            &self,
            source_fqin: &ServiceFqin,
            unit_type: &UnitType,
        ) -> Option<PublicationDescriptor>;

        /// Returns metadata for a RPC server method, provided by source [ServiceFqin].
        async fn get_rpc_method_descriptor(
            &self,
            source_fqin: &ServiceFqin,
            unit_type: &UnitType,
            method_name: &str,
        ) -> Option<RpcMethodDescriptor>;

        /// Returns [MessageDescriptor] for a message, provided by source [ServiceFqin].
        async fn get_message_descriptor(
            &self,
            source_fqin: &ServiceFqin,
            message_name: &str,
        ) -> Option<MessageDescriptor>;

        /// Returns a list of SOME/IP mappings available on a local VM, provided by source [ServiceFqin].
        async fn get_someip_mappings(&self, source_fqin: &ServiceFqin) -> Option<Vec<SomeIpMapping>>;

        /// Returns [DiagnosticsDeclaration] for the provided [ServiceFqin].
        async fn get_diagnostics_declaration(
            &self,
            fqin: &ServiceFqin,
        ) -> Option<DiagnosticsDeclaration>;
    }

    impl Display for VsidlConfigProviderV1 {
        fn fmt<'a>(&self, f: &mut std::fmt::Formatter<'a>) -> std::fmt::Result;
    }
}

mock! {
    pub VsidlConfigProvider {}

    #[async_trait]
    impl VsidlConfigProvider for VsidlConfigProvider {
        async fn subscribe_availability_change_by_vm(
            &self,
            vm_name: &str,
        ) -> SdvResult<AvailabilityStream>;

        /// Returns metadata for a publication, provided by source [ServiceFqin].
        async fn get_publication_descriptor(
            &self,
            source_fqin: &ServiceFqin,
            unit_type: &UnitType,
        ) -> SdvResult<PublicationDescriptor>;

        /// Returns metadata for a RPC server method, provided by source [ServiceFqin].
        async fn get_rpc_method_descriptor(
            &self,
            source_fqin: &ServiceFqin,
            unit_type: &UnitType,
            method_name: &str,
        ) -> SdvResult<RpcMethodDescriptor>;

        /// Returns [MessageDescriptor] for a message, provided by source [ServiceFqin].
        async fn get_message_descriptor(
            &self,
            source_fqin: &ServiceFqin,
            message_name: &str,
        ) -> SdvResult<MessageDescriptor>;

        /// Returns a list of SOME/IP mappings available on a local VM, provided by source [ServiceFqin].
        async fn get_someip_mappings(&self, source_fqin: &ServiceFqin) -> SdvResult<Vec<SomeIpMapping>>;

        /// Returns [DiagnosticsDeclaration] for the provided [ServiceFqin].
        async fn get_diagnostics_declaration(
            &self,
            fqin: &ServiceFqin,
        ) -> SdvResult<DiagnosticsDeclaration>;
    }

    impl Display for VsidlConfigProvider {
        fn fmt<'a>(&self, f: &mut std::fmt::Formatter<'a>) -> std::fmt::Result;
    }
}

/// RPC method runtime information
#[derive(Debug, PartialEq, Clone, Default)]
pub struct RpcMethodDescriptor {
    /// Message descriptor for RPC request.
    pub request_message: Option<MessageDescriptor>,

    /// Message descriptor for RPC response.
    pub response_message: Option<MessageDescriptor>,
}

/// Publication runtime information.
#[derive(Debug, PartialEq, Clone)]
pub struct PublicationDescriptor {
    /// Message descriptor for publication.
    pub message_descriptor: MessageDescriptor,

    /// Information about the size of the publication message.
    pub size_metadata: SizeMetadata,
}

/// Information about the size of the publication message.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SizeMetadata {
    /// Message size in bytes.
    pub message_size: u32,

    /// Message count.
    pub message_count: u32,

    /// VSIDL-declared size constraints for the message fields.
    pub field_size_constraints: HashMap<String, FieldSizeConstraint>,
}

/// VSIDL-declared size constraints for the message field.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FieldSizeConstraint {
    /// Max number of repeated fields allowed for the field.
    pub repeated_max_count: Option<u32>,

    /// Max size of variable sized types (string, bytes) in bytes.
    pub variable_type_max_size: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use protobuf::well_known_types::empty::Empty;
    use sdv_status_rs::{SdvStatus, SdvStatusCode};
    use std::str::FromStr;

    fn get_fqin() -> ServiceFqin {
        ServiceFqin::from_str("vm:pkg.bundle.Service/instance").unwrap()
    }

    fn get_unit_type() -> UnitType {
        UnitType::from_str("pkg.bundle.Unit").unwrap()
    }

    #[tokio::test]
    async fn test_blanket_impl_get_publication_descriptor() {
        let mut mock = MockVsidlConfigProvider::new();
        let expected = PublicationDescriptor {
            message_descriptor: MessageDescriptor::for_type::<Empty>(),
            size_metadata: SizeMetadata::default(),
        };
        let ret = expected.clone();

        mock.expect_get_publication_descriptor().return_once(move |_, _| Ok(ret));

        let result =
            VsidlConfigProviderV1::get_publication_descriptor(&mock, &get_fqin(), &get_unit_type())
                .await;
        assert_eq!(result, Some(expected));
    }

    #[tokio::test]
    async fn test_blanket_impl_get_publication_descriptor_error() {
        let mut mock = MockVsidlConfigProvider::new();
        mock.expect_get_publication_descriptor()
            .return_once(|_, _| Err(SdvStatus::new(SdvStatusCode::Internal)));

        let result =
            VsidlConfigProviderV1::get_publication_descriptor(&mock, &get_fqin(), &get_unit_type())
                .await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_blanket_impl_get_rpc_method_descriptor() {
        let mut mock = MockVsidlConfigProvider::new();
        let expected = RpcMethodDescriptor::default();
        let ret = expected.clone();

        mock.expect_get_rpc_method_descriptor().return_once(move |_, _, _| Ok(ret));

        let result = VsidlConfigProviderV1::get_rpc_method_descriptor(
            &mock,
            &get_fqin(),
            &get_unit_type(),
            "method",
        )
        .await;
        assert_eq!(result, Some(expected));
    }

    #[tokio::test]
    async fn test_blanket_impl_get_rpc_method_descriptor_error() {
        let mut mock = MockVsidlConfigProvider::new();
        mock.expect_get_rpc_method_descriptor()
            .return_once(|_, _, _| Err(SdvStatus::new(SdvStatusCode::Internal)));

        let result = VsidlConfigProviderV1::get_rpc_method_descriptor(
            &mock,
            &get_fqin(),
            &get_unit_type(),
            "method",
        )
        .await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_blanket_impl_get_message_descriptor() {
        let mut mock = MockVsidlConfigProvider::new();
        let expected = MessageDescriptor::for_type::<Empty>();
        let ret = expected.clone();

        mock.expect_get_message_descriptor().return_once(move |_, _| Ok(ret));

        let result =
            VsidlConfigProviderV1::get_message_descriptor(&mock, &get_fqin(), "message").await;
        assert_eq!(result, Some(expected));
    }

    #[tokio::test]
    async fn test_blanket_impl_get_message_descriptor_error() {
        let mut mock = MockVsidlConfigProvider::new();
        mock.expect_get_message_descriptor()
            .return_once(|_, _| Err(SdvStatus::new(SdvStatusCode::Internal)));

        let result =
            VsidlConfigProviderV1::get_message_descriptor(&mock, &get_fqin(), "message").await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_blanket_impl_get_someip_mappings() {
        let mut mock = MockVsidlConfigProvider::new();
        let expected = vec![SomeIpMapping::default()];
        let ret = expected.clone();

        mock.expect_get_someip_mappings().return_once(move |_| Ok(ret));

        let result = VsidlConfigProviderV1::get_someip_mappings(&mock, &get_fqin()).await;
        assert_eq!(result, Some(expected));
    }

    #[tokio::test]
    async fn test_blanket_impl_get_someip_mappings_error() {
        let mut mock = MockVsidlConfigProvider::new();
        mock.expect_get_someip_mappings()
            .return_once(|_| Err(SdvStatus::new(SdvStatusCode::Internal)));

        let result = VsidlConfigProviderV1::get_someip_mappings(&mock, &get_fqin()).await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_blanket_impl_get_diagnostics_declaration() {
        let mut mock = MockVsidlConfigProvider::new();
        let expected = DiagnosticsDeclaration::default();
        let ret = expected.clone();

        mock.expect_get_diagnostics_declaration().return_once(move |_| Ok(ret));

        let result = VsidlConfigProviderV1::get_diagnostics_declaration(&mock, &get_fqin()).await;
        assert_eq!(result, Some(expected));
    }

    #[tokio::test]
    async fn test_blanket_impl_get_diagnostics_declaration_error() {
        let mut mock = MockVsidlConfigProvider::new();
        mock.expect_get_diagnostics_declaration()
            .return_once(|_| Err(SdvStatus::new(SdvStatusCode::Internal)));

        let result = VsidlConfigProviderV1::get_diagnostics_declaration(&mock, &get_fqin()).await;
        assert_eq!(result, None);
    }
}
