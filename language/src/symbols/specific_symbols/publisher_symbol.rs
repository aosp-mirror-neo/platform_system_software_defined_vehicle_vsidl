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
    implement_ast_node_deref, implement_symbol_deref, proto_message_symbol::ProtoMessageSymbol,
    SymbolCommon, SymbolId, SymbolTable,
};
use heck::ToKebabCase;
use vsidl_v1::syntax::service_bundle::Publisher;
use vsidl_v1::syntax::Span;
use vsidl_v1_common::{constants::UNIQUE_CHANNEL, format_service_unit_name};

use super::{
    proto_publication_symbol::ProtoPublicationSymbol, service_bundle_symbol::ServiceBundleSymbol,
    topic_symbol::TopicSymbol,
};

/// Symbol representing a VSIDL Publisher definition within a service bundle.
///
/// This struct holds information about a specific publisher declared in a `.vsidl` file,
/// including its AST node, common symbol properties, the publication it publishes,
/// and the service bundle it belongs to. It allows accessing the underlying
/// [Publisher] AST node via `Deref`.
#[derive(Default, Debug, Clone)]
pub struct PublisherSymbol {
    /// The original VSIDL AST node ([Publisher]) this symbol was created from.
    pub ast_node: Publisher,
    /// Common information shared across all symbol types (like ID, name, scope).
    pub symbol: SymbolCommon,
    /// The number of messages the channel queue can hold.
    pub capacity: u32,
    /// Symbol ID of the [ProtoMessageSymbol] published by this publisher. Requires the symbol table to be linked.
    pub(crate) message: Option<SymbolId>,
    /// (PDK29 only) Symbol ID of the [ProtoPublicationSymbol] published by this publisher. Requires the symbol table to be linked.
    pub(crate) publication: Option<SymbolId>,
    /// Symbol IDs of the [TopicSymbol]s this publisher publishes to. Requires the symbol table to be linked.
    pub(crate) topics: Vec<SymbolId>,
    /// Symbol ID of the [ServiceBundleSymbol] that owns this publisher. Requires the symbol table to be linked.
    pub(crate) owning_service_bundle: Option<SymbolId>,
}
// Implements Deref for the ast_node field, allowing direct access to the Publisher fields.
implement_ast_node_deref!(PublisherSymbol, Publisher);

impl PublisherSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();

    /// Returns the [ProtoMessageSymbol] associated with this publisher.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the message ID.
    ///
    /// # Returns
    ///
    /// The [ProtoMessageSymbol] that this publisher publishes.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn message<'a>(&self, symbol_table: &'a SymbolTable) -> &'a ProtoMessageSymbol {
        symbol_table.panic_if_not_linked();
        symbol_table.get_proto_message_symbol(&self.message.clone().unwrap()).unwrap()
    }

    /// Returns the [ProtoPublicationSymbol] associated with this publisher.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the publication ID.
    ///
    /// # Returns
    ///
    /// The [ProtoPublicationSymbol] that this publisher publishes.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn publication<'a>(&self, symbol_table: &'a SymbolTable) -> &'a ProtoPublicationSymbol {
        symbol_table.panic_if_not_linked();
        symbol_table.get_proto_publication_symbol(&self.publication.clone().unwrap()).unwrap()
    }

    /// Returns the owning [ServiceBundleSymbol] for this publisher.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the service bundle ID.
    ///
    /// # Returns
    ///
    /// The [ServiceBundleSymbol] that owns this publisher.
    pub fn owning_service_bundle<'a>(
        &self,
        symbol_table: &'a SymbolTable,
    ) -> &'a ServiceBundleSymbol {
        symbol_table
            .get_service_bundle_symbol(&self.owning_service_bundle.clone().unwrap())
            .unwrap()
    }

    /// Checks if the associated publication follows the SINGLE_PUB model.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the publication.
    ///
    /// # Returns
    ///
    /// `true` if the publication model is [SINGLE_PUB], `false` otherwise.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn is_single_pub(&self, symbol_table: &SymbolTable) -> bool {
        symbol_table.panic_if_not_linked();
        self.publication(symbol_table).is_single_pub()
    }

    /// Checks if the publisher has a user-defined service unit name specified in the VSIDL.
    ///
    /// # Returns
    ///
    /// `true` if `service_unit_name` is defined in the AST node, `false` otherwise.
    pub fn has_user_defined_name(&self) -> bool {
        !self.service_unit_name.is_empty()
    }

    /// Returns the service unit name for this publisher and a specific channel / topic it publishes.
    ///
    /// If the publisher has a user-defined `service_unit_name` in the VSIDL, that name is returned.
    /// Otherwise, a service unit name is generated based on the publication's fully qualified name
    /// and the provided `channel_or_topic`. The generated name follows the kebab-case convention.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table required for resolving names and channels.
    /// * `channel_or_topic` - The specific channel or topic for which the service unit name is requested.
    ///
    /// # Returns
    ///
    /// The service unit name as a `String`.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    /// * If the provided `channel_name` is not one of the channels published by this publisher (see [channels]).
    pub fn service_unit_name(&self, symbol_table: &SymbolTable, channel_or_topic: &str) -> String {
        // Verify that requested channel exists
        symbol_table.panic_if_not_linked();
        if symbol_table.uses_pdk29()
            && !self.channels(symbol_table).contains(&channel_or_topic.to_string())
        {
            panic!("service_unit_name: Channel '{channel_or_topic}' not found for publisher '{}'. Available channels: {:?}",
                self.full_name(symbol_table),
                self.channels(symbol_table)
            );
        } else if !symbol_table.uses_pdk29()
            && !self.topics(symbol_table).contains(&channel_or_topic.to_string())
        {
            panic!("service_unit_name: Topic '{channel_or_topic}' not found for publisher '{}'. Available topics: {:?}",
                self.full_name(symbol_table),
                self.topics(symbol_table)
            );
        }

        // Return service unit name
        if self.has_user_defined_name() {
            self.service_unit_name.clone()
        } else if symbol_table.uses_pdk29() {
            let publication_fqn = self.publication(symbol_table).full_name(symbol_table);
            format_service_unit_name(publication_fqn, channel_or_topic.to_string())
        } else {
            let message_fqn = self.message(symbol_table).full_name(symbol_table);
            format_service_unit_name(message_fqn, channel_or_topic.to_string().to_kebab_case())
        }
    }

    /// Checks if the associated publication follows the MULTI_PUB model.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the publication.
    ///
    /// # Returns
    ///
    /// `true` if the publication model is [MULTI_PUB], `false` otherwise.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn is_multi_pub(&self, symbol_table: &SymbolTable) -> bool {
        symbol_table.panic_if_not_linked();
        self.publication(symbol_table).is_multi_pub()
    }

    /// Returns the list of channel names published by this publisher.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table required for resolving the channels.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` containing the names of the channels published.
    pub fn channels(&self, symbol_table: &SymbolTable) -> Vec<String> {
        symbol_table.panic_if_not_linked();
        assert!(symbol_table.uses_pdk29());

        // Case 1: Publisher defines explicit channels
        let mut channels = self.channel.clone();
        // Case 2: Publisher defines no channels, but publication has channels => use all
        if channels.is_empty() {
            channels = self.publication(symbol_table).channels(symbol_table).clone();
        }
        // Case 3: Publisher defines no channels and publication has no channels => use UNIQUE
        if channels.is_empty() {
            channels.push(UNIQUE_CHANNEL.to_string());
        }
        channels
    }

    /// Returns the list of topic names published by this publisher.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table required for resolving the topics.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` containing the names of the topics published.
    pub fn topics(&self, symbol_table: &SymbolTable) -> Vec<String> {
        symbol_table.panic_if_not_linked();
        assert!(!symbol_table.uses_pdk29());

        self.topic_symbols(symbol_table).into_iter().map(|topic| topic.name()).collect()
    }

    /// Returns the [TopicSymbol]s associated with this publisher.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the topic IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<&TopicSymbol>` containing the topics for this publisher.
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
    use crate::publisher_symbol::PublisherSymbol;
    use crate::symbol_table_creator::SymbolTableCreator;
    use crate::symbol_table_linker::SymbolTableLinker;
    use crate::SymbolKind;
    use crate::SymbolTable;
    use vsidl_v1_common::constants::UNIQUE_CHANNEL;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;

    fn setup_test_pdk29() -> (SymbolTable, PublisherSymbol) {
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
                .resolve_publisher_fqn("sdv.test.Pressure")
                .into_iter()
                .find(|symbol| symbol.channel[0] == "ONE")
                .unwrap()
                .clone(),
        )
    }

    fn setup_test() -> (SymbolTable, PublisherSymbol) {
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
        (
            linker.symbol_table().clone(),
            linker
                .symbol_table()
                .resolve_publisher_fqn("sdv.test.Pressure")
                .into_iter()
                .find(|symbol| symbol.topic[0] == "one")
                .unwrap()
                .clone(),
        )
    }

    #[test]
    fn test_message() {
        // GIVEN a valid symbol table
        let (symbol_table, publisher_symbol) = setup_test_pdk29();

        // WHEN getting the message
        let message = publisher_symbol.message(&symbol_table);

        // THEN the message should be found
        assert_eq!(message.full_name(&symbol_table), "sdv.test.Pressure");
    }

    #[test]
    fn test_publication() {
        // GIVEN a valid symbol table
        let (symbol_table, publisher_symbol) = setup_test_pdk29();

        // WHEN getting the publication
        let publication = publisher_symbol.publication(&symbol_table);

        // THEN the publication should be found
        assert_eq!(publication.full_name(&symbol_table), "sdv.test.Pressure");
    }

    #[test]
    fn test_owning_service_bundle() {
        // GIVEN a valid symbol table
        let (symbol_table, publisher_symbol) = setup_test_pdk29();

        // WHEN getting the service bundle
        let bundle = publisher_symbol.owning_service_bundle(&symbol_table);

        // THEN the service bundle should be found
        assert_eq!(bundle.full_name(&symbol_table), "sdv.test.ServiceBundleOne");
    }

    #[test]
    fn test_is_single_pub() {
        // GIVEN a valid symbol table
        let (symbol_table, publisher_symbol) = setup_test_pdk29();

        // WHEN requesting is_single_pub
        let result = publisher_symbol.is_single_pub(&symbol_table);

        // THEN is_single_pub should be true
        assert!(result);
    }

    #[test]
    fn test_is_multi_pub() {
        // GIVEN a valid symbol table
        let (symbol_table, publisher_symbol) = setup_test_pdk29();

        // WHEN requesting is_multi_pub
        let result = publisher_symbol.is_multi_pub(&symbol_table);

        // THEN is_multi_pub should be false
        assert!(!result);
    }

    #[test]
    fn test_service_unit_name_generated_name() {
        // GIVEN A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_USER_DEFINED_NAME_MULTI_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // GIVEN A symbol with a user-defined name
        let symbol_table = linker.symbol_table().clone();
        let publisher_symbol = symbol_table.resolve_publisher_fqn("sdv.test.BatteryLevel")[0];

        // WHEN requesting service unit name
        let result = publisher_symbol.service_unit_name(&symbol_table, "MAIN");

        // THEN it should be the user-defined name
        assert_eq!(result, "my-battery");
    }

    #[test]
    fn test_service_unit_name_user_defined_name() {
        // GIVEN a valid symbol table
        let (symbol_table, publisher_symbol) = setup_test_pdk29();

        // WHEN requesting service unit name
        let result = publisher_symbol.service_unit_name(&symbol_table, "ONE");

        // THEN it should be the generated name
        assert_eq!(result, "sdv-test-pressure-one");
    }

    #[test]
    #[should_panic]
    fn test_service_unit_name_non_existing_channel() {
        // GIVEN a valid symbol table
        let (symbol_table, publisher_symbol) = setup_test_pdk29();

        // WHEN requesting service unit name
        let _result = publisher_symbol.service_unit_name(&symbol_table, "UNIQUE");

        // THEN should panic, ensured by #[should_panic] flag
    }

    #[test]
    fn test_channels_explicit() {
        // GIVEN a valid symbol table
        let (symbol_table, publisher_symbol) = setup_test_pdk29();

        // WHEN requesting channels
        let result = publisher_symbol.channels(&symbol_table);

        // THEN channels should be ONE
        assert_eq!(result, vec!["ONE"]);
    }

    #[test]
    fn test_topics_explicit() {
        // GIVEN a non-PDK29 symbol table
        let (symbol_table, publisher_symbol) = setup_test();

        // WHEN requesting channels
        let result = publisher_symbol.topics(&symbol_table);

        // THEN channels should be ONE
        assert_eq!(result, vec!["one"]);
    }

    #[test]
    fn test_channels_implicit() {
        // Given: A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_PUBLISHER_WITHOUT_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();
        let symbol_table = linker.symbol_table().clone();
        let publisher_symbol =
            linker.symbol_table().resolve_publisher_fqn("sdv.test.Pressure")[1].clone();

        // WHEN requesting channels
        let result = publisher_symbol.channels(&symbol_table);

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
            .push(vsidl_v1_test_data::VSIDL_PUBLISHER_WITH_IMPLICIT_UNIQUE_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();
        let symbol_table = linker.symbol_table().clone();
        let publisher_symbol =
            linker.symbol_table().resolve_publisher_fqn("sdv.test.FuelLevel")[0].clone();

        // WHEN requesting channels
        let result = publisher_symbol.channels(&symbol_table);

        // THEN channels should be UNIQUE
        assert_eq!(result, vec![UNIQUE_CHANNEL.to_string()]);
    }

    #[test]
    fn test_channels_empty() {
        // GIVEN a placeholder symbol table that pretends to be linked
        let mut symbol_table = SymbolTable::default();
        let publisher_id = symbol_table.create_symbol(
            "placeholder".to_string(),
            None,
            SymbolKind::Publisher(PublisherSymbol::default()),
        );
        let publication_id = symbol_table.create_symbol(
            "placeholder".to_string(),
            None,
            SymbolKind::ProtoPublication(ProtoPublicationSymbol::default()),
        );

        // Link the two fake symbols
        if let SymbolKind::Publisher(symbol) =
            &mut symbol_table.get_symbol_mut(&publisher_id.clone()).unwrap()
        {
            symbol.publication = Some(publication_id.clone());
        }

        // Pretend the symbol table is linked
        symbol_table.set_state(crate::SymbolTableState::Linked);
        let publisher = symbol_table.get_publisher_symbol(&publisher_id).unwrap();

        // WHEN requesting channels
        let result = publisher.channels(&symbol_table);

        // THEN channels should be UNIQUE
        assert_eq!(result, vec![UNIQUE_CHANNEL.to_string()]);
    }
}
