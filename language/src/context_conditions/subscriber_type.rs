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
use vsidl_v1_common::UNIQUE_CHANNEL;
use vsidl_v1_symbols::specific_symbols::proto_publication_symbol::ProtoPublicationSymbol;
use vsidl_v1_symbols::specific_symbols::subscriber_symbol::SubscriberSymbol;
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

#[derive(Default)]
pub struct SubscriberType {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(SubscriberType);
impl VsidlVisitor for SubscriberType {}

/// Issues an error if the type of a publisher is either not defined at all or not a publication
impl SymbolTableChecker for SubscriberType {
    fn check_subscriber(&mut self, symbol_table: &SymbolTable, subscriber: &SubscriberSymbol) {
        // Find publication
        let publications: Vec<ProtoPublicationSymbol> = subscriber
            .enclosing_scope(symbol_table)
            .resolve_proto_publication(symbol_table, subscriber.message.clone());

        // ERROR: No publication for subscriber
        if publications.is_empty() {
            self.finding_collector.error(
                FindingId::SubscriberTypeDoesNotExist,
                format!(
                    "Subscriber '{}' uses message '{}' that does not exist as a publication in Proto.",
                    subscriber.full_name(symbol_table),
                    subscriber.message
                ),
                vec![subscriber.message_span.clone().into_option()],
            );
            return;
        }

        // Get channels
        let publication = publications[0].clone();
        let channels_enum_name = publication.channels_enum_name();
        let publication_channels = if let Some(ref channels_enum_name) = channels_enum_name {
            publication
                .enclosing_scope(symbol_table)
                .resolve_proto_enum(symbol_table, channels_enum_name.clone())[0]
                .values()
        } else {
            vec![UNIQUE_CHANNEL.to_string()] // no channels defined for Proto publication, means there is an implicit UNIQUE channel
        };

        // ERROR: Channel specified by subscriber does not exist for publication
        let subscriber_channels = subscriber.channel.clone();
        for (index, channel) in subscriber_channels.iter().enumerate() {
            if !publication_channels.contains(&channel.clone()) {
                self.finding_collector.error(
                    FindingId::SubscriberTypeDoesNotHaveRequestedChannel,
                    format!(
                        "Subscriber '{}' uses channel '{}' that is not defined for type '{}'",
                        subscriber.full_name(symbol_table),
                        channel,
                        subscriber.message
                    ),
                    vec![subscriber.channel_span.get(index).cloned()],
                );
            } else if channels_enum_name.is_none() && channel == UNIQUE_CHANNEL {
                self.finding_collector.warn(
                    FindingId::VsidlRedundantUniqueChannelDeclaration,
                    format!(
                        "'UNIQUE' channel specification is redundant in subscriber '{}'",
                        subscriber.symbol.name.clone()
                    ),
                    vec![subscriber.channel_span.get(index).cloned()],
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
    fn test_should_accept_subscriber_with_valid_channel() {
        // GIVEN A catalog with a subscriber that has a valid channel
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
    fn test_should_reject_subscriber_with_invalid_channel() {
        // GIVEN A catalog with a subscriber that has a invalid channel
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_NON_EXISTING_SUBSCRIBER_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = SubscriberType::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "INVALID_CHANNEL"));
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

    #[test]
    fn test_should_accept_subscriber_with_explicit_defined_unique_channel() {
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
            .push(vsidl_v1_test_data::VSIDL_SUBSCRIBER_WITH_UNIQUE_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = SubscriberType::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found a warning
        assert!(check.is_ok());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "'UNIQUE' channel specification is redundant in subscriber 'FuelLevel'",
        ));
    }
}
