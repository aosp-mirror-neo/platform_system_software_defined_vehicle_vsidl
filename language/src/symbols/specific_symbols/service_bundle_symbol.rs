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

use crate::{implement_ast_node_deref, implement_symbol_deref, SymbolCommon};
use crate::{SymbolId, SymbolTable};
use indexmap::IndexSet;
use protobuf::MessageField;
use std::path::PathBuf;
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::Event;
use vsidl_v1::syntax::ServiceBundle;
use vsidl_v1::syntax::Span;
use vsidl_v1_common::DiagnosticsInfo;

use super::publisher_symbol::PublisherSymbol;
use super::rpc_client_symbol::RpcClientSymbol;
use super::rpc_server_symbol::RpcServerSymbol;
use super::subscriber_symbol::SubscriberSymbol;

/// Specifies where the VSIDL file defining a [ServiceBundleSymbol] originates from.
#[derive(Default, Debug, Clone, PartialEq)]
pub enum ServiceBundleSource {
    /// Indicates the service bundle is defined in a dependency VSIDL file.
    #[default]
    DEPENDENCY,
    /// Indicates the service bundle is defined directly within the processed catalog.
    CATALOG,
}

/// Symbol representing a VSIDL Service Bundle definition.
///
/// This struct holds information about a specific service bundle defined in a `.vsidl` file,
/// including its AST node, common symbol properties, associated symbols (like unit types,
/// publishers, subscribers, servers, clients), diagnostics information, and source location.
/// It allows accessing the underlying [ServiceBundle] AST node via `Deref`.
#[derive(Default, Debug, Clone)]
pub struct ServiceBundleSymbol {
    /// The original VSIDL AST node ([ServiceBundle]) this symbol was created from.
    pub ast_node: ServiceBundle,
    /// Common information shared across all symbol types (like ID, name, scope).
    pub symbol: SymbolCommon,

    // Note: the following sets are `IndexSet`s to keep the order of the
    // generated code stable to avoid false positive changes in golden tests
    /// Symbol IDs of the [PublisherSymbol]s defined within this service bundle.
    pub(crate) publishers: IndexSet<SymbolId>,
    /// Symbol IDs of the [SubscriberSymbol]s defined within this service bundle.
    pub(crate) subscribers: IndexSet<SymbolId>,
    /// Symbol IDs of the [RpcServerSymbol]s defined within this service bundle.
    pub(crate) servers: IndexSet<SymbolId>,
    /// Symbol IDs of the [RpcClientSymbol]s defined within this service bundle.
    pub(crate) clients: IndexSet<SymbolId>,
    /// Optional diagnostics information associated with this service bundle.
    pub diagnostics_info: Option<DiagnosticsInfo>,
    /// Indicates whether the service bundle definition comes from the main catalog or a dependency.
    pub source: ServiceBundleSource,
    /// The absolute path to the `.vsidl` file where this service bundle is defined.
    pub file_path: PathBuf,
}
// Implements Deref for the ast_node field, allowing direct access to the ServiceBundle fields.
implement_ast_node_deref!(ServiceBundleSymbol, ServiceBundle);

impl ServiceBundleSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();

    /// Returns the [PublisherSymbol]s defined within this service bundle.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the publisher IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<PublisherSymbol>` containing the publishers defined in this service bundle.
    pub fn publishers<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a PublisherSymbol> {
        self.publishers
            .iter()
            .map(|publisher_symbol| symbol_table.get_publisher_symbol(publisher_symbol).unwrap())
            .collect()
    }

    /// Returns the [SubscriberSymbol]s defined within this service bundle.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the subscriber IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<SubscriberSymbol>` containing the subscribers defined in this service bundle.
    pub fn subscribers<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a SubscriberSymbol> {
        self.subscribers
            .iter()
            .map(|subscriber_symbol| symbol_table.get_subscriber_symbol(subscriber_symbol).unwrap())
            .collect()
    }

    /// Returns the [RpcServerSymbol]s defined within this service bundle.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the server IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<RpcServerSymbol>` containing the RPC servers defined in this service bundle.
    pub fn servers<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a RpcServerSymbol> {
        self.servers
            .iter()
            .map(|server_symbol| symbol_table.get_rpc_server_symbol(server_symbol).unwrap())
            .collect()
    }

    /// Returns the [RpcClientSymbol]s defined within this service bundle.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the client IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<RpcClientSymbol>` containing the RPC clients defined in this service bundle.
    pub fn clients<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a RpcClientSymbol> {
        self.clients
            .iter()
            .map(|client_symbol| symbol_table.get_rpc_client_symbol(client_symbol).unwrap())
            .collect()
    }

    /// Returns the diagnostics [Event]s defined within this service bundle's AST node.
    ///
    /// This method accesses the `diagnostics_declaration` field of the underlying
    /// [ServiceBundle] AST node.
    ///
    /// # Returns
    ///
    /// A `Vec<Event>` containing the diagnostics events defined in the service bundle,
    /// or an empty vector if no diagnostics declaration exists or no events are defined.
    pub fn diagnostics_events(&self) -> Vec<Event> {
        let mut result = Vec::new();
        if let MessageField(Some(diagnostics_declaration)) = &self.ast_node.diagnostics_declaration
        {
            result = diagnostics_declaration.event.clone()
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use crate::service_bundle_symbol::ServiceBundleSymbol;
    use crate::symbol_table_creator::SymbolTableCreator;
    use crate::symbol_table_linker::SymbolTableLinker;
    use crate::SymbolTable;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;

    fn setup_test(fqn: &str) -> (SymbolTable, ServiceBundleSymbol) {
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
            linker.symbol_table().resolve_service_bundle_fqn(fqn)[0].clone(),
        )
    }

    #[test]
    fn test_publishers() {
        // GIVEN a valid symbol table
        let (symbol_table, bundle_symbol) = setup_test("sdv.test.ServiceBundleOne");

        // WHEN getting the publishers
        let publishers = bundle_symbol.publishers(&symbol_table);

        // THEN the publishers should be found
        assert_eq!(publishers.len(), 2);
        assert_eq!(publishers[0].full_name(&symbol_table), "sdv.test.Pressure");
        assert_eq!(publishers[0].channel, vec!["ONE"]);
        assert_eq!(publishers[1].full_name(&symbol_table), "sdv.test.Pressure");
        assert_eq!(publishers[1].channel, vec!["TWO"]);
    }

    #[test]
    fn test_subscribers() {
        // GIVEN a valid symbol table
        let (symbol_table, bundle_symbol) = setup_test("sdv.test.ServiceBundleTwo");

        // WHEN getting the subscriber
        let subscriber = bundle_symbol.subscribers(&symbol_table);

        // THEN the subscriber should be found
        assert_eq!(subscriber.len(), 1);
        assert_eq!(subscriber[0].full_name(&symbol_table), "sdv.test.Pressure");
        assert_eq!(subscriber[0].channel, vec!["TWO"]);
    }

    #[test]
    fn test_servers() {
        // GIVEN a valid symbol table
        let (symbol_table, bundle_symbol) = setup_test("sdv.test.ServiceBundleTwo");

        // WHEN getting the servers
        let servers = bundle_symbol.servers(&symbol_table);

        // THEN the servers should be found
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].full_name(&symbol_table), "sdv.test.DummyRpc");
        assert_eq!(servers[1].full_name(&symbol_table), "sdv.test.AnotherDummyRpc");
    }

    #[test]
    fn test_clients() {
        // GIVEN a valid symbol table
        let (symbol_table, bundle_symbol) = setup_test("sdv.test.ServiceBundleOne");

        // WHEN getting the subscriber
        let clients = bundle_symbol.clients(&symbol_table);

        // THEN the subscriber should be found
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].full_name(&symbol_table), "sdv.test.DummyRpc");
        assert_eq!(clients[1].full_name(&symbol_table), "sdv.test.AnotherDummyRpc");
    }

    #[test]
    fn test_diagnostics_events() {
        // GIVEN a valid symbol table
        let (_, bundle_symbol) = setup_test("sdv.test.ServiceBundleOne");

        // WHEN getting the subscriber
        let diagnostics_events = bundle_symbol.diagnostics_events();

        // THEN the subscriber should be found
        assert_eq!(diagnostics_events.len(), 1);
        assert_eq!(diagnostics_events[0].id, "event1");
        assert_eq!(
            diagnostics_events[0].extended_data_message_name,
            "sdv.test.diagnostics.FirstEventExt"
        );
        assert_eq!(diagnostics_events[0].fault_listener_mask, 0x3);
    }
}
