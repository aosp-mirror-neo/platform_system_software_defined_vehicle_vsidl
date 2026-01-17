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
use protobuf::descriptor::{MethodDescriptorProto, ServiceDescriptorProto};
use vsidl_v1::syntax::Span;
use vsidl_v1_common::ProtoMetadata;

use super::{rpc_client_symbol::RpcClientSymbol, rpc_server_symbol::RpcServerSymbol};

/// Specifies where the VSIDL file defining a [ProtoRpcServiceSymbol] originates from.
#[derive(Default, Debug, Clone, PartialEq)]
pub enum ProtoRpcServiceSource {
    /// Indicates the service bundle is defined in a dependency VSIDL file.
    #[default]
    DEPENDENCY,
    /// Indicates the service bundle is defined directly within the processed catalog.
    CATALOG,
}

/// Represents a Protobuf Service definition within the symbol table.
///
/// This struct holds information about a specific RPC service defined in a `.proto` file,
/// including its AST node, common symbol properties, metadata, dependencies,
/// and links to related symbols like servers, clients, and the owning unit type.
/// It allows accessing the underlying [ServiceDescriptorProto] AST node via `Deref`.
#[derive(Default, Debug, Clone)]
pub struct ProtoRpcServiceSymbol {
    /// The original Protobuf AST node ([ServiceDescriptorProto]) this symbol was created from.
    pub ast_node: ServiceDescriptorProto,
    /// Common information shared across all symbol types (like ID, name, scope).
    pub symbol: SymbolCommon,
    // Doc string in front of the definition of the symbol in the Protobuf file
    pub doc_string: String,
    /// Indicates whether the RPC service definition comes from the main catalog or a dependency.
    pub source: ProtoRpcServiceSource,
    /// Metadata related to the `.proto` file where this service is defined.
    pub proto_metadata: ProtoMetadata,
    /// Metadata for the `.proto` files that the defining `.proto` file depends on.
    pub proto_dependencies: Vec<ProtoMetadata>,
    /// Symbol IDs of the [RpcServerSymbol]s that serve this service. Requires the symbol table to be linked.
    pub(crate) servers: Vec<SymbolId>,
    /// Symbol IDs of the [RpcClientSymbol]s that consume this service. Requires the symbol table to be linked.
    pub(crate) clients: Vec<SymbolId>,
}
// Implements Deref for the ast_node field, allowing direct access to the DescriptorProto fields.
implement_ast_node_deref!(ProtoRpcServiceSymbol, ServiceDescriptorProto);

impl ProtoRpcServiceSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();

    /// Returns the [RpcServerSymbol]s associated with this service.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the server IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<RpcServerSymbol>` containing the servers for this service.
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

    /// Returns the [RpcClientSymbol]s associated with this service.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the client IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<RpcClientSymbol>` containing the clients for this service.
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

    /// helper function to get MethodDescriptorProto from method name
    fn get_method_descriptor_proto(&self, method_name: impl Into<String>) -> MethodDescriptorProto {
        let method_name_string = method_name.into();
        let method: &MethodDescriptorProto = self
            .method
            .iter()
            .find(|method_descriptor| {
                method_descriptor.name.as_deref() == Some(&method_name_string)
            })
            .unwrap_or_else(|| {
                panic!(
                    "method_uses_client_streaming: There's no method with name '{}'",
                    method_name_string
                )
            });
        method.clone()
    }

    /// Checks if a specific RPC method uses client-side streaming.
    ///
    /// # Parameters
    ///
    /// * `method_name` - The name of the RPC method to check.
    ///
    /// # Returns
    ///
    /// `true` if the method uses client streaming, `false` otherwise.
    ///
    /// # Panics
    ///
    /// * If no method with the given `method_name` is found in the service.
    pub fn method_uses_client_streaming(&self, method_name: impl Into<String>) -> bool {
        let method = self.get_method_descriptor_proto(method_name);
        method.client_streaming.unwrap_or(false)
    }

    /// Checks if a specific RPC method uses server-side streaming.
    ///
    /// # Parameters
    ///
    /// * `method_name` - The name of the RPC method to check.
    ///
    /// # Returns
    ///
    /// `true` if the method uses server streaming, `false` otherwise.
    ///
    /// # Panics
    ///
    /// * If no method with the given `method_name` is found in the service.
    pub fn method_uses_server_streaming(&self, method_name: impl Into<String>) -> bool {
        let method = self.get_method_descriptor_proto(method_name);
        method.server_streaming.unwrap_or(false)
    }
}

#[cfg(test)]
/// Unit tests for the [ProtoRpcServiceSymbol].
mod tests {
    use crate::proto_rpc_service_symbol::ProtoRpcServiceSymbol;
    use crate::symbol_table_creator::SymbolTableCreator;
    use crate::symbol_table_linker::SymbolTableLinker;
    use crate::SymbolTable;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;

    /// Sets up a test environment with a linked symbol table and a specific [ProtoRpcServiceSymbol].
    ///
    /// # Parameters
    ///
    /// * `fqn` - The fully qualified name of the Protobuf RPC service symbol to retrieve.
    ///
    /// # Returns
    ///
    /// A tuple containing the linked [SymbolTable] and the requested [ProtoRpcServiceSymbol].
    fn setup_test(fqn: &str) -> (SymbolTable, ProtoRpcServiceSymbol) {
        // Given: A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_RPC_STREAMING_CALL.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();
        (
            linker.symbol_table().clone(),
            linker.symbol_table().resolve_proto_rpc_service_fqn(fqn)[0].clone(),
        )
    }

    #[test]
    fn test_servers() {
        // GIVEN a valid symbol table and a ProtoRpcServiceSymbol
        let (symbol_table, proto_rpc_service_symbol) = setup_test("sdv.test.DummyRpc");

        // WHEN getting the servers
        let servers = proto_rpc_service_symbol.servers(&symbol_table);

        // THEN the correct server should be found and associated with the correct service bundle
        assert!(servers.len() == 1);
        let owner = servers[0].owning_service_bundle(&symbol_table).full_name(&symbol_table);
        assert_eq!(owner, "sdv.test.ServiceBundleTwo");
    }

    #[test]
    fn test_clients() {
        // GIVEN a valid symbol table and a ProtoRpcServiceSymbol
        let (symbol_table, proto_rpc_service_symbol) = setup_test("sdv.test.DummyRpc");

        // WHEN getting the clients
        let clients = proto_rpc_service_symbol.clients(&symbol_table);

        // THEN the correct client should be found and associated with the correct service bundle
        assert!(clients.len() == 1);
        let owner = clients[0].owning_service_bundle(&symbol_table).full_name(&symbol_table);
        assert_eq!(owner, "sdv.test.ServiceBundleOne");
    }

    #[test]
    fn test_method_uses_client_streaming_true() {
        // GIVEN a valid symbol table
        let (_, proto_rpc_service_symbol) = setup_test("sdv.test.StreamingRpc");

        // WHEN calling method_uses_client_streaming with a method that uses client streaming
        let result =
            proto_rpc_service_symbol.method_uses_client_streaming("method_with_client_streaming");

        // THEN should have identified the method as a method that uses client streaming
        assert!(result);
    }

    #[test]
    fn test_method_ses_client_streaming_false() {
        // GIVEN a valid symbol table
        let (_, proto_rpc_service_symbol) = setup_test("sdv.test.StreamingRpc");

        // WHEN calling method_uses_client_streaming with a method that uses no streaming
        let result =
            proto_rpc_service_symbol.method_uses_client_streaming("method_without_streaming");

        // THEN should have identified the method as a method that DOES NOT use client streaming
        assert!(!result);

        // WHEN calling method_uses_client_streaming with a method that uses server streaming
        let result =
            proto_rpc_service_symbol.method_uses_client_streaming("method_with_server_streaming");

        // THEN should have identified the method as a method that DOES NOT use client streaming
        assert!(!result);
    }

    #[test]
    #[should_panic]
    fn test_method_uses_client_streaming_method_unknown() {
        // GIVEN a valid symbol table
        let (_, proto_rpc_service_symbol) = setup_test("sdv.test.StreamingRpc");

        // WHEN calling method_uses_client_streaming with a method that does not exist
        let _ = proto_rpc_service_symbol.method_uses_client_streaming("unknown_method");

        // THEN should panic - ensured by the #[should_panic] annotation
    }

    #[test]
    fn test_method_uses_server_streaming_true() {
        // GIVEN a valid symbol table
        let (_, proto_rpc_service_symbol) = setup_test("sdv.test.StreamingRpc");

        // WHEN calling method_uses_server_streaming with a method that uses server streaming
        let result =
            proto_rpc_service_symbol.method_uses_server_streaming("method_with_server_streaming");

        // THEN should hae identified the method as a method that uses server streaming
        assert!(result);
    }

    #[test]
    fn test_method_uses_server_streaming_false() {
        // GIVEN a valid symbol table
        let (_, proto_rpc_service_symbol) = setup_test("sdv.test.StreamingRpc");

        // WHEN calling method_uses_server_streaming with a method that uses no streaming
        let result =
            proto_rpc_service_symbol.method_uses_server_streaming("method_without_streaming");

        // THEN should have identified the method as a method that DOES NOT use server streaming
        assert!(!result);

        // WHEN calling method_uses_server_streaming with a method that uses client streaming
        let result =
            proto_rpc_service_symbol.method_uses_server_streaming("method_with_client_streaming");

        // THEN should have identified the method as a method that DOES NOT use client streaming
        assert!(!result);
    }

    #[test]
    #[should_panic]
    fn test_method_uses_server_streaming_unknown() {
        // GIVEN a valid symbol table
        let (_, proto_rpc_service_symbol) = setup_test("sdv.test.StreamingRpc");

        // WHEN calling method_uses_server_streaming with a method that does not exist
        let _ = proto_rpc_service_symbol.method_uses_server_streaming("unknown_method");

        // THEN should panic - ensured by the #[should_panic] annotation
    }
}
