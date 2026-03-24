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
use vsidl_v1::someip::some_ip_mapping::some_ip_instance_mapping::SdvTopicParameters;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct SomeIpEventCapacity {
    finding_collector: FindingCollector,
}

impl_context_condition!(SomeIpEventCapacity);

impl SomeIpEventCapacity {
    fn validate_capacity(&mut self, capacity: u32, span: &vsidl_v1::span::Span, topic: &str) {
        if capacity < 2 {
            self.finding_collector.error(
                FindingId::PublisherCapacity,
                format!(
                    "SOME/IP topic {} capacity must be at least 2, but was {}",
                    topic, capacity
                ),
                vec![Some(span.clone())],
            );
        } else if !capacity.is_multiple_of(2) {
            self.finding_collector.error(
                FindingId::PublisherCapacity,
                format!(
                    "SOME/IP topic {} capacity must be an even number, but was {}",
                    topic, capacity
                ),
                vec![Some(span.clone())],
            );
        }
    }
}

impl VsidlVisitor for SomeIpEventCapacity {
    fn start_visit_sdv_topic_parameters(&mut self, node: &SdvTopicParameters) {
        if let Some(span) = node.capacity_span.as_ref() {
            self.validate_capacity(node.capacity, span, &node.topic);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_condition::ContextCondition;
    use crate::finding_severity::FindingSeverity;
    use std::collections::HashMap;
    use vsidl_v1::someip::some_ip_mapping::some_ip_instance_mapping::SdvTopicParameters;
    use vsidl_v1::someip::some_ip_mapping::SomeIpInstanceMapping;
    use vsidl_v1::someip::SomeIpMapping;
    use vsidl_v1::syntax::VsidlEntry;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_invalid_small_capacity() {
        let mut sdv_topic_parameters = SdvTopicParameters::new();
        sdv_topic_parameters.capacity = 1;
        sdv_topic_parameters.capacity_span =
            protobuf::MessageField::some(vsidl_v1::span::Span::default());
        let topic_parameterization_map = HashMap::from([(0, sdv_topic_parameters)]);

        let mut instance_node = SomeIpInstanceMapping::new();
        instance_node.sdv_topic_parameterization = topic_parameterization_map;
        let mut service_mapping = vsidl_v1::someip::some_ip_mapping::SomeIpServiceMapping::new();
        service_mapping.instance.push(instance_node);
        let mut someip_mapping = SomeIpMapping::new();
        someip_mapping.set_someip_service_mapping(service_mapping);
        let mut vsidl_entry = VsidlEntry::new();
        vsidl_entry.some_ip_mapping.push(someip_mapping);

        let mut catalog = Catalog::new();
        catalog.catalog_vsidl_files.push((std::path::PathBuf::from("test.vsidl"), vsidl_entry));

        let mut check = SomeIpEventCapacity::default();
        check.check_catalog(&catalog);

        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "capacity must be at least 2"));
    }

    #[test]
    fn test_invalid_odd_capacity() {
        let mut sdv_topic_parameters = SdvTopicParameters::new();
        sdv_topic_parameters.capacity = 11;
        sdv_topic_parameters.capacity_span =
            protobuf::MessageField::some(vsidl_v1::span::Span::default());
        let topic_parameterization_map = HashMap::from([(0, sdv_topic_parameters)]);

        let mut instance_node = SomeIpInstanceMapping::new();
        instance_node.sdv_topic_parameterization = topic_parameterization_map;
        let mut service_mapping = vsidl_v1::someip::some_ip_mapping::SomeIpServiceMapping::new();
        service_mapping.instance.push(instance_node);
        let mut someip_mapping = SomeIpMapping::new();
        someip_mapping.set_someip_service_mapping(service_mapping);
        let mut vsidl_entry = VsidlEntry::new();
        vsidl_entry.some_ip_mapping.push(someip_mapping);

        let mut catalog = Catalog::new();
        catalog.catalog_vsidl_files.push((std::path::PathBuf::from("test.vsidl"), vsidl_entry));

        let mut check = SomeIpEventCapacity::default();
        check.check_catalog(&catalog);

        assert!(check.is_err());
        assert!(
            check.any_finding_contains(FindingSeverity::Error, "capacity must be an even number")
        );
    }
}
