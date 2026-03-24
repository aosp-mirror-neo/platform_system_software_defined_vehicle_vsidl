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
use vsidl_v1_symbols::specific_symbols::rpc_client_symbol::RpcClientSymbol;
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

#[derive(Default)]
pub struct RpcClientType {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(RpcClientType);
impl VsidlVisitor for RpcClientType {}

/// Issues an error if the unit type specified for an RPC client is unknown
impl SymbolTableChecker for RpcClientType {
    fn check_rpc_client(&mut self, symbol_table: &SymbolTable, rpc_client: &RpcClientSymbol) {
        // Resolve service type
        let service_symbols = rpc_client
            .enclosing_scope(symbol_table)
            .resolve_proto_rpc_service(symbol_table, rpc_client.service.clone());

        // ERROR: No Proto RPC service
        if service_symbols.is_empty() {
            self.finding_collector.error(
                FindingId::RpcClientTypeDoesNotExist,
                format!(
                    "RPC client '{}' does not have a corresponding Proto RPC service",
                    rpc_client.service
                ),
                Some("RPC service does not exist in Proto".to_string()),
                vec![rpc_client.service_span.clone().into_option()],
            );
        }

        // WARN: More than one Proto RPC service (will cause undefined behavior)
        if service_symbols.len() > 1 {
            self.finding_collector.warn(
                FindingId::RpcClientTypeIsAmbiguous,
                format!(
                    "RPC client '{}' has more than one corresponding Proto RPC service - the first found RPC service will be used!",
                    rpc_client.service
                ),
                Some("Ambiguous - multiple Proto RPC services found".to_string()),
                vec![rpc_client.service_span.clone().into_option()],
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::rpc_client_type::RpcClientType;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;

    #[test]
    fn test_should_reject_client_with_undefined_type() {
        // GIVEN A catalog with a service bundle that contains a client with a non existing rpc type
        let mut catalog = Catalog::new();
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_NON_EXISTING_RPC_CLIENT_TYPE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = RpcClientType::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "sdv.test.NonExisting"));
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
        let mut check = RpcClientType::default();
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
        let mut check = RpcClientType::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }
}
