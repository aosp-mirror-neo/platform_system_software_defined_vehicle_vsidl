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
use vsidl_v1::someip::some_ip_mapping::SomeIpServiceMapping;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct SomeIpParameterizationConflict {
    finding_collector: FindingCollector,
}
impl_context_condition!(SomeIpParameterizationConflict);

impl VsidlVisitor for SomeIpParameterizationConflict {
    fn start_visit_someip_service_mapping(&mut self, node: &SomeIpServiceMapping) {
        let has_parameterization =
            node.instance.iter().any(|i| !i.sdv_service_unit_parameterization.is_empty());

        if !has_parameterization {
            return;
        }

        for impl_node in &node.instance_impl {
            if impl_node.has_offered_interface() {
                let interface = impl_node.offered_interface();
                if !interface.channel.is_empty() {
                    self.finding_collector.error(
                        FindingId::SomeIpParameterizationMixedWithLiteralNames,
                        format!(
                            "SOME/IP service mapping for service ID {} defines sdv_service_unit_parameterization, but its implementation for unit type '{}' sets literal channel '{}'",
                            node.someip_service_id,
                            interface.sdv_unit_type.type_name,
                            interface.channel
                        ),
                        vec![interface.channel_span.clone().into_option()],
                    );
                }
            }

            if impl_node.has_offered_event() {
                let event = impl_node.offered_event();
                if event.has_simple_event() {
                    let simple = event.simple_event();
                    if !simple.topic.is_empty() {
                        self.finding_collector.error(
                            FindingId::SomeIpParameterizationMixedWithLiteralNames,
                            format!(
                                "SOME/IP service mapping for service ID {} defines sdv_service_unit_parameterization, but its simple event mapping for unit type '{}' sets literal topic '{}'",
                                node.someip_service_id,
                                simple.sdv_unit_type.type_name,
                                simple.topic
                            ),
                            vec![simple.topic_span.clone().into_option()],
                        );
                    }
                }
                if event.has_group_event() {
                    let group = event.group_event();
                    for part in &group.event_part {
                        if !part.topic.is_empty() {
                            self.finding_collector.error(
                                FindingId::SomeIpParameterizationMixedWithLiteralNames,
                                format!(
                                    "SOME/IP service mapping for service ID {} defines sdv_service_unit_parameterization, but its group event part mapping for unit type '{}' sets literal topic '{}'",
                                    node.someip_service_id,
                                    part.sdv_unit_type.type_name,
                                    part.topic
                                ),
                                vec![part.topic_span.clone().into_option()],
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_condition::ContextCondition;
    use crate::FindingSeverity;
    use vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::event::{
        GroupEvent, GroupEventPart, SimpleEvent,
    };
    use vsidl_v1::someip::some_ip_mapping::some_ip_instance_implementation::{
        Event as SomeIpEvent, RpcInterface,
    };
    use vsidl_v1::someip::some_ip_mapping::{
        SomeIpInstanceImplementation, SomeIpInstanceMapping, UnitType,
    };
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_mixed_parameterization_and_literal_channel() {
        // GIVEN a SomeIpServiceMapping with parameterization and literal channel
        let mut service_mapping = SomeIpServiceMapping::new();
        service_mapping.someip_service_id = 0x1234;

        let mut instance = SomeIpInstanceMapping::new();
        instance.sdv_service_unit_parameterization.insert(0, "unit1".to_string());
        service_mapping.instance.push(instance);

        let mut impl_node = SomeIpInstanceImplementation::new();
        let mut interface = RpcInterface::new();
        let mut unit_type = UnitType::new();
        unit_type.type_name = "MyType".to_string();
        interface.sdv_unit_type = protobuf::MessageField::some(unit_type);
        interface.channel = "my-channel".to_string();
        impl_node.set_offered_interface(interface);
        service_mapping.instance_impl.push(impl_node);

        let mut catalog = Catalog::new();
        // Catalog traversal visits some_ip_mapping -> mapping_type -> someip_service_mapping
        let mut someip_mapping = vsidl_v1::someip::SomeIpMapping::new();
        someip_mapping.set_someip_service_mapping(service_mapping);
        let mut vsidl_entry = vsidl_v1::syntax::VsidlEntry::new();
        vsidl_entry.some_ip_mapping.push(someip_mapping);
        catalog.catalog_vsidl_files.push((std::path::PathBuf::from("test.vsidl"), vsidl_entry));

        // WHEN executing context condition
        let mut check = SomeIpParameterizationConflict::default();
        check.check_catalog(&catalog);

        // THEN an error should be found
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "defines sdv_service_unit_parameterization, but its implementation for unit type 'MyType' sets literal channel 'my-channel'"
        ));
    }

    #[test]
    fn test_mixed_parameterization_and_literal_topic_simple() {
        // GIVEN a SomeIpServiceMapping with parameterization and literal topic in simple event
        let mut service_mapping = SomeIpServiceMapping::new();
        service_mapping.someip_service_id = 0x1234;

        let mut instance = SomeIpInstanceMapping::new();
        instance.sdv_service_unit_parameterization.insert(0, "unit1".to_string());
        service_mapping.instance.push(instance);

        let mut impl_node = SomeIpInstanceImplementation::new();
        let mut event = SomeIpEvent::new();
        let mut simple = SimpleEvent::new();
        let mut unit_type = UnitType::new();
        unit_type.type_name = "MyType".to_string();
        simple.sdv_unit_type = protobuf::MessageField::some(unit_type);
        simple.topic = "my-topic".to_string();
        event.set_simple_event(simple);
        impl_node.set_offered_event(event);
        service_mapping.instance_impl.push(impl_node);

        let mut catalog = Catalog::new();
        let mut someip_mapping = vsidl_v1::someip::SomeIpMapping::new();
        someip_mapping.set_someip_service_mapping(service_mapping);
        let mut vsidl_entry = vsidl_v1::syntax::VsidlEntry::new();
        vsidl_entry.some_ip_mapping.push(someip_mapping);
        catalog.catalog_vsidl_files.push((std::path::PathBuf::from("test.vsidl"), vsidl_entry));

        // WHEN executing context condition
        let mut check = SomeIpParameterizationConflict::default();
        check.check_catalog(&catalog);

        // THEN an error should be found
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "defines sdv_service_unit_parameterization, but its simple event mapping for unit type 'MyType' sets literal topic 'my-topic'"
        ));
    }

    #[test]
    fn test_mixed_parameterization_and_literal_topic_group() {
        // GIVEN a SomeIpServiceMapping with parameterization and literal topic in group event part
        let mut service_mapping = SomeIpServiceMapping::new();
        service_mapping.someip_service_id = 0x1234;

        let mut instance = SomeIpInstanceMapping::new();
        instance.sdv_service_unit_parameterization.insert(0, "unit1".to_string());
        service_mapping.instance.push(instance);

        let mut impl_node = SomeIpInstanceImplementation::new();
        let mut event = SomeIpEvent::new();
        let mut group = GroupEvent::new();
        let mut part = GroupEventPart::new();
        let mut unit_type = UnitType::new();
        unit_type.type_name = "MyType".to_string();
        part.sdv_unit_type = protobuf::MessageField::some(unit_type);
        part.topic = "my-topic".to_string();
        group.event_part.push(part);
        event.set_group_event(group);
        impl_node.set_offered_event(event);
        service_mapping.instance_impl.push(impl_node);

        let mut catalog = Catalog::new();
        let mut someip_mapping = vsidl_v1::someip::SomeIpMapping::new();
        someip_mapping.set_someip_service_mapping(service_mapping);
        let mut vsidl_entry = vsidl_v1::syntax::VsidlEntry::new();
        vsidl_entry.some_ip_mapping.push(someip_mapping);
        catalog.catalog_vsidl_files.push((std::path::PathBuf::from("test.vsidl"), vsidl_entry));

        // WHEN executing context condition
        let mut check = SomeIpParameterizationConflict::default();
        check.check_catalog(&catalog);

        // THEN an error should be found
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "defines sdv_service_unit_parameterization, but its group event part mapping for unit type 'MyType' sets literal topic 'my-topic'"
        ));
    }

    #[test]
    fn test_valid_parameterization() {
        // GIVEN a SomeIpServiceMapping with parameterization and no literal names
        let mut service_mapping = SomeIpServiceMapping::new();
        service_mapping.someip_service_id = 0x1234;

        let mut instance = SomeIpInstanceMapping::new();
        instance.sdv_service_unit_parameterization.insert(0, "unit1".to_string());
        service_mapping.instance.push(instance);

        let mut impl_node = SomeIpInstanceImplementation::new();
        let mut interface = RpcInterface::new();
        interface.service_unit = 0; // parameterized
        impl_node.set_offered_interface(interface);
        service_mapping.instance_impl.push(impl_node);

        let mut catalog = Catalog::new();
        let mut someip_mapping = vsidl_v1::someip::SomeIpMapping::new();
        someip_mapping.set_someip_service_mapping(service_mapping);
        let mut vsidl_entry = vsidl_v1::syntax::VsidlEntry::new();
        vsidl_entry.some_ip_mapping.push(someip_mapping);
        catalog.catalog_vsidl_files.push((std::path::PathBuf::from("test.vsidl"), vsidl_entry));

        // WHEN executing context condition
        let mut check = SomeIpParameterizationConflict::default();
        check.check_catalog(&catalog);

        // THEN no findings should be reported
        assert!(check.is_ok());
    }

    #[test]
    fn test_valid_literal_names() {
        // GIVEN a SomeIpServiceMapping with no parameterization and literal names
        let mut service_mapping = SomeIpServiceMapping::new();
        service_mapping.someip_service_id = 0x1234;

        let instance = SomeIpInstanceMapping::new(); // empty parameterization
        service_mapping.instance.push(instance);

        let mut impl_node = SomeIpInstanceImplementation::new();
        let mut interface = RpcInterface::new();
        interface.channel = "my-channel".to_string();
        impl_node.set_offered_interface(interface);
        service_mapping.instance_impl.push(impl_node);

        let mut catalog = Catalog::new();
        let mut someip_mapping = vsidl_v1::someip::SomeIpMapping::new();
        someip_mapping.set_someip_service_mapping(service_mapping);
        let mut vsidl_entry = vsidl_v1::syntax::VsidlEntry::new();
        vsidl_entry.some_ip_mapping.push(someip_mapping);
        catalog.catalog_vsidl_files.push((std::path::PathBuf::from("test.vsidl"), vsidl_entry));

        // WHEN executing context condition
        let mut check = SomeIpParameterizationConflict::default();
        check.check_catalog(&catalog);

        // THEN no findings should be reported
        assert!(check.is_ok());
    }
}
