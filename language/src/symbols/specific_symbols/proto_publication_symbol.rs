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
    implement_ast_node_deref, implement_symbol_deref, SymbolCommon, SymbolId, SymbolKind,
    SymbolTable,
};
use protobuf::descriptor::DescriptorProto;
use vsidl_v1::span::Span;
use vsidl_v1_common::message_size::MessageSize;
use vsidl_v1_common::ProtoMetadata;

use super::proto_enum_symbol::ProtoEnumSymbol;
use super::publisher_symbol::PublisherSymbol;
use super::subscriber_symbol::SubscriberSymbol;
use vsidl_v1::annotations::sdv_publication;

/// Represents a Protobuf Publication definition within the symbol table.
///
/// This struct holds information about a specific publication defined in a `.proto` file,
/// including its AST node, common symbol properties, metadata, dependencies,
/// publication characteristics, and links to related symbols like publishers,
/// subscribers, and the owning unit type. It allows accessing the underlying
/// [DescriptorProto] AST node via `Deref`.
#[derive(Default, Debug, Clone)]
pub struct ProtoPublicationSymbol {
    /// The original Protobuf AST node ([DescriptorProto]) this symbol was created from.
    pub ast_node: DescriptorProto,
    /// Common information shared across all symbol types (like ID, name, scope).
    pub symbol: SymbolCommon,
    // Doc string in front of the definition of the symbol in the Protobuf file
    pub doc_string: String,
    /// Metadata related to the `.proto` file where this publication is defined.
    pub proto_metadata: ProtoMetadata,
    /// Metadata for the `.proto` files that the defining `.proto` file depends on.
    pub proto_dependencies: Vec<ProtoMetadata>,
    /// The number of messages the publication queue can hold, derived from the [sdv_publication] annotation.
    pub message_count: u32,
    /// The calculated maximum size of the Protobuf message associated with this publication.
    pub message_size: MessageSize,
    /// The publication model (e.g., SINGLE_PUB or MULTI_PUB), derived from the [sdv_publication] annotation.
    pub model: sdv_publication::PublisherModel,
    /// Symbol IDs of the [PublisherSymbol]s that publish this publication. Requires the symbol table to be linked.
    pub(crate) publishers: Vec<SymbolId>,
    /// Symbol IDs of the [SubscriberSymbol]s that subscribe to this publication. Requires the symbol table to be linked.
    pub(crate) subscribers: Vec<SymbolId>,
    /// Symbol ID of the [ProtoEnumSymbol] that defines the channels for this publication, if any. Requires the symbol table to be linked.
    pub(crate) channels_enum: Option<SymbolId>,
}
// Implements Deref for the ast_node field, allowing direct access to the DescriptorProto fields.
implement_ast_node_deref!(ProtoPublicationSymbol, DescriptorProto);

impl ProtoPublicationSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();

    /// Checks if the publication model is [SINGLE_PUB].
    ///
    /// # Returns
    ///
    /// `true` if the publication model is [SINGLE_PUB], `false` otherwise.
    pub fn is_single_pub(&self) -> bool {
        self.model == sdv_publication::PublisherModel::SINGLE_PUB
    }

    /// Checks if the publication model is [MULTI_PUB].
    ///
    /// # Returns
    ///
    /// `true` if the publication model is [MULTI_PUB], `false` otherwise.
    pub fn is_multi_pub(&self) -> bool {
        self.model == sdv_publication::PublisherModel::MULTI_PUB
    }

    /// Returns the [PublisherSymbol]s associated with this publication.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the publisher IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<PublisherSymbol>` containing the publishers for this publication.
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

    /// Returns the [SubscriberSymbol]s associated with this publication.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the subscriber IDs.
    ///
    /// # Returns
    ///
    /// A `Vec<SubscriberSymbol>` containing the subscribers for this publication.
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

    /// Returns the [ProtoEnumSymbol] that defines the channels for this publication.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the channels enum ID.
    ///
    /// # Returns
    ///
    /// The [ProtoEnumSymbol] representing the channels enum.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn channels_enum<'a>(&self, symbol_table: &'a SymbolTable) -> &'a ProtoEnumSymbol {
        symbol_table.panic_if_not_linked();
        let symbol_kind = self
            .channels_enum
            .as_ref()
            .map(|channels_enum| symbol_table.get_symbol(channels_enum).unwrap())
            .unwrap();
        match symbol_kind {
            SymbolKind::ProtoEnum(proto_enum) => proto_enum,
            _ => panic!("Expected ProtoEnum symbol"),
        }
    }

    /// Returns the names of the channels defined for this publication.
    ///
    /// If the publication does not explicitly define channels via `channels_enum`,
    /// it returns an empty vector. Use [channels_enum] to get the actual enum symbol.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The linked symbol table to resolve the channels enum.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` containing the names of the channels, or an empty vector if no
    /// channels enum is explicitly defined.
    ///
    /// # Panics
    ///
    /// * If the symbol table is not linked.
    pub fn channels(&self, symbol_table: &SymbolTable) -> Vec<String> {
        symbol_table.panic_if_not_linked();
        if self.channels_enum.is_none() {
            return Vec::new();
        }
        self.channels_enum(symbol_table).values()
    }

    /// Returns the name of the enum that defines the channels for this publication.
    ///
    /// This function supports a transition from the legacy `instances_enum` field to
    /// the new `channels_enum` field in the `(sdv.publication)` annotation.
    ///
    /// It first attempts to retrieve the enum name from `channels_enum`. If that
    /// field is not defined or is empty, it falls back to `instances_enum`.
    ///
    /// # Returns
    ///
    /// An `Option<String>` containing the name of the channels enum if either
    /// `channels_enum` or `instances_enum` is specified, otherwise `None`.
    pub fn channels_enum_name(&self) -> Option<String> {
        self.options
            .as_ref()
            .and_then(|options| vsidl_v1::annotations::exts::publication.get(options))
            .and_then(|p| {
                if !p.channels_enum.is_empty() {
                    Some(p.channels_enum.clone())
                } else if !p.instances_enum.is_empty() {
                    Some(p.instances_enum.clone())
                } else {
                    None
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use crate::proto_publication_symbol::ProtoPublicationSymbol;
    use crate::symbol_table_creator::SymbolTableCreator;
    use crate::symbol_table_linker::SymbolTableLinker;
    use crate::SymbolCommon;
    use crate::SymbolId;
    use crate::SymbolKind;
    use crate::SymbolTable;
    use vsidl_v1_common::constants::{UNIQUE_CHANNEL, UNIQUE_ENUM};
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;

    fn setup_test(fqn: &str) -> (SymbolTable, ProtoPublicationSymbol) {
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
            linker.symbol_table().resolve_proto_publication_fqn(fqn)[0].clone(),
        )
    }

    #[test]
    fn test_is_single_pub() {
        // GIVEN a valid symbol table
        let (_, proto_publication_symbol) = setup_test("sdv.test.BatteryLevel");

        // WHEN / THEN sdv.test.BatteryLevel is single pub
        assert!(proto_publication_symbol.is_single_pub());
        assert!(!proto_publication_symbol.is_multi_pub());
    }

    #[test]
    fn test_is_multi_pub() {
        // GIVEN a valid symbol table
        let (_, proto_publication_symbol) = setup_test("sdv.test.Speed");

        // WHEN / THEN sdv.test.Speed is multi pub
        assert!(proto_publication_symbol.is_multi_pub());
        assert!(!proto_publication_symbol.is_single_pub());
    }

    #[test]
    fn test_publishers() {
        // GIVEN a valid symbol table
        let (symbol_table, proto_publication_symbol) = setup_test("sdv.test.Pressure");

        // WHEN getting the publishers
        let publishers = proto_publication_symbol.publishers(&symbol_table);

        // THEN the publishers should be found
        assert!(publishers.len() == 2);
        let owner = publishers[0].owning_service_bundle(&symbol_table).full_name(&symbol_table);
        assert_eq!(owner, "sdv.test.ServiceBundleOne");
        let owner = publishers[1].owning_service_bundle(&symbol_table).full_name(&symbol_table);
        assert_eq!(owner, "sdv.test.ServiceBundleOne");
    }

    #[test]
    fn test_subscribers() {
        // GIVEN a valid symbol table
        let (symbol_table, proto_publication_symbol) = setup_test("sdv.test.Pressure");

        // WHEN getting the subscribers
        let subscribers = proto_publication_symbol.subscribers(&symbol_table);

        // THEN the subscriber should be found
        assert!(subscribers.len() == 1);
        let owner = subscribers[0].owning_service_bundle(&symbol_table).full_name(&symbol_table);
        assert_eq!(owner, "sdv.test.ServiceBundleTwo");
    }

    #[test]
    fn test_channels_enum() {
        // GIVEN a valid symbol table
        let (symbol_table, proto_publication_symbol) = setup_test("sdv.test.Pressure");

        // WHEN getting the channels enum
        let channels_enum = proto_publication_symbol.channels_enum(&symbol_table);

        // THEN the channels enum should be found
        assert_eq!(channels_enum.full_name(&symbol_table), "sdv.test.TestChannels");
    }

    #[test]
    fn test_channels_enum_empty() {
        // GIVEN a valid symbol table
        let (symbol_table, proto_publication_symbol) = setup_test("sdv.test.FuelLevel");

        // WHEN getting the channels enum
        let channels_enum = proto_publication_symbol.channels_enum(&symbol_table);

        // THEN the channels enum should be found
        assert_eq!(channels_enum.full_name(&symbol_table), UNIQUE_ENUM.to_string());
    }

    #[test]
    fn test_channels() {
        // GIVEN a valid symbol table
        let (symbol_table, proto_publication_symbol) = setup_test("sdv.test.Pressure");

        // WHEN getting the channels
        let channels: Vec<String> = proto_publication_symbol.channels(&symbol_table);

        // THEN the channels enum should be found
        assert_eq!(channels.len(), 2);
        assert!(channels.contains(&"ONE".to_string()));
        assert!(channels.contains(&"TWO".to_string()));
    }

    #[test]
    fn test_channels_empty() {
        // GIVEN a valid symbol table
        let (symbol_table, proto_publication_symbol) = setup_test("sdv.test.FuelLevel");

        // WHEN getting the channels
        let channels: Vec<String> = proto_publication_symbol.channels(&symbol_table);

        // THEN the channels enum should be found
        assert_eq!(channels.len(), 1);
        assert!(channels.contains(&UNIQUE_CHANNEL.to_string()));
    }

    #[test]
    #[should_panic]
    fn test_channels_broken() {
        // GIVEN an invalid symbol table (impossible via public APIs)
        let mut symbol_table = SymbolTable::default();
        symbol_table.set_state(crate::SymbolTableState::Linked);

        let channels_symbol_id = symbol_table.create_symbol(
            "TestSymbol".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon {
                id: SymbolId(999),
                ..Default::default()
            }),
        );

        let proto_publication_symbol = ProtoPublicationSymbol {
            channels_enum: Some(channels_symbol_id),
            ..Default::default()
        };

        // WHEN getting the channels
        let _ = proto_publication_symbol.channels_enum(&symbol_table);

        // THEN should_panic
    }

    #[test]
    fn test_channels_enum_name_is_filled() {
        // GIVEN a valid catalog with a publication that has a channels_enum
        let (_, proto_publication_symbol) = setup_test("sdv.test.BatteryLevel");

        // WHEN getting the channels_enum_name
        let channels_enum_name = proto_publication_symbol.channels_enum_name();

        // THEN the channels_enum_name should be found
        assert_eq!(channels_enum_name, Some("sdv.test.BatteryChannels".to_string()));
    }

    #[test]
    fn test_channels_enum_name_is_empty() {
        // GIVEN a valid catalog with a publication that has no channels_enum
        let (_, proto_publication_symbol) = setup_test("sdv.test.Speed");

        // WHEN getting the channels_enum_name
        let channels_enum_name = proto_publication_symbol.channels_enum_name();

        // THEN the channels_enum_name should be None
        assert_eq!(channels_enum_name, None);
    }
}
