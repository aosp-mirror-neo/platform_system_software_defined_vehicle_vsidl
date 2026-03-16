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
//! Implicitly creates topics from channel names if they are missing

use crate::impl_traverse_catalog_mut;
use crate::visitors::visitor_mut::{AstNodeMut, VsidlVisitorMut};
use crate::Catalog;
use heck::ToKebabCase;
use protobuf::descriptor::{
    DescriptorProto as ProtoDescriptor, FileDescriptorProto as ProtoFileDescriptor,
};
use std::collections::HashMap;
use std::path::PathBuf;
use vsidl_v1::syntax::service_bundle::{Publisher, Subscriber};
use vsidl_v1::syntax::VsidlEntry;

#[derive(Default)]
pub struct ConvertPdk29ToSOA {
    package_name: String,
    vsidl_package: String,
    message_fqn_is_multipub: HashMap<String, bool>,
    message_fqn_to_message_count: HashMap<String, i64>,
    catalog_vsidl_files: Vec<(PathBuf, VsidlEntry)>,
}

impl_traverse_catalog_mut!(ConvertPdk29ToSOA);

impl VsidlVisitorMut for ConvertPdk29ToSOA {
    fn start_visit_catalog(&mut self, catalog: &mut Catalog) {
        // Temporarily remove main catalog files from Catalog so that the AST Trafo
        // only runs on dependencies
        self.catalog_vsidl_files = std::mem::take(&mut catalog.catalog_vsidl_files);
    }

    fn start_visit_file_descriptor_proto(&mut self, node: &mut ProtoFileDescriptor) {
        self.package_name = node.package().to_string();
    }

    fn start_visit_proto_descriptor(&mut self, node: &mut ProtoDescriptor) {
        let fqn = if self.package_name.is_empty() {
            node.name().to_string()
        } else {
            format!("{}.{}", self.package_name, node.name())
        };

        let mut is_multipub = false;
        if let Some(publication) = vsidl_v1::annotations::exts::publication.get(&node.options) {
            if let Ok(vsidl_v1::annotations::sdv_publication::PublisherModel::MULTI_PUB) =
                publication.model.enum_value()
            {
                is_multipub = true;
            }
            if publication.message_count > 0 {
                self.message_fqn_to_message_count
                    .insert(fqn.clone(), publication.message_count as i64);
            }
        }
        self.message_fqn_is_multipub.insert(fqn, is_multipub);
    }

    fn start_visit_vsidl_entry(&mut self, node: &mut VsidlEntry) {
        self.vsidl_package = node.package.clone();
    }

    fn start_visit_publisher(&mut self, node: &mut Publisher) {
        // SOA publishers use a 'capacity' field that replaces the 'message_count' annotation
        // that used to be attached to Protobuf messages used a publications.
        if node.capacity == 0 && node.capacity_span.is_none() {
            let fqn = format!("{}.{}", self.vsidl_package, node.message);
            if let Some(&message_count) = self.message_fqn_to_message_count.get(&fqn) {
                node.capacity = message_count;
            } else if let Some(&message_count) =
                self.message_fqn_to_message_count.get(&node.message)
            {
                node.capacity = message_count;
            }
        }

        // no user-defined topic but user-defined channel (i.e. variant/instance) means that pre-SOA syntax is used
        if node.topic.is_empty() && node.topic_span.is_empty() {
            let fqn = format!("{}.{}", self.vsidl_package, node.message);
            let (message_name, is_multipub) =
                if let Some(&is_multi) = self.message_fqn_is_multipub.get(&fqn) {
                    (&fqn, is_multi)
                } else {
                    (&node.message, false)
                };

            // LINT.IfChange(set_multipub_topic)
            if is_multipub {
                // legacy 'MULTIPUB' publishers must always set the same topic, so they can be multiplexed.
                // This is needed because user can define custom service_unit_names.
                let topic = format!("{}-{}", message_name, "unique").to_kebab_case();
                node.topic.push(topic);
                // LINT.ThenChange(software_defined_vehicle/middleware/transport/rust/interface/src/types.rs:set_multipub_topic)
            } else if !node.service_unit_name.is_empty() {
                node.topic.push(node.service_unit_name.clone());
            } else if !node.channel.is_empty() {
                // If not custom service_unit_name, use the generated service unit name
                for channel in &node.channel {
                    let topic = format!("{}-{}", message_name, channel).to_kebab_case();
                    node.topic.push(topic);
                }
            }
        }
    }

    fn start_visit_subscriber(&mut self, node: &mut Subscriber) {
        // no user-defined topic but user-defined channel (i.e. variant/instance) means that pre-SOA syntax is used
        if node.topic.is_empty() && node.topic_span.is_empty() {
            let fqn = format!("{}.{}", self.vsidl_package, node.message);
            let (message_name, is_multipub) =
                if let Some(&is_multi) = self.message_fqn_is_multipub.get(&fqn) {
                    (&fqn, is_multi)
                } else {
                    (&node.message, false)
                };

            if is_multipub {
                // legacy 'MULTIPUB' publishers must always set the same topic, so they can be multiplexed.
                // This is needed because user can define custom service_unit_names.
                let topic = format!("{}-{}", message_name, "unique").to_kebab_case();
                node.topic.push(topic);
            } else if !node.channel.is_empty() {
                for channel in &node.channel {
                    let topic = format!("{}-{}", message_name, channel).to_kebab_case();
                    node.topic.push(topic);
                }
            }
        }
    }

    fn end_visit_catalog(&mut self, catalog: &mut Catalog) {
        catalog.catalog_vsidl_files = std::mem::take(&mut self.catalog_vsidl_files);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_data;
    use crate::Catalog;
    use std::path::PathBuf;

    #[test]
    fn test_should_set_capacity_from_message_count_if_unset() {
        // GIVEN A catalog with a publisher having 0 capacity and NO capacity_span
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog.dependencies_vsidl_files.push(test_data::VSIDL_PUBLISHER_WITHOUT_CHANNEL.1.clone());

        // WHEN executing the visitor
        ConvertPdk29ToSOA::traverse(&mut catalog);

        // THEN capacity field is set to 8 (from Pressure message in messages.proto)
        let publisher = &catalog.dependencies_vsidl_files[0].service_bundle[0].publisher[0];
        assert_eq!(publisher.capacity, 8);
    }

    #[test]
    fn test_should_convert_to_unique_topic_if_message_is_multipub() {
        // GIVEN A catalog with a publisher using channel and a MULTI_PUB message
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog.dependencies_vsidl_files.push(test_data::VSIDL_PUBLISHER_WITHOUT_CHANNEL.1.clone());

        // PRE-CONDITION: message "Speed" is MULTI_PUB in messages.proto
        let publisher = &mut catalog.dependencies_vsidl_files[0].service_bundle[0].publisher[0];
        publisher.message = "Speed".to_string();
        publisher.channel = vec!["ONE".to_string(), "TWO".to_string()];
        publisher.topic.clear();

        // WHEN executing the visitor
        ConvertPdk29ToSOA::traverse(&mut catalog);

        // THEN topic field is implicitly created and suffixed with "unique"
        let publisher = &catalog.dependencies_vsidl_files[0].service_bundle[0].publisher[0];
        assert_eq!(publisher.topic, vec!["sdv-test-speed-unique"]);
    }

    #[test]
    fn test_should_convert_to_unique_topic_if_message_is_multipub_for_subscriber() {
        // GIVEN A catalog with a subscriber using channel and a MULTI_PUB message
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog.dependencies_vsidl_files.push(test_data::VSIDL_EXPLICIT_CHANNEL_LISTING.1.clone());

        // PRE-CONDITION: message "Speed" is MULTI_PUB in messages.proto
        let subscriber = &mut catalog.dependencies_vsidl_files[0].service_bundle[1].subscriber[0];
        subscriber.message = "Speed".to_string();
        subscriber.channel = vec!["ONE".to_string(), "TWO".to_string()];
        subscriber.topic.clear();

        // WHEN executing the visitor
        ConvertPdk29ToSOA::traverse(&mut catalog);

        // THEN topic field is implicitly created and suffixed with "unique"
        let subscriber = &catalog.dependencies_vsidl_files[0].service_bundle[1].subscriber[0];
        assert_eq!(subscriber.topic, vec!["sdv-test-speed-unique"]);
    }

    #[test]
    fn test_should_convert_channel_to_topic_for_publisher() {
        // GIVEN A catalog with a publisher using channel but NO topic
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog.dependencies_vsidl_files.push(test_data::VSIDL_PUBLISHER_WITHOUT_CHANNEL.1.clone());

        // PRE-CONDITION: channel has "ONE" and "TWO", topic is empty
        let publisher = &catalog.dependencies_vsidl_files[0].service_bundle[0].publisher[0];
        assert_eq!(publisher.channel, vec!["ONE", "TWO"]);
        assert!(publisher.topic.is_empty());
        assert_eq!(publisher.message, "Pressure");

        // WHEN executing the visitor
        ConvertPdk29ToSOA::traverse(&mut catalog);

        // THEN topic field is implicitly created and prefixed with message name
        let publisher = &catalog.dependencies_vsidl_files[0].service_bundle[0].publisher[0];
        assert_eq!(publisher.topic, vec!["sdv-test-pressure-one", "sdv-test-pressure-two"]);
    }

    #[test]
    fn test_should_convert_channel_to_topic_for_subscriber() {
        // GIVEN A catalog with a subscriber using channel but NO topic
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog.dependencies_vsidl_files.push(test_data::VSIDL_EXPLICIT_CHANNEL_LISTING.1.clone());

        // PRE-CONDITION: subscriber has channel "FRONT_LEFT" and "FRONT_RIGHT", topic is empty
        let subscriber = &catalog.dependencies_vsidl_files[0].service_bundle[1].subscriber[0];
        assert_eq!(subscriber.channel, vec!["FRONT_LEFT", "FRONT_RIGHT"]);
        assert!(subscriber.topic.is_empty());
        assert_eq!(subscriber.message, "TirePressure");

        // WHEN executing the visitor
        ConvertPdk29ToSOA::traverse(&mut catalog);

        // THEN topic field is implicitly created and prefixed with message name
        let subscriber = &catalog.dependencies_vsidl_files[0].service_bundle[1].subscriber[0];
        assert_eq!(subscriber.topic, vec!["tire-pressure-front-left", "tire-pressure-front-right"]);
    }

    #[test]
    fn test_should_not_convert_if_topic_already_exists() {
        // GIVEN A catalog with a publisher that ALREADY has a topic
        let mut catalog = Catalog::new();
        catalog.dependencies_vsidl_files.push(test_data::VSIDL_TIRE_PRESSURE_PUBLISHER.1.clone());

        // PRE-CONDITION: topic is "test"
        let publisher = &catalog.dependencies_vsidl_files[0].service_bundle[0].publisher[0];
        assert_eq!(publisher.topic, vec!["test"]);

        // WHEN executing the visitor
        ConvertPdk29ToSOA::traverse(&mut catalog);

        // THEN topic remains "test"
        let publisher = &catalog.dependencies_vsidl_files[0].service_bundle[0].publisher[0];
        assert_eq!(publisher.topic, vec!["test"]);
    }

    #[test]
    fn test_should_use_fqn_if_present_in_map() {
        // GIVEN A catalog with a publisher and a proto message in the map
        let mut catalog = Catalog::new();
        catalog.dependencies_vsidl_files.push(test_data::VSIDL_PUBLISHER_WITHOUT_CHANNEL.1.clone());

        let mut file_desc = ProtoFileDescriptor::new();
        file_desc.set_package("sdv.test".to_string());
        let mut msg_desc = ProtoDescriptor::new();
        msg_desc.set_name("Pressure".to_string());
        file_desc.message_type.push(msg_desc);

        let metadata = crate::catalog::ProtoBuildTargetMetadata {
            folder: PathBuf::from("."),
            target_name: Some("test_target".to_string()),
            crate_name: Some("test_crate".to_string()),
        };

        catalog.proto_descriptors.push((metadata, file_desc));

        // WHEN executing the visitor
        ConvertPdk29ToSOA::traverse(&mut catalog);

        // THEN topic field is implicitly created and prefixed with FQN
        let publisher = &catalog.dependencies_vsidl_files[0].service_bundle[0].publisher[0];
        assert_eq!(publisher.topic, vec!["sdv-test-pressure-one", "sdv-test-pressure-two"]);
    }
}
