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

use crate::{implement_symbol_deref, SymbolCommon, SymbolId, SymbolTable};
use vsidl_v1::span::Span;

use super::{rpc_client_symbol::RpcClientSymbol, rpc_server_symbol::RpcServerSymbol};

/// Symbol representing an RPC Channel.
///
/// RPC channels decouple RPC servers and clients by providing a named communication
/// path for RPC services.
#[derive(Default, Debug, Clone)]
pub struct RpcChannelSymbol {
    /// Common information shared across all symbol types (like ID, name, scope).
    pub symbol: SymbolCommon,
    /// Symbol IDs of the [RpcServerSymbol]s that serve on this channel. Requires the symbol table to be linked.
    pub(crate) servers: Vec<SymbolId>,
    /// Symbol IDs of the [RpcClientSymbol]s that consume from this channel. Requires the symbol table to be linked.
    pub(crate) clients: Vec<SymbolId>,
}

impl RpcChannelSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();

    /// Returns the [RpcServerSymbol]s associated with this channel.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the server IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<&RpcServerSymbol>` containing the servers for this channel.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn servers<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a RpcServerSymbol> {
        symbol_table.panic_if_not_linked();
        self.servers
            .iter()
            .map(|server| symbol_table.get_rpc_server_symbol(server).unwrap())
            .collect()
    }

    /// Returns the [RpcClientSymbol]s associated with this channel.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the client IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<&RpcClientSymbol>` containing the clients for this channel.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn clients<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a RpcClientSymbol> {
        symbol_table.panic_if_not_linked();
        self.clients
            .iter()
            .map(|client| symbol_table.get_rpc_client_symbol(client).unwrap())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::rpc_channel_symbol::RpcChannelSymbol;
    use crate::symbol_table_creator::SymbolTableCreator;
    use crate::symbol_table_linker::SymbolTableLinker;
    use crate::SymbolTable;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;

    fn setup_test(channel_name: &str) -> (SymbolTable, RpcChannelSymbol) {
        // Given: A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        // Modify VSIDL to include channels
        let mut vsidl_entry = vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.1.clone();
        vsidl_entry.service_bundle[0].client[0].channel = "my-channel".to_string();
        vsidl_entry.service_bundle[1].server[0].channel = "my-channel".to_string();

        catalog
            .catalog_vsidl_files
            .push((vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.0.clone(), vsidl_entry));

        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();
        let symbol_table = linker.symbol_table().clone();
        let channel_symbol = symbol_table.resolve_rpc_channel_fqn(channel_name)[0].clone();
        (symbol_table, channel_symbol)
    }

    #[test]
    fn test_servers() {
        // GIVEN a valid symbol table with an RPC channel
        let (symbol_table, channel_symbol) = setup_test("my-channel");

        // WHEN getting the servers
        let servers = channel_symbol.servers(&symbol_table);

        // THEN the server should be found
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].service(&symbol_table).full_name(&symbol_table), "sdv.test.DummyRpc");
    }

    #[test]
    fn test_clients() {
        // GIVEN a valid symbol table with an RPC channel
        let (symbol_table, channel_symbol) = setup_test("my-channel");

        // WHEN getting the clients
        let clients = channel_symbol.clients(&symbol_table);

        // THEN the client should be found
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].service(&symbol_table).full_name(&symbol_table), "sdv.test.DummyRpc");
    }
}
