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
use vsidl_v1_symbols::specific_symbols::proto_rpc_service_symbol::ProtoRpcServiceSymbol;
use vsidl_v1_symbols::specific_symbols::rpc_client_symbol::RpcClientSymbol;
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

#[derive(Default)]
pub struct RpcClientRequiresRpcServer {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(RpcClientRequiresRpcServer);
impl VsidlVisitor for RpcClientRequiresRpcServer {}

/// Issues a warning if the unit type specified for an RPC client is unknown
impl SymbolTableChecker for RpcClientRequiresRpcServer {
    fn check_rpc_client(&mut self, symbol_table: &SymbolTable, rpc_client: &RpcClientSymbol) {
        // Resolve Proto RPC service
        let service_symbols = rpc_client
            .enclosing_scope(symbol_table)
            .resolve_proto_rpc_service(symbol_table, rpc_client.service.clone());
        if service_symbols.is_empty() {
            // handled by RpcClientType context condition
            return;
        }
        let service_symbol: ProtoRpcServiceSymbol = service_symbols[0].clone();

        // ERROR: There's no RPC server for the RPC client
        if service_symbol.servers(symbol_table).is_empty() {
            self.finding_collector.warn(
                FindingId::RpcClientRequiresRpcServer,
                format!(
                    "RPC client '{}' does not have a corresponding RPC server",
                    rpc_client.full_name(symbol_table)
                ),
                vec![rpc_client.service_span.clone().into_option()],
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::rpc_client_requires_rpc_server::RpcClientRequiresRpcServer;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;

    #[test]
    fn test_should_reject_client_without_server() {
        // GIVEN A catalog with a service bundle that contains a client without an RPC server
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_RPC_CLIENT_WITHOUT_SERVER.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = RpcClientRequiresRpcServer::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN Should have found a warning
        assert!(check.is_ok());
        assert!(check.any_finding_contains(FindingSeverity::Warn, "sdv.test.DummyRpc"));
    }

    #[test]
    fn test_should_ignore_client_with_undefined_type() {
        // GIVEN A catalog with a service bundle that contains a client with a non existing rpc type
        let mut catalog = Catalog::new();
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_NON_EXISTING_RPC_CLIENT_TYPE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = RpcClientRequiresRpcServer::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
        assert!(check.finding_collector().findings().is_empty());
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
        let mut check = RpcClientRequiresRpcServer::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
        assert!(check.finding_collector().findings().is_empty());
    }
}
