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
use vsidl_v1::syntax::service_bundle::Publisher;
use vsidl_v1::syntax::Span;

use super::proto_message_symbol::ProtoMessageSymbol;
use super::publisher_symbol::PublisherSymbol;
use super::subscriber_symbol::SubscriberSymbol;

/// Symbol representing a VSIDL Channel.
///
/// Channels decouple publishers and subscribers by providing a named communication
/// path associated with a specific Protobuf message type.
#[derive(Default, Debug, Clone)]
pub struct ChannelSymbol {
    /// The original VSIDL AST node ([Publisher]) this symbol was created from.
    /// Channels are created inline by publishers, i.e., each channel has one publisher defining it.
    pub ast_node: Publisher,
    /// Common information shared across all symbol types (like ID, name, scope).
    pub symbol: SymbolCommon,
    /// Symbol ID of the [ProtoMessageSymbol] sent over this channel. Requires the symbol table to be linked.
    pub(crate) message: Option<SymbolId>,
    /// Symbol IDs of the [PublisherSymbol]s that publish to this channel. Requires the symbol table to be linked.
    pub(crate) publishers: Vec<SymbolId>,
    /// Symbol IDs of the [SubscriberSymbol]s that subscribe to this channel. Requires the symbol table to be linked.
    pub(crate) subscribers: Vec<SymbolId>,
}
// Implements Deref for the ast_node field, allowing direct access to the Publisher fields.
implement_ast_node_deref!(ChannelSymbol, Publisher);

impl ChannelSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();

    /// Returns the [ProtoMessageSymbol] associated with this channel.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the message ID.
    ///
    /// # Returns
    ///
    /// The [ProtoMessageSymbol] that this channel carries.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    /// * If the symbol is not a [ProtoMessageSymbol].
    pub fn message<'a>(&self, symbol_table: &'a SymbolTable) -> &'a ProtoMessageSymbol {
        symbol_table.panic_if_not_linked();
        symbol_table.get_proto_message_symbol(&self.message.clone().unwrap()).unwrap()
    }

    /// Returns the [PublisherSymbol]s associated with this channel.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the publisher IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<&PublisherSymbol>` containing the publishers for this channel.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn publishers<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a PublisherSymbol> {
        symbol_table.panic_if_not_linked();
        self.publishers
            .iter()
            .map(|publisher| symbol_table.get_publisher_symbol(publisher).unwrap())
            .collect()
    }

    /// Returns the [SubscriberSymbol]s associated with this channel.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the subscriber IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<&SubscriberSymbol>` containing the subscribers for this channel.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn subscribers<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a SubscriberSymbol> {
        symbol_table.panic_if_not_linked();
        self.subscribers
            .iter()
            .map(|subscriber| symbol_table.get_subscriber_symbol(subscriber).unwrap())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::channel_symbol::ChannelSymbol;
    use crate::symbol_table_creator::SymbolTableCreator;
    use crate::symbol_table_linker::SymbolTableLinker;
    use crate::SymbolTable;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;

    fn setup_test() -> (SymbolTable, ChannelSymbol) {
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
        let symbol_table = linker.symbol_table().clone();
        let channel_symbol =
            symbol_table.resolve_channel_fqn("TWO").into_iter().next().unwrap().clone();
        (symbol_table, channel_symbol)
    }

    #[test]
    fn test_message() {
        // GIVEN a valid symbol table
        let (symbol_table, channel_symbol) = setup_test();

        // WHEN getting the message
        let message = channel_symbol.message(&symbol_table);

        // THEN the message should be found
        assert_eq!(message.full_name(&symbol_table), "sdv.test.Pressure");
    }

    #[test]
    fn test_publishers() {
        // GIVEN a valid symbol table
        let (symbol_table, channel_symbol) = setup_test();

        // WHEN getting the publishers
        let publishers = channel_symbol.publishers(&symbol_table);

        // THEN the publishers should be found
        assert_eq!(publishers.len(), 1);
        let owner = publishers[0].owning_service_bundle(&symbol_table).full_name(&symbol_table);
        assert_eq!(owner, "sdv.test.ServiceBundleOne");
    }

    #[test]
    fn test_subscribers() {
        // GIVEN a valid symbol table
        let (symbol_table, channel_symbol) = setup_test();

        // WHEN getting the subscribers
        let subscribers = channel_symbol.subscribers(&symbol_table);

        // THEN the subscribers should be found
        assert_eq!(subscribers.len(), 1);
        let owner = subscribers[0].owning_service_bundle(&symbol_table).full_name(&symbol_table);
        assert_eq!(owner, "sdv.test.ServiceBundleTwo");
    }

    #[test]
    #[should_panic(expected = "Symbol table is used without being linked.")]
    fn test_message_not_linked() {
        // GIVEN an unlinked symbol table
        let symbol_table = SymbolTable::default();
        let channel_symbol = ChannelSymbol::default();

        // WHEN calling message
        channel_symbol.message(&symbol_table);
    }
}
