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
use mockall::mock;
use protobuf::reflect::MessageDescriptor;
use sdv_comms::id::ServiceFqin;
use sdv_comms::sd::UnitType;
use std::collections::HashMap;
use std::fmt::Display;
use std::marker::{Send, Sync};
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;
use vsidl_v1::someip::SomeIpMapping;

/// Trait for VSIDL config provider.
///
/// Allows to retrieve metadata from VSIDL catalog, as well as SOME/IP and Diagnostics-specific metadata.
#[async_trait]
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
