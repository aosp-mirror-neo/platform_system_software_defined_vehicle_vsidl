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
use vsidl_v1::syntax::service_bundle::Subscriber;
use vsidl_v1::syntax::Span;
use vsidl_v1_common::constants::UNIQUE_CHANNEL;

use super::{
    proto_message_symbol::ProtoMessageSymbol, proto_publication_symbol::ProtoPublicationSymbol,
    publisher_symbol::PublisherSymbol, service_bundle_symbol::ServiceBundleSymbol,
    topic_symbol::TopicSymbol,
};

/// Symbol representing a VSIDL Subscriber definition within a service bundle.
///
/// This struct holds information about a specific subscriber declared in a `.vsidl` file,
/// including its AST node, common symbol properties, the publication it subscribes to,
/// and the service bundle it belongs to. It allows accessing the underlying
/// [Subscriber] AST node via `Deref`.
#[derive(Default, Debug, Clone)]
pub struct SubscriberSymbol {
    /// The original VSIDL AST node ([Subscriber]) this symbol was created from.
    pub ast_node: Subscriber,
    /// Common information shared across all symbol types (like ID, name, scope).
    pub symbol: SymbolCommon,
    /// Symbol ID of the [ProtoMessageSymbol] this subscriber subscribes to. Requires the symbol table to be linked.
    pub(crate) message: Option<SymbolId>,
    /// (PDK29 only) Symbol ID of the [ProtoPublicationSymbol] this subscriber subscribes to. Requires the symbol table to be linked.
    pub(crate) publication: Option<SymbolId>,
    /// Symbol IDs of the [TopicSymbol]s this subscriber subscribes to. Requires the symbol table to be linked.
    pub(crate) topics: Vec<SymbolId>,
    /// Symbol ID of the [ServiceBundleSymbol] that owns this subscriber. Requires the symbol table to be linked.
    pub(crate) owning_service_bundle: Option<SymbolId>,
}
// Implements Deref for the ast_node field, allowing direct access to the Subscriber fields.
implement_ast_node_deref!(SubscriberSymbol, Subscriber);

impl SubscriberSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();

    /// Returns the [ProtoMessageSymbol] this subscriber subscribes to.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the message ID.
    ///
    /// # Returns
    ///
    /// The [ProtoMessageSymbol] that this subscriber subscribes to.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn message<'a>(&self, symbol_table: &'a SymbolTable) -> &'a ProtoMessageSymbol {
        symbol_table.panic_if_not_linked();
        symbol_table.get_proto_message_symbol(&self.message.clone().unwrap()).unwrap()
    }

    /// Returns the [ProtoPublicationSymbol] this subscriber subscribes to.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the publication ID.
    ///
    /// # Returns
    ///
    /// The [ProtoPublicationSymbol] that this subscriber subscribes to.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn publication<'a>(&self, symbol_table: &'a SymbolTable) -> &'a ProtoPublicationSymbol {
        symbol_table.panic_if_not_linked();
        symbol_table.get_proto_publication_symbol(&self.publication.clone().unwrap()).unwrap()
    }

    /// Returns a publisher that publishes the given channel of the publication
    /// this subscriber subscribes to.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table containing all symbols.
    /// * `channel_name` - The name of the channel to find the publisher for.
    ///
    /// # Returns
    ///
    /// The [PublisherSymbol] that publishes the specified channel.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn publisher_for_channel<'a>(
        &self,
        symbol_table: &'a SymbolTable,
        channel_name: &str,
    ) -> &'a PublisherSymbol {
        symbol_table.panic_if_not_linked();
        assert!(symbol_table.uses_pdk29());

        if !self.channels(symbol_table).contains(&channel_name.to_string()) {
            panic!("publisher_for_channel: Channel '{channel_name}' not found for subscriber '{}'. Available channels: {:?}",
                self.full_name(symbol_table),
                self.channels(symbol_table)
            );
        }

        let publication = self.publication(symbol_table);
        let mut publishers = publication.publishers(symbol_table);
        // TODO: b/477164517 - Stable order of publishers. Remove the following line as soon as the generator only prints 'deprecated'.
        publishers.sort_by_key(|publisher| {
            publisher.owning_service_bundle(symbol_table).full_name(symbol_table)
        });
        publishers
            .iter()
            .find(|publisher| {
                let channels = publisher.channels(symbol_table);
                channels.iter().any(|v| *v == channel_name)
            })
            .unwrap()
    }

    /// Returns the owning [ServiceBundleSymbol] for this subscriber.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the service bundle ID.
    ///
    /// # Returns
    ///
    /// The [ServiceBundleSymbol] that owns this subscriber.
    pub fn owning_service_bundle<'a>(
        &self,
        symbol_table: &'a SymbolTable,
    ) -> &'a ServiceBundleSymbol {
        symbol_table
            .get_service_bundle_symbol(&self.owning_service_bundle.clone().unwrap())
            .unwrap()
    }

    /// Returns the list of channel names this subscriber subscribes to.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table required for resolving the channels.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` containing the names of the channels subscribed to.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    /// * If the publication or its channels enum cannot be resolved when needed.
    pub fn channels(&self, symbol_table: &SymbolTable) -> Vec<String> {
        symbol_table.panic_if_not_linked();
        assert!(symbol_table.uses_pdk29());

        // Case 1: Subscriber defines explicit channels
        let mut channels = self.channel.clone();
        // Case 2: Subscriber defines no channels, but publication has channels => use all
        if channels.is_empty() {
            channels = self.publication(symbol_table).channels(symbol_table).clone();
        }
        // Case 3: Subscriber defines no channels and publication has no channels => use UNIQUE
        if channels.is_empty() {
            channels.push(UNIQUE_CHANNEL.to_string());
        }
        channels
    }

    /// Returns the list of topic this subscriber subscribes to.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table required for resolving the topics.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` containing the topics subscribed to.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    /// * If the publication or its channels enum cannot be resolved when needed.
    pub fn topics(&self, symbol_table: &SymbolTable) -> Vec<String> {
        symbol_table.panic_if_not_linked();
        assert!(!symbol_table.uses_pdk29());

        self.topic_symbols(symbol_table).into_iter().map(|topic| topic.name()).collect()
    }

    /// Returns the [TopicSymbol]s associated with this subscriber.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the topic IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<&TopicSymbol>` containing the topics for this subscriber.
    pub fn topic_symbols<'a>(&self, symbol_table: &'a SymbolTable) -> Vec<&'a TopicSymbol> {
        symbol_table.panic_if_not_linked();
        assert!(!symbol_table.uses_pdk29());

        let mut topic_symbols: Vec<_> = self
            .topics
            .iter()
            .map(|topic_id| symbol_table.get_topic_symbol(topic_id).unwrap())
            .collect();

        // Sorts the topics by name; necessary for a stable order in generated files
        topic_symbols.sort_by_key(|topic| topic.name());

        topic_symbols
    }
}

#[cfg(test)]
mod tests {
    use crate::proto_publication_symbol::ProtoPublicationSymbol;
    use crate::subscriber_symbol::SubscriberSymbol;
    use crate::symbol_table_creator::SymbolTableCreator;
    use crate::symbol_table_linker::SymbolTableLinker;
    use crate::SymbolKind;
    use crate::SymbolTable;
    use vsidl_v1_common::constants::UNIQUE_CHANNEL;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;

    fn setup_test_pdk29() -> (SymbolTable, SubscriberSymbol) {
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
            linker
                .symbol_table()
                .resolve_subscriber_fqn("sdv.test.Pressure")
                .into_iter()
                .find(|symbol| symbol.channel[0] == "TWO")
                .unwrap()
                .clone(),
        )
    }

    fn setup_test() -> (SymbolTable, SubscriberSymbol) {
        // Given: A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());
        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(crate::TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();
        assert!(!linker.symbol_table().uses_pdk29());
        (
            linker.symbol_table().clone(),
            linker
                .symbol_table()
                .resolve_subscriber_fqn("sdv.test.Pressure")
                .into_iter()
                .find(|symbol| symbol.topic[0] == "two")
                .unwrap()
                .clone(),
        )
    }

    #[test]
    fn test_message() {
        // GIVEN a valid symbol table
        let (symbol_table, subscriber_symbol) = setup_test_pdk29();

        // WHEN getting the message
        let message = subscriber_symbol.message(&symbol_table);

        // THEN the message should be found
        assert_eq!(message.full_name(&symbol_table), "sdv.test.Pressure");
    }

    #[test]
    fn test_publication() {
        // GIVEN a valid symbol table
        let (symbol_table, subscriber_symbol) = setup_test_pdk29();

        // WHEN getting the publication
        let publication = subscriber_symbol.publication(&symbol_table);

        // THEN the publication should be found
        assert_eq!(publication.full_name(&symbol_table), "sdv.test.Pressure");
    }

    #[test]
    fn test_publisher_for_channel() {
        // GIVEN a valid symbol table
        let (symbol_table, subscriber_symbol) = setup_test_pdk29();

        // WHEN getting the publisher
        let publisher = subscriber_symbol.publisher_for_channel(&symbol_table, "TWO");

        // THEN the publisher should be found
        assert!(publisher.channels(&symbol_table).contains(&"TWO".to_string()));
    }

    #[test]
    #[should_panic]
    fn test_publisher_for_channel_nonexisting_channel() {
        // GIVEN a valid symbol table
        let (symbol_table, subscriber_symbol) = setup_test_pdk29();

        // WHEN getting the publisher
        let _publisher = subscriber_symbol.publisher_for_channel(&symbol_table, "UNIQUE");

        // THEN the call should panic, ensured by #[should_panic]
    }

    #[test]
    fn test_owning_service_bundle() {
        // GIVEN a valid symbol table
        let (symbol_table, subscriber_symbol) = setup_test_pdk29();

        // WHEN getting the service bundle
        let bundle = subscriber_symbol.owning_service_bundle(&symbol_table);

        // THEN the service bundle should be found
        assert_eq!(bundle.full_name(&symbol_table), "sdv.test.ServiceBundleTwo");
    }

    #[test]
    fn test_channels_explicit() {
        // GIVEN a valid symbol table
        let (symbol_table, subscriber_symbol) = setup_test_pdk29();

        // WHEN requesting channels
        let result = subscriber_symbol.channels(&symbol_table);

        // THEN channels should be ONE
        assert_eq!(result, vec!["TWO"]);
    }

    #[test]
    fn test_topics() {
        // GIVEN a non-PDK29 symbol table
        let (symbol_table, subscriber_symbol) = setup_test();

        // WHEN requesting channels
        let result = subscriber_symbol.topics(&symbol_table);

        // THEN channels should be TWO
        assert_eq!(result, vec!["two"]);
    }

    #[test]
    fn test_channels_implicit() {
        // Given: A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SUBSCRIBER_WITHOUT_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();
        let symbol_table = linker.symbol_table().clone();
        let subscriber_symbol =
            linker.symbol_table().resolve_subscriber_fqn("sdv.test.Pressure")[0].clone();

        // WHEN requesting channels
        let result = subscriber_symbol.channels(&symbol_table);

        // THEN channels should be ONE and TWO (i.e. all channels)
        assert_eq!(result, vec!["ONE", "TWO"]);
    }

    #[test]
    fn test_channels_unique() {
        // Given: A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SUBSCRIBER_WITH_IMPLICIT_UNIQUE_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();
        let symbol_table = linker.symbol_table().clone();
        let subscriber_symbol =
            linker.symbol_table().resolve_subscriber_fqn("sdv.test.FuelLevel")[0].clone();

        // WHEN requesting channels
        let result = subscriber_symbol.channels(&symbol_table);

        // THEN channels should be UNIQUE
        assert_eq!(result, vec![UNIQUE_CHANNEL.to_string()]);
    }

    #[test]
    fn test_channels_empty() {
        // GIVEN a placeholder symbol table that pretends to be linked
        let mut symbol_table = SymbolTable::default();
        let subscriber_id = symbol_table.create_symbol(
            "placeholder".to_string(),
            None,
            SymbolKind::Subscriber(SubscriberSymbol::default()),
        );
        let publication_id = symbol_table.create_symbol(
            "placeholder".to_string(),
            None,
            SymbolKind::ProtoPublication(ProtoPublicationSymbol::default()),
        );

        // Link the two fake symbols
        if let SymbolKind::Subscriber(symbol) =
            &mut symbol_table.get_symbol_mut(&subscriber_id.clone()).unwrap()
        {
            symbol.publication = Some(publication_id.clone());
        }

        // Pretend the symbol table is linked
        symbol_table.set_state(crate::SymbolTableState::Linked);
        let subscriber = symbol_table.get_subscriber_symbol(&subscriber_id).unwrap();

        // WHEN requesting channels
        let result = subscriber.channels(&symbol_table);

        // THEN channels should be UNIQUE
        assert_eq!(result, vec![UNIQUE_CHANNEL.to_string()]);
    }
}
