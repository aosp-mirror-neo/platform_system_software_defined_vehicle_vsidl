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
use heck::ToKebabCase;
use std::collections::{HashMap, HashSet};
use vsidl_v1::syntax::Span;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::specific_symbols::proto_message_symbol::ProtoMessageSymbol;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

const IS_PUBLISHER: bool = true;

/// Checks that the name of a service unit (both generated and user-defined) is not used multiple times in
/// a service bundle or globally for publishers
#[derive(Debug, Default)]
pub struct ServiceUnitNameDuplicate {
    finding_collector: FindingCollector,
    /// maps service bundle fully-qualified names to the service unit names found in the bundle
    seen_service_unit_names_in_bundle: HashMap<String, HashSet<String>>,
    /// publisher names found in the full catalog
    seen_service_unit_names_overall: HashSet<String>,
}
impl_context_condition_symtab!(ServiceUnitNameDuplicate);
impl VsidlVisitor for ServiceUnitNameDuplicate {}

/// Checks that the name of a service unit not duplicated
impl ServiceUnitNameDuplicate {
    fn check(
        &mut self,
        service_unit_name: String,
        service_bundle_name: String,
        is_publisher: bool,
        span: Option<Span>,
    ) {
        if !self
            .seen_service_unit_names_in_bundle
            .entry(service_bundle_name.clone())
            .or_default()
            .insert(service_unit_name.clone())
        {
            self.finding_collector.error(
                FindingId::ServiceUnitNameDuplicate,
                format!(
                    "Service unit name '{service_unit_name}' is used multiple times in service bundle '{service_bundle_name}'"
                ),
                vec![span.clone()],
            );
        }
        if is_publisher && !self.seen_service_unit_names_overall.insert(service_unit_name.clone()) {
            self.finding_collector.error(
                FindingId::ServiceUnitNameDuplicate,
                format!("Service unit name '{service_unit_name}' is used by multiple publishers"),
                vec![span],
            );
        }
    }
}

impl SymbolTableChecker for ServiceUnitNameDuplicate {
    fn check_service_bundle(
        &mut self,
        symbol_table: &vsidl_v1_symbols::symbol_table::SymbolTable,
        service_bundle: &vsidl_v1_symbols::specific_symbols::service_bundle_symbol::ServiceBundleSymbol,
    ) {
        // Check diagnostic event service unit names
        for event in service_bundle.diagnostics_events() {
            let service_unit_name = if event.service_unit_name.is_empty() {
                // Check generated name
                format!(
                    "com-android-sdv-diagnostics-event-event-{}",
                    event.id.replace('.', "-").to_kebab_case()
                )
            } else {
                // Check user-defined name
                event.service_unit_name.clone()
            };
            self.check(
                service_unit_name,
                service_bundle.full_name(symbol_table),
                !IS_PUBLISHER,
                service_bundle.span.clone().into_option(),
            );
        }
    }

    fn check_publisher(
        &mut self,
        symbol_table: &vsidl_v1_symbols::symbol_table::SymbolTable,
        publisher: &vsidl_v1_symbols::specific_symbols::publisher_symbol::PublisherSymbol,
    ) {
        // Get service bundle name
        let service_bundle_name =
            publisher.owning_service_bundle(symbol_table).full_name(symbol_table);

        // Get message symbol
        // The context condition is only executed if the symbol table was successfully linked,
        // so the symbol can be looked up directly, instead of resolving the name.
        let proto_message_symbol: &ProtoMessageSymbol = publisher.message(symbol_table);

        // User-defined service unit name
        if !publisher.service_unit_name.is_empty() {
            self.check(
                publisher.service_unit_name.clone(),
                service_bundle_name.clone(),
                IS_PUBLISHER,
                publisher.span.clone().into_option(),
            );
            return;
        }

        // Generated service unit names
        let channels = publisher.channel.clone();

        // Validate generated name
        let fqn = proto_message_symbol.full_name(symbol_table);
        for channel in channels {
            let service_unit_name =
                format!("{}-{}", fqn.replace('.', "-"), channel).to_kebab_case();
            self.check(
                service_unit_name,
                service_bundle_name.clone(),
                IS_PUBLISHER,
                publisher.span.clone().into_option(),
            );
        }
    }

    fn check_rpc_server(
        &mut self,
        symbol_table: &vsidl_v1_symbols::symbol_table::SymbolTable,
        rpc_server: &vsidl_v1_symbols::specific_symbols::rpc_server_symbol::RpcServerSymbol,
    ) {
        // Get service bundle name
        let service_bundle_name =
            rpc_server.owning_service_bundle(symbol_table).full_name(symbol_table);

        let service_unit_name = if rpc_server.service_unit_name.is_empty() {
            // Validate generated name
            format!("{}.{}", rpc_server.package(symbol_table), rpc_server.service)
                .replace('.', "-")
                .to_kebab_case()
        } else {
            // Validate user-defined name
            rpc_server.service_unit_name.clone()
        };
        self.check(
            service_unit_name,
            service_bundle_name,
            !IS_PUBLISHER,
            rpc_server.span.clone().into_option(),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::context_condition::ContextCondition;
    use crate::service_unit_name_duplicate::ServiceUnitNameDuplicate;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;

    #[test]
    fn test_service_unit_name_conflict_pubsub_within_service_bundle() {
        // GIVEN a Catalog and SymbolTable with a name conflict
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_PUBSUB_PUBSUB_CHANNEL_CONFLICT_PROTO.clone()]);
        catalog.catalog_vsidl_files.push(
            vsidl_v1_test_data::VSIDL_SERVICE_UNIT_NAME_PUBSUB_CONFLICT_WITHIN_BUNDLE.clone(),
        );
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "sdv-test-service-bundle-pubsub-conflict-tire-pressure-front-left"
        ));
    }

    #[test]
    fn test_service_unit_name_conflict_generated_and_user_defined_name() {
        // GIVEN a Catalog and SymbolTable with a name conflict
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_RPC_PUBSUB_CHANNEL_CONFLICT_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(
            vsidl_v1_test_data::VSIDL_SERVICE_UNIT_NAME_GENERATED_AND_MANUAL_CONFLICT_WITHIN_BUNDLE
                .clone(),
        );
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "sdv-test-service-bundle-conflict-tire-pressure-front-left"
        ));
    }

    #[test]
    fn test_service_unit_name_conflict_within_service_bundle_manual_and_diagnostics_generated() {
        // GIVEN a Catalog and SymbolTable with a name conflict
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_RPC_PUBSUB_CHANNEL_CONFLICT_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(
            vsidl_v1_test_data::VSIDL_SERVICE_UNIT_NAME_MANUAL_CONFLICT_WITH_ONE_IN_DIAGNOSTICS
                .clone(),
        );
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN Should have found an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "com-android-sdv-diagnostics-event-event-name"
        ));
    }

    #[test]
    fn test_service_unit_name_conflict_within_service_bundle_generated_and_diagnostics_user_defined(
    ) {
        // GIVEN a Catalog and SymbolTable with a name conflict
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_RPC_PUBSUB_CHANNEL_CONFLICT_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(
            vsidl_v1_test_data::VSIDL_GENERATED_SERVICE_UNIT_NAME_CONFLICT_WITH_USER_DEFINED_DIAGNOSTIC
                .clone(),
        );
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN Should have found an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "sdv-test-service-bundle-conflict-tire-pressure-front-left"
        ));
    }

    #[test]
    fn test_service_unit_name_conflict_single_pub_generated_and_user_defined() {
        // GIVEN a Catalog and SymbolTable with a name conflict
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_RPC_PUBSUB_CHANNEL_CONFLICT_PROTO.clone()]);
        catalog.catalog_vsidl_files.push(
            vsidl_v1_test_data::VSIDL_GENERATED_SINGLE_PUB_SERVICE_UNIT_NAME_CONFLICT_WITH_USER_DEFINED_BY_TYPE
                .clone(),
        );
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN Should have found an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "sdv-test-service-bundle-conflict-tire-pressure-front-left"
        ));
    }

    #[test]
    fn test_service_unit_name_conflict_multi_pub_generated_and_single_user_defined() {
        // GIVEN a Catalog and SymbolTable with no name conflict
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_RPC_PUBSUB_CHANNEL_CONFLICT_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(
            vsidl_v1_test_data::VSIDL_GENERATED_MULTI_PUB_SERVICE_UNIT_NAME_CONFLICT_WITH_USER_DEFINED_BY_TYPE_ALLOWED
                .clone(),
        );
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_accept_valid_service_bundle() {
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
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }

    #[test]
    fn test_channels_unique() {
        // Given: A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_PUBLISHER_WITH_IMPLICIT_UNIQUE_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // When: Executing CoCo
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_symbol_table(linker.symbol_table());

        // Then: Should not find errors
        assert!(check.is_ok());
    }

    #[test]
    fn test_channels_empty() {
        // Given: A valid catalog
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_INVALID_SDV_PUBLICATION.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_PUBLISHER_WITH_EMPTY_CHANNEL_ENUM.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // When: Executing CoCo
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_symbol_table(linker.symbol_table());

        // Then: Should not find errors
        assert!(check.is_ok());
    }
}
