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

use super::{FindingCollector, FindingId};
use crate::impl_context_condition;
use protobuf::descriptor::{MethodDescriptorProto, ServiceDescriptorProto};
use vsidl_v1_common::visitors::visitor::{ProtoFile, VsidlVisitor};

#[derive(Debug, Default)]
pub struct RpcStreaming {
    finding_collector: FindingCollector,
    protofile_name: String,
    service_name: String,
}
impl_context_condition!(RpcStreaming);

/// Issues an error if bidirectional streaming method is defined in proto file
impl VsidlVisitor for RpcStreaming {
    fn start_visit_proto_file(&mut self, node: &ProtoFile) {
        self.protofile_name = node.1.name().to_string();
    }

    fn start_visit_proto_service_descriptor(&mut self, node: &ServiceDescriptorProto) {
        self.service_name = node.name().to_string();
    }

    fn start_visit_proto_method_descriptor(&mut self, node: &MethodDescriptorProto) {
        if node.client_streaming() && node.server_streaming() {
            self.finding_collector.error(
                FindingId::RpcMethodBidirectionalStreaming,
                format!(
                    "File name: {}, bidirectional streaming method '{}' in service '{}' is not supported",
                    self.protofile_name.clone(),
                    node.name(),
                    self.service_name.clone()
                ),
                None, Vec::new(),
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::rpc_streaming::RpcStreaming;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_rpc_streaming_ok() {
        // GIVEN a service with no bidirectional streaming method
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_RPC_STREAMING_VALID.clone()]);

        // WHEN executing context condition
        let mut check = RpcStreaming::default();
        check.check_catalog(&catalog);

        // THEN check should be OK
        assert!(check.is_ok());
    }

    #[test]
    fn test_rpc_streaming_error() {
        // GIVEN a service with bidirectional streaming method
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_RPC_STREAMING_INVALID.clone()]);

        // WHEN executing context condition
        let mut check = RpcStreaming::default();
        check.check_catalog(&catalog);

        // THEN error message should be found
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: rpc_streaming_call_invalid.proto, bidirectional streaming method 'method_with_bidirectional_streaming' in service 'StreamingRpc' is not supported"
        ));
    }
}
