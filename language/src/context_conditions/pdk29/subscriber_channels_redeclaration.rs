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
use std::collections::HashMap;
use vsidl_v1::syntax::Span;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;
use vsidl_v1_symbols::{service_bundle_symbol::ServiceBundleSymbol, SymbolTable};

#[derive(Debug, Default)]

pub struct SubscriberChannelsRedeclaration {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(SubscriberChannelsRedeclaration);
impl VsidlVisitor for SubscriberChannelsRedeclaration {}

impl SymbolTableChecker for SubscriberChannelsRedeclaration {
    fn check_service_bundle(
        &mut self,
        symbol_table: &SymbolTable,
        service_bundle: &ServiceBundleSymbol,
    ) {
        let mut channel_spans: HashMap<(String, String), Vec<Option<Span>>> = HashMap::new();

        // Pass 1: Collect all spans for every (subscriber, channel) pair
        for subscriber in service_bundle.subscribers(symbol_table) {
            let full_name = subscriber.full_name(symbol_table);
            for (index, channel_name) in subscriber.channel.iter().enumerate() {
                let key = (full_name.clone(), channel_name.clone());
                channel_spans
                    .entry(key)
                    .or_default()
                    .push(subscriber.channel_span.get(index).cloned());
            }
        }

        // Pass 2: Report errors for duplicate channel declarations
        for ((sub_name, channel_name), spans) in channel_spans {
            if spans.len() > 1 {
                self.finding_collector.error(
                FindingId::SubscriberChannelsRedeclaration,
                format!(
                    "Service Bundle: '{}', channel '{}' of subscriber '{}' is redeclared in this service bundle",
                    service_bundle.full_name(symbol_table),
                    channel_name,
                    sub_name,
                ),
                spans,
            );
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::pdk29::SubscriberChannelsRedeclaration;
    use crate::FindingSeverity;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;

    #[test]
    fn test_subscriber_channels_redeclaration_error() {
        // GIVEN a catalog with subscriber channels redeclared in vsidl
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_EXPLICIT_CHANNEL_LISTING.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SUBSCRIBER_CHANNELS_REDECLARATION.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = SubscriberChannelsRedeclaration::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN errors should be found
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Service Bundle: 'sdv.test.SubscribingBundle', channel 'FRONT_LEFT' of subscriber 'sdv.test.TirePressure' is redeclared in this service bundle"
        ));
    }

    #[test]
    fn test_subscriber_channels_redeclaration_separate_error() {
        // GIVEN a catalog with subscriber channels redeclared in vsidl
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_EXPLICIT_CHANNEL_LISTING.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SUBSCRIBER_CHANNELS_REDECLARATION_SEPARATE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = SubscriberChannelsRedeclaration::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN errors should be found
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Service Bundle: 'sdv.test.SubscribingBundle', channel 'FRONT_LEFT' of subscriber 'sdv.test.TirePressure' is redeclared in this service bundle"
        ));
    }

    #[test]
    fn test_subscriber_channels_no_redeclaration_ok() {
        // GIVEN a catalog with no subscriber channels redeclared in vsidl
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
        let mut check = SubscriberChannelsRedeclaration::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN result should be ok
        assert!(check.is_ok());
        assert!(!check.has_findings());
    }
}
