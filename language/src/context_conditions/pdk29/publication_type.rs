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
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::SymbolTableChecker;
use vsidl_v1_symbols::{publisher_symbol::PublisherSymbol, SymbolTable};

#[derive(Debug, Default)]
pub struct PublicationType {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(PublicationType);
impl VsidlVisitor for PublicationType {}

/// Issues an error if the type of a publisher is either not defined at all or not a publication
impl SymbolTableChecker for PublicationType {
    fn check_publisher(&mut self, symbol_table: &SymbolTable, publisher: &PublisherSymbol) {
        // Get publication symbol
        let proto_publication_symbols = publisher
            .enclosing_scope(symbol_table)
            .resolve_proto_publication(symbol_table, publisher.message.clone());

        // Check that Proto publication exists
        if proto_publication_symbols.is_empty() {
            // Get RPC service symbol (just for printing a better error message)
            let proto_rpc_service_symbols = publisher
                .enclosing_scope(symbol_table)
                .resolve_proto_rpc_service(symbol_table, publisher.message.clone());

            if !proto_rpc_service_symbols.is_empty() {
                self.finding_collector.error(
                    FindingId::PublicationTypeIsNotPublication,
                    format!(
                        "Service bundle '{}': Publisher message type `{}` corresponds to an RPC service, not a publication",
                        publisher.owning_service_bundle(symbol_table).full_name(symbol_table),
                        publisher.message.clone()
                    ),
                    Some("Message field corresponds to an RPC service, not a publication".to_string()), vec![publisher.message_span.clone().into_option()],
                );
            } else {
                self.finding_collector.error(
                    FindingId::PublicationTypeDoesNotExist,
                    format!(
                        "Service bundle '{}': Publisher message type `{}` has no Proto publication",
                        publisher.owning_service_bundle(symbol_table).full_name(symbol_table),
                        publisher.message.clone()
                    ),
                    Some("Message has no Proto publication".to_string()),
                    vec![publisher.message_span.clone().into_option()],
                );
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::pdk29::PublicationType;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;

    #[test]
    fn test_should_reject_publisher_with_invalid_type() {
        // Given: A catalog with a service bundle that contains a publisher using an RPC type as publication type
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_INVALID_PUBLICATION_TYPE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // When: Executing CoCo
        let mut check = PublicationType::default();
        check.check_symbol_table(creator.symbol_table());

        // Then: Should have found error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "Service bundle 'sdv.test.PublisherWithInvalidType': Publisher message type `sdv.test.DummyRpc` corresponds to an RPC service, not a publication"));
    }

    #[test]
    fn test_should_reject_publisher_with_undefined_type() {
        // Given: A catalog with a service bundle that contains a publisher with a non existing publication type
        let mut catalog = Catalog::new();
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_NON_EXISTING_PUBLICATION_TYPE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // When: Executing CoCo
        let mut check = PublicationType::default();
        check.check_symbol_table(creator.symbol_table());

        // Then: Should have found error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "Service bundle 'sdv.test.PublisherWithNonexistingType': Publisher message type `sdv.test.NonExisting` has no Proto publication"));
    }

    #[test]
    fn test_should_accept_valid_service_bundle() {
        // Given: A valid catalog
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

        // When: Executing CoCo
        let mut check = PublicationType::default();
        check.check_symbol_table(creator.symbol_table());

        // Then: Should not have found any errors
        assert!(check.is_ok());
    }
}
