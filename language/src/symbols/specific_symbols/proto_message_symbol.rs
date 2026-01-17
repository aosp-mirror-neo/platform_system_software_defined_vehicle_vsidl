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
use protobuf::descriptor::DescriptorProto;
use vsidl_v1::syntax::Span;
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
}
// Implements Deref for the ast_node field, allowing direct access to the DescriptorProto fields.
implement_ast_node_deref!(ProtoMessageSymbol, DescriptorProto);

impl ProtoMessageSymbol {
    // This macro implements accessors for SymbolCommon, allowing access to its fields directly.
    implement_symbol_deref!();
}
