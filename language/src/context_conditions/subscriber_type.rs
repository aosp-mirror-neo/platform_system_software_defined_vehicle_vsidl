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
use vsidl_v1_symbols::specific_symbols::proto_message_symbol::ProtoMessageSymbol;
use vsidl_v1_symbols::specific_symbols::subscriber_symbol::SubscriberSymbol;
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;
use vsidl_v1_symbols::topic_symbol::TopicSymbol;

#[derive(Default)]
pub struct SubscriberType {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(SubscriberType);
impl VsidlVisitor for SubscriberType {}

/// Issues an error if the type of a subscriber is either not defined at all or not a message
impl SymbolTableChecker for SubscriberType {
    fn check_subscriber(&mut self, symbol_table: &SymbolTable, subscriber: &SubscriberSymbol) {
        // Find message
        let messages: Vec<ProtoMessageSymbol> = subscriber
            .enclosing_scope(symbol_table)
            .resolve_proto_message(symbol_table, subscriber.message.clone());

        // ERROR: No message for subscriber
        if messages.is_empty() {
            self.finding_collector.error(
                FindingId::SubscriberTypeDoesNotExist,
                format!(
                    "Subscriber '{}' uses message '{}' which does not exist as a message in Proto.",
                    subscriber.full_name(symbol_table),
                    subscriber.message
                ),
                vec![subscriber.message_span.clone().into_option()],
            );
            return;
        }

        for (index, topic) in subscriber.topic.iter().enumerate() {
            let topics: Vec<TopicSymbol> =
                subscriber.enclosing_scope(symbol_table).resolve_topic(symbol_table, topic);

            // ERROR: Channel specified by subscriber does not exist for message
            if topics.is_empty() {
                self.finding_collector.error(
                    FindingId::SubscriberTypeDoesNotHaveRequestedChannel,
                    format!(
                        "Subscriber '{}' uses undefined topic '{}' for message '{}'",
                        subscriber.full_name(symbol_table),
                        topic,
                        subscriber.message
                    ),
                    vec![subscriber.topic_span.get(index).cloned()],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::context_condition::ContextCondition;
    use crate::subscriber_type::SubscriberType;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;

    #[test]
    fn test_should_accept_subscriber_with_valid_topic() {
        // GIVEN A catalog with a subscriber that has a valid topic
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = SubscriberType::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_reject_subscriber_with_invalid_topic() {
        // GIVEN A catalog with a subscriber that has a invalid topic
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_NON_EXISTING_SUBSCRIBER_TOPIC.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = SubscriberType::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "invalid-topic"));
    }

    #[test]
    fn test_should_reject_subscriber_with_unknown_type() {
        // GIVEN A catalog with a subscriber that has a non existing type
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_NON_EXISTING_SUBSCRIBER_TYPE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = SubscriberType::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "NonExisting"));
    }
}
