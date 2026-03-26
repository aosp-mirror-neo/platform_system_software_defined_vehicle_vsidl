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
use crate::impl_context_condition_symtab;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::specific_symbols::subscriber_symbol::SubscriberSymbol;
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

#[derive(Default)]
pub struct SubscriberRequiresPublisher {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(SubscriberRequiresPublisher);
impl VsidlVisitor for SubscriberRequiresPublisher {}

/// Issues a warning if there is no publisher for a subscriber's message
impl SymbolTableChecker for SubscriberRequiresPublisher {
    fn check_subscriber(&mut self, symbol_table: &SymbolTable, subscriber: &SubscriberSymbol) {
        // Case 1: The message has no publishers at all
        if subscriber.message(symbol_table).publishers(symbol_table).is_empty() {
            // WARN: There's no publisher for the topic requested by the subscriber
            self.finding_collector.warn(
                FindingId::SubscriberRequiresPublisher,
                format!(
                    "There is no publisher for the message of subscriber '{}'",
                    subscriber.full_name(symbol_table)
                ),
                Some("There is no publisher for this message".to_string()),
                vec![subscriber.ast_node.message_span.clone().into_option()],
            );
            return;
        }

        // Case 2: Subscriber has explicit topics, some of them are not published
        let message_topics = subscriber.message(symbol_table).topics(symbol_table);
        for (index, topic) in subscriber.topics(symbol_table).iter().enumerate() {
            if !message_topics.iter().any(|c| c.name() == *topic) {
                // WARN: There's no publisher for the topic requested by the subscriber
                self.finding_collector.warn(
                    FindingId::SubscriberRequiresPublisher,
                    format!(
                        "Topic '{topic}' of subscriber '{}' does not have a corresponding publisher",
                        subscriber.full_name(symbol_table)
                    ),
                    Some("Topic does not have a corresponding publisher".to_string()),
                    vec![subscriber.topic_span.get(index).cloned()],
                );
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::subscriber_requires_publisher::SubscriberRequiresPublisher;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;
    use vsidl_v1_symbols::TargetApi;

    #[test]
    fn test_should_reject_subscriber_without_publisher() {
        // GIVEN A catalog with a service bundle that contains a subscriber without a publisher at all for the requested type
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SUBSCRIBER_WITHOUT_PUBLISHER.clone());
        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = SubscriberRequiresPublisher::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN Should have found errors
        assert!(check.is_ok());
        assert!(check.any_finding_contains(FindingSeverity::Warn, "sdv.test.Pressure"));
    }

    #[test]
    fn test_should_reject_subscriber_without_publisher_for_topic_one() {
        // GIVEN A catalog with a service bundle that contains a subscriber with no publishers for one of the requested topics
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SUBSCRIBER_WITHOUT_PUBLISHER_FOR_TOPIC_ONE.clone());
        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = SubscriberRequiresPublisher::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN Should have found errors
        assert!(check.is_ok());
        assert!(check.any_finding_contains(FindingSeverity::Warn, "sdv.test.Pressure"));
        assert!(check.any_finding_contains(FindingSeverity::Warn, "one"));
    }

    #[test]
    fn test_should_accept_valid_service_bundle_with_explicit_publisher_topics() {
        // GIVEN A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());
        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = SubscriberRequiresPublisher::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_accept_valid_service_bundle_with_implicit_publisher_topics() {
        // GIVEN A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SUBSCRIBER_REQUIRES_PUBLISHER_VALID.clone());
        let mut creator: SymbolTableCreator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker: SymbolTableLinker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = SubscriberRequiresPublisher::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }
}
