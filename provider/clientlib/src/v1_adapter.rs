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

use crate::vsidl_provider_api::{
    PublicationDescriptor, RpcMethodDescriptor, VsidlConfigProvider, VsidlConfigProviderV1,
};
use async_trait::async_trait;
use log::error;
use protobuf::reflect::MessageDescriptor;
use sdv_comms::id::ServiceFqin;
use sdv_comms::sd::UnitType;
use std::fmt;
use std::sync::Arc;
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;
use vsidl_v1::someip::SomeIpMapping;

/// Adapter to convert a VsidlConfigProvider into a VsidlConfigProviderV1 trait object.
pub(crate) struct V1Adapter {
    pub(crate) inner: Arc<dyn VsidlConfigProvider>,
}

impl fmt::Display for V1Adapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "V1Adapter({})", self.inner)
    }
}

#[async_trait]
impl VsidlConfigProviderV1 for V1Adapter {
    async fn get_publication_descriptor(
        &self,
        source_fqin: &ServiceFqin,
        unit_type: &UnitType,
    ) -> Option<PublicationDescriptor> {
        match self.inner.get_publication_descriptor(source_fqin, unit_type).await {
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
        match self.inner.get_rpc_method_descriptor(source_fqin, unit_type, method_name).await {
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
        match self.inner.get_message_descriptor(source_fqin, message_name).await {
            Ok(result) => Some(result),
            Err(e) => {
                error!("Failed to get message descriptor: {e}");
                None
            }
        }
    }

    async fn get_someip_mappings(&self, source_fqin: &ServiceFqin) -> Option<Vec<SomeIpMapping>> {
        match self.inner.get_someip_mappings(source_fqin).await {
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
        match self.inner.get_diagnostics_declaration(fqin).await {
            Ok(result) => Some(result),
            Err(e) => {
                error!("Failed to get diagnostics declaration: {e}");
                None
            }
        }
    }
}
