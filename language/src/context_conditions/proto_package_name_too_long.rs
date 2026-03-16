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
use protobuf::descriptor::FileDescriptorProto as ProtoFileDescriptor;
use sdv_comms_id_validator::package_identifier_validation::PACKAGE_IDENTIFIER_MAX_LENGTH_BYTES;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct ProtoPackageNameTooLong {
    finding_collector: FindingCollector,
}

impl_context_condition!(ProtoPackageNameTooLong);

impl VsidlVisitor for ProtoPackageNameTooLong {
    fn start_visit_file_descriptor_proto(&mut self, node: &ProtoFileDescriptor) {
        if let Some(package) = node.package.as_ref() {
            let package_str: &str = package;
            if package_str.len() > PACKAGE_IDENTIFIER_MAX_LENGTH_BYTES {
                self.finding_collector.error(
                    FindingId::ProtoPackageNameTooLong,
                    format!(
                        "Proto package name '{}' exceeds the length limit of {} characters",
                        package_str, PACKAGE_IDENTIFIER_MAX_LENGTH_BYTES
                    ),
                    Vec::new(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_condition::ContextCondition;
    use crate::FindingSeverity;

    #[test]
    fn test_should_reject_long_proto_package_name() {
        let mut check = ProtoPackageNameTooLong::default();
        let mut descriptor = ProtoFileDescriptor::new();
        descriptor.package = Some("a".repeat(128));

        check.start_visit_file_descriptor_proto(&descriptor);
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Proto package name 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' exceeds the length limit"
        ));
    }

    #[test]
    fn test_should_accept_short_proto_package_name() {
        let mut check = ProtoPackageNameTooLong::default();
        let mut descriptor = ProtoFileDescriptor::new();
        descriptor.package = Some("a".repeat(127));

        check.start_visit_file_descriptor_proto(&descriptor);
        assert!(check.is_ok());
    }
}
