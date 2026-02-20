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

pub struct VsidlRedundantExplicitTopicListing {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(VsidlRedundantExplicitTopicListing);
impl VsidlVisitor for VsidlRedundantExplicitTopicListing {}

impl VsidlRedundantExplicitTopicListing {
    fn check_publisher(
        &mut self,
        symbol_table: &SymbolTable,
        service_bundle: &ServiceBundleSymbol,
    ) {
        // Maps a publisher's fully qualified name to the set of its explicitly declared topics.
        let mut explicit_topics: HashMap<String, HashSet<String>> = HashMap::new();
        // Maps a publisher's fully qualified name to the total number of topics available for its publication type.
        let mut all_topics: HashMap<String, usize> = HashMap::new();

        let publishers = service_bundle.publishers(symbol_table);

        // Collect all explicitly declared topics for each publisher type from
        // potentially multiple publisher declarations, and store the total number of
        // topics available for that type to compare against later.
        for publisher in publishers {
            // Publishers that define a topic are not redundant
            if publisher.capacity != 0 {
                continue;
            }

            let full_name = publisher.full_name(symbol_table);
            explicit_topics.entry(full_name.clone()).or_default();
            all_topics.insert(
                full_name.clone(),
                publisher.message(symbol_table).topics(symbol_table).len(),
            );

            for topic in &publisher.topic {
                if let Some(inst) = explicit_topics.get_mut(&full_name) {
                    inst.insert(topic.clone());
                }
            }
        }

        // For each publisher type, check if the number of explicitly declared topics
        // equals the total number of available topics. If so, the explicit listing is
        // redundant, and a warning is issued.
        for (name, topics) in &explicit_topics {
            if all_topics[name] == 0 {
                continue;
            } else if all_topics[name] == topics.len() {
                self.finding_collector.warn(
                    FindingId::VsidlRedundantExplicitChannelListing,
                    format!(
                        "Service Bundle: '{}', Publisher: '{}', topics declarations could be removed. Omitting topic declarations implicitly uses all topics",
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
        // Maps a subscriber's fully qualified name to the set of its explicitly declared topics.
        let mut explicit_topics: HashMap<String, HashSet<String>> = HashMap::new();
        // Maps a subscriber's fully qualified name to the total number of topics available for its publication type.
        let mut all_topics: HashMap<String, usize> = HashMap::new();

        let subscribers = service_bundle.subscribers(symbol_table);

        // Collect all explicitly declared topics for each subscriber type from
        // potentially multiple subscriber declarations, and store the total number of
        // topics available for that type to compare against later.
        for subscriber in subscribers {
            let full_name = subscriber.full_name(symbol_table);
            explicit_topics.entry(full_name.clone()).or_default();
            all_topics.insert(
                full_name.clone(),
                subscriber.message(symbol_table).topics(symbol_table).len(),
            );

            for topic in &subscriber.topic {
                if let Some(inst) = explicit_topics.get_mut(&full_name) {
                    inst.insert(topic.clone());
                }
            }
        }

        // For each subscriber type, check if the number of explicitly declared topics
        // equals the total number of available topics. If so, the explicit listing is
        // redundant, and a warning is issued.
        for (name, topics) in &explicit_topics {
            if all_topics[name] == 0 {
                continue;
            } else if all_topics[name] == topics.len() {
                self.finding_collector.warn(
                    FindingId::VsidlRedundantExplicitChannelListing,
                    format!(
                        "Service Bundle: '{}', Subscriber: '{}', topics declarations could be removed. Omitting topic declarations implicitly uses all topics",
                        service_bundle.full_name(symbol_table),
                        name.clone(),
                    ),
                    vec![symbol_table.resolve_subscriber_fqn(name)[0].span.clone().into_option()],
                );
            }
        }
    }
}

impl SymbolTableChecker for VsidlRedundantExplicitTopicListing {
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
    use crate::redundant_explicit_topic_listing::VsidlRedundantExplicitTopicListing;
    use crate::FindingSeverity;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;
    use vsidl_v1_symbols::TargetApi;

    #[test]
    fn test_redundant_explicit_topic_listing_warning() {
        // GIVEN a catalog with redundant explicit topic listing in vsidl file
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_EXPLICIT_CHANNEL_LISTING.clone()]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_EXPLICIT_TOPIC_LISTING.clone());
        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = VsidlRedundantExplicitTopicListing::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN warnings should be found
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Service Bundle: 'sdv.test.SubscribingBundle', Subscriber: 'sdv.test.TirePressure', topics declarations could be removed. Omitting topic declarations implicitly uses all topics"
        ));
    }

    #[test]
    fn test_redundant_explicit_topic_listing_separate_declaration_warning() {
        // GIVEN a catalog with redundant explicit topic listing in vsidl file
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_EXPLICIT_CHANNEL_LISTING.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_EXPLICIT_TOPIC_LISTING_SEPARATE.clone());
        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = VsidlRedundantExplicitTopicListing::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN warnings should be found
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Service Bundle: 'sdv.test.SubscribingBundle', Subscriber: 'sdv.test.TirePressure', topics declarations could be removed. Omitting topic declarations implicitly uses all topics"
        ));
    }

    #[test]
    fn test_redundant_implicit_topic_listing_ok() {
        // GIVEN a catalog with no topic listing in vsidl file
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_EXPLICIT_CHANNEL_LISTING.clone()]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_IMPLICIT_TOPIC_LISTING.clone());
        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = VsidlRedundantExplicitTopicListing::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN no warning should be found
        assert!(check.is_ok());
        assert!(!check.has_findings());
    }

    #[test]
    fn test_redundant_explicit_topic_listing_ok() {
        // GIVEN a catalog with explicit topic listing in vsidl file
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_EXPLICIT_CHANNEL_LISTING.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_EXPLICIT_TOPIC_LISTING_OK.clone());
        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = VsidlRedundantExplicitTopicListing::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN no warning should be found
        assert!(check.is_ok());
        assert!(!check.has_findings());
    }
}
