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
use protobuf::descriptor::DescriptorProto as ProtoDescriptor;
use sdv_comms_id_validator::unit_type_name_validation::UNIT_TYPE_NAME_MAX_LENGTH_BYTES;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct ProtoDescriptorNameTooLong {
    finding_collector: FindingCollector,
}

impl_context_condition!(ProtoDescriptorNameTooLong);

impl VsidlVisitor for ProtoDescriptorNameTooLong {
    fn start_visit_proto_descriptor(&mut self, node: &ProtoDescriptor) {
        if let Some(name) = node.name.as_ref() {
            let name_str: &str = name;
            if name_str.len() > UNIT_TYPE_NAME_MAX_LENGTH_BYTES {
                self.finding_collector.error(
                    FindingId::ProtoDescriptorNameTooLong,
                    format!(
                        "Proto message name '{}' exceeds the length limit of {} characters",
                        name_str, UNIT_TYPE_NAME_MAX_LENGTH_BYTES
                    ),
                    vec![None],
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
    fn test_should_reject_long_proto_descriptor_name() {
        let mut check = ProtoDescriptorNameTooLong::default();
        let mut descriptor = ProtoDescriptor::new();
        descriptor.name = Some("a".repeat(96));

        check.start_visit_proto_descriptor(&descriptor);
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Proto message name 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' exceeds the length limit"
        ));
    }

    #[test]
    fn test_should_accept_short_proto_descriptor_name() {
        let mut check = ProtoDescriptorNameTooLong::default();
        let mut descriptor = ProtoDescriptor::new();
        descriptor.name = Some("a".repeat(95));

        check.start_visit_proto_descriptor(&descriptor);
        assert!(check.is_ok());
    }
}
