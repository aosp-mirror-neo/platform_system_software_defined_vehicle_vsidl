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

use super::channel_symbol::ChannelSymbol;
use super::publisher_symbol::PublisherSymbol;
use super::subscriber_symbol::SubscriberSymbol;
use crate::{
    implement_ast_node_deref, implement_symbol_deref, SymbolCommon, SymbolId, SymbolTable,
};
use protobuf::descriptor::DescriptorProto;
use vsidl_v1::syntax::Span;
use vsidl_v1_common::message_size::MessageSize;
use vsidl_v1_common::ProtoMetadata;

/// Represents a Protobuf Message definition within the symbol table.
///
/// This struct holds information about a specific message defined in a `.proto` file,
/// including its AST node, common symbol properties, and metadata about the
/// originating `.proto` file. It allows accessing the underlying [DescriptorProto]
/// AST node via `Deref`.
#[derive(Default, Debug, Clone)]
pub struct ProtoMessageSymbol {
    /// The original Protobuf AST node ([DescriptorProto]) this symbol was created from.
    pub ast_node: DescriptorProto,
    /// Common information shared across all symbol types (like ID, name, scope).
    pub symbol: SymbolCommon,
    // Doc string in front of the definition of the symbol in the Protobuf file
    pub doc_string: String,
    /// Metadata related to the `.proto` file where this message is defined.
    pub proto_metadata: ProtoMetadata,
    /// Metadata for the `.proto` files that the defining `.proto` file depends on.
    pub proto_dependencies: Vec<ProtoMetadata>,
    /// The calculated maximum size of the Protobuf message associated with this publication.
    /// Optional, because it can be not be calculated for messages with variable size fields that
    /// do not specify a max size. All messages sent over channels have to have a defined max size.
    pub max_size: Option<MessageSize>,
    /// Symbol IDs of the [ChannelSymbol]s using this message. Requires the symbol table to be linked.
    pub(crate) channels: Vec<SymbolId>,
}
// Implements Deref for the ast_node field, allowing direct access to the DescriptorProto fields.
implement_ast_node_deref!(ProtoMessageSymbol, DescriptorProto);

impl ProtoMessageSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();

    /// Returns the [ChannelSymbol]s that use this message.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the channel IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<ChannelSymbol>` containing the channels using this message.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn channels<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a ChannelSymbol> {
        symbol_table.panic_if_not_linked();
        let mut channels: Vec<_> = self
            .channels
            .iter()
            .map(|channel_id| symbol_table.get_channel_symbol(channel_id).unwrap())
            .collect();

        // Sorts the channels by name; necessary for a stable order in generated files
        channels.sort_by_key(|channel| channel.name());

        channels
    }

    /// Returns the [PublisherSymbol]s that publish messages of this type.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the publisher IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<&PublisherSymbol>` containing the publishers using this message.
    pub fn publishers<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a PublisherSymbol> {
        symbol_table.panic_if_not_linked();
        self.channels(symbol_table)
            .iter()
            .flat_map(|channel| channel.publishers(symbol_table))
            .collect()
    }

    /// Returns the [SubscriberSymbol]s that subscribe to messages of this type.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the subscriber IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<&SubscriberSymbol>` containing the subscribers using this message.
    pub fn subscribers<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a SubscriberSymbol> {
        symbol_table.panic_if_not_linked();
        self.channels(symbol_table)
            .iter()
            .flat_map(|channel| channel.subscribers(symbol_table))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::proto_message_symbol::ProtoMessageSymbol;
    use crate::symbol_table_creator::SymbolTableCreator;
    use crate::symbol_table_linker::SymbolTableLinker;
    use crate::SymbolTable;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;

    fn setup_test() -> (SymbolTable, ProtoMessageSymbol) {
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
        let message_symbol = symbol_table
            .resolve_proto_message_fqn("sdv.test.Pressure")
            .into_iter()
            .next()
            .unwrap()
            .clone();
        (symbol_table, message_symbol)
    }

    #[test]
    fn test_channels() {
        // GIVEN a valid symbol table
        let (symbol_table, message_symbol) = setup_test();

        // WHEN getting the channels
        let channels = message_symbol.channels(&symbol_table);

        // THEN the channels should be found
        assert_eq!(channels.len(), 2);
        let channel_names: Vec<String> = channels.iter().map(|c| c.name()).collect();
        assert!(channel_names.contains(&"ONE".to_string()));
        assert!(channel_names.contains(&"TWO".to_string()));
    }

    #[test]
    fn test_publishers() {
        // GIVEN a valid symbol table
        let (symbol_table, message_symbol) = setup_test();

        // WHEN getting the publishers
        let publishers = message_symbol.publishers(&symbol_table);

        // THEN the publishers should be found (2 publishers in ServiceBundleOne)
        assert_eq!(publishers.len(), 2);
        for publisher in publishers {
            assert_eq!(
                publisher.owning_service_bundle(&symbol_table).full_name(&symbol_table),
                "sdv.test.ServiceBundleOne"
            );
        }
    }

    #[test]
    fn test_subscribers() {
        // GIVEN a valid symbol table
        let (symbol_table, message_symbol) = setup_test();

        // WHEN getting the subscribers
        let subscribers = message_symbol.subscribers(&symbol_table);

        // THEN the subscribers should be found (1 subscriber in ServiceBundleTwo)
        assert_eq!(subscribers.len(), 1);
        assert_eq!(
            subscribers[0].owning_service_bundle(&symbol_table).full_name(&symbol_table),
            "sdv.test.ServiceBundleTwo"
        );
    }
}
