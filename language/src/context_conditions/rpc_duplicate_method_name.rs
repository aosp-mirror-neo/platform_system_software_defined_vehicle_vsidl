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
use protobuf::descriptor::ServiceDescriptorProto;
use std::collections::HashSet;
use vsidl_v1_common::visitors::visitor::{ProtoFile, VsidlVisitor};

#[derive(Debug, Default)]
pub struct RpcDuplicateMethodName {
    finding_collector: FindingCollector,
    protofile_name: String,
}
impl_context_condition!(RpcDuplicateMethodName);

/// Issues an error if more than one method used same name in proto file service
impl VsidlVisitor for RpcDuplicateMethodName {
    fn start_visit_proto_file(&mut self, node: &ProtoFile) {
        self.protofile_name = node.1.name().to_string();
    }

    fn start_visit_proto_service_descriptor(&mut self, node: &ServiceDescriptorProto) {
        let mut seen_method_names: HashSet<String> = HashSet::new();
        let mut duplicate_method_names: HashSet<String> = HashSet::new();

        for method in &node.method {
            let method_name = method.name().to_string();
            if !seen_method_names.insert(method_name.clone()) {
                duplicate_method_names.insert(method_name);
            }
        }

        for method_name in duplicate_method_names {
            self.finding_collector.error(
                FindingId::RpcDuplicateMethodName,
                format!(
                    "File name: {}, method name '{}' in service '{}' is duplicated",
                    self.protofile_name.clone(),
                    method_name,
                    node.name()
                ),
                Vec::new(),
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::rpc_duplicate_method_name::RpcDuplicateMethodName;
    use crate::FindingSeverity;
    use protobuf::descriptor::{
        FileDescriptorProto, MethodDescriptorProto, ServiceDescriptorProto,
    };
    use vsidl_v1_common::catalog::ProtoBuildTargetMetadata;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_rpc_no_duplicate_method_name_ok() {
        // GIVEN a service with bidirectional streaming method
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone()]);

        // WHEN executing context condition
        let mut check = RpcDuplicateMethodName::default();
        check.check_catalog(&catalog);

        // THEN check should be ok
        assert!(check.is_ok());
    }

    #[test]
    fn test_rpc_duplicate_method_name_error() {
        // GIVEN a service with duplicate method names,
        // constructed programmatically as a proto file with this error would not compile
        let mut catalog = Catalog::new();
        let service = ServiceDescriptorProto {
            name: Some("DuplicateMethodNameService".to_string()),
            method: vec![
                MethodDescriptorProto {
                    name: Some("rpc_unary_method".to_string()),
                    ..Default::default()
                },
                MethodDescriptorProto {
                    name: Some("rpc_unary_method".to_string()),
                    ..Default::default()
                },
                MethodDescriptorProto {
                    name: Some("rpc_streaming_method".to_string()),
                    client_streaming: Some(true),
                    ..Default::default()
                },
                MethodDescriptorProto {
                    name: Some("rpc_streaming_method".to_string()),
                    server_streaming: Some(true),
                    ..Default::default()
                },
                MethodDescriptorProto {
                    name: Some("rpc_hybrid_method".to_string()),
                    ..Default::default()
                },
                MethodDescriptorProto {
                    name: Some("rpc_hybrid_method".to_string()),
                    client_streaming: Some(true),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let file_descriptor = FileDescriptorProto {
            name: Some("rpc_duplicate_method_name.proto".to_string()),
            service: vec![service],
            ..Default::default()
        };

        let metadata = ProtoBuildTargetMetadata {
            folder: std::path::PathBuf::new(),
            target_name: Some("test_target".to_string()),
            crate_name: Some("test_crate".to_string()),
        };

        catalog.proto_descriptors.push((metadata, file_descriptor));

        // WHEN executing context condition
        let mut check = RpcDuplicateMethodName::default();
        check.check_catalog(&catalog);

        // THEN error message should be found
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: rpc_duplicate_method_name.proto, method name 'rpc_unary_method' in service 'DuplicateMethodNameService' is duplicated"
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: rpc_duplicate_method_name.proto, method name 'rpc_streaming_method' in service 'DuplicateMethodNameService' is duplicated"
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: rpc_duplicate_method_name.proto, method name 'rpc_hybrid_method' in service 'DuplicateMethodNameService' is duplicated"
        ));
    }

    #[test]
    fn test_rpc_duplicate_method_name_hybrid_result() {
        // GIVEN a service with duplicate method names,
        // constructed programmatically as a proto file with this error would not compile
        let mut catalog = Catalog::new();
        let service = ServiceDescriptorProto {
            name: Some("DuplicateMethodNameService".to_string()),
            method: vec![
                MethodDescriptorProto {
                    name: Some("rpc_unary_method".to_string()),
                    ..Default::default()
                },
                MethodDescriptorProto {
                    name: Some("rpc_unary_method".to_string()),
                    ..Default::default()
                },
                MethodDescriptorProto {
                    name: Some("rpc_unique_method".to_string()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let file_descriptor = FileDescriptorProto {
            name: Some("rpc_duplicate_method_name.proto".to_string()),
            service: vec![service],
            ..Default::default()
        };

        let metadata = ProtoBuildTargetMetadata {
            folder: std::path::PathBuf::new(),
            target_name: Some("test_target".to_string()),
            crate_name: Some("test_crate".to_string()),
        };

        catalog.proto_descriptors.push((metadata, file_descriptor));

        // WHEN executing context condition
        let mut check = RpcDuplicateMethodName::default();
        check.check_catalog(&catalog);

        // THEN error message should be found
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "File name: rpc_duplicate_method_name.proto, method name 'rpc_unary_method' in service 'DuplicateMethodNameService' is duplicated"
        ));

        // THEN unique method name should not be found
        assert!(!check.any_finding_contains(
            FindingSeverity::Error,
            "File name: rpc_duplicate_method_name.proto, method name 'rpc_unique_method' in service 'DuplicateMethodNameService' is duplicated"
        ));
    }
}
