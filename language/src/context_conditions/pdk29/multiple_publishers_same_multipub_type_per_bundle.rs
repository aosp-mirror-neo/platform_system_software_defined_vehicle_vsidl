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
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;
use vsidl_v1_symbols::{publisher_symbol::PublisherSymbol, SymbolTable};

#[derive(Debug, Default)]
pub struct MultiplePublishersSameMultipubTopicPerBundle {
    finding_collector: FindingCollector,
    // Service Bundle -> Proto Type -> Publishers using the type
    seen_message_types: HashMap<String, HashMap<String, Vec<PublisherSymbol>>>,
}
impl_context_condition_symtab!(MultiplePublishersSameMultipubTopicPerBundle);
impl VsidlVisitor for MultiplePublishersSameMultipubTopicPerBundle {}

/// Issues an error if there are multiple publishers of the same multipub topic in a service bundle
impl SymbolTableChecker for MultiplePublishersSameMultipubTopicPerBundle {
    fn check_publisher(&mut self, symbol_table: &SymbolTable, publisher: &PublisherSymbol) {
        // Only multipub types are considered by this context condition
        if !publisher.is_multi_pub(symbol_table) {
            return;
        }

        let bundle_fqn = publisher.owning_service_bundle(symbol_table).full_name(symbol_table);

        let proto_message_fqn = publisher
            .enclosing_scope(symbol_table)
            .resolve_proto_message(symbol_table, publisher.message.clone())[0]
            .full_name(symbol_table);

        self.seen_message_types
            .entry(bundle_fqn.clone())
            .or_default()
            .entry(proto_message_fqn.clone())
            .or_default()
            .push(publisher.clone());
    }

    fn after_checks(&mut self, _symbol_table: &SymbolTable) {
        for (bundle_fqn, message_types) in self.seen_message_types.iter() {
            for (message_type_fqn, publishers) in message_types.iter() {
                if publishers.len() > 1 {
                    // Collect all spans from the publishers
                    let spans: Vec<_> = publishers
                        .iter()
                        .map(|publisher| publisher.message_span.clone().into_option())
                        .collect();

                    self.finding_collector.error(
                        FindingId::MultiplePublishersSameMultipubTopic,
                        format!(
                            "Multipub type '{message_type_fqn}' is used by multiple publishers in the same service bundle ('{bundle_fqn}')",
                        ),
                        None, spans,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::pdk29::MultiplePublishersSameMultipubTopicPerBundle;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;

    #[test]
    fn test_should_reject_service_bundle_with_multiple_publishers_for_same_multipub_type() {
        // Given: A catalog with a service bundle that contains multiple publishers for the same multipub type
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_DUPLICATED_MULTIPUB_TYPE_USAGE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // When: Executing CoCo
        let mut check = MultiplePublishersSameMultipubTopicPerBundle::default();
        check.check_symbol_table(linker.symbol_table());

        // Then: Should have found error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "Multipub type 'sdv.test.Speed' is used by multiple publishers in the same service bundle ('sdv.test.DuplicatedMultipubTypeUsage')"));
    }

    #[test]
    fn test_should_accept_valid_catalog() {
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
        let mut check = MultiplePublishersSameMultipubTopicPerBundle::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }
}
