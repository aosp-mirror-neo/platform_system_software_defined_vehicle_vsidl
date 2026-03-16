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
use protobuf::descriptor::EnumDescriptorProto;
use vsidl_v1::span::Span;
use vsidl_v1_common::ProtoMetadata;

/// Represents a Protobuf Enum definition within the symbol table.
///
/// This struct holds information about a specific enum defined in a `.proto` file,
/// including its AST node, common symbol properties, and metadata about the
/// originating `.proto` file. It allows accessing the underlying
/// [EnumDescriptorProto] AST node via `Deref`.
#[derive(Default, Debug, Clone)]
pub struct ProtoEnumSymbol {
    /// The original Protobuf AST node ([EnumDescriptorProto]) this symbol was created from.
    pub ast_node: EnumDescriptorProto,
    /// Common information shared across all symbol types (like ID, name, scope).
    pub symbol: SymbolCommon,
    // Doc string in front of the definition of the symbol in the Protobuf file
    pub doc_string: String,
    /// Metadata related to the `.proto` file where this enum is defined.
    pub proto_metadata: ProtoMetadata,
}
// Implements Deref for the ast_node field, allowing direct access to the DescriptorProto fields.
implement_ast_node_deref!(ProtoEnumSymbol, EnumDescriptorProto);

impl ProtoEnumSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();

    /// Returns a vector containing the names of all values defined within this enum.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` where each string is the name of an enum value.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Assuming `enum_symbol` represents an enum like:
    /// // enum MyEnum {
    /// //   UNKNOWN = 0;
    /// //   VALUE_A = 1;
    /// //   VALUE_B = 2;
    /// // }
    /// let values = enum_symbol.values();
    /// assert_eq!(values, vec!["UNKNOWN", "VALUE_A", "VALUE_B"]);
    /// ```
    pub fn values(&self) -> Vec<String> {
        self.ast_node.value.iter().filter_map(|value| value.name.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::proto_enum_symbol::ProtoEnumSymbol;
    use crate::symbol_table_creator::SymbolTableCreator;
    use crate::symbol_table_linker::SymbolTableLinker;
    use vsidl_v1_common::constants::{UNIQUE_CHANNEL, UNIQUE_ENUM};
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;

    fn setup_test(fqn: &str) -> ProtoEnumSymbol {
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
        linker.symbol_table().resolve_proto_enum_fqn(fqn)[0].clone()
    }

    #[test]
    fn test_values() {
        // GIVEN a valid symbol table
        let enum_symbol = setup_test("sdv.test.TestChannels");

        // WHEN retrieving values
        let result = enum_symbol.values();

        // THEN values match the values in the .proto file
        assert_eq!(result, vec!["ONE", "TWO"])
    }

    #[test]
    fn test_values_unique() {
        // GIVEN a valid symbol table
        let enum_symbol = setup_test(UNIQUE_ENUM);

        // WHEN retrieving values
        let result = enum_symbol.values();

        // THEN values match the values in the .proto file
        assert_eq!(result, vec![UNIQUE_CHANNEL])
    }
}
