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
use vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::event::{
    GroupEventPart, SimpleEvent,
};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct SomeIpEventTopicCapacityMandatory {
    finding_collector: FindingCollector,
}

impl_context_condition!(SomeIpEventTopicCapacityMandatory);

impl VsidlVisitor for SomeIpEventTopicCapacityMandatory {
    fn start_visit_someip_simple_event(&mut self, node: &SimpleEvent) {
        let has_topic = !node.topic.is_empty();
        let has_capacity = node.capacity_span.is_some();

        if has_topic && !has_capacity {
            self.finding_collector.error(
                FindingId::SomeIpTopicAndCapacityMandatoryTogether,
                format!(
                    "SOME/IP simple event mapping for unit type '{}' defines 'topic' but missing mandatory 'capacity'",
                    node.sdv_unit_type.type_name
                ),
                vec![node.span.clone().into_option()],
            );
        } else if !has_topic && has_capacity {
            self.finding_collector.error(
                FindingId::SomeIpTopicAndCapacityMandatoryTogether,
                format!(
                    "SOME/IP simple event mapping for unit type '{}' defines 'capacity' but missing mandatory 'topic'",
                    node.sdv_unit_type.type_name
                ),
                vec![node.span.clone().into_option()],
            );
        }
    }

    fn start_visit_someip_group_event_part(&mut self, node: &GroupEventPart) {
        let has_topic = !node.topic.is_empty();
        let has_capacity = node.capacity_span.is_some();

        if has_topic && !has_capacity {
            self.finding_collector.error(
                FindingId::SomeIpTopicAndCapacityMandatoryTogether,
                format!(
                    "SOME/IP group event part mapping for unit type '{}' defines 'topic' but missing mandatory 'capacity'",
                    node.sdv_unit_type.type_name
                ),
                vec![node.span.clone().into_option()],
            );
        } else if !has_topic && has_capacity {
            self.finding_collector.error(
                FindingId::SomeIpTopicAndCapacityMandatoryTogether,
                format!(
                    "SOME/IP group event part mapping for unit type '{}' defines 'capacity' but missing mandatory 'topic'",
                    node.sdv_unit_type.type_name
                ),
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
    use vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::Event as SomeIpEvent;
    use vsidl_v1::someip::some_ip_mapping::SomeIpInstanceImplementation;
    use vsidl_v1::someip::SomeIpMapping;
    use vsidl_v1::syntax::VsidlEntry;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_simple_event_missing_capacity() {
        let mut simple_event = SimpleEvent::new();
        simple_event.topic = "my-topic".to_string();
        let mut unit_type = vsidl_v1::someip::some_ip_mapping::UnitType::new();
        unit_type.type_name = "MyType".to_string();
        simple_event.sdv_unit_type = protobuf::MessageField::some(unit_type);

        let mut event = SomeIpEvent::new();
        event.set_simple_event(simple_event);
        let mut impl_node = SomeIpInstanceImplementation::new();
        impl_node.set_offered_event(event);
        let mut service_mapping = vsidl_v1::someip::some_ip_mapping::SomeIpServiceMapping::new();
        service_mapping.instance_impl.push(impl_node);
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
            "defines 'topic' but missing mandatory 'capacity'"
        ));
    }

    #[test]
    fn test_simple_event_missing_topic() {
        let mut simple_event = SimpleEvent::new();
        simple_event.capacity = 8;
        simple_event.capacity_span = protobuf::MessageField::some(vsidl_v1::span::Span::default());
        let mut unit_type = vsidl_v1::someip::some_ip_mapping::UnitType::new();
        unit_type.type_name = "MyType".to_string();
        simple_event.sdv_unit_type = protobuf::MessageField::some(unit_type);

        let mut event = SomeIpEvent::new();
        event.set_simple_event(simple_event);
        let mut impl_node = SomeIpInstanceImplementation::new();
        impl_node.set_offered_event(event);
        let mut service_mapping = vsidl_v1::someip::some_ip_mapping::SomeIpServiceMapping::new();
        service_mapping.instance_impl.push(impl_node);
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
    fn test_group_event_part_missing_capacity() {
        let mut part = GroupEventPart::new();
        part.topic = "my-topic".to_string();
        let mut unit_type = vsidl_v1::someip::some_ip_mapping::UnitType::new();
        unit_type.type_name = "MyType".to_string();
        part.sdv_unit_type = protobuf::MessageField::some(unit_type);

        let mut group =
            vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::event::GroupEvent::new();
        group.event_part.push(part);
        let mut event = SomeIpEvent::new();
        event.set_group_event(group);
        let mut impl_node = SomeIpInstanceImplementation::new();
        impl_node.set_offered_event(event);
        let mut service_mapping = vsidl_v1::someip::some_ip_mapping::SomeIpServiceMapping::new();
        service_mapping.instance_impl.push(impl_node);
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
            "defines 'topic' but missing mandatory 'capacity'"
        ));
    }

    #[test]
    fn test_group_event_part_missing_topic() {
        let mut part = GroupEventPart::new();
        part.capacity = 8;
        part.capacity_span = protobuf::MessageField::some(vsidl_v1::span::Span::default());
        let mut unit_type = vsidl_v1::someip::some_ip_mapping::UnitType::new();
        unit_type.type_name = "MyType".to_string();
        part.sdv_unit_type = protobuf::MessageField::some(unit_type);

        let mut group = vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::event::GroupEvent::new();
        group.event_part.push(part);
        let mut event = SomeIpEvent::new();
        event.set_group_event(group);
        let mut impl_node = SomeIpInstanceImplementation::new();
        impl_node.set_offered_event(event);
        let mut service_mapping = vsidl_v1::someip::some_ip_mapping::SomeIpServiceMapping::new();
        service_mapping.instance_impl.push(impl_node);
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
    fn test_valid_simple_event() {
        let mut simple_event = SimpleEvent::new();
        simple_event.topic = "my-topic".to_string();
        simple_event.capacity = 8;
        simple_event.capacity_span = protobuf::MessageField::some(vsidl_v1::span::Span::default());

        let mut event = SomeIpEvent::new();
        event.set_simple_event(simple_event);
        let mut impl_node = SomeIpInstanceImplementation::new();
        impl_node.set_offered_event(event);
        let mut service_mapping = vsidl_v1::someip::some_ip_mapping::SomeIpServiceMapping::new();
        service_mapping.instance_impl.push(impl_node);
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
