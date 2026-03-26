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
pub struct SomeIpEventTopicCapacityMandatory {
    finding_collector: FindingCollector,
}

impl_context_condition!(SomeIpEventTopicCapacityMandatory);

impl VsidlVisitor for SomeIpEventTopicCapacityMandatory {
    fn start_visit_sdv_topic_parameters(&mut self, node: &SdvTopicParameters) {
        let has_topic = !node.topic.is_empty();
        let has_capacity = node.capacity_span.is_some();

        if has_topic && !has_capacity {
            self.finding_collector.error(
                FindingId::SomeIpTopicAndCapacityMandatoryTogether,
                format!("SOME/IP topic {} is missing mandatory 'capacity'", node.topic),
                Some("Missing mandatory 'capacity'".to_string()),
                vec![node.span.clone().into_option()],
            );
        } else if !has_topic && has_capacity {
            self.finding_collector.error(
                FindingId::SomeIpTopicAndCapacityMandatoryTogether,
                "SOME/IP SDV topic parameter defines 'capacity' but missing mandatory 'topic'"
                    .to_string(),
                Some("Missing mandatory 'topic'".to_string()),
                vec![node.span.clone().into_option()],
            );
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
    fn test_event_missing_capacity() {
        let mut sdv_topic_parameters = SdvTopicParameters::new();
        sdv_topic_parameters.topic = "my-topic".to_string();
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

        let mut check = SomeIpEventTopicCapacityMandatory::default();
        check.check_catalog(&catalog);

        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "missing mandatory 'capacity'"));
    }

    #[test]
    fn test_event_missing_topic() {
        let mut sdv_topic_parameters = SdvTopicParameters::new();
        sdv_topic_parameters.capacity = 8;
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

        let mut check = SomeIpEventTopicCapacityMandatory::default();
        check.check_catalog(&catalog);

        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "defines 'capacity' but missing mandatory 'topic'"
        ));
    }

    #[test]
    fn test_simple_event() {
        let mut sdv_topic_parameters = SdvTopicParameters::new();
        sdv_topic_parameters.topic = "my-topic".to_string();
        sdv_topic_parameters.capacity = 8;
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

        let mut check = SomeIpEventTopicCapacityMandatory::default();
        check.check_catalog(&catalog);

        assert!(check.is_ok());
    }
}
