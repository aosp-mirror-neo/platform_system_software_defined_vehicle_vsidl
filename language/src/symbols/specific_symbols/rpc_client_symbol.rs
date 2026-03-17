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
use vsidl_v1::span::Span;
use vsidl_v1::syntax::service_bundle::Client;

use super::{
    proto_rpc_service_symbol::ProtoRpcServiceSymbol, rpc_channel_symbol::RpcChannelSymbol,
    service_bundle_symbol::ServiceBundleSymbol,
};

/// Symbol representing a VSIDL RPC Client definition within a service bundle.
///
/// This struct holds information about a specific RPC client declared in a `.vsidl` file,
/// including its AST node, common symbol properties, the service it consumes,
/// and the service bundle it belongs to. It allows accessing the underlying
/// [Client] AST node via `Deref`.
#[derive(Default, Debug, Clone)]
pub struct RpcClientSymbol {
    /// The original VSIDL AST node ([Client]) this symbol was created from.
    pub ast_node: Client,
    /// Common information shared across all symbol types (like ID, name, scope).
    pub symbol: SymbolCommon,
    /// Symbol ID of the [ProtoRpcServiceSymbol] consumed by this client. Requires the symbol table to be linked.
    pub(crate) requested_service: Option<SymbolId>,
    /// Symbol ID of the [RpcChannelSymbol] used by this client. Requires the symbol table to be linked.
    pub(crate) channel: Option<SymbolId>,
    /// Symbol ID of the [ServiceBundleSymbol] that owns this RPC client. Requires the symbol table to be linked.
    pub(crate) owning_service_bundle: Option<SymbolId>,
}
// Implements Deref for the ast_node field, allowing direct access to the Client fields.
implement_ast_node_deref!(RpcClientSymbol, Client);

impl RpcClientSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();

    /// Returns the [ProtoRpcServiceSymbol] associated with this client.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the service ID.
    ///
    /// # Returns
    ///
    /// The [ProtoRpcServiceSymbol] that this client consumes.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn service<'a>(&self, symbol_table: &'a SymbolTable) -> &'a ProtoRpcServiceSymbol {
        symbol_table.panic_if_not_linked();
        symbol_table.get_proto_rpc_service_symbol(&self.requested_service.clone().unwrap()).unwrap()
    }

    /// Returns the [RpcChannelSymbol] associated with this client.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the channel ID.
    ///
    /// # Returns
    ///
    /// The [RpcChannelSymbol] that this client uses.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn channel<'a>(&self, symbol_table: &'a SymbolTable) -> &'a RpcChannelSymbol {
        symbol_table.panic_if_not_linked();
        assert!(!symbol_table.uses_pdk29());
        symbol_table.get_rpc_channel_symbol(&self.channel.clone().unwrap()).unwrap()
    }

    /// Returns the owning [ServiceBundleSymbol] for this RPC client.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the service bundle ID.
    ///
    /// # Returns
    ///
    /// The [ServiceBundleSymbol] that owns this RPC client.
    pub fn owning_service_bundle<'a>(
        &self,
        symbol_table: &'a SymbolTable,
    ) -> &'a ServiceBundleSymbol {
        symbol_table
            .get_service_bundle_symbol(&self.owning_service_bundle.clone().unwrap())
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::rpc_client_symbol::RpcClientSymbol;
    use crate::symbol_table_creator::SymbolTableCreator;
    use crate::symbol_table_linker::SymbolTableLinker;
    use crate::SymbolTable;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;

    /// Sets up a test environment with a linked symbol table and a specific [RpcClientSymbol].
    ///
    /// # Returns
    ///
    /// A tuple containing the linked [SymbolTable] and the requested [RpcClientSymbol].
    fn setup_test() -> (SymbolTable, RpcClientSymbol) {
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
            linker.symbol_table().resolve_rpc_client_fqn("sdv.test.DummyRpc")[0].clone(),
        )
    }

    #[test]
    fn test_publication() {
        // GIVEN a valid symbol table
        let (symbol_table, client_symbol) = setup_test();

        // WHEN getting the service
        let service = client_symbol.service(&symbol_table);

        // THEN the publication should be found
        assert_eq!(service.full_name(&symbol_table), "sdv.test.DummyRpc");
    }

    #[test]
    fn test_owning_service_bundle() {
        // GIVEN a valid symbol table
        let (symbol_table, client_symbol) = setup_test();

        // WHEN getting the service bundle
        let bundle = client_symbol.owning_service_bundle(&symbol_table);

        // THEN the service bundle should be found
        assert_eq!(bundle.full_name(&symbol_table), "sdv.test.ServiceBundleOne");
    }
}
