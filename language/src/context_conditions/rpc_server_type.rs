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
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;
use vsidl_v1_symbols::{rpc_server_symbol::RpcServerSymbol, SymbolTable};

#[derive(Debug, Default)]
pub struct RpcServerType {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(RpcServerType);
impl VsidlVisitor for RpcServerType {}

/// Issues an error if the type of a publisher is either not defined at all or not a publication
impl SymbolTableChecker for RpcServerType {
    fn check_rpc_server(&mut self, symbol_table: &SymbolTable, rpc_server: &RpcServerSymbol) {
        // Resolve service type
        let service_symbols = rpc_server
            .enclosing_scope(symbol_table)
            .resolve_proto_rpc_service(symbol_table, rpc_server.service.clone());

        // ERROR: No Proto RPC service
        if service_symbols.is_empty() {
            self.finding_collector.error(
                FindingId::RpcServerTypeDoesNotExist,
                format!(
                    "RPC server '{}' in service bundle '{}' does not have a corresponding Proto RPC service",
                    rpc_server.service,
                    rpc_server.owning_service_bundle(symbol_table).full_name(symbol_table)
                ),
                Some("RPC service does not exist in Proto".to_string()),
                vec![rpc_server.service_span.clone().into_option()],
            );
        }

        // WARN: More than one Proto RPC service (will cause undefined behavior)
        if service_symbols.len() > 1 {
            self.finding_collector.warn(
                FindingId::RpcClientTypeIsAmbiguous,
                format!(
                    "RPC server '{}' in service bundle '{}' has more than one corresponding Proto RPC service - the first found RPC service will be used!",
                    rpc_server.service,
                    rpc_server.owning_service_bundle(symbol_table).full_name(symbol_table)
                ),
                Some("Ambiguous - multiple Proto RPC services found".to_string()),
                vec![rpc_server.service_span.clone().into_option()],
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::rpc_server_type::RpcServerType;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;

    #[test]
    fn test_should_reject_server_with_undefined_proto_type() {
        // GIVEN A catalog with a service bundle that contains a server with a non existing rpc type
        let mut catalog = Catalog::new();
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_NON_EXISTING_RPC_SERVER_TYPE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = RpcServerType::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "RPC server 'sdv.test.DummyRpc' in service bundle 'sdv.test.RpcServerNonexistingType' does not have a corresponding Proto RPC service"));
    }

    #[test]
    fn test_should_reject_server_with_publication_as_service() {
        // GIVEN A catalog with a service bundle that contains a server
        // that uses a publication as its service
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_PUBLICATION_AS_RPC_SERVER_SERVICE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = RpcServerType::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found error
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "RPC server 'BatteryLevel' in service bundle 'sdv.test.RpcServerUsingPublication' does not have a corresponding Proto RPC service"
        ));
    }

    #[test]
    fn test_should_warn_about_client_with_ambiguous_type() {
        // GIVEN A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            // DESCRIPTOR_DUMMY_RPC_PROTO is used twice to cause ambiguity
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = RpcServerType::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found a warning
        assert!(check.any_finding_contains(FindingSeverity::Warn, "DummyRpc"));
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

        // WHEN Executing context condition
        let mut check = RpcServerType::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }
}
