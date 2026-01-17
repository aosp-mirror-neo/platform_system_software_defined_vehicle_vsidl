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
    scope::{Scope, ScopeId},
    specific_symbols::{
        proto_enum_symbol::ProtoEnumSymbol, proto_message_symbol::ProtoMessageSymbol,
        proto_publication_symbol::ProtoPublicationSymbol,
        proto_rpc_service_symbol::ProtoRpcServiceSymbol, publisher_symbol::PublisherSymbol,
        rpc_client_symbol::RpcClientSymbol, rpc_server_symbol::RpcServerSymbol,
        service_bundle_symbol::ServiceBundleSymbol, subscriber_symbol::SubscriberSymbol,
    },
    symbol_table::SymbolTable,
};
use vsidl_v1::syntax::Span;

/// A unique identifier for a symbol within a [SymbolTable].
///
/// It wraps a `usize` index used internally by the [SymbolTable].
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolId(pub usize);

// Since we have an empty channel for testing, disable the check for size differences
#[allow(clippy::large_enum_variant)]
/// Represents the specific kind of a symbol defined in VSIDL or Protobuf.
///
/// This enum encompasses all possible named entities that can be represented
/// in the [SymbolTable]. It allows distinguishing between different types of
/// symbols that might share the same name (e.g., a service bundle and a unit type
/// both named `com.example.MyThing`). Each channel holds the specific data
/// associated with that kind of symbol, along with common symbol information
/// managed by [SymbolCommon].
#[derive(Debug, Clone)]
pub enum SymbolKind {
    /// A placeholder channel used exclusively for testing purposes.
    /// It contains only the [SymbolCommon] data without specific symbol details.
    PlaceholderForTests(SymbolCommon),

    // ---- VSIDL specific symbols ----
    /// Represents a VSIDL service bundle definition. See [ServiceBundleSymbol].
    ServiceBundle(ServiceBundleSymbol),
    /// Represents a VSIDL publisher definition within a service bundle. See [PublisherSymbol].
    Publisher(PublisherSymbol),
    /// Represents a VSIDL subscriber definition within a service bundle. See [SubscriberSymbol].
    Subscriber(SubscriberSymbol),
    /// Represents a VSIDL RPC server definition within a service bundle. See [RpcServerSymbol].
    RpcServer(RpcServerSymbol),
    /// Represents a VSIDL RPC client definition within a service bundle. See [RpcClientSymbol].
    RpcClient(RpcClientSymbol),

    // ---- PROTO specific symbols (derived from Protobuf definitions) ----
    /// Represents a Protobuf message definition. See [ProtoMessageSymbol].
    ProtoMessage(ProtoMessageSymbol),
    /// Represents a Protobuf enum definition. See [ProtoEnumSymbol].
    ProtoEnum(ProtoEnumSymbol),
    /// Represents a Protobuf message used as a publication type. See [ProtoPublicationSymbol].
    ProtoPublication(ProtoPublicationSymbol),
    /// Represents a Protobuf RPC service definition. See [ProtoRpcServiceSymbol].
    ProtoRpcService(ProtoRpcServiceSymbol),
}

/// Macro to implement common accessor methods for all channels of [SymbolKind].
///
/// This macro generates boilerplate code to delegate calls for common properties
/// (like `id`, `name`, `enclosing_scope`, etc.) to the underlying [SymbolCommon]
/// struct contained within each [SymbolKind] channel.
///
/// # Parameters
///
/// * `$channel`: The identifier of the [SymbolKind] channel (e.g., `ServiceBundle`).
/// * `$symbol_type`: The specific symbol struct type associated with the channel (e.g., [ServiceBundleSymbol]).
macro_rules! implement_symbol_kind_methods {
    (
        $(
            ($channel:ident, $symbol_type:ty)
        ),*
    ) => {
        impl SymbolKind {
            /// Returns a clone of the common symbol information ([SymbolCommon]).
            ///
            /// This provides access to shared properties like ID, name, and scope information,
            /// regardless of the specific symbol kind.
            pub fn symbol(&self) -> SymbolCommon {
                match self {
                    SymbolKind::PlaceholderForTests(symbol) => symbol.clone(),
                    $(
                        SymbolKind::$channel(symbol) => symbol.symbol.clone(),
                    )*
                }
            }

            /// Returns a clone of the symbol's unique identifier ([SymbolId]).
            pub fn id(&self) -> SymbolId {
                match self {
                    SymbolKind::PlaceholderForTests(symbol) => symbol.id.clone(),
                    $(
                        SymbolKind::$channel(symbol) => symbol.symbol.id.clone(),
                    )*
                }
            }

            /// Returns a clone of the symbol's unqualified name.
            pub fn name(&self) -> String {
                match self {
                    SymbolKind::PlaceholderForTests(symbol) => symbol.name.clone(),
                    $(
                        SymbolKind::$channel(symbol) => symbol.symbol.name.clone(),
                    )*
                }
            }

            /// Returns the fully-qualified name (FQN) of the symbol.
            ///
            /// The FQN is constructed by prepending the package name (derived from enclosing scopes)
            /// to the unqualified name, separated by dots.
            ///
            /// # Parameters
            ///
            /// * `symbol_table`: A reference to the [SymbolTable] needed to look up scope information.
            pub fn full_name(&self, symbol_table: &crate::SymbolTable) -> String {
                match self {
                    SymbolKind::PlaceholderForTests(symbol) => symbol.full_name(symbol_table).clone(),
                    $(
                        SymbolKind::$channel(symbol) => symbol.symbol.full_name(symbol_table).clone(),
                    )*
                }
            }

            /// Returns a clone of the [Scope] in which this symbol is defined.
            ///
            /// # Parameters
            ///
            /// * `symbol_table`: A reference to the [SymbolTable] needed to look up the scope.
            ///
            /// # Panics
            ///
            /// Panics if the enclosing scope ID stored in the symbol does not correspond to a valid
            /// scope in the provided `symbol_table`. This indicates an internal inconsistency.
            pub fn enclosing_scope(&self, symbol_table: &crate::SymbolTable) -> crate::Scope {
                match self {
                    SymbolKind::PlaceholderForTests(symbol) => symbol.enclosing_scope(symbol_table).clone(),
                    $(
                        SymbolKind::$channel(symbol) => symbol.symbol.enclosing_scope(symbol_table).clone(),
                    )*
                }
            }

            /// Returns an `Option` containing a clone of the [ScopeId] spanned by this symbol, if any.
            ///
            /// A "spanned scope" is a scope that is directly represented or defined by this symbol.
            /// For example, a [ServiceBundleSymbol] spans the scope containing its members.
            /// Returns `None` if this symbol does not span a scope.
            pub fn spanned_scope(&self) -> Option<crate::ScopeId> {
                match self {
                    SymbolKind::PlaceholderForTests(symbol) => symbol.spanned_scope.clone(),
                    $(
                        SymbolKind::$channel(symbol) => symbol.symbol.spanned_scope.clone(),
                    )*
                }
            }

            /// Sets the enclosing scope for this symbol.
            ///
            /// This is intended for internal use during symbol table construction.
            ///
            /// # Parameters
            ///
            /// * `enclosing_scope`: The [ScopeId] of the scope where this symbol is defined.
            pub(crate) fn set_enclosing_scope(&mut self, enclosing_scope: crate::ScopeId) {
                match self {
                    SymbolKind::PlaceholderForTests(symbol) => symbol.enclosing_scope = enclosing_scope,
                    $(
                        SymbolKind::$channel(symbol) => symbol.symbol.enclosing_scope = enclosing_scope,
                    )*
                }
            }

            /// Sets or clears the spanned scope for this symbol.
            ///
            /// This is intended for internal use during symbol table construction or linking.
            ///
            /// # Parameters
            ///
            /// * `spanned_scope`: An `Option` containing the [ScopeId] of the scope spanned by this
            ///   symbol, or `None` if it doesn't span a scope.
            pub(crate) fn set_spanned_scope(&mut self, spanned_scope: Option<crate::ScopeId>) {
                match self {
                    SymbolKind::PlaceholderForTests(symbol) => symbol.spanned_scope = spanned_scope,
                    $(
                        SymbolKind::$channel(symbol) => symbol.symbol.spanned_scope = spanned_scope,
                    )*
                }
            }
        }
    };
}

// Apply the macro to generate methods for all SymbolKind channels
implement_symbol_kind_methods!(
    (ServiceBundle, ServiceBundleSymbol),
    (Publisher, PublisherSymbol),
    (Subscriber, SubscriberSymbol),
    (RpcServer, RpcServerSymbol),
    (RpcClient, RpcClientSymbol),
    (ProtoMessage, ProtoMessageSymbol),
    (ProtoEnum, ProtoEnumSymbol),
    (ProtoPublication, ProtoPublicationSymbol),
    (ProtoRpcService, ProtoRpcServiceSymbol)
);

/// Contains common information shared by all symbol kinds in the [SymbolTable].
///
/// This struct holds fundamental properties like the symbol's unique ID, its name,
/// and references to its defining (enclosing) scope and potentially the scope it
/// represents (spanned scope). Specific symbol types ([SymbolKind] channels) embed
/// this struct and add their own unique data.
#[derive(Debug, Clone, Default)]
pub struct SymbolCommon {
    /// Unique identifier ([SymbolId]) for this symbol within its [SymbolTable].
    pub id: SymbolId,
    /// The unqualified name of the symbol (e.g., "MyService", "Temperature").
    pub name: String,
    /// The location of the symbol in the source file.
    pub span: Option<Span>,
    /// The [ScopeId] of the [Scope] where this symbol is directly defined.
    pub enclosing_scope: ScopeId,
    /// An optional [ScopeId] pointing to the [Scope] that this symbol represents or defines.
    /// For example, a [ServiceBundleSymbol] would have its `spanned_scope` set to the ID
    /// of the scope containing the bundle's members. `None` if the symbol doesn't span a scope.
    pub spanned_scope: Option<ScopeId>,
}

impl SymbolCommon {
    /// Returns the package name of the symbol.
    ///
    /// The package name is derived from the fully-qualified name of the symbol's
    /// enclosing scope.
    ///
    /// # Parameters
    ///
    /// * `symbol_table`: A reference to the [SymbolTable] needed to look up scope information.
    ///
    /// # Returns
    ///
    /// A `String` representing the package name (e.g., "com.google.sdv"). Returns an empty
    /// string if the symbol is defined in the global scope.
    pub fn package(&self, symbol_table: &SymbolTable) -> String {
        self.enclosing_scope(symbol_table).full_name(symbol_table)
    }

    /// Returns the fully-qualified name (FQN) of the symbol.
    ///
    /// Constructs the FQN by combining the package name (from the enclosing scope)
    /// and the symbol's unqualified name. If the symbol is in the global scope (empty package name),
    /// the FQN is just the unqualified name.
    ///
    /// # Parameters
    ///
    /// * `symbol_table`: A reference to the [SymbolTable] needed to look up scope information.
    ///
    /// # Returns
    ///
    /// A `String` representing the fully-qualified name (e.g., "com.google.sdv.MyService").
    pub fn full_name(&self, symbol_table: &SymbolTable) -> String {
        let package_name = self.package(symbol_table);
        if package_name.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", package_name, self.name)
        }
    }

    /// Returns a clone of the [Scope] in which this symbol was defined.
    ///
    /// # Parameters
    ///
    /// * `symbol_table`: A reference to the [SymbolTable] needed to look up the scope.
    ///
    /// # Returns
    ///
    /// A clone of the enclosing [Scope].
    ///
    /// # Panics
    ///
    /// Panics if the `enclosing_scope` ID stored within the symbol does not correspond
    /// to a valid scope in the `symbol_table`. This indicates an internal inconsistency.
    pub fn enclosing_scope(&self, symbol_table: &SymbolTable) -> Scope {
        // Unwrap is safe assuming the symbol table is consistent (creator/linker ensures this)
        symbol_table.get_scope(&self.enclosing_scope).unwrap().clone()
    }
}

/// Macro to delegate common symbol accessors ([id], [name], [full_name], [package], [enclosing_scope])
/// from a specific symbol struct (e.g., [PublisherSymbol]) to its embedded [SymbolCommon] field.
///
/// This avoids redundant implementations in each specific symbol struct.
#[macro_export]
macro_rules! implement_symbol_deref {
    () => {
        /// Returns a clone of the symbol's unique identifier ([SymbolId]).
        #[inline]
        pub fn id(&self) -> $crate::SymbolId {
            self.symbol.id.clone()
        }

        /// Returns a clone of the symbol's unqualified name.
        #[inline]
        pub fn name(&self) -> String {
            self.symbol.name.clone()
        }

        /// Returns a clone of the symbol's span.
        #[inline]
        pub fn span(&self) -> Option<Span> {
            self.symbol.span.clone()
        }

        /// Returns the fully-qualified name (FQN) of the symbol.
        /// See [SymbolCommon::full_name] for details.
        ///
        /// # Parameters
        /// * `symbol_table`: A reference to the [SymbolTable].
        #[inline]
        pub fn full_name(&self, symbol_table: &$crate::SymbolTable) -> String {
            self.symbol.full_name(symbol_table)
        }

        /// Returns the package name of the symbol.
        /// See [SymbolCommon::package] for details.
        ///
        /// # Parameters
        /// * `symbol_table`: A reference to the [SymbolTable].
        #[inline]
        pub fn package(&self, symbol_table: &$crate::SymbolTable) -> String {
            self.symbol.package(symbol_table)
        }

        /// Returns a clone of the [Scope] in which this symbol was defined.
        /// See [SymbolCommon::enclosing_scope] for details.
        ///
        /// # Parameters
        /// * `symbol_table`: A reference to the [SymbolTable].
        ///
        /// # Panics
        /// Panics if the underlying scope ID is invalid in the `symbol_table`.
        #[inline]
        pub fn enclosing_scope(&self, symbol_table: &$crate::SymbolTable) -> $crate::Scope {
            self.symbol.enclosing_scope(symbol_table).clone()
        }
    };
}

/// Macro to implement [Deref] for a specific symbol struct, allowing direct access
/// to the fields of its associated AST node.
///
/// This provides convenient syntax, making the symbol struct behave like a smart pointer
/// to the underlying AST node it represents.
///
/// # Parameters
///
/// * `$symbol`: The specific symbol struct type (e.g., [PublisherSymbol]).
/// * `$ast_node`: The corresponding AST node type (e.g., `ast::Publisher`).
#[macro_export]
macro_rules! implement_ast_node_deref {
    ($symbol:ty, $ast_node:ty) => {
        /// Implements [std::ops::Deref] to allow direct access to the fields of the underlying AST node.
        impl std::ops::Deref for $symbol {
            type Target = $ast_node;

            /// Dereferences the symbol to its associated AST node.
            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.ast_node
            }
        }
    };
}

#[cfg(test)]
mod test {
    use super::*;

    // Define a placeholder struct that uses the implement_symbol_deref macro
    #[derive(Debug, Clone, Default)]
    struct TestSymbol {
        pub symbol: SymbolCommon,
    }

    impl TestSymbol {
        implement_symbol_deref!();
    }

    fn setup_symbol_table_for_deref_test() -> (SymbolTable, SymbolId, SymbolId, ScopeId) {
        let mut symbol_table = SymbolTable::default();
        let global_scope_id = symbol_table.create_scope("".to_string());
        let com_scope_id = symbol_table.create_scope("com".to_string());
        let google_scope_id = symbol_table.create_scope("google".to_string());

        symbol_table.add_subscope(&global_scope_id, &com_scope_id);
        symbol_table.add_subscope(&com_scope_id, &google_scope_id);

        let span = Span {
            file_path: "/this/is/a/test".to_string(),
            start: 10,
            end: 20,
            special_fields: Default::default(),
        };

        let symbol_common = SymbolCommon {
            id: SymbolId(1),
            name: "MyTestSymbol".to_string(),
            enclosing_scope: google_scope_id.clone(),
            spanned_scope: None,
            span: Some(span.clone()),
        };

        let symbol_common2 = SymbolCommon {
            id: SymbolId(2),
            name: "MyTestSymbol2".to_string(),
            enclosing_scope: google_scope_id.clone(),
            spanned_scope: None,
            span: Some(span.clone()),
        };

        let symbol_id = symbol_table.create_symbol(
            symbol_common.name.clone(),
            Some(span.clone()),
            SymbolKind::PlaceholderForTests(symbol_common.clone()),
        );

        let symbol_id2 = symbol_table.create_symbol(
            symbol_common2.name.clone(),
            Some(span),
            SymbolKind::Subscriber(SubscriberSymbol {
                symbol: symbol_common2.clone(),
                ast_node: vsidl_v1::syntax::service_bundle::Subscriber {
                    type_: "MyTestSymbol2".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );

        // Manually update the ID and enclosing scope of the created symbol
        // as create_symbol assigns a new ID and default scope.
        if let Some(SymbolKind::PlaceholderForTests(s)) = symbol_table.get_symbol_mut(&symbol_id) {
            s.id = symbol_common.id.clone();
            s.enclosing_scope = symbol_common.enclosing_scope.clone();
        }
        symbol_table.add_symbol_to_scope(&google_scope_id, &symbol_id);

        if let Some(SymbolKind::Subscriber(subscriber)) = symbol_table.get_symbol_mut(&symbol_id2) {
            subscriber.symbol.id = symbol_common2.id.clone();
            subscriber.symbol.enclosing_scope = symbol_common2.enclosing_scope.clone();
        }
        symbol_table.add_symbol_to_scope(&google_scope_id, &symbol_id2);

        (symbol_table, symbol_id, symbol_id2, google_scope_id)
    }

    #[test]
    fn test_implement_symbol_deref_accessors() {
        let (symbol_table, symbol_id, _, scope_id) = setup_symbol_table_for_deref_test();
        let symbol_kind = symbol_table.get_symbol(&symbol_id).unwrap();
        let symbol_common = match symbol_kind {
            SymbolKind::PlaceholderForTests(s) => s.clone(),
            _ => panic!("Expected PlaceholderForTests"),
        };

        let test_symbol = TestSymbol { symbol: symbol_common };

        // Test id()
        assert_eq!(test_symbol.id(), SymbolId(1));

        // Test name()
        assert_eq!(test_symbol.name(), "MyTestSymbol");

        // Test span()
        assert!(test_symbol.span().is_some());
        assert_eq!(test_symbol.span().unwrap().start, 10);
        assert_eq!(test_symbol.span().unwrap().end, 20);

        // Test full_name()
        assert_eq!(test_symbol.full_name(&symbol_table), "com.google.MyTestSymbol");

        // Test package()
        assert_eq!(test_symbol.package(&symbol_table), "com.google");

        // Test enclosing_scope()
        let retrieved_scope = test_symbol.enclosing_scope(&symbol_table);
        assert_eq!(retrieved_scope.id, scope_id);
    }

    #[test]
    fn test_implement_symbol_kind_methods() {
        let (symbol_table, symbol_id, symbol_id2, scope_id) = setup_symbol_table_for_deref_test();
        let symbol_kind = symbol_table.get_symbol(&symbol_id).unwrap();
        let symbol_kind2 = symbol_table.get_symbol(&symbol_id2).unwrap();

        // Test symbol()
        let common_from_kind = symbol_kind.symbol();
        assert_eq!(common_from_kind.id, SymbolId(1));
        assert_eq!(common_from_kind.name, "MyTestSymbol");
        assert_eq!(common_from_kind.enclosing_scope, scope_id);
        assert_eq!(common_from_kind.spanned_scope, None);

        let common_from_kind = symbol_kind2.symbol();
        assert_eq!(common_from_kind.id, SymbolId(2));
        assert_eq!(common_from_kind.name, "MyTestSymbol2");
        assert_eq!(common_from_kind.enclosing_scope, scope_id);
        assert_eq!(common_from_kind.spanned_scope, None);

        // Test id()
        assert_eq!(symbol_kind.id(), SymbolId(1));
        assert_eq!(symbol_kind2.id(), SymbolId(2));

        // Test name()
        assert_eq!(symbol_kind.name(), "MyTestSymbol");
        assert_eq!(symbol_kind2.name(), "MyTestSymbol2");

        // Test full_name()
        assert_eq!(symbol_kind.full_name(&symbol_table), "com.google.MyTestSymbol");
        assert_eq!(symbol_kind2.full_name(&symbol_table), "com.google.MyTestSymbol2");

        // Test enclosing_scope()
        let retrieved_scope = symbol_kind.enclosing_scope(&symbol_table);
        assert_eq!(retrieved_scope.id, scope_id);
        let retrieved_scope = symbol_kind2.enclosing_scope(&symbol_table);
        assert_eq!(retrieved_scope.id, scope_id);

        // Test spanned_scope()
        assert_eq!(symbol_kind.spanned_scope(), None);
        assert_eq!(symbol_kind2.spanned_scope(), None);

        // Test set_enclosing_scope()
        {
            let (mut symbol_table_mut, symbol_id_for_test, _, _) =
                setup_symbol_table_for_deref_test();
            let new_scope_id = symbol_table_mut.create_scope("new_scope".to_string());
            let symbol_kind_mut = symbol_table_mut.get_symbol_mut(&symbol_id_for_test).unwrap();
            symbol_kind_mut.set_enclosing_scope(new_scope_id.clone());
            // Re-fetch the symbol immutably to check the change, releasing the mutable borrow
            let updated_symbol_kind = symbol_table_mut.get_symbol(&symbol_id_for_test).unwrap();
            assert_eq!(updated_symbol_kind.enclosing_scope(&symbol_table_mut).id, new_scope_id);
        }

        // Test set_spanned_scope()
        {
            let (mut symbol_table_mut, symbol_id_for_test, _, _) =
                setup_symbol_table_for_deref_test();
            let spanned_scope_id = symbol_table_mut.create_scope("spanned_scope".to_string());
            let symbol_kind_mut = symbol_table_mut.get_symbol_mut(&symbol_id_for_test).unwrap();
            symbol_kind_mut.set_spanned_scope(Some(spanned_scope_id.clone()));
            // Re-fetch the symbol immutably
            let updated_symbol_kind = symbol_table_mut.get_symbol(&symbol_id_for_test).unwrap();
            assert_eq!(updated_symbol_kind.spanned_scope(), Some(spanned_scope_id));
            let symbol_kind_mut_again =
                symbol_table_mut.get_symbol_mut(&symbol_id_for_test).unwrap();
            symbol_kind_mut_again.set_spanned_scope(None);
            let updated_symbol_kind_none =
                symbol_table_mut.get_symbol(&symbol_id_for_test).unwrap();
            assert_eq!(updated_symbol_kind_none.spanned_scope(), None);
        }
    }

    #[test]
    fn test_full_name_symbol_in_global_scope() {
        let mut symbol_table = SymbolTable::default();
        let global_scope_id = symbol_table.create_scope("".to_string());
        let symbol_id = symbol_table.create_symbol(
            "MySymbol".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        symbol_table.add_symbol_to_scope(&global_scope_id, &symbol_id);
        let symbol = symbol_table.get_symbol(&symbol_id).unwrap();
        assert_eq!(symbol.full_name(&symbol_table), "MySymbol");
    }

    #[test]
    fn test_full_name_symbol_in_nested_scopes() {
        // GIVEN
        let mut symbol_table = SymbolTable::default();
        let global_scope_id = symbol_table.create_scope("".to_string());
        let com_scope_id = symbol_table.create_scope("com".to_string());
        let google_scope_id = symbol_table.create_scope("google".to_string());
        let sdv_scope_id = symbol_table.create_scope("sdv".to_string());

        symbol_table.add_subscope(&global_scope_id, &com_scope_id);
        symbol_table.add_subscope(&com_scope_id, &google_scope_id);
        symbol_table.add_subscope(&google_scope_id, &sdv_scope_id);

        let symbol_id = symbol_table.create_symbol(
            "Monitor".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        symbol_table.add_symbol_to_scope(&sdv_scope_id, &symbol_id);
        let symbol = symbol_table.get_symbol(&symbol_id).unwrap();

        // WHEN
        let result = symbol.full_name(&symbol_table);

        // THEN
        assert_eq!(result, "com.google.sdv.Monitor");
    }

    #[test]
    fn test_full_name_symbol_with_empty_scope_name() {
        // GIVEN
        let mut symbol_table = SymbolTable::default();
        let global_scope_id = symbol_table.create_scope("".to_string());
        let empty_scope_id = symbol_table.create_scope("".to_string());
        let sdv_scope_id = symbol_table.create_scope("sdv".to_string());

        symbol_table.add_subscope(&global_scope_id, &empty_scope_id);
        symbol_table.add_subscope(&empty_scope_id, &sdv_scope_id);

        let symbol_id = symbol_table.create_symbol(
            "Monitor".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        symbol_table.add_symbol_to_scope(&sdv_scope_id, &symbol_id);
        let symbol = symbol_table.get_symbol(&symbol_id).unwrap();

        // WHEN
        let result = symbol.full_name(&symbol_table);

        // THEN
        assert_eq!(result, "sdv.Monitor");
    }
}
