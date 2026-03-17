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

use super::{
    symbol::SymbolKind,
    symbol_table::{SymbolTable, SymbolTableState, GLOBAL_SCOPE_ID},
    symbol_table_creator::SymbolTableCreator,
};
use std::collections::HashSet;
use vsidl_v1_common::UNIQUE_ENUM;

/// Responsible for establishing cross-tree relationships between symbols in a [SymbolTable].
///
/// After a [SymbolTableCreator] builds the initial symbol table with containment relationships,
/// the `SymbolTableLinker` resolves references between symbols defined in different parts
/// of the VSIDL and Protobuf definitions. For example, it links [RpcServerSymbol]s to the
/// [ProtoRpcServiceSymbol]s they implement, or [PublisherSymbol]s to the
/// [ProtoPublicationSymbol]s they publish.
pub struct SymbolTableLinker {
    /// The symbol table being linked.
    symbol_table: SymbolTable,
}

/// Links referenced elements in symbol table
impl SymbolTableLinker {
    /// Creates a new `SymbolTableLinker` instance from a completed [SymbolTableCreator].
    ///
    /// This takes ownership of the symbol table built by the creator.
    ///
    /// # Parameters
    ///
    /// * `creator` - The [SymbolTableCreator] that has finished processing the input files.
    pub fn from<F: vsidl_v1_common::FileProvider>(creator: SymbolTableCreator<F>) -> Self {
        SymbolTableLinker { symbol_table: creator.into_symbol_table() }
    }

    /// Returns an immutable reference to the symbol table being linked.
    pub fn symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }

    /// Consumes the linker and returns the linked symbol table.
    pub fn into_symbol_table(self) -> SymbolTable {
        self.symbol_table
    }

    /// Executes the linking process for all supported symbol relationships.
    ///
    /// This method calls the individual linking functions in the correct order
    /// to resolve dependencies between different symbol types. After execution,
    /// the symbol table's state is set to [SymbolTableState::Linked].
    pub fn execute(&mut self) {
        self.link_servers_to_rpc_services();
        if self.symbol_table.uses_pdk29() {
            self.link_publishers_to_publications();
            self.link_publications_to_channels_enum();
            self.link_subscribers_to_publications();
        }
        self.link_publishers_to_messages();
        self.link_subscribers_to_messages();
        self.link_clients_to_rpc_services();
        self.link_rpc_servers_to_channels();
        self.link_rpc_clients_to_channels();
        self.link_publishers_to_topics();
        self.link_topics_to_proto_messages();
        self.link_subscribers_to_topics();
        self.symbol_table.set_state(SymbolTableState::Linked)
    }

    /// Links [RpcServerSymbol]s to the corresponding [ProtoRpcServiceSymbol]s they implement.
    ///
    /// It iterates through all RPC server symbols, resolves the `service` name to a
    /// Protobuf RPC service symbol, and establishes a bi-directional link between them.
    ///
    /// # Panics
    ///
    /// * If the service name cannot be resolved to exactly one [ProtoRpcServiceSymbol].
    ///   (This case should ideally be caught by context conditions before linking).
    fn link_servers_to_rpc_services(&mut self) {
        // Find links
        let mut links = HashSet::new();
        let unlinked = self.symbol_table.get_unlinked_symbols();
        for rpc_server_symbol in self
            .symbol_table
            .get_all_rpc_server_symbols()
            .into_iter()
            .filter(|s| unlinked.contains(&s.id()))
        {
            let rpc_service_name = &rpc_server_symbol.service;
            let enclosing_scope_id = &rpc_server_symbol.symbol.enclosing_scope;

            // Resolve the served service name
            let resolved_symbols: Vec<_> = self
                .symbol_table
                .resolve(enclosing_scope_id, rpc_service_name, false)
                .into_iter()
                .filter(|symbol| matches!(symbol, SymbolKind::ProtoRpcService(_)))
                .collect();

            if let Some(resolved_symbol) = resolved_symbols.first() {
                links.insert((rpc_server_symbol.id().clone(), resolved_symbol.id().clone()));
            } else {
                eprintln!("Warning: Unresolved RPC service reference '{}'", rpc_service_name);
            }
        }

        // Create links
        for (vsidl_server, proto_service) in links {
            if let Some(SymbolKind::RpcServer(vsidl_rpc_server_symbol)) =
                self.symbol_table.get_symbol_mut(&vsidl_server)
            {
                vsidl_rpc_server_symbol.provided_service = Some(proto_service.clone());
            }
            if let Some(SymbolKind::ProtoRpcService(proto_service_symbol)) =
                self.symbol_table.get_symbol_mut(&proto_service)
            {
                proto_service_symbol.servers.push(vsidl_server);
            }
        }
    }

    /// Links [RpcClientSymbol]s to the corresponding [ProtoRpcServiceSymbol]s they consume.
    ///
    /// It iterates through all RPC client symbols, resolves the `service` name to a
    /// Protobuf RPC service symbol, and establishes a bi-directional link between them.
    ///
    /// # Panics
    ///
    /// * If the service name cannot be resolved to exactly one [ProtoRpcServiceSymbol].
    ///   (This case should ideally be caught by context conditions before linking).
    fn link_clients_to_rpc_services(&mut self) {
        // Find links
        let mut links = HashSet::new();
        let unlinked = self.symbol_table.get_unlinked_symbols();
        for rpc_client_symbol in self
            .symbol_table
            .get_all_rpc_client_symbols()
            .into_iter()
            .filter(|s| unlinked.contains(&s.id()))
        {
            let rpc_service_name = &rpc_client_symbol.service;
            let enclosing_scope_id = &rpc_client_symbol.symbol.enclosing_scope;

            // Resolve the consumed service name
            let resolved_symbols: Vec<_> = self
                .symbol_table
                .resolve(enclosing_scope_id, rpc_service_name, false)
                .into_iter()
                .filter(|symbol| matches!(symbol, SymbolKind::ProtoRpcService(_)))
                .collect();
            // Cases where resolved_symbols does not have length 1 are catched by a context condition
            if let Some(resolved_symbol) = resolved_symbols.first() {
                links.insert((rpc_client_symbol.id().clone(), resolved_symbol.id().clone()));
            } else {
                eprintln!("Warning: Unresolved RPC service reference '{}'", rpc_service_name);
            }
        }

        // Create links
        for (vsidl_client, proto_service) in links {
            if let Some(SymbolKind::RpcClient(vsidl_rpc_client_symbol)) =
                self.symbol_table.get_symbol_mut(&vsidl_client)
            {
                vsidl_rpc_client_symbol.requested_service = Some(proto_service.clone());
            }
            if let Some(SymbolKind::ProtoRpcService(proto_service_symbol)) =
                self.symbol_table.get_symbol_mut(&proto_service)
            {
                proto_service_symbol.clients.push(vsidl_client);
            }
        }
    }

    /// Links [RpcServerSymbol]s to the [RpcChannelSymbol] they use.
    fn link_rpc_servers_to_channels(&mut self) {
        let mut server_links = HashSet::new();

        // 1. Collect links for Servers
        for rpc_server_symbol in self.symbol_table.get_all_rpc_server_symbols() {
            let channel_name = rpc_server_symbol.ast_node.channel.clone();

            if channel_name.is_empty() {
                continue;
            }

            let resolved_symbols: Vec<_> = self
                .symbol_table
                .resolve(&GLOBAL_SCOPE_ID, &channel_name, false)
                .into_iter()
                .filter(|symbol| matches!(symbol, SymbolKind::RpcChannel(_)))
                .collect();

            if let Some(resolved_symbol) = resolved_symbols.first() {
                server_links.insert((rpc_server_symbol.id().clone(), resolved_symbol.id().clone()));
            }
        }

        // 2. Apply links for Servers
        for (server_id, channel_id) in server_links {
            if let Some(SymbolKind::RpcServer(server_symbol)) =
                self.symbol_table.get_symbol_mut(&server_id)
            {
                server_symbol.channel = Some(channel_id.clone());
            }
            if let Some(channel_symbol) = self.symbol_table.get_rpc_channel_symbol_mut(&channel_id)
            {
                if !channel_symbol.servers.contains(&server_id) {
                    channel_symbol.servers.push(server_id);
                }
            }
        }
    }

    /// Links [RpcClientSymbol]s to the [RpcChannelSymbol] they use.
    fn link_rpc_clients_to_channels(&mut self) {
        let mut client_links = HashSet::new();

        // 1. Collect links for Clients
        for rpc_client_symbol in self.symbol_table.get_all_rpc_client_symbols() {
            let channel_name = rpc_client_symbol.ast_node.channel.clone();

            if channel_name.is_empty() {
                continue;
            }

            let resolved_symbols: Vec<_> = self
                .symbol_table
                .resolve(&GLOBAL_SCOPE_ID, &channel_name, false)
                .into_iter()
                .filter(|symbol| matches!(symbol, SymbolKind::RpcChannel(_)))
                .collect();

            if let Some(resolved_symbol) = resolved_symbols.first() {
                client_links.insert((rpc_client_symbol.id().clone(), resolved_symbol.id().clone()));
            }
        }

        // 2. Apply links for Clients
        for (client_id, channel_id) in client_links {
            if let Some(SymbolKind::RpcClient(client_symbol)) =
                self.symbol_table.get_symbol_mut(&client_id)
            {
                client_symbol.channel = Some(channel_id.clone());
            }
            if let Some(channel_symbol) = self.symbol_table.get_rpc_channel_symbol_mut(&channel_id)
            {
                if !channel_symbol.clients.contains(&client_id) {
                    channel_symbol.clients.push(client_id);
                }
            }
        }
    }

    /// Links [PublisherSymbol]s to the corresponding [ProtoPublicationSymbol]s they publish.
    ///
    /// It iterates through all publisher symbols, resolves the `message` name to a
    /// Protobuf publication symbol, and establishes a bi-directional link between them.
    ///
    /// # Panics
    ///
    /// * If the type name cannot be resolved to exactly one [ProtoPublicationSymbol].
    ///   (This case should ideally be caught by context conditions before linking).
    fn link_publishers_to_publications(&mut self) {
        // Find links
        let mut links = HashSet::new();
        let unlinked = self.symbol_table.get_unlinked_symbols();
        for publisher in self
            .symbol_table
            .get_all_publisher_symbols()
            .into_iter()
            .filter(|s| unlinked.contains(&s.id()))
        {
            let publication_type = &publisher.ast_node.message;
            let enclosing_scope_id = &publisher.symbol.enclosing_scope;

            // Resolve the publication name
            let resolved_symbols: Vec<_> = self
                .symbol_table
                .resolve(enclosing_scope_id, publication_type, false)
                .into_iter()
                .filter(|symbol| matches!(symbol, SymbolKind::ProtoPublication(_)))
                .collect();

            if let Some(resolved_symbol) = resolved_symbols.first() {
                links.insert((publisher.id().clone(), resolved_symbol.id().clone()));
            } else {
                eprintln!("Warning: Unresolved publication reference '{}'", publication_type);
            }
        }

        // Create links
        for (vsidl_publisher, proto_publication) in links {
            if let Some(SymbolKind::Publisher(vsidl_publisher_symbol)) =
                self.symbol_table.get_symbol_mut(&vsidl_publisher)
            {
                vsidl_publisher_symbol.publication = Some(proto_publication.clone());
            }
            if let Some(SymbolKind::ProtoPublication(proto_publication_symbol)) =
                self.symbol_table.get_symbol_mut(&proto_publication)
            {
                proto_publication_symbol.publishers.push(vsidl_publisher);
            }
        }
    }

    /// Links [ProtoPublicationSymbol]s to the [ProtoEnumSymbol] specified in their
    /// `channels_enum` annotation option.
    ///
    /// If a publication does not specify `channels_enum` or it's empty, it links to the
    /// implicitly defined [UNIQUE_ENUM].
    ///
    /// # Panics
    ///
    /// * If the specified `channels_enum` name cannot be resolved to exactly one [ProtoEnumSymbol].
    ///   (This case should ideally be caught by context conditions before linking).
    fn link_publications_to_channels_enum(&mut self) {
        // Find links
        let mut links = HashSet::new();
        let unlinked = self.symbol_table.get_unlinked_symbols();
        for proto_publication_symbol in self
            .symbol_table
            .get_all_proto_publication_symbols()
            .into_iter()
            .filter(|s| unlinked.contains(&s.id()))
        {
            let mut channels_enum_name = proto_publication_symbol.channels_enum_name();

            let unique_enum_name = Some(UNIQUE_ENUM.to_string());
            if channels_enum_name.is_none()
                || channels_enum_name.as_ref().map(|s| s.is_empty()).unwrap_or(true)
            {
                channels_enum_name = unique_enum_name;
            }

            if let Some(channels_enum_name) = channels_enum_name {
                let enclosing_scope_id = &proto_publication_symbol.symbol.enclosing_scope;
                if !channels_enum_name.is_empty() {
                    // Resolve the enum name
                    let resolved_symbols: Vec<_> = self
                        .symbol_table
                        .resolve(enclosing_scope_id, &channels_enum_name, false)
                        .into_iter()
                        .filter(|symbol| matches!(symbol, SymbolKind::ProtoEnum(_)))
                        .collect();
                    if let Some(resolved_symbol) = resolved_symbols.first() {
                        // In well-defined Proto files, there can only be one topics enum per publication
                        links.insert((
                            proto_publication_symbol.id().clone(),
                            resolved_symbol.id().clone(),
                        ));
                    }
                }
            }
        }

        // Create links
        for (proto_publication, proto_enum) in links {
            if let Some(SymbolKind::ProtoPublication(proto_publication_symbol)) =
                self.symbol_table.get_symbol_mut(&proto_publication)
            {
                proto_publication_symbol.channels_enum = Some(proto_enum.clone());
            }
        }
    }

    /// Links [SubscriberSymbol]s to the corresponding [ProtoPublicationSymbol]s they subscribe to.
    ///
    /// It iterates through all subscriber symbols, resolves the `message` name to a
    /// Protobuf publication symbol, and establishes a bi-directional link between them.
    ///
    /// # Panics
    ///
    /// * If the type name cannot be resolved to exactly one [ProtoPublicationSymbol].
    ///   (This case should ideally be caught by context conditions before linking).
    fn link_subscribers_to_publications(&mut self) {
        // Find links
        let mut links = HashSet::new();
        let unlinked = self.symbol_table.get_unlinked_symbols();
        for subscriber in self
            .symbol_table
            .get_all_subscriber_symbols()
            .into_iter()
            .filter(|s| unlinked.contains(&s.id()))
        {
            let publication_type = &subscriber.ast_node.message;
            let enclosing_scope_id = &subscriber.symbol.enclosing_scope;

            // Resolve the publication name
            let resolved_symbols: Vec<_> = self
                .symbol_table
                .resolve(enclosing_scope_id, publication_type, false)
                .into_iter()
                .filter(|symbol| matches!(symbol, SymbolKind::ProtoPublication(_)))
                .collect();

            if let Some(resolved_symbol) = resolved_symbols.first() {
                links.insert((subscriber.id().clone(), resolved_symbol.id().clone()));
            } else {
                eprintln!("Warning: Unresolved publication reference '{}'", publication_type);
            }
        }

        // Create links
        for (vsidl_subscriber, proto_publication) in links {
            if let Some(vsidl_subscriber_symbol) =
                self.symbol_table.get_subscriber_symbol_mut(&vsidl_subscriber)
            {
                vsidl_subscriber_symbol.publication = Some(proto_publication.clone());
            }
            if let Some(proto_publication_symbol) =
                self.symbol_table.get_proto_publication_symbol_mut(&proto_publication)
            {
                proto_publication_symbol.subscribers.push(vsidl_subscriber);
            }
        }
    }

    /// Links [PublisherSymbol]s to the corresponding [ProtoMessageSymbol]s they publish.
    ///
    /// It iterates through all publisher symbols, resolves the `message` name to a
    /// Protobuf message symbol, and establishes a bi-directional link between them.
    ///
    /// # Panics
    ///
    /// * If the type name cannot be resolved to exactly one [ProtoMessageSymbol].
    ///   (This case should ideally be caught by context conditions before linking).
    fn link_publishers_to_messages(&mut self) {
        // Find links
        let mut links = HashSet::new();
        let unlinked = self.symbol_table.get_unlinked_symbols();
        for publisher in self
            .symbol_table
            .get_all_publisher_symbols()
            .into_iter()
            .filter(|s| unlinked.contains(&s.id()))
        {
            let message_type = &publisher.ast_node.message;
            let enclosing_scope_id = &publisher.symbol.enclosing_scope;

            // Resolve the message name
            let resolved_symbols: Vec<_> = self
                .symbol_table
                .resolve(enclosing_scope_id, message_type, false)
                .into_iter()
                .filter(|symbol| matches!(symbol, SymbolKind::ProtoMessage(_)))
                .collect();

            if let Some(resolved_symbol) = resolved_symbols.first() {
                links.insert((publisher.id().clone(), resolved_symbol.id().clone()));
            } else {
                eprintln!("Warning: Unresolved message reference '{}'", message_type);
            }
        }

        // Create links
        for (vsidl_publisher, proto_message) in links {
            if let Some(SymbolKind::Publisher(vsidl_publisher_symbol)) =
                self.symbol_table.get_symbol_mut(&vsidl_publisher)
            {
                vsidl_publisher_symbol.message = Some(proto_message.clone());
            }
        }
    }

    /// Links [SubscriberSymbol]s to the corresponding [ProtoMessageSymbol]s they subscribe to.
    ///
    /// It iterates through all subscriber symbols, resolves the `message` name to a
    /// Protobuf message symbol, and establishes a bi-directional link between them.
    ///
    /// # Panics
    ///
    /// * If the type name cannot be resolved to exactly one [ProtoMessageSymbol].
    ///   (This case should ideally be caught by context conditions before linking).
    fn link_subscribers_to_messages(&mut self) {
        // Find links
        let mut links = HashSet::new();
        let unlinked = self.symbol_table.get_unlinked_symbols();
        for subscriber in self
            .symbol_table
            .get_all_subscriber_symbols()
            .into_iter()
            .filter(|s| unlinked.contains(&s.id()))
        {
            let message_type = &subscriber.ast_node.message;
            let enclosing_scope_id = &subscriber.symbol.enclosing_scope;

            // Resolve the message name
            let resolved_symbols: Vec<_> = self
                .symbol_table
                .resolve(enclosing_scope_id, message_type, false)
                .into_iter()
                .filter(|symbol| matches!(symbol, SymbolKind::ProtoMessage(_)))
                .collect();

            if let Some(resolved_symbol) = resolved_symbols.first() {
                links.insert((subscriber.id().clone(), resolved_symbol.id().clone()));
            } else {
                eprintln!("Warning: Unresolved message reference '{}'", message_type);
            }
        }

        // Create links
        for (vsidl_subscriber, proto_message) in links {
            if let Some(vsidl_subscriber_symbol) =
                self.symbol_table.get_subscriber_symbol_mut(&vsidl_subscriber)
            {
                vsidl_subscriber_symbol.message = Some(proto_message.clone());
            }
        }
    }

    /// Links [PublisherSymbol]s to the [TopicSymbol]s they publish to.
    fn link_publishers_to_topics(&mut self) {
        // Find links
        let mut links = HashSet::new();
        let unlinked = self.symbol_table.get_unlinked_symbols();
        for publisher in self
            .symbol_table
            .get_all_publisher_symbols()
            .into_iter()
            .filter(|s| unlinked.contains(&s.id()))
        {
            for topic_name in &publisher.ast_node.topic {
                // Resolve the topic name in global scope
                let resolved_symbols: Vec<_> = self
                    .symbol_table
                    .resolve(&GLOBAL_SCOPE_ID, topic_name, false)
                    .into_iter()
                    .filter(|symbol| matches!(symbol, SymbolKind::Topic(_)))
                    .collect();

                if !resolved_symbols.is_empty() {
                    links.insert((publisher.id().clone(), resolved_symbols[0].id().clone()));
                }
            }
        }

        // Create links
        for (publisher_id, topic_id) in links {
            if let Some(SymbolKind::Publisher(publisher_symbol)) =
                self.symbol_table.get_symbol_mut(&publisher_id)
            {
                if !publisher_symbol.topics.contains(&topic_id) {
                    publisher_symbol.topics.push(topic_id.clone());
                }
            }
            if let Some(topic_symbol) = self.symbol_table.get_topic_symbol_mut(&topic_id) {
                if !topic_symbol.publishers.contains(&publisher_id) {
                    topic_symbol.publishers.push(publisher_id.clone());
                }
            }
        }
    }

    /// Links [TopicSymbol]s to the [ProtoMessageSymbol] they carry.
    fn link_topics_to_proto_messages(&mut self) {
        let mut message_to_topics = HashSet::new();

        // Iterate through all publishers to find all message-topic associations
        for publisher in self.symbol_table.get_all_publisher_symbols() {
            if let Some(message_id) = &publisher.message {
                for topic_id in &publisher.topics {
                    message_to_topics.insert((message_id.clone(), topic_id.clone()));
                }
            }
        }

        // Create links
        for (proto_message_id, topic_id) in message_to_topics {
            // Link Topic to Message
            if let Some(topic_symbol) = self.symbol_table.get_topic_symbol_mut(&topic_id) {
                if topic_symbol.message.is_none() {
                    topic_symbol.message = Some(proto_message_id.clone());
                } else if topic_symbol.message != Some(proto_message_id.clone()) {
                    // This scenario, where the same topic name is used for different message types,
                    // should ideally be caught by another context condition.
                    eprintln!(
                        "Warning: TopicSymbol {:?} ({}) is already linked to message {:?}, ignoring link to {:?}",
                        topic_id,
                        topic_symbol.name(),
                        topic_symbol.message,
                        proto_message_id
                    );
                }
            }

            // Link Message to Topic
            if let Some(message_symbol) =
                self.symbol_table.get_proto_message_symbol_mut(&proto_message_id)
            {
                if !message_symbol.topics.contains(&topic_id) {
                    message_symbol.topics.push(topic_id);
                }
            }
        }
    }

    /// Links [SubscriberSymbol]s to the [TopicSymbol]s they subscribe to.
    ///
    /// This function re-evaluates all subscriber symbols, not just unlinked ones.
    /// This is necessary because TopicSymbols are implicitly generated from Publishers
    /// in VSIDL files. Unlike symbols derived from .proto files, TopicSymbols
    /// can be removed and recreated with new IDs during incremental file processing.
    /// Therefore, existing subscribers may have stale Topic IDs that need to be updated.
    fn link_subscribers_to_topics(&mut self) {
        let mut links = HashSet::new();
        // Iterate through ALL subscribers, as existing ones might need relinking to new TopicSymbol instances.
        for subscriber in self.symbol_table.get_all_subscriber_symbols().into_iter() {
            for topic_name in &subscriber.topic {
                // Resolve the topic name in global scope
                let resolved_symbols: Vec<_> = self
                    .symbol_table
                    .resolve(&GLOBAL_SCOPE_ID, topic_name, false)
                    .into_iter()
                    .filter(|symbol| matches!(symbol, SymbolKind::Topic(_)))
                    .collect();

                if let Some(resolved_symbol) = resolved_symbols.first() {
                    links.insert((subscriber.id().clone(), resolved_symbol.id().clone()));
                }
            }
        }

        // Clear old topic links for all subscribers involved in the new links.
        // This is crucial to remove stale IDs from topics that might have been recreated.
        let subscriber_ids_to_update: HashSet<_> = links.iter().map(|(s, _)| s).collect();
        for subscriber_id in subscriber_ids_to_update {
            if let Some(subscriber_symbol) =
                self.symbol_table.get_subscriber_symbol_mut(subscriber_id)
            {
                subscriber_symbol.topics.clear();
            }
        }

        // Create links
        for (subscriber_id, topic_id) in links {
            if let Some(subscriber_symbol) =
                self.symbol_table.get_subscriber_symbol_mut(&subscriber_id)
            {
                if !subscriber_symbol.topics.contains(&topic_id) {
                    subscriber_symbol.topics.push(topic_id.clone());
                }
            }
            if let Some(topic_symbol) = self.symbol_table.get_topic_symbol_mut(&topic_id) {
                if !topic_symbol.subscribers.contains(&subscriber_id) {
                    topic_symbol.subscribers.push(subscriber_id.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        symbol_table_creator::SymbolTableCreator, symbol_table_linker::SymbolKind,
        symbol_table_linker::SymbolTableLinker,
    };
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;

    fn setup() -> SymbolTableCreator {
        // GIVEN a symbol table with a service bundle, an RPC server and a Proto RPC service
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_BUNDLE_ONE_VALID.clone());
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_BUNDLE_TWO_VALID.clone());

        // WHEN creating and linking the symbol table
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        creator
    }

    #[test]
    fn test_link_servers_to_rpc_services() {
        // GIVEN a symbol table from catalog
        let creator = setup();

        // WHEN linking (VSIDL) RPC servers to (Proto) RPC services
        let mut linker = SymbolTableLinker::from(creator);
        linker.link_servers_to_rpc_services();
        let symbol_table = linker.symbol_table;

        // THEN each RPC server shall have a corresponding RPC service with the name of the service
        let all_rpc_server_symbols = symbol_table.get_all_rpc_server_symbols();
        assert!(!all_rpc_server_symbols.is_empty());
        for rpc_server_symbol in all_rpc_server_symbols {
            assert!(rpc_server_symbol.provided_service.is_some());
            if let SymbolKind::ProtoRpcService(rpc_service_symbol) = &symbol_table
                .get_symbol(&rpc_server_symbol.provided_service.clone().unwrap())
                .unwrap()
            {
                assert_eq!(
                    rpc_server_symbol.service,
                    rpc_service_symbol.name.clone().expect("RPC service should have a name")
                );
            }
        }
    }

    #[test]
    fn test_link_clients_to_rpc_services() {
        // GIVEN a symbol table from catalog
        let creator = setup();

        // WHEN linking (VSIDL) RPC clients to (Proto) RPC services
        let mut linker = SymbolTableLinker::from(creator);
        linker.link_clients_to_rpc_services();
        let symbol_table = linker.symbol_table;

        // THEN each RPC client shall have a corresponding RPC service with the name of the service
        let all_rpc_client_symbols = symbol_table.get_all_rpc_client_symbols();
        assert!(!all_rpc_client_symbols.is_empty());
        for rpc_client_symbol in all_rpc_client_symbols {
            assert!(rpc_client_symbol.requested_service.is_some());
            if let SymbolKind::ProtoRpcService(rpc_service_symbol) = &symbol_table
                .get_symbol(&rpc_client_symbol.requested_service.clone().unwrap())
                .unwrap()
            {
                assert_eq!(
                    rpc_client_symbol.service,
                    rpc_service_symbol.name.clone().expect("RPC service should have a name")
                );
            } else {
                panic!("rpc_client_symbol.requested_service is not an RPC service");
            }
        }
    }

    #[test]
    fn test_link_publishers_to_publications() {
        // GIVEN a symbol table from catalog
        let creator = setup();

        // WHEN linking (VSIDL) publishers to (Proto) publications
        let mut linker = SymbolTableLinker::from(creator);
        linker.link_publishers_to_publications();
        let symbol_table = linker.symbol_table;

        // THEN each publisher shall have a corresponding publication with the name of the type
        let all_publisher_symbols = symbol_table.get_all_publisher_symbols();
        assert!(!all_publisher_symbols.is_empty());
        for publisher_symbol in all_publisher_symbols {
            assert!(publisher_symbol.publication.is_some());
            if let SymbolKind::ProtoPublication(proto_publication_symbol) =
                &symbol_table.get_symbol(&publisher_symbol.publication.clone().unwrap()).unwrap()
            {
                assert_eq!(
                    publisher_symbol.ast_node.message,
                    proto_publication_symbol.name.clone().expect("Publication should have a name")
                );
            }
        }
    }

    #[test]
    fn test_link_publications_to_channels_enum() {
        // GIVEN a symbol table from catalog
        let creator = setup();

        // WHEN linking (Proto) publications to (Proto) enums
        let mut linker = SymbolTableLinker::from(creator);
        linker.link_publications_to_channels_enum();
        let symbol_table = linker.symbol_table;

        // THEN each publication shall have a corresponding enum with the name of the channels_enum
        let publication_symbols = symbol_table.get_all_proto_publication_symbols();
        assert_eq!(publication_symbols.len(), 6);
        for proto_publication_symbol in publication_symbols {
            // Get enum name from AST
            let enum_name = proto_publication_symbol.channels_enum_name();

            // Enum name from AST shall match the name from the enum linked to the symbol
            if let Some(enum_name) = enum_name {
                if !enum_name.is_empty() {
                    assert!(proto_publication_symbol.channels_enum.is_some());
                    if let SymbolKind::ProtoEnum(proto_enum_symbol) = &symbol_table
                        .get_symbol(&proto_publication_symbol.channels_enum.clone().unwrap())
                        .unwrap()
                    {
                        // Assert that the unqualified names match
                        assert_eq!(
                            enum_name.rsplit('.').next().unwrap(),
                            proto_enum_symbol
                                .name
                                .clone()
                                .expect("Enum should have a name")
                                .rsplit('.')
                                .next()
                                .unwrap()
                        );
                    } else {
                        panic!("No proto enum symbol found for '{enum_name}'");
                    }
                }
            }
        }
    }

    #[test]
    fn test_link_subscribers_to_publications() {
        // GIVEN a symbol table from catalog
        let creator = setup();

        // WHEN linking (VSIDL) subscribers to (Proto) publications
        let mut linker = SymbolTableLinker::from(creator);
        linker.link_subscribers_to_publications();
        let symbol_table = linker.symbol_table;

        // THEN each subscriber shall have a corresponding publication with the name of the type
        let all_subscriber_symbols = symbol_table.get_all_subscriber_symbols();
        assert!(!all_subscriber_symbols.is_empty());
        for subscriber_symbol in all_subscriber_symbols {
            assert!(subscriber_symbol.publication.is_some());
            if let SymbolKind::ProtoPublication(proto_publication_symbol) =
                &symbol_table.get_symbol(&subscriber_symbol.publication.clone().unwrap()).unwrap()
            {
                assert_eq!(
                    subscriber_symbol.ast_node.message,
                    proto_publication_symbol.name.clone().expect("Publication should have a name")
                );
                assert!(proto_publication_symbol.subscribers.contains(&subscriber_symbol.id()));
            } else {
                panic!(
                    "No proto publication symbol found for subscriber '{}'",
                    subscriber_symbol.ast_node.message
                );
            }
        }
    }

    #[test]
    fn test_link_topics_to_proto_messages() {
        // GIVEN a symbol table from catalog
        let creator = setup();

        // WHEN linking (VSIDL) topics to (Proto) messages
        let mut linker = SymbolTableLinker::from(creator);
        linker.link_publishers_to_messages(); // This is required before linking topics
        linker.link_topics_to_proto_messages();
        let symbol_table = linker.symbol_table;

        // THEN each topic used in a publisher or subscriber shall have a corresponding message
        let all_topic_symbols = symbol_table.get_all_topic_symbols();
        assert!(!all_topic_symbols.is_empty());

        let mut pubsub_topic_ids = std::collections::HashSet::new();
        for pub_symbol in symbol_table.get_all_publisher_symbols() {
            pubsub_topic_ids.extend(pub_symbol.topics.iter());
        }
        for sub_symbol in symbol_table.get_all_subscriber_symbols() {
            pubsub_topic_ids.extend(sub_symbol.topics.iter());
        }

        for topic_symbol in all_topic_symbols {
            if pubsub_topic_ids.contains(&topic_symbol.id()) {
                assert!(
                    topic_symbol.message.is_some(),
                    "Topic {:?} linked to Pub/Sub should have a message",
                    topic_symbol.id()
                );
                if let SymbolKind::ProtoMessage(proto_message_symbol) =
                    &symbol_table.get_symbol(&topic_symbol.message.clone().unwrap()).unwrap()
                {
                    assert_eq!(
                        proto_message_symbol.name.clone().expect("Message should have a name"),
                        "Pressure"
                    );
                } else {
                    panic!("topic_symbol.message is not a ProtoMessage");
                }
            }
        }
    }

    #[test]
    fn test_link_subscribers_to_topics() {
        // GIVEN a symbol table from catalog
        let creator = setup();

        // WHEN linking (VSIDL) subscribers to (VSIDL) topics
        let mut linker = SymbolTableLinker::from(creator);
        linker.link_subscribers_to_topics();
        let symbol_table = linker.symbol_table;

        // THEN each subscriber shall have a corresponding topic
        let all_subscriber_symbols = symbol_table.get_all_subscriber_symbols();
        assert!(!all_subscriber_symbols.is_empty());
        for subscriber_symbol in all_subscriber_symbols {
            // ServiceBundleTwo subscribes to topic "two" of "Pressure"
            if subscriber_symbol.ast_node.message == "Pressure" {
                assert!(!subscriber_symbol.topics.is_empty());
                let topic_id = &subscriber_symbol.topics[0];
                let topic_symbol = symbol_table.get_topic_symbol(topic_id).unwrap();
                assert_eq!(topic_symbol.name(), "two");
                assert!(topic_symbol.subscribers.contains(&subscriber_symbol.id()));
            }
        }
    }

    #[test]
    fn test_symbol_table_linker_e2e_test() {
        // GIVEN a symbol table from catalog
        let creator = setup();

        // WHEN linking the symbol table
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();
        let symbol_table = linker.symbol_table;

        // THEN Check RpcServer to RpcService link
        let vsidl_rpc_server_symbol =
            symbol_table.resolve_rpc_server_fqn("sdv.test.DummyRpc")[0].clone();
        let proto_rpc_service_symbol =
            symbol_table.resolve_proto_rpc_service_fqn("sdv.test.DummyRpc")[0].clone();
        assert_eq!(
            vsidl_rpc_server_symbol.provided_service.as_ref().unwrap(),
            &proto_rpc_service_symbol.id()
        );
        assert!(proto_rpc_service_symbol.servers.contains(&vsidl_rpc_server_symbol.id()));

        // THEN Check RpcClient to RpcService link
        let vsidl_rpc_client_symbol =
            symbol_table.resolve_rpc_client_fqn("sdv.test.DummyRpc")[0].clone();
        let proto_rpc_service_symbol =
            symbol_table.resolve_proto_rpc_service_fqn("sdv.test.DummyRpc")[0].clone();
        assert_eq!(
            vsidl_rpc_client_symbol.requested_service.as_ref().unwrap(),
            &proto_rpc_service_symbol.id()
        );
        assert!(proto_rpc_service_symbol.clients.contains(&vsidl_rpc_client_symbol.id()));

        // THEN Check Publisher to Publication link
        let vsidl_publisher_symbol =
            symbol_table.resolve_publisher_fqn("sdv.test.Pressure")[0].clone();
        let proto_publication_symbol =
            symbol_table.resolve_proto_publication_fqn("sdv.test.Pressure")[0].clone();
        assert_eq!(
            vsidl_publisher_symbol.publication.as_ref().unwrap(),
            &proto_publication_symbol.id()
        );
        assert!(proto_publication_symbol.publishers.contains(&vsidl_publisher_symbol.id()));

        // THEN Check Publication to Channels Enum
        let proto_publication_symbol_with_enum =
            symbol_table.resolve_proto_publication_fqn("sdv.test.BatteryLevel")[0].clone();
        let proto_enum_symbol =
            symbol_table.resolve_proto_enum_fqn("sdv.test.BatteryChannels")[0].clone();
        assert_eq!(
            proto_publication_symbol_with_enum.channels_enum.as_ref().unwrap(),
            &proto_enum_symbol.id()
        );

        // THEN Check Subscriber to Publication
        let vsidl_subscriber_symbol =
            symbol_table.resolve_subscriber_fqn("sdv.test.Pressure")[0].clone();
        let proto_publication_symbol_from_subscriber =
            symbol_table.resolve_proto_publication_fqn("sdv.test.Pressure")[0].clone();
        assert_eq!(
            vsidl_subscriber_symbol.publication.as_ref().unwrap(),
            &proto_publication_symbol_from_subscriber.id()
        );
        assert!(proto_publication_symbol_from_subscriber
            .subscribers
            .contains(&vsidl_subscriber_symbol.id()));

        // THEN Check Topic to Publication link
        let topic_one_symbol =
            symbol_table.get_all_topic_symbols().into_iter().find(|s| s.name() == "one").unwrap();
        assert_eq!(
            topic_one_symbol.message.as_ref().unwrap(),
            vsidl_subscriber_symbol.message.as_ref().unwrap()
        );

        // THEN Check Subscriber to Topic link
        let topic_two_symbol =
            symbol_table.get_all_topic_symbols().into_iter().find(|s| s.name() == "two").unwrap();
        assert!(vsidl_subscriber_symbol.topics.contains(&topic_two_symbol.id()));
        assert!(topic_two_symbol.subscribers.contains(&vsidl_subscriber_symbol.id()));
    }

    #[test]
    fn test_symbol_table_linker_incremental_test() {
        // GIVEN a symbol table with a service bundle, an RPC server and a Proto RPC service
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_BUNDLE_ONE_VALID.clone());
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_BUNDLE_TWO_VALID.clone());

        // WHEN creating and linking the symbol table
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN linking the symbol table
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // THEN Check Topic to Message link
        let symbol_table = linker.symbol_table.clone();
        let proto_message_symbol_from_subscriber =
            symbol_table.resolve_proto_message_fqn("sdv.test.Pressure")[0].clone();
        let vsidl_subscriber_symbol =
            symbol_table.resolve_subscriber_fqn("sdv.test.Pressure")[0].clone();

        let topic_one_symbol =
            symbol_table.get_all_topic_symbols().into_iter().find(|s| s.name() == "one").unwrap();
        assert_eq!(
            topic_one_symbol.message.as_ref().unwrap(),
            &proto_message_symbol_from_subscriber.id()
        );

        // THEN Check Subscriber to Topic link
        let topic_two_symbol =
            symbol_table.get_all_topic_symbols().into_iter().find(|s| s.name() == "two").unwrap();
        assert!(vsidl_subscriber_symbol.topics.contains(&topic_two_symbol.id()));
        assert!(topic_two_symbol.subscribers.contains(&vsidl_subscriber_symbol.id()));

        // WHEN removing a file from the symbol table
        let file_to_reprocess = vsidl_v1_test_data::VSIDL_BUNDLE_ONE_VALID.clone().0;
        let mut symbol_table = linker.symbol_table.clone();
        symbol_table.remove_symbols_for_file(&file_to_reprocess);

        // THEN symbols from other files are present
        assert!(!symbol_table.resolve_rpc_server_fqn("sdv.test.DummyRpc").is_empty());
        // THEN it's symbols shall not be present
        assert!(symbol_table.resolve_publisher_fqn("sdv.test.Pressure").is_empty());

        // WHEN reprocessing the file
        let mut creator = SymbolTableCreator::from_symbol_table(symbol_table.clone());
        creator.restrict_to_file(file_to_reprocess);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        let symbol_table = linker.symbol_table.clone();

        // THEN Check RpcServer to RpcService link
        let vsidl_rpc_server_symbol =
            symbol_table.resolve_rpc_server_fqn("sdv.test.DummyRpc")[0].clone();
        let proto_rpc_service_symbol =
            symbol_table.resolve_proto_rpc_service_fqn("sdv.test.DummyRpc")[0].clone();
        assert_eq!(
            vsidl_rpc_server_symbol.provided_service.as_ref().unwrap(),
            &proto_rpc_service_symbol.id()
        );
        assert!(proto_rpc_service_symbol.servers.contains(&vsidl_rpc_server_symbol.id()));

        // THEN Check RpcClient to RpcService link
        let vsidl_rpc_client_symbol =
            symbol_table.resolve_rpc_client_fqn("sdv.test.DummyRpc")[0].clone();
        let proto_rpc_service_symbol =
            symbol_table.resolve_proto_rpc_service_fqn("sdv.test.DummyRpc")[0].clone();
        assert_eq!(
            vsidl_rpc_client_symbol.requested_service.as_ref().unwrap(),
            &proto_rpc_service_symbol.id()
        );
        assert!(proto_rpc_service_symbol.clients.contains(&vsidl_rpc_client_symbol.id()));

        // THEN Check Publisher to Publication link
        let vsidl_publisher_symbol =
            symbol_table.resolve_publisher_fqn("sdv.test.Pressure")[0].clone();
        let proto_publication_symbol =
            symbol_table.resolve_proto_publication_fqn("sdv.test.Pressure")[0].clone();
        assert_eq!(
            vsidl_publisher_symbol.publication.as_ref().unwrap(),
            &proto_publication_symbol.id()
        );
        assert!(proto_publication_symbol.publishers.contains(&vsidl_publisher_symbol.id()));

        // THEN Check Publication to Channels Enum
        let proto_publication_symbol_with_enum =
            symbol_table.resolve_proto_publication_fqn("sdv.test.BatteryLevel")[0].clone();
        let proto_enum_symbol =
            symbol_table.resolve_proto_enum_fqn("sdv.test.BatteryChannels")[0].clone();
        assert_eq!(
            proto_publication_symbol_with_enum.channels_enum.as_ref().unwrap(),
            &proto_enum_symbol.id()
        );

        // THEN Check Subscriber to Publication
        let vsidl_subscriber_symbol =
            symbol_table.resolve_subscriber_fqn("sdv.test.Pressure")[0].clone();
        let proto_publication_symbol_from_subscriber =
            symbol_table.resolve_proto_publication_fqn("sdv.test.Pressure")[0].clone();
        assert_eq!(
            vsidl_subscriber_symbol.publication.as_ref().unwrap(),
            &proto_publication_symbol_from_subscriber.id()
        );
        assert!(proto_publication_symbol_from_subscriber
            .subscribers
            .contains(&vsidl_subscriber_symbol.id()));

        // THEN Check Topic to Message link
        let topic_one_symbol =
            symbol_table.get_all_topic_symbols().into_iter().find(|s| s.name() == "one").unwrap();
        assert_eq!(
            topic_one_symbol.message.as_ref().unwrap(),
            vsidl_subscriber_symbol.message.as_ref().unwrap()
        );

        // THEN Check Subscriber to Topic link
        let topic_two_symbol =
            symbol_table.get_all_topic_symbols().into_iter().find(|s| s.name() == "two").unwrap();
        assert!(vsidl_subscriber_symbol.topics.contains(&topic_two_symbol.id()));
        assert!(topic_two_symbol.subscribers.contains(&vsidl_subscriber_symbol.id()));
    }
}
