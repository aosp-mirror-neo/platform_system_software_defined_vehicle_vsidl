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

use crate::{
    implement_ast_node_deref, implement_symbol_deref, SymbolCommon, SymbolId, SymbolTable,
};
use heck::ToKebabCase;
use vsidl_v1::span::Span;
use vsidl_v1::syntax::service_bundle::Server;

use super::{
    proto_rpc_service_symbol::ProtoRpcServiceSymbol, rpc_channel_symbol::RpcChannelSymbol,
    service_bundle_symbol::ServiceBundleSymbol,
};

/// Symbol representing a VSIDL RPC Server definition within a service bundle.
///
/// This struct holds information about a specific RPC server declared in a `.vsidl` file,
/// including its AST node, common symbol properties, the service it provides,
/// and the service bundle it belongs to. It allows accessing the underlying
/// [Server] AST node via `Deref`.
#[derive(Default, Debug, Clone)]
pub struct RpcServerSymbol {
    /// The original VSIDL AST node ([Server]) this symbol was created from.
    pub ast_node: Server,
    /// Common information shared across all symbol types (like ID, name, scope).
    pub symbol: SymbolCommon,
    /// Symbol ID of the [ProtoRpcServiceSymbol] provided by this server. Requires the symbol table to be linked.
    pub(crate) provided_service: Option<SymbolId>,
    /// Symbol ID of the [RpcChannelSymbol] used by this server. Requires the symbol table to be linked.
    pub(crate) channel: Option<SymbolId>,
    /// Symbol ID of the [ServiceBundleSymbol] that owns this RPC server. Requires the symbol table to be linked.
    pub(crate) owning_service_bundle: Option<SymbolId>,
}
// Implements Deref for the ast_node field, allowing direct access to the Server fields.
implement_ast_node_deref!(RpcServerSymbol, Server);

impl RpcServerSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();

    /// Returns the [ProtoRpcServiceSymbol] associated with this server.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the service ID.
    ///
    /// # Returns
    ///
    /// The [ProtoRpcServiceSymbol] that this server provides.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn service<'a>(&self, symbol_table: &'a SymbolTable) -> &'a ProtoRpcServiceSymbol {
        symbol_table.panic_if_not_linked();
        symbol_table.get_proto_rpc_service_symbol(&self.provided_service.clone().unwrap()).unwrap()
    }

    /// Returns the [RpcChannelSymbol] associated with this server.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the channel ID.
    ///
    /// # Returns
    ///
    /// The [RpcChannelSymbol] that this server uses.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn channel<'a>(&self, symbol_table: &'a SymbolTable) -> &'a RpcChannelSymbol {
        symbol_table.panic_if_not_linked();
        assert!(!symbol_table.uses_pdk29());
        symbol_table.get_rpc_channel_symbol(&self.channel.clone().unwrap()).unwrap()
    }

    /// Checks if the RPC server has a user-defined service unit name specified in the VSIDL.
    ///
    /// # Returns
    ///
    /// `true` if `service_unit_name` is defined in the AST node, `false` otherwise.
    pub fn has_user_defined_name(&self) -> bool {
        !self.service_unit_name.is_empty()
    }

    /// Returns the service unit name for this RPC server.
    ///
    /// If the server has a user-defined `service_unit_name` in the VSIDL, that name is returned.
    /// Otherwise, a service unit name is generated based on the service's fully qualified name.
    /// The generated name follows the kebab-case convention.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table required for resolving the service name.
    ///
    /// # Returns
    ///
    /// The service unit name as a `String`.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn service_unit_name(&self, symbol_table: &SymbolTable) -> String {
        symbol_table.panic_if_not_linked();

        if self.has_user_defined_name() {
            self.service_unit_name.clone()
        } else {
            self.service(symbol_table).full_name(symbol_table).to_kebab_case()
        }
    }

    /// Returns the owning [ServiceBundleSymbol] for this RPC server.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the service bundle ID.
    ///
    /// # Returns
    ///
    /// The [ServiceBundleSymbol] that owns this RPC server.
    pub fn owning_service_bundle(&self, symbol_table: &SymbolTable) -> ServiceBundleSymbol {
        symbol_table
            .get_service_bundle_symbol(&self.owning_service_bundle.clone().unwrap())
            .unwrap()
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::rpc_server_symbol::RpcServerSymbol;
    use crate::symbol_table_creator::SymbolTableCreator;
    use crate::symbol_table_linker::SymbolTableLinker;
    use crate::SymbolTable;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;

    /// Sets up a test environment with a linked symbol table and a specific [RpcServerSymbol].
    ///
    /// # Parameters
    ///
    /// * `fqn` - The fully qualified name of the Protobuf RPC service symbol to retrieve the server for.
    ///
    /// # Returns
    ///
    /// A tuple containing the linked [SymbolTable] and the requested [RpcServerSymbol].
    fn setup_test(fqn: &str) -> (SymbolTable, RpcServerSymbol) {
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
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();
        (
            linker.symbol_table().clone(),
            linker.symbol_table().resolve_rpc_server_fqn(fqn)[0].clone(),
        )
    }

    #[test]
    fn test_publication() {
        // GIVEN a valid symbol table
        let (symbol_table, server_symbol) = setup_test("sdv.test.DummyRpc");

        // WHEN getting the service
        let service = server_symbol.service(&symbol_table);

        // THEN the publication should be found
        assert_eq!(service.full_name(&symbol_table), "sdv.test.DummyRpc");
    }

    #[test]
    fn test_service_unit_name_with_generated_name() {
        // GIVEN a valid symbol table
        let (symbol_table, server_symbol) = setup_test("sdv.test.DummyRpc");

        // WHEN getting the service unit name
        let result = server_symbol.service_unit_name(&symbol_table);

        // THEN the service unit name should be the generated one
        assert_eq!(result, "sdv-test-dummy-rpc");
    }

    #[test]
    fn test_service_unit_name_with_user_defined_name() {
        // GIVEN a valid symbol table
        let (symbol_table, server_symbol) = setup_test("sdv.test.AnotherDummyRpc");

        // WHEN getting the service unit name
        let result = server_symbol.service_unit_name(&symbol_table);

        // THEN the service unit name should be the user-defined one
        assert_eq!(result, "myrpc");
    }

    #[test]
    fn test_has_user_defined_name_with_generated_name() {
        // GIVEN a valid symbol table
        let (_symbol_table, server_symbol) = setup_test("sdv.test.DummyRpc");

        // WHEN getting the service unit name
        let result = server_symbol.has_user_defined_name();

        // THEN the service unit name should not claim to have a user-defined name
        assert!(!result);
    }

    #[test]
    fn test_has_user_defined_name_with_user_defined_name() {
        // GIVEN a valid symbol table
        let (_symbol_table, server_symbol) = setup_test("sdv.test.AnotherDummyRpc");

        // WHEN getting the service unit name
        let result = server_symbol.has_user_defined_name();

        // THEN the service unit name should identify it has a user-defined name
        assert!(result);
    }

    #[test]
    fn test_owning_service_bundle() {
        // GIVEN a valid symbol table
        let (symbol_table, server_symbol) = setup_test("sdv.test.DummyRpc");

        // WHEN getting the service bundle
        let bundle = server_symbol.owning_service_bundle(&symbol_table);

        // THEN the service bundle should be found
        assert_eq!(bundle.full_name(&symbol_table), "sdv.test.ServiceBundleTwo");
    }
}
