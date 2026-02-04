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
use std::collections::{HashMap, HashSet};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_common::UNIQUE_CHANNEL;
use vsidl_v1_symbols::symbol::SymbolId;
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

#[derive(Default)]
pub struct PublisherChannel {
    finding_collector: FindingCollector,
    /// Maps Proto publication symbols + their channels to the publishers that publish them
    single_pub_publishers: HashMap<(SymbolId, String), HashSet<SymbolId>>,
}
impl_context_condition_symtab!(PublisherChannel);
impl VsidlVisitor for PublisherChannel {}

impl SymbolTableChecker for PublisherChannel {
    fn check_publisher(
        &mut self,
        symbol_table: &SymbolTable,
        publisher: &vsidl_v1_symbols::specific_symbols::publisher_symbol::PublisherSymbol,
    ) {
        // Get publication symbol
        let proto_publication_symbol = publisher
            .enclosing_scope(symbol_table)
            .resolve_proto_publication(symbol_table, publisher.message.clone());
        if proto_publication_symbol.is_empty() {
            // This error is handled by the publication_type context condition
            return;
        }
        let proto_publication_symbol = proto_publication_symbol[0].clone();
        let proto_publication_symbol_id = &proto_publication_symbol.id();

        // Get name of channels enum
        let channels_enum_name = &proto_publication_symbol.channels_enum_name();

        // ERROR: Publication doesn't use channels => using channels is wrong
        if channels_enum_name.is_none() {
            for (index, channel) in publisher.channel.iter().enumerate() {
                if channel == UNIQUE_CHANNEL {
                    // Explicitly naming the 'UNIQUE' channel is allowed
                    // for publications that use it implicitly without an
                    // explicit channels_enum
                    self.finding_collector.warn(
                        FindingId::VsidlRedundantUniqueChannelDeclaration,
                        format!(
                            "'UNIQUE' channel specification is redundant in publisher '{}'",
                            publisher.symbol.name.clone()
                        ),
                        vec![publisher.channel_span.get(index).cloned()],
                    );
                    continue;
                }
                self.finding_collector.error(
                    FindingId::PublisherChannelForChannellessProto,
                    format!("Publisher channel '{channel}' does not exist"),
                    vec![publisher.channel_span.get(index).cloned()],
                );
            }
            // Keep track that this UNIQUE channel was used
            if proto_publication_symbol.is_single_pub() {
                let usage = self
                    .single_pub_publishers
                    .entry((proto_publication_symbol_id.clone(), UNIQUE_CHANNEL.to_string()))
                    .or_default();
                usage.insert(publisher.symbol.id.clone());
            }
            return;
        }

        // Resolve channels enum
        let channels_enums = proto_publication_symbol
            .enclosing_scope(symbol_table)
            .resolve_proto_enum(symbol_table, channels_enum_name.clone().unwrap());
        if channels_enums.is_empty() {
            // Invalid proto input - not the responsibility of this context condition
            return;
        }
        let channels_enum = channels_enums[0].clone();

        // ERROR: VSIDL publisher channel does not exist for Proto publication
        let channels_enum_name = channels_enum_name.clone().unwrap();
        for (index, channel) in publisher.channel.iter().enumerate() {
            if !channels_enum.clone().values().contains(channel) {
                self.finding_collector.error(
                    FindingId::PublisherChannelDoesNotExist,
                    format!("Publisher channel '{channel}' does not exist in enum '{channels_enum_name}'"),
                    vec![publisher.channel_span.get(index).cloned()],
                )
            }
        }

        if proto_publication_symbol.is_single_pub() {
            // Keep track of channel usage if a publisher specifies channels
            for channel in &publisher.channel {
                let usage = self
                    .single_pub_publishers
                    .entry((proto_publication_symbol_id.clone(), channel.clone()))
                    .or_default();
                usage.insert(publisher.symbol.id.clone());
            }
            // Keep track of channel usage if a publisher DOES NOT specify channels (i.e. uses all channels)
            if publisher.channel.is_empty() {
                for channel in &channels_enum.clone().values() {
                    let usage = self
                        .single_pub_publishers
                        .entry((proto_publication_symbol_id.clone(), channel.clone()))
                        .or_default();
                    usage.insert(publisher.symbol.id.clone());
                }
            }
        }
    }

    fn after_checks(&mut self, symbol_table: &SymbolTable) {
        for ((proto_publication, channel), usages) in &self.single_pub_publishers {
            if usages.len() > 1 {
                let publication_symbol = symbol_table.get_symbol(proto_publication).unwrap();
                let (publication_name, span) = match publication_symbol {
                    vsidl_v1_symbols::symbol::SymbolKind::ProtoPublication(publication) => {
                        (publication.name.clone().unwrap(), publication.span())
                    }
                    _ => panic!("Expected ProtoPublication symbol"), // This should never happen
                };
                self.finding_collector.error(FindingId::PublisherChannelSinglePubUsedByMultiplePublishers,
                    format!("Publication channel '{channel}' of '{publication_name}' is used by multiple publishers despite being Single Pub"),
                    vec![span],
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::context_condition::ContextCondition;
    use crate::pdk29::PublisherChannel;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;

    #[test]
    fn test_should_accept_publisher_with_valid_channel() {
        // GIVEN A catalog with a publisher that has a valid channel
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
        let mut check = PublisherChannel::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_reject_publisher_with_invalid_channel() {
        // Given: A catalog with a publisher that has an invalid channel
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_INVALID_PUBLICATION_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // When: Executing context condition
        let mut check = PublisherChannel::default();
        check.check_symbol_table(creator.symbol_table());

        // Then: Should have found an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "INVALID_CHANNEL"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "INVALID_CHANNEL2"));
    }

    #[test]
    fn test_should_accept_publisher_with_explicit_unique_channel() {
        // Given: A catalog with a publisher that has an invalid channel
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_PUBLISHER_WITH_EXPLICIT_UNIQUE_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // When: Executing context condition
        let mut check = PublisherChannel::default();
        check.check_symbol_table(creator.symbol_table());

        // Then: Should have found a warning
        assert!(check.is_ok());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "'UNIQUE' channel specification is redundant in publisher 'FuelLevel'",
        ));
    }

    #[test]
    fn test_should_accept_publisher_without_channel() {
        // GIVEN A catalog with a publisher that does not have a channel
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_PUBLISHER_WITHOUT_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = PublisherChannel::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_reject_single_pub_channel_with_multiple_publishers_explicit() {
        // Given: A catalog with a publisher that has multiple publishers for a single pub channel
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SINGLE_PUB_MULTIPLE_PUBLISHERS_EXPLICIT.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // When: Executing context condition
        let mut check = PublisherChannel::default();
        check.check_symbol_table(creator.symbol_table());

        // Then: Should have found an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "ONE"));
    }

    #[test]
    fn test_should_reject_single_pub_channel_with_multiple_publishers_implicit() {
        // Given: A catalog with a publisher that has multiple publishers for a single pub channel
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SINGLE_PUB_MULTIPLE_PUBLISHERS_IMPLICIT.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // When: Executing context condition
        let mut check = PublisherChannel::default();
        check.check_symbol_table(creator.symbol_table());

        // Then: Should have found an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "ONE"));
    }

    #[test]
    fn test_should_reject_single_pub_channel_with_multiple_publishers_unique_channel() {
        // Given: A catalog with a publisher that has multiple publishers for a single pub channel
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SINGLE_PUB_MULTIPLE_PUBLISHERS_UNIQUE_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // When: Executing context condition
        let mut check = PublisherChannel::default();
        check.check_symbol_table(creator.symbol_table());

        // Then: Should have found an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "UNIQUE"));
    }

    #[test]
    fn test_should_ignore_publisher_with_undefined_type() {
        // Given: A catalog with a service bundle that contains a publisher with a non existing publication type
        let mut catalog = Catalog::new();
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_NON_EXISTING_PUBLICATION_TYPE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // When: Executing CoCo
        let mut check = PublisherChannel::default();
        check.check_symbol_table(creator.symbol_table());

        // Then: Should not have found errors
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_ignore_undefined_channels_enum() {
        // GIVEN A catalog with an SDV publication without a model
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_INVALID_SDV_PUBLICATION.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_PUBLISHER_WITH_EMPTY_CHANNEL_ENUM.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = PublisherChannel::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found error
        assert!(check.is_ok());
    }
}
