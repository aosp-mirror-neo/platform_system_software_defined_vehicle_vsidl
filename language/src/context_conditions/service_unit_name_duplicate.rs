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
use vsidl_v1::span::Span;
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
                Some("Used multiple times in this service bundle".to_string()),
                vec![span.clone()],
            );
        }
        if is_publisher && !self.seen_service_unit_names_overall.insert(service_unit_name.clone()) {
            self.finding_collector.error(
                FindingId::ServiceUnitNameDuplicate,
                format!("Service unit name '{service_unit_name}' is used by multiple publishers"),
                Some("Used by multiple publishers".to_string()),
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
                publisher.ast_node.service_unit_name_span.clone().into_option(),
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
                publisher.ast_node.message_span.clone().into_option(),
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

        // 1. Check channel name (it's also stored as service_unit_name in SD)
        if !rpc_server.channel.is_empty() {
            self.check(
                rpc_server.channel.clone(),
                service_bundle_name,
                !IS_PUBLISHER,
                rpc_server.ast_node.channel_span.clone().into_option(),
            );
        } else {
            // 2. Check service unit name
            let service_unit_name = if rpc_server.service_unit_name.is_empty() {
                // Validate generated name
                let name = format!("{}.{}", rpc_server.package(symbol_table), rpc_server.service)
                    .replace('.', "-")
                    .to_kebab_case();
                self.check(
                    name,
                    service_bundle_name.clone(),
                    !IS_PUBLISHER,
                    rpc_server.ast_node.service_span.clone().into_option(),
                );
                return;
            } else {
                // Validate user-defined name
                rpc_server.service_unit_name.clone()
            };
            self.check(
                service_unit_name.clone(),
                service_bundle_name.clone(),
                !IS_PUBLISHER,
                rpc_server.ast_node.service_unit_name_span.clone().into_option(),
            );
        }
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
    use vsidl_v1_symbols::TargetApi;

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

        // THEN Should have found any errors
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

        // THEN Should have found any errors
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
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);

        let mut vsidl_entry = vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.1.clone();
        // Set mandatory channels
        vsidl_entry.service_bundle[0].client[0].channel = "client-chan-1".to_string();
        vsidl_entry.service_bundle[0].client[1].channel = "client-chan-2".to_string();
        vsidl_entry.service_bundle[1].server[0].channel = "server-chan-1".to_string();
        vsidl_entry.service_bundle[1].server[1].channel = "server-chan-2".to_string();

        catalog
            .catalog_vsidl_files
            .push((vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.0.clone(), vsidl_entry));

        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
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
    fn test_should_reject_conflicting_channel_names() {
        // GIVEN a catalog where two servers share the same channel name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);

        let mut vsidl_entry = vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.1.clone();
        // Set mandatory channels for clients
        vsidl_entry.service_bundle[0].client[0].channel = "client-chan-1".to_string();
        vsidl_entry.service_bundle[0].client[1].channel = "client-chan-2".to_string();

        // Server 1 uses channel "main"
        vsidl_entry.service_bundle[1].server[0].channel = "main".to_string();
        // Server 2 also uses channel "main" -> conflict
        vsidl_entry.service_bundle[1].server[1].channel = "main".to_string();

        catalog
            .catalog_vsidl_files
            .push((vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.0.clone(), vsidl_entry));

        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN Should have found errors
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Service unit name 'main' is used multiple times in service bundle 'sdv.test.ServiceBundleTwo'"
        ));
    }

    #[test]
    fn test_should_reject_conflicting_channel_and_service_unit_name() {
        // GIVEN a catalog where a channel name conflicts with a publisher's service_unit_name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);

        let mut vsidl_entry = vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.1.clone();

        // Bundle 1: Publisher has service_unit_name "shared-name"
        vsidl_entry.service_bundle[0].publisher[0].service_unit_name = "shared-name".to_string();
        // Bundle 1: Client needs a channel
        vsidl_entry.service_bundle[0].client[0].channel = "client-chan".to_string();

        // Bundle 2: Server has channel "shared-name"
        vsidl_entry.service_bundle[1].server[0].channel = "shared-name".to_string();
        // Conflict if they were in the same bundle, but here they are in different bundles, which is OK.
        // Let's move them to the same bundle for the test.
        let mut bundle = vsidl_entry.service_bundle[0].clone();
        bundle.server.push(vsidl_entry.service_bundle[1].server[0].clone());
        // Now bundle 0 has both.
        vsidl_entry.service_bundle[0] = bundle;

        catalog
            .catalog_vsidl_files
            .push((vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.0.clone(), vsidl_entry));

        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN Should have found errors
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Service unit name 'shared-name' is used multiple times in service bundle 'sdv.test.ServiceBundleOne'"
        ));
    }

    #[test]
    fn test_should_accept_same_channel_and_service_unit_name_on_same_server() {
        // GIVEN a catalog where a server has the same name for channel and service_unit_name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);

        let mut vsidl_entry = vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.1.clone();
        // Bundle 2: Server has channel "shared-name" and service_unit_name "shared-name"
        vsidl_entry.service_bundle[1].server[0].channel = "shared-name".to_string();
        vsidl_entry.service_bundle[1].server[0].service_unit_name = "shared-name".to_string();

        catalog
            .catalog_vsidl_files
            .push((vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.0.clone(), vsidl_entry));

        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
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

    #[test]
    fn test_unique_rpc_channel_ok() {
        // Given: A valid catalog
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS.clone()]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_RPC_UNIQUE_CHANNEL.clone());

        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // When: Executing CoCo
        let mut check = ServiceUnitNameDuplicate::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // Then: Should not find errors
        assert!(check.is_ok());
    }
}
