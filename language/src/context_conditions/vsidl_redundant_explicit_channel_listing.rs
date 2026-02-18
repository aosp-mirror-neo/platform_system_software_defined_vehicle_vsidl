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
use std::collections::{HashMap, HashSet};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;
use vsidl_v1_symbols::{service_bundle_symbol::ServiceBundleSymbol, SymbolTable};

#[derive(Debug, Default)]

pub struct VsidlRedundantExplicitChannelListing {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(VsidlRedundantExplicitChannelListing);
impl VsidlVisitor for VsidlRedundantExplicitChannelListing {}

impl VsidlRedundantExplicitChannelListing {
    fn check_publisher(
        &mut self,
        symbol_table: &SymbolTable,
        service_bundle: &ServiceBundleSymbol,
    ) {
        // Maps a publisher's fully qualified name to the set of its explicitly declared channels.
        let mut explicit_channels: HashMap<String, HashSet<String>> = HashMap::new();
        // Maps a publisher's fully qualified name to the total number of channels available for its publication type.
        let mut all_channels: HashMap<String, usize> = HashMap::new();

        let publishers = service_bundle.publishers(symbol_table);

        // Collect all explicitly declared channels for each publisher type from
        // potentially multiple publisher declarations, and store the total number of
        // channels available for that type to compare against later.
        for publisher in publishers {
            // Publishers that define a channel are not redundant
            if publisher.capacity != 0 {
                continue;
            }

            let full_name = publisher.full_name(symbol_table);
            explicit_channels.entry(full_name.clone()).or_default();
            all_channels.insert(
                full_name.clone(),
                publisher.message(symbol_table).channels(symbol_table).len(),
            );

            for channel in &publisher.channel {
                if let Some(inst) = explicit_channels.get_mut(&full_name) {
                    inst.insert(channel.clone());
                }
            }
        }

        // For each publisher type, check if the number of explicitly declared channels
        // equals the total number of available channels. If so, the explicit listing is
        // redundant, and a warning is issued.
        for (name, channels) in &explicit_channels {
            if all_channels[name] == 0 {
                continue;
            } else if all_channels[name] == channels.len() {
                self.finding_collector.warn(
                    FindingId::VsidlRedundantExplicitChannelListing,
                    format!(
                        "Service Bundle: '{}', Publisher: '{}', channels declarations could be removed. Omitting channel declarations implicitly uses all channels",
                        service_bundle.full_name(symbol_table),
                        name.clone(),
                    ),
                    vec![symbol_table.resolve_publisher_fqn(name)[0].span.clone().into_option()],
                );
            }
        }
    }

    fn check_subscriber(
        &mut self,
        symbol_table: &SymbolTable,
        service_bundle: &ServiceBundleSymbol,
    ) {
        // Maps a subscriber's fully qualified name to the set of its explicitly declared channels.
        let mut explicit_channels: HashMap<String, HashSet<String>> = HashMap::new();
        // Maps a subscriber's fully qualified name to the total number of channels available for its publication type.
        let mut all_channels: HashMap<String, usize> = HashMap::new();

        let subscribers = service_bundle.subscribers(symbol_table);

        // Collect all explicitly declared channels for each subscriber type from
        // potentially multiple subscriber declarations, and store the total number of
        // channels available for that type to compare against later.
        for subscriber in subscribers {
            let full_name = subscriber.full_name(symbol_table);
            explicit_channels.entry(full_name.clone()).or_default();
            all_channels.insert(
                full_name.clone(),
                subscriber.message(symbol_table).channels(symbol_table).len(),
            );

            for channel in &subscriber.channel {
                if let Some(inst) = explicit_channels.get_mut(&full_name) {
                    inst.insert(channel.clone());
                }
            }
        }

        // For each subscriber type, check if the number of explicitly declared channels
        // equals the total number of available channels. If so, the explicit listing is
        // redundant, and a warning is issued.
        for (name, channels) in &explicit_channels {
            if all_channels[name] == 0 {
                continue;
            } else if all_channels[name] == channels.len() {
                self.finding_collector.warn(
                    FindingId::VsidlRedundantExplicitChannelListing,
                    format!(
                        "Service Bundle: '{}', Subscriber: '{}', channels declarations could be removed. Omitting channel declarations implicitly uses all channels",
                        service_bundle.full_name(symbol_table),
                        name.clone(),
                    ),
                    vec![symbol_table.resolve_subscriber_fqn(name)[0].span.clone().into_option()],
                );
            }
        }
    }
}

impl SymbolTableChecker for VsidlRedundantExplicitChannelListing {
    fn check_service_bundle(
        &mut self,
        symbol_table: &SymbolTable,
        service_bundle: &ServiceBundleSymbol,
    ) {
        self.check_publisher(symbol_table, service_bundle);
        self.check_subscriber(symbol_table, service_bundle);
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::vsidl_redundant_explicit_channel_listing::VsidlRedundantExplicitChannelListing;
    use crate::FindingSeverity;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;

    #[test]
    fn test_redundant_explicit_channel_listing_warning() {
        // GIVEN a catalog with redundant explicit channel listing in vsidl file
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_EXPLICIT_CHANNEL_LISTING.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_EXPLICIT_CHANNEL_LISTING.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = VsidlRedundantExplicitChannelListing::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN warnings should be found
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Service Bundle: 'sdv.test.PublishingBundle', Publisher: 'sdv.test.TirePressure', channels declarations could be removed. Omitting channel declarations implicitly uses all channels"
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Service Bundle: 'sdv.test.SubscribingBundle', Subscriber: 'sdv.test.TirePressure', channels declarations could be removed. Omitting channel declarations implicitly uses all channels"
        ));
    }

    #[test]
    fn test_redundant_explicit_channel_listing_separate_declaration_warning() {
        // GIVEN a catalog with redundant explicit channel listing in vsidl file
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_EXPLICIT_CHANNEL_LISTING.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_EXPLICIT_CHANNEL_LISTING_SEPARATE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = VsidlRedundantExplicitChannelListing::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN warnings should be found
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Service Bundle: 'sdv.test.PublishingBundle', Publisher: 'sdv.test.TirePressure', channels declarations could be removed. Omitting channel declarations implicitly uses all channels"
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Service Bundle: 'sdv.test.SubscribingBundle', Subscriber: 'sdv.test.TirePressure', channels declarations could be removed. Omitting channel declarations implicitly uses all channels"
        ));
    }

    #[test]
    fn test_redundant_implicit_channel_listing_ok() {
        // GIVEN a catalog with no channel listing in vsidl file
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_EXPLICIT_CHANNEL_LISTING.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_IMPLICIT_CHANNEL_LISTING.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = VsidlRedundantExplicitChannelListing::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN no warning should be found
        assert!(check.is_ok());
        assert!(!check.has_findings());
    }

    #[test]
    fn test_redundant_explicit_channel_listing_ok() {
        // GIVEN a catalog with explicit channel listing in vsidl file
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_EXPLICIT_CHANNEL_LISTING.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_EXPLICIT_CHANNEL_LISTING_OK.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = VsidlRedundantExplicitChannelListing::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN no warning should be found
        assert!(check.is_ok());
        assert!(!check.has_findings());
    }
}
