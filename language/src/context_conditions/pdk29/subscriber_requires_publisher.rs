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

use crate::impl_context_condition_symtab;
use crate::{FindingCollector, FindingId};
use std::collections::HashSet;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::specific_symbols::proto_publication_symbol::ProtoPublicationSymbol;
use vsidl_v1_symbols::specific_symbols::subscriber_symbol::SubscriberSymbol;
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

#[derive(Default)]
pub struct SubscriberRequiresPublisher {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(SubscriberRequiresPublisher);
impl VsidlVisitor for SubscriberRequiresPublisher {}

/// Issues an error if the unit type specified for an RPC client is unknown
impl SymbolTableChecker for SubscriberRequiresPublisher {
    fn check_subscriber(&mut self, symbol_table: &SymbolTable, subscriber: &SubscriberSymbol) {
        // Resolve Proto publication
        let publication_symbols = subscriber
            .enclosing_scope(symbol_table)
            .resolve_proto_publication(symbol_table, subscriber.message.clone());
        if publication_symbols.is_empty() {
            // handled by SubscriberType context condition
            return;
        }
        let publication: ProtoPublicationSymbol = publication_symbols[0].clone();

        // Find all published channels of the requested publiction
        let mut published_channels: HashSet<String> = HashSet::new();
        for publisher in publication.publishers(symbol_table) {
            for channel in &publisher.channels(symbol_table) {
                published_channels.insert(channel.to_string());
            }
        }

        // Get the channels requested by the subscriber
        let mut requested_channels = subscriber.channel.clone();
        if requested_channels.is_empty() {
            // If no channels are explicitly requested, all channels of the publication are requested
            requested_channels = publication.channels(symbol_table);
        }

        // For each requested channel, check if there's a publisher for it
        for (index, channel) in requested_channels.iter().enumerate() {
            if !published_channels.contains(channel) {
                // ERROR: There's no publisher for the channel requested by the subscriber
                self.finding_collector.error(
                    FindingId::SubscriberRequiresPublisher,
                    format!(
                        "Channel '{}' of subscriber '{}' does not have a corresponding publisher",
                        channel,
                        subscriber.full_name(symbol_table)
                    ),
                    vec![subscriber.channel_span.get(index).cloned()],
                );
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::pdk29::SubscriberRequiresPublisher;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;

    #[test]
    fn test_should_reject_subscriber_without_publisher() {
        // GIVEN A catalog with a service bundle that contains a subscriber without a publisher at all for the requested type
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SUBSCRIBER_WITHOUT_PUBLISHER.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = SubscriberRequiresPublisher::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN Should have found errors
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "sdv.test.Pressure"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "ONE"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "TWO"));
    }

    #[test]
    fn test_should_reject_subscriber_without_publisher_for_one_channel() {
        // GIVEN A catalog with a service bundle that contains a subscriber with no publishers for one of the requested channels
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SUBSCRIBER_WITHOUT_PUBLISHER_FOR_ONE_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = SubscriberRequiresPublisher::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN Should have found errors
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "sdv.test.Pressure"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "ONE"));
    }

    #[test]
    fn test_should_accept_valid_service_bundle_with_explicit_publisher_channels() {
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
    fn test_should_accept_valid_service_bundle_with_implicit_publisher_channels() {
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
        let mut creator = SymbolTableCreator::new();
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
    fn test_should_ignore_subscriber_with_unknown_type() {
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
        let mut check = SubscriberRequiresPublisher::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found an error
        assert!(check.is_ok());
        assert!(check.finding_collector().findings().is_empty());
    }
}
