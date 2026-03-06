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
    proto_enum_symbol::ProtoEnumSymbol, proto_message_symbol::ProtoMessageSymbol,
    proto_publication_symbol::ProtoPublicationSymbol,
    proto_rpc_service_symbol::ProtoRpcServiceSymbol, publisher_symbol::PublisherSymbol,
    rpc_channel_symbol::RpcChannelSymbol, rpc_client_symbol::RpcClientSymbol,
    rpc_server_symbol::RpcServerSymbol, subscriber_symbol::SubscriberSymbol,
    topic_symbol::TopicSymbol, SymbolCommon,
};

use super::{
    scope::{Scope, ScopeId},
    specific_symbols::service_bundle_symbol::ServiceBundleSymbol,
    symbol::{SymbolId, SymbolKind},
    symbol_table_checker::SymbolTableChecker,
};
use indexmap::IndexMap;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use vsidl_v1::syntax::Span;

/// Represents the state of the symbol table during its construction process.
///
/// This enum tracks whether the symbol table has been created and whether
/// symbols within it have been linked (resolved).
#[derive(Default, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SymbolTableState {
    /// The initial state before the [SymbolTableCreator] has run.
    #[default]
    NotCreated,
    /// The state after the [SymbolTableCreator] has run but before the [SymbolTableLinker].
    /// Basic symbols and scopes exist, but references might not be resolved.
    CreatedNotLinked,
    /// The final state after the [SymbolTableLinker] has successfully run.
    /// All symbols and references should be resolved and validated.
    Linked,
}

/// Represents the target API level for code generation.
#[derive(Default, Clone, Eq, PartialEq, Hash, Debug)]
pub enum TargetApi {
    /// Target PDK29.
    #[default]
    Pdk29,
    /// Target the latest API version.
    Latest,
}

/// The fixed [ScopeId] for the global scope, which is the root of the scope hierarchy.
pub(crate) const GLOBAL_SCOPE_ID: ScopeId = ScopeId(0);

/// The central data structure for managing symbols and scopes in VSIDL.
///
/// It provides mechanisms for creating, storing, and resolving named entities (symbols)
/// within a hierarchical structure of scopes. Scopes typically correspond to namespaces
/// or packages defined in the input `.vsidl` and `.proto` files.
///
/// The symbol table is built in stages, tracked by [SymbolTableState].
#[derive(Default, Clone)]
pub struct SymbolTable {
    /// Stores all scopes, mapped by their unique [ScopeId].
    scopes: IndexMap<ScopeId, Scope>,
    /// Stores mappings from fully qualified names (FQN) to the [SymbolId]s that have that name.
    /// A single FQN might map to multiple symbols if names overlap (e.g., in different files).
    symbols_fqn: IndexMap<String, Vec<SymbolId>>,
    /// Stores all symbols, mapped by their unique [SymbolId].
    symbols: IndexMap<SymbolId, SymbolKind>,
    /// Symbols that still need to be linked. Enables incremental symbol table linking.
    unlinked_symbols: HashSet<SymbolId>,
    /// Stores a mapping from a location (file path and start offset) to a symbol ID.
    pub location_to_symbol: HashMap<PathBuf, BTreeMap<(usize, usize), SymbolId>>,
    /// The next available ID to assign to a new scope.
    next_scope_id: usize,
    /// The next available ID to assign to a new symbol.
    next_symbol_id: usize,
    /// The current construction state of the symbol table. See [SymbolTableState].
    state: SymbolTableState,
    /// The target API level for code generation.
    pub(crate) target_api: TargetApi,
}

/// Macro to generate a function that retrieves all symbols of a specific kind.
///
/// # Parameters
///
/// * `$func_name`: The identifier for the generated function.
/// * `$symbol_kind`: The [SymbolKind] channel to filter for (e.g., [SymbolKind::ServiceBundle]).
/// * `$symbol_type`: The specific symbol struct type to return (e.g., [ServiceBundleSymbol]).
macro_rules! get_all_symbols {
    ($func_name:ident, $symbol_kind:path, $symbol_type:ty) => {
        /// Returns a vector containing all symbols of a specific kind present in the table.
        ///
        /// This function iterates through all symbols and collects references to those
        /// matching the specified [$symbol_type] channel.
        pub fn $func_name(&self) -> Vec<&$symbol_type> {
            self.symbols
                .values()
                .filter_map(|symbol| match symbol {
                    $symbol_kind(specific_symbol) => Some(specific_symbol),
                    _ => None,
                })
                .collect()
        }
    };
}

/// Macro to generate a function that retrieves a specific symbol type by its ID.
///
/// # Parameters
///
/// * `$func_name`: The identifier for the generated function.
/// * `$symbol_kind`: The [SymbolKind] channel expected for the given ID.
/// * `$symbol_type`: The specific symbol struct type to return.
///
/// # Panics
///
/// Panics if the symbol with the given ID does not exist or is not of the expected [SymbolKind].
macro_rules! get_symbol {
    ($func_name:ident, $symbol_kind:path, $symbol_type:ty) => {
        /// Returns an immutable reference to a symbol of a specific kind, identified by its [SymbolId].
        ///
        /// Retrieves the symbol using the general [SymbolTable::get_symbol] method and then
        /// attempts to downcast it to the specified type.
        ///
        /// # Parameters
        ///
        /// * `id`: The [SymbolId] of the symbol to retrieve.
        ///
        /// # Returns
        ///
        /// An `Option` containing a reference to the symbol if found and of the correct type,
        /// otherwise `None`.
        ///
        /// # Panics
        ///
        /// Panics if the symbol associated with the `id` exists but is not of the expected type
        /// specified by `$symbol_kind`. This indicates an internal logic error or incorrect usage.
        pub fn $func_name(&self, id: &SymbolId) -> Option<&$symbol_type> {
            let opt_symbol_kind = self.get_symbol(id);
            match opt_symbol_kind {
                Some($symbol_kind(inner)) => Some(inner),
                _ => panic!(
                    "Tried to get symbol '{:?}' (with id {}) as {}",
                    opt_symbol_kind,
                    id.0,
                    stringify!($symbol_kind)
                ),
            }
        }
    };
}

/// Macro to generate a function that resolves symbols of a specific kind by their fully qualified name (FQN).
///
/// # Parameters
///
/// * `$func_name`: The identifier for the generated function.
/// * `$symbol_kind`: The [SymbolKind] to filter the resolved symbols by.
/// * `$symbol_type`: The specific symbol struct type to return in the resulting vector.
macro_rules! implement_fqn_resolver {
    ($func_name:ident, $symbol_kind:path, $symbol_type:ty) => {
        /// Resolves symbols of a specific kind by their fully qualified name (FQN).
        ///
        /// This function uses the general [SymbolTable::resolve] method starting from the
        /// global scope and then filters the results to include only symbols of the specified kind.
        ///
        /// # Parameters
        ///
        /// * `fqn`: The fully qualified name to resolve, convertible into a [String].
        ///
        /// # Returns
        ///
        /// A vector containing references to all symbols of the specified type that match the FQN.
        /// The vector may be empty if no matching symbols are found.
        pub fn $func_name(&self, fqn: impl Into<String>) -> Vec<&$symbol_type> {
            self.resolve(&GLOBAL_SCOPE_ID, &fqn.into(), false)
                .into_iter()
                .filter_map(|symbol| match symbol {
                    $symbol_kind(specific_symbol) => Some(specific_symbol),
                    _ => None,
                })
                .collect()
        }
    };
}

impl SymbolTable {
    /// Sets the construction state of the symbol table.
    ///
    /// This is typically called by [SymbolTableCreator] and [SymbolTableLinker]
    /// to update the state as the table is built. Intended for internal use within the crate.
    ///
    /// # Parameters
    ///
    /// * `state`: The new [SymbolTableState] to set.
    pub(super) fn set_state(&mut self, state: SymbolTableState) {
        if state == SymbolTableState::Linked {
            self.unlinked_symbols = HashSet::new();
        }
        self.state = state;
    }

    /// Returns the current construction state of the symbol table.
    ///
    /// # Returns
    ///
    /// The current [SymbolTableState].
    pub fn get_state(&self) -> SymbolTableState {
        self.state.clone()
    }

    /// Sets the target API level for code generation.
    pub fn set_target_api(&mut self, target_api: TargetApi) {
        self.target_api = target_api;
    }

    /// Returns the symbols of the symbol table that still need to be linked.
    ///
    /// # Returns
    ///
    /// The set of symbols that still need to be linked.
    pub(crate) fn get_unlinked_symbols(&self) -> &HashSet<SymbolId> {
        &self.unlinked_symbols
    }

    /// Panics if the symbol table is not in the [SymbolTableState::Linked] state.
    ///
    /// This is used as a safeguard in methods that require a fully constructed and linked
    /// symbol table to function correctly.
    ///
    /// # Panics
    ///
    /// Panics if `self.state` is not equal to [SymbolTableState::Linked].
    pub fn panic_if_not_linked(&self) {
        if self.state != SymbolTableState::Linked {
            panic!("Symbol table is used without being linked.");
        }
    }

    /// True if this symbol table was created from a catalog using PDK29 syntax
    ///
    /// PDK29 uses Protobuf annotations for creating channels.
    /// Example:
    /// ```protobuf
    /// message TirePressure {
    ///   option (.sdv.vsidl.v1.publication) = {
    ///     instances_enum: "Location"
    ///     message_count: 8
    ///     model: SINGLE_PUB
    ///   };
    ///
    ///   uint32 pressure = 1;
    /// }
    /// ```
    ///
    /// Releases after PDK29 define topics inline in publishers by setting metadata.
    /// Example:
    /// ```vsidl
    /// service_bundle {
    ///   name: "Monitor"
    ///   publisher {
    ///     message: "TirePressure"
    ///     topic: "my-channel"
    ///     capacity: 8
    ///   }
    /// }
    /// ```
    pub fn uses_pdk29(&self) -> bool {
        self.target_api == TargetApi::Pdk29
    }

    /// Creates a new, empty scope with the given name and adds it to the table.
    ///
    /// Returns the unique ID assigned to the newly created scope.
    /// Intended for internal use by the [SymbolTableCreator].
    ///
    /// # Parameters
    ///
    /// * `name`: The name of the scope (e.g., a package name). Can be empty for anonymous scopes.
    ///
    /// # Returns
    ///
    /// The unique [ScopeId] assigned to the new scope.
    pub(super) fn create_scope(&mut self, name: String) -> ScopeId {
        let id = ScopeId(self.next_scope_id);
        self.next_scope_id += 1;
        let scope = Scope {
            id: id.clone(),
            name,
            subscopes: Vec::new(),
            enclosing_scope: None, // Set when added as a subscope
            symbols: IndexMap::new(),
            spanning_symbol: None, // Set later if applicable
        };
        self.scopes.insert(id.clone(), scope);
        id
    }

    /// Creates a new symbol of the specified kind with the given name and adds it to the table.
    ///
    /// Initializes the symbol's common data ([SymbolCommon]) and assigns a unique ID.
    /// The symbol is initially placed in the global scope; it should be subsequently added
    /// to its correct enclosing scope using [SymbolTable::add_symbol_to_scope].
    /// Intended for internal use by the [SymbolTableCreator].
    ///
    /// # Parameters
    ///
    /// * `name`: The unqualified name of the symbol.
    /// * `kind`: The specific [SymbolKind] channel representing the symbol's type and data.
    ///
    /// # Returns
    ///
    /// The unique [SymbolId] assigned to the new symbol.
    pub(super) fn create_symbol(
        &mut self,
        name: String,
        span: Option<Span>,
        mut kind: SymbolKind,
    ) -> SymbolId {
        let id: SymbolId = SymbolId(self.next_symbol_id);
        self.next_symbol_id += 1;

        if let Some(s) = &span {
            let file_map = self.location_to_symbol.entry(s.file_path.clone().into()).or_default();
            file_map.insert((s.start.try_into().unwrap(), s.end.try_into().unwrap()), id.clone());
        }

        // Initialize common symbol properties
        let symbol = SymbolCommon {
            id: id.clone(),
            name,
            span,
            enclosing_scope: GLOBAL_SCOPE_ID.clone(), // Will be set when added to a scope
            spanned_scope: None, // Will be set when spanning symbol of a scope is set
        };

        // Clone the kind and embed the common symbol data into the specific symbol struct
        match &mut kind {
            SymbolKind::PlaceholderForTests(_) => kind = SymbolKind::PlaceholderForTests(symbol), // Special case for tests
            SymbolKind::Topic(topic_symbol) => topic_symbol.symbol = symbol,
            SymbolKind::ServiceBundle(service_bundle_symbol) => {
                service_bundle_symbol.symbol = symbol
            }
            SymbolKind::Publisher(publisher_symbol) => publisher_symbol.symbol = symbol,
            SymbolKind::RpcChannel(rpc_channel_symbol) => rpc_channel_symbol.symbol = symbol,
            SymbolKind::Subscriber(subscriber_symbol) => subscriber_symbol.symbol = symbol,
            SymbolKind::RpcServer(rpc_server_symbol) => rpc_server_symbol.symbol = symbol,
            SymbolKind::RpcClient(rpc_client_symbol) => rpc_client_symbol.symbol = symbol,
            SymbolKind::ProtoMessage(proto_message_symbol) => proto_message_symbol.symbol = symbol,
            SymbolKind::ProtoEnum(proto_enum_symbol) => proto_enum_symbol.symbol = symbol,
            SymbolKind::ProtoPublication(proto_publication_symbol) => {
                proto_publication_symbol.symbol = symbol
            }
            SymbolKind::ProtoRpcService(proto_rpc_service_symbol) => {
                proto_rpc_service_symbol.symbol = symbol
            }
        }

        // Store the symbol
        self.symbols.insert(id.clone(), kind);
        self.unlinked_symbols.insert(id.clone());
        id
    }

    /// Establishes a parent-child relationship between two scopes.
    ///
    /// Adds `subscope_id` to the list of subscopes for `enclosing_scope_id` and
    /// sets the `enclosing_scope` field of the subscope.
    /// Logs an error if either scope ID does not exist in the table.
    /// Intended for internal use by the [SymbolTableCreator].
    ///
    /// # Parameters
    ///
    /// * `enclosing_scope_id`: The [ScopeId] of the parent scope.
    /// * `subscope_id`: The [ScopeId] of the child scope.
    pub(super) fn add_subscope(&mut self, enclosing_scope_id: &ScopeId, subscope_id: &ScopeId) {
        if !self.scopes.contains_key(enclosing_scope_id) {
            eprintln!("ERROR: add_subscope: enclosing scope {enclosing_scope_id:?} does not exist");
            return;
        }
        if !self.scopes.contains_key(subscope_id) {
            eprintln!("ERROR: add_subscope: subscope {subscope_id:?} does not exist");
            return;
        }

        // Update enclosing scope's subscopes list
        if let Some(enclosing_scope) = self.scopes.get_mut(enclosing_scope_id) {
            enclosing_scope.subscopes.push(subscope_id.clone());
        }
        // Update subscope's enclosing scope reference
        if let Some(subscope) = self.scopes.get_mut(subscope_id) {
            subscope.enclosing_scope = Some(enclosing_scope_id.clone());
        }
    }

    /// Adds a symbol to the specified scope.
    ///
    /// Updates the scope's internal symbol map and sets the symbol's `enclosing_scope` field.
    /// Also updates the `symbols_fqn` map with the symbol's fully qualified name.
    /// Intended for internal use by the [SymbolTableCreator].
    ///
    /// # Parameters
    ///
    /// * `scope_id`: The [ScopeId] of the scope to add the symbol to.
    /// * `symbol_id`: The [SymbolId] of the symbol being added.
    pub(super) fn add_symbol_to_scope(&mut self, scope_id: &ScopeId, symbol_id: &SymbolId) {
        if let Some(scope) = self.scopes.get_mut(scope_id) {
            if let Some(symbol) = self.symbols.get_mut(symbol_id) {
                scope.symbols.entry(symbol.name()).or_insert_with(Vec::new).push(symbol_id.clone());
                symbol.set_enclosing_scope(scope_id.clone()); // Set the enclosing scope for the symbol
            }
        }
        if let Some(symbol) = self.symbols.get(symbol_id) {
            self.symbols_fqn.entry(symbol.full_name(self)).or_default().push(symbol_id.clone());
        }
    }

    /// Designates a symbol as the "spanning symbol" for a given scope.
    ///
    /// A spanning symbol is a symbol that represents the scope itself (e.g., a service bundle
    /// symbol might span the scope corresponding to that service bundle). This creates a
    /// bidirectional link between the scope and the symbol.
    /// Intended for internal use by the [SymbolTableCreator] or [SymbolTableLinker].
    ///
    /// # Parameters
    ///
    /// * `scope_id`: The [ScopeId] of the scope.
    /// * `symbol_id`: The [SymbolId] of the symbol that spans the scope.
    pub(super) fn set_spanning_symbol(&mut self, scope_id: &ScopeId, symbol_id: &SymbolId) {
        if self.symbols.contains_key(symbol_id) {
            if let Some(scope) = self.scopes.get_mut(scope_id) {
                scope.spanning_symbol = Some(symbol_id.clone());
            }
            if let Some(symbol) = self.get_symbol_mut(symbol_id) {
                symbol.set_spanned_scope(Some(scope_id.clone()));
            }
        }
    }

    /// Returns an immutable reference to the scope with the given ID.
    ///
    /// Intended for internal use within the crate.
    ///
    /// # Parameters
    ///
    /// * `id`: The [ScopeId] of the scope to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [Scope] if found, otherwise `None`.
    pub(super) fn get_scope(&self, id: &ScopeId) -> Option<&Scope> {
        self.scopes.get(id)
    }

    /// Finds a direct subscope of a given scope by its name.
    ///
    /// Only searches immediate children, does not search recursively.
    /// Logs an error if the provided name is empty.
    /// Intended for internal use within the crate.
    ///
    /// # Parameters
    ///
    /// * `scope_id`: The [ScopeId] of the parent scope to search within.
    /// * `name`: The name of the subscope to find.
    ///
    /// # Returns
    ///
    /// An `Option` containing the [ScopeId] of the found subscope, otherwise `None`.
    pub(super) fn get_subscope_by_name(&self, scope_id: &ScopeId, name: &str) -> Option<ScopeId> {
        if name.is_empty() {
            eprintln!("ERROR: get_subscope_by_name: empty name");
            return None;
        }
        if let Some(scope) = self.scopes.get(scope_id) {
            for subscope_id in &scope.subscopes {
                if let Some(subscope) = self.scopes.get(subscope_id) {
                    if subscope.name == name {
                        return Some(subscope.id.clone());
                    }
                }
            }
        }
        None // Not found
    }

    /// Returns an immutable reference to the symbol with the given ID.
    ///
    /// This provides access to the [SymbolKind] enum, allowing callers to match on the
    /// specific type of symbol.
    ///
    /// # Parameters
    ///
    /// * `id`: The [SymbolId] of the symbol to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [SymbolKind] if found, otherwise `None`.
    pub fn get_symbol(&self, id: &SymbolId) -> Option<&SymbolKind> {
        self.symbols.get(id)
    }

    /// Returns a mutable reference to the symbol with the given ID.
    ///
    /// Intended for internal use, primarily during symbol table construction and linking.
    ///
    /// # Parameters
    ///
    /// * `id`: The [SymbolId] of the symbol to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option` containing a mutable reference to the [SymbolKind] if found, otherwise `None`.
    pub(super) fn get_symbol_mut(&mut self, id: &SymbolId) -> Option<&mut SymbolKind> {
        self.symbols.get_mut(id)
    }

    /// Returns a vector containing all symbols stored in the table.
    ///
    /// Intended for internal use or debugging.
    ///
    /// # Returns
    ///
    /// A vector containing references to all [SymbolKind] channels in the table.
    #[allow(dead_code)]
    pub(super) fn get_all_symbols(&self) -> Vec<&SymbolKind> {
        self.symbols.values().collect()
    }

    /// Returns a vector containing all scopes stored in the table.
    ///
    /// Intended for internal use or debugging.
    ///
    /// # Returns
    ///
    /// A vector containing references to all [Scope]s in the table.
    #[allow(dead_code)]
    pub(super) fn get_all_scopes(&self) -> Vec<&Scope> {
        self.scopes.values().collect()
    }

    /// Returns the global scope that is the root of all scopes
    pub fn get_global_scope(&self) -> &Scope {
        self.scopes.get(&GLOBAL_SCOPE_ID).unwrap()
    }

    /// Returns the symbol at the given location.
    pub fn get_symbol_at_location(&self, file_path: &Path, offset: usize) -> Option<&SymbolKind> {
        // 1. O(1) average time to find the file's BTreeMap
        let file_map = self.location_to_symbol.get(file_path)?;

        // 2. The key to search for is (start, end). We want the symbol that
        //    *starts* just before or at the offset, and *ends* after it.
        //    We query for all symbols whose start <= offset.
        //    The upper bound key is (offset, usize::MAX). Since tuples are
        //    lexicographical, any key (start, end) where start <= offset will be
        //    <= this upper bound.
        let upper_bound = (offset, usize::MAX);

        // O(log M) search to find the start of the range iterator.
        // The iterator contains all entries where (start, end) <= upper_bound.
        let matching_range = file_map.range(..=upper_bound);

        // 3. Iterate the range in reverse (from largest start down to smallest).
        //    The first one we find that satisfies end > offset is the 'innermost'.
        let best_match = matching_range.rev().find(|((_start, end), _id)| *end > offset);

        // 4. Extract and retrieve the SymbolKind
        best_match.and_then(|((_start, _end), id)| self.get_symbol(id))
    }

    /// Removes all symbols originating from the specified file.
    ///
    /// This removes the symbols from the main symbol map, the FQN map, and their enclosing scopes.
    /// It also removes any scopes that were spanned by these symbols (e.g., service bundle scopes).
    /// Finally, it cleans up references to the removed symbols from other symbols in the table.
    pub fn remove_symbols_for_file(&mut self, file_path: &Path) {
        let canonical_path = file_path.to_path_buf();

        let mut ids_to_remove = HashSet::new();
        if let Some(map) = self.location_to_symbol.remove(&canonical_path) {
            ids_to_remove.extend(map.into_values());
        }

        if ids_to_remove.is_empty() {
            return;
        }

        // 1. Collect symbol data needed for removal (FQN, enclosing scope, etc.)
        // We do this in a separate pass to avoid borrowing issues while modifying the table.
        let mut symbols_data = Vec::new();
        for id in &ids_to_remove {
            if let Some(sym) = self.symbols.get(id) {
                symbols_data.push((
                    id.clone(),
                    sym.full_name(self),
                    sym.symbol().enclosing_scope,
                    sym.name(),
                    sym.spanned_scope(),
                ));
            }
        }

        // 2. Remove from FQN map and Scopes
        for (id, fqn, enclosing_scope_id, name, spanned_scope_id) in symbols_data {
            // Remove from FQN map
            if let Some(ids) = self.symbols_fqn.get_mut(&fqn) {
                ids.retain(|i| i != &id);
                if ids.is_empty() {
                    self.symbols_fqn.shift_remove(&fqn);
                }
            }

            // Remove from enclosing scope
            if let Some(scope) = self.scopes.get_mut(&enclosing_scope_id) {
                if let Some(ids) = scope.symbols.get_mut(&name) {
                    ids.retain(|i| i != &id);
                    if ids.is_empty() {
                        scope.symbols.shift_remove(&name);
                    }
                }
            }

            // Remove spanned scope if it exists
            if let Some(scope_id) = spanned_scope_id {
                self.remove_scope(&scope_id);
            }
        }

        // 3. Remove symbols from the main map
        for id in &ids_to_remove {
            self.symbols.shift_remove(id);
        }

        // 4. Cleanup references in remaining symbols
        self.cleanup_references(&ids_to_remove);
    }

    /// Recursively removes a scope and its subscopes from the table.
    fn remove_scope(&mut self, scope_id: &ScopeId) {
        if let Some(scope) = self.scopes.shift_remove(scope_id) {
            // Remove from parent's subscopes list
            if let Some(parent_id) = scope.enclosing_scope {
                if let Some(parent) = self.scopes.get_mut(&parent_id) {
                    parent.subscopes.retain(|id| id != scope_id);
                }
            }

            // Recursively remove subscopes
            for subscope_id in scope.subscopes {
                self.remove_scope(&subscope_id);
            }
        }
    }

    /// Cleans up references to deleted symbols in all remaining symbols.
    fn cleanup_references(&mut self, deleted_ids: &HashSet<SymbolId>) {
        for symbol in self.symbols.values_mut() {
            match symbol {
                SymbolKind::Topic(s) => {
                    if let Some(id) = &s.message {
                        if deleted_ids.contains(id) {
                            s.message = None;
                        }
                    }
                    s.publishers.retain(|id| !deleted_ids.contains(id));
                    s.subscribers.retain(|id| !deleted_ids.contains(id));
                }
                SymbolKind::ServiceBundle(s) => {
                    s.publishers.retain(|id| !deleted_ids.contains(id));
                    s.subscribers.retain(|id| !deleted_ids.contains(id));
                    s.servers.retain(|id| !deleted_ids.contains(id));
                    s.clients.retain(|id| !deleted_ids.contains(id));
                }
                SymbolKind::Publisher(s) => {
                    if let Some(id) = &s.owning_service_bundle {
                        if deleted_ids.contains(id) {
                            s.owning_service_bundle = None;
                        }
                    }
                    if let Some(id) = &s.publication {
                        if deleted_ids.contains(id) {
                            s.publication = None;
                        }
                    }
                    if let Some(id) = &s.message {
                        if deleted_ids.contains(id) {
                            s.message = None;
                        }
                    }
                    s.topics.retain(|id| !deleted_ids.contains(id));
                }
                SymbolKind::RpcChannel(s) => {
                    s.servers.retain(|id| !deleted_ids.contains(id));
                    s.clients.retain(|id| !deleted_ids.contains(id));
                }
                SymbolKind::Subscriber(s) => {
                    if let Some(id) = &s.owning_service_bundle {
                        if deleted_ids.contains(id) {
                            s.owning_service_bundle = None;
                        }
                    }
                    if let Some(id) = &s.publication {
                        if deleted_ids.contains(id) {
                            s.publication = None;
                        }
                    }
                    if let Some(id) = &s.message {
                        if deleted_ids.contains(id) {
                            s.message = None;
                        }
                    }
                    s.topics.retain(|id| !deleted_ids.contains(id));
                }
                SymbolKind::RpcServer(s) => {
                    if let Some(id) = &s.owning_service_bundle {
                        if deleted_ids.contains(id) {
                            s.owning_service_bundle = None;
                        }
                    }
                    if let Some(id) = &s.provided_service {
                        if deleted_ids.contains(id) {
                            s.provided_service = None;
                        }
                    }
                    if let Some(id) = &s.channel {
                        if deleted_ids.contains(id) {
                            s.channel = None;
                        }
                    }
                }
                SymbolKind::RpcClient(s) => {
                    if let Some(id) = &s.owning_service_bundle {
                        if deleted_ids.contains(id) {
                            s.owning_service_bundle = None;
                        }
                    }
                    if let Some(id) = &s.requested_service {
                        if deleted_ids.contains(id) {
                            s.requested_service = None;
                        }
                    }
                    if let Some(id) = &s.channel {
                        if deleted_ids.contains(id) {
                            s.channel = None;
                        }
                    }
                }
                SymbolKind::ProtoPublication(s) => {
                    if let Some(id) = &s.channels_enum {
                        if deleted_ids.contains(id) {
                            s.channels_enum = None;
                        }
                    }
                    s.publishers.retain(|id| !deleted_ids.contains(id));
                    s.subscribers.retain(|id| !deleted_ids.contains(id));
                }
                SymbolKind::ProtoRpcService(s) => {
                    s.servers.retain(|id| !deleted_ids.contains(id));
                    s.clients.retain(|id| !deleted_ids.contains(id));
                }
                SymbolKind::ProtoMessage(s) => {
                    s.topics.retain(|id| !deleted_ids.contains(id));
                }
                _ => {}
            }
        }
    }

    // --- Functions generated by get_all_symbols! macro ---
    get_all_symbols!(get_all_topic_symbols, SymbolKind::Topic, TopicSymbol);
    get_all_symbols!(
        get_all_service_bundle_symbols,
        SymbolKind::ServiceBundle,
        ServiceBundleSymbol
    );
    get_all_symbols!(get_all_publisher_symbols, SymbolKind::Publisher, PublisherSymbol);
    get_all_symbols!(get_all_rpc_channel_symbols, SymbolKind::RpcChannel, RpcChannelSymbol);
    get_all_symbols!(get_all_subscriber_symbols, SymbolKind::Subscriber, SubscriberSymbol);
    get_all_symbols!(get_all_rpc_server_symbols, SymbolKind::RpcServer, RpcServerSymbol);
    get_all_symbols!(get_all_rpc_client_symbols, SymbolKind::RpcClient, RpcClientSymbol);
    get_all_symbols!(get_all_proto_message_symbols, SymbolKind::ProtoMessage, ProtoMessageSymbol);
    get_all_symbols!(get_all_proto_enum_symbols, SymbolKind::ProtoEnum, ProtoEnumSymbol);
    get_all_symbols!(
        get_all_proto_publication_symbols,
        SymbolKind::ProtoPublication,
        ProtoPublicationSymbol
    );
    get_all_symbols!(
        get_all_proto_rpc_service_symbols,
        SymbolKind::ProtoRpcService,
        ProtoRpcServiceSymbol
    );

    // --- Functions generated by get_symbol! macro ---
    get_symbol!(get_topic_symbol, SymbolKind::Topic, TopicSymbol);
    get_symbol!(get_service_bundle_symbol, SymbolKind::ServiceBundle, ServiceBundleSymbol);
    get_symbol!(get_publisher_symbol, SymbolKind::Publisher, PublisherSymbol);
    get_symbol!(get_rpc_channel_symbol, SymbolKind::RpcChannel, RpcChannelSymbol);
    get_symbol!(get_subscriber_symbol, SymbolKind::Subscriber, SubscriberSymbol);
    get_symbol!(get_rpc_server_symbol, SymbolKind::RpcServer, RpcServerSymbol);
    get_symbol!(get_rpc_client_symbol, SymbolKind::RpcClient, RpcClientSymbol);
    get_symbol!(get_proto_message_symbol, SymbolKind::ProtoMessage, ProtoMessageSymbol);
    get_symbol!(get_proto_enum_symbol, SymbolKind::ProtoEnum, ProtoEnumSymbol);
    get_symbol!(get_proto_publication_symbol, SymbolKind::ProtoPublication, ProtoPublicationSymbol);
    get_symbol!(get_proto_rpc_service_symbol, SymbolKind::ProtoRpcService, ProtoRpcServiceSymbol);

    /// Returns a mutable reference to the [TopicSymbol] with the given ID.
    ///
    /// Intended for internal use, primarily during symbol table construction and linking.
    ///
    /// # Parameters
    ///
    /// * `id`: The [SymbolId] of the channel symbol to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option` containing a mutable reference to the [TopicSymbol] if found
    /// and is of the correct type, otherwise `None`.
    pub(super) fn get_topic_symbol_mut(&mut self, id: &SymbolId) -> Option<&mut TopicSymbol> {
        if let Some(SymbolKind::Topic(topic_symbol)) = self.symbols.get_mut(id) {
            return Some(topic_symbol);
        }
        None
    }

    /// Returns a mutable reference to the [RpcChannelSymbol] with the given ID.
    ///
    /// # Parameters
    ///
    /// * `id`: The [SymbolId] of the RPC channel symbol to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option` containing a mutable reference to the [RpcChannelSymbol] if found
    /// and is of the correct type, otherwise `None`.
    pub(super) fn get_rpc_channel_symbol_mut(
        &mut self,
        id: &SymbolId,
    ) -> Option<&mut RpcChannelSymbol> {
        if let Some(SymbolKind::RpcChannel(rpc_channel_symbol)) = self.symbols.get_mut(id) {
            return Some(rpc_channel_symbol);
        }
        None
    }

    /// Returns a mutable reference to the [SubscriberSymbol] with the given ID.
    ///
    /// # Parameters
    ///
    /// * `id`: The [SymbolId] of the subscriber symbol to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option` containing a mutable reference to the [SubscriberSymbol] if found
    /// and is of the correct type, otherwise `None`.
    pub(super) fn get_subscriber_symbol_mut(
        &mut self,
        id: &SymbolId,
    ) -> Option<&mut SubscriberSymbol> {
        if let Some(SymbolKind::Subscriber(subscriber_symbol)) = self.symbols.get_mut(id) {
            return Some(subscriber_symbol);
        }
        None
    }

    /// Returns a mutable reference to the [ProtoPublicationSymbol] with the given ID.
    ///
    /// # Parameters
    ///
    /// * `id`: The [SymbolId] of the proto publication symbol to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option` containing a mutable reference to the [ProtoPublicationSymbol] if found
    /// and is of the correct type, otherwise `None`.
    pub(super) fn get_proto_publication_symbol_mut(
        &mut self,
        id: &SymbolId,
    ) -> Option<&mut ProtoPublicationSymbol> {
        if let Some(SymbolKind::ProtoPublication(proto_publication_symbol)) =
            self.symbols.get_mut(id)
        {
            return Some(proto_publication_symbol);
        }
        None
    }

    /// Returns a mutable reference to the [ProtoMessageSymbol] with the given ID.
    ///
    /// # Parameters
    ///
    /// * `id`: The [SymbolId] of the proto publication symbol to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option` containing a mutable reference to the [ProtoMessageSymbol] if found
    /// and is of the correct type, otherwise `None`.
    pub(super) fn get_proto_message_symbol_mut(
        &mut self,
        id: &SymbolId,
    ) -> Option<&mut ProtoMessageSymbol> {
        if let Some(SymbolKind::ProtoMessage(proto_message_symbol)) = self.symbols.get_mut(id) {
            return Some(proto_message_symbol);
        }
        None
    }

    /// Returns a mutable reference to the [ServiceBundleSymbol] with the given ID.
    ///
    /// Intended for internal use, primarily during symbol table construction and linking.
    ///
    /// # Parameters
    ///
    /// * `id`: The [SymbolId] of the service bundle symbol to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option` containing a mutable reference to the [ServiceBundleSymbol] if found
    /// and is of the correct type, otherwise `None`.
    pub(super) fn get_service_bundle_symbol_mut(
        &mut self,
        id: &SymbolId,
    ) -> Option<&mut ServiceBundleSymbol> {
        if let Some(SymbolKind::ServiceBundle(service_bundle_symbol)) = self.symbols.get_mut(id) {
            return Some(service_bundle_symbol);
        }
        None
    }

    // --- Functions generated by implement_fqn_resolver! macro ---
    implement_fqn_resolver!(resolve_topic_fqn, SymbolKind::Topic, TopicSymbol);
    implement_fqn_resolver!(
        resolve_service_bundle_fqn,
        SymbolKind::ServiceBundle,
        ServiceBundleSymbol
    );
    implement_fqn_resolver!(resolve_publisher_fqn, SymbolKind::Publisher, PublisherSymbol);
    implement_fqn_resolver!(resolve_rpc_channel_fqn, SymbolKind::RpcChannel, RpcChannelSymbol);
    implement_fqn_resolver!(resolve_subscriber_fqn, SymbolKind::Subscriber, SubscriberSymbol);
    implement_fqn_resolver!(resolve_rpc_server_fqn, SymbolKind::RpcServer, RpcServerSymbol);
    implement_fqn_resolver!(resolve_rpc_client_fqn, SymbolKind::RpcClient, RpcClientSymbol);
    implement_fqn_resolver!(
        resolve_proto_message_fqn,
        SymbolKind::ProtoMessage,
        ProtoMessageSymbol
    );
    implement_fqn_resolver!(resolve_proto_enum_fqn, SymbolKind::ProtoEnum, ProtoEnumSymbol);
    implement_fqn_resolver!(
        resolve_proto_publication_fqn,
        SymbolKind::ProtoPublication,
        ProtoPublicationSymbol
    );
    implement_fqn_resolver!(
        resolve_proto_rpc_service_fqn,
        SymbolKind::ProtoRpcService,
        ProtoRpcServiceSymbol
    );

    /// Resolves a name to potentially multiple symbols from the perspective of a starting scope.
    ///
    /// This is the core name resolution logic used throughout the VSIDL compiler.
    /// It handles unqualified, partially qualified, and fully qualified names based on
    /// standard C++/Java-like visibility and scoping rules, adapted for VSIDL's structure
    /// (including anonymous scopes).
    ///
    /// The resolution process generally follows these steps:
    /// 1.  **Qualified Name Lookup:** If `resolve_qualified_name` is true, it first attempts
    ///     a direct lookup using the provided `name` as a fully qualified name (FQN) in the
    ///     global FQN map ([SymbolTable::symbols_fqn]). It might also try prepending the
    ///     starting scope's FQN for relative lookups (common for unit types).
    /// 2.  **Recursive Unqualified Lookup:** If `resolve_qualified_name` is false, or if the
    ///     qualified lookup fails, it initiates a recursive search ([SymbolTable::resolve_recursive])
    ///     starting from `starting_scope_id`. This recursive search checks:
    ///     *   Symbols directly within the current scope.
    ///     *   Navigating into subscopes if the name is qualified (e.g., `subscope.SymbolName`).
    ///     *   Searching upwards into enclosing scopes until the global scope is reached.
    ///     *   Searching downwards from the global scope if the name wasn't found upwards (handles FQNs used within nested scopes).
    ///     *   Special handling for anonymous scopes (scopes with empty names).
    ///
    /// # Parameters
    ///
    /// * `starting_scope_id`: The [ScopeId] representing the context or location from which the name resolution should begin.
    /// * `name`: The name to resolve. This can be unqualified (`MySymbol`), partially qualified (`sub_scope.MySymbol`), or fully qualified (`.com.example.MySymbol`).
    ///
    /// Note that if the name starts with a dot ('.'), the name will always be interpreted as a fully qualified name regardless of the resolve_qualified_name parameter.
    ///
    /// # Returns
    ///
    /// A vector containing immutable references ([&SymbolKind]) to all symbols that match the
    /// given `name` according to the resolution rules. The vector can be empty if the name
    /// cannot be resolved. It might contain multiple symbols if the name is ambiguous (e.g.,
    /// defined in multiple imported files or scopes).
    pub fn resolve(
        &self,
        starting_scope_id: &ScopeId,
        name: &str,
        resolve_qualified_name: bool,
    ) -> Vec<&SymbolKind> {
        if resolve_qualified_name || name.starts_with('.') {
            // Strategy 1: Direct FQN lookup (potentially with prefixing)
            let mut fqn = name.to_string();

            if let Some(stripped) = name.strip_prefix('.') {
                // Remove the leading dot
                fqn = stripped.to_string();
            }

            // Attempt 1: Look up the name exactly as provided in the global FQN map.
            let result: Vec<_> = self
                .symbols_fqn
                .get(&fqn)
                .unwrap_or(&Vec::new())
                .iter()
                .map(|symbol_id| self.get_symbol(symbol_id).unwrap())
                .collect();

            // Stop search if found a result or enforced fully qualified name
            if !result.is_empty() || name.starts_with('.') {
                return result;
            }

            // Attempt 2: Prepend the current scope's FQN (useful for relative paths like unit types).
            // Example: If in scope `com.google` and looking for `sub.MyType`, try `com.google.sub.MyType`.
            if let Some(current_scope) = self.get_scope(starting_scope_id) {
                let scope_name = current_scope.full_name(self);
                if !scope_name.is_empty() {
                    fqn = format!("{scope_name}.{name}");
                }
            }
            self.symbols_fqn
                .get(&fqn)
                .unwrap_or(&Vec::new())
                .iter()
                .map(|symbol_id| self.get_symbol(symbol_id).unwrap())
                .collect()
        } else {
            // Strategy 2: Recursive scope-based lookup
            self.resolve_recursive(starting_scope_id, name, name, true, &mut HashSet::new())
        }
    }

    /// Performs the recursive part of name resolution.
    ///
    /// This function implements the scope traversal logic (upwards, downwards, into subscopes)
    /// used by [SymbolTable::resolve] when `resolve_qualified_name` is false.
    /// It uses a `visited_scopes` set to prevent infinite recursion in case of cyclic scope
    /// structures (though such structures should ideally be prevented earlier).
    /// Intended for internal use by [SymbolTable::resolve].
    ///
    /// # Parameters
    ///
    /// * `current_scope_id`: The [ScopeId] of the scope currently being examined.
    /// * `original_request`: The original name requested to be resolved. It is used when searching upwards to search the full name in the enclosing scope.
    /// * `name`: The name (potentially qualified relative to `current_scope_id`) being searched for.
    /// * `resolving_direction_is_down`: Controls the traversal logic.
    ///     * `true`: Search downwards into subscopes (including anonymous ones) and then upwards.
    ///     * `false`: Search only upwards into enclosing scopes (used when descending from the global scope).
    /// * `visited_scopes`: A set to track visited (`ScopeId`, `name`, `direction`) tuples to prevent cycles.
    ///
    /// # Returns
    ///
    /// A vector containing references to symbols found during the recursive search from this point.
    pub(super) fn resolve_recursive(
        &self,
        current_scope_id: &ScopeId,
        original_request: &str,
        name: &str,
        resolving_direction_is_down: bool,
        visited_scopes: &mut HashSet<(ScopeId, String, bool)>,
    ) -> Vec<&SymbolKind> {
        let mut results = Vec::new();

        // Check if scope was visited before with the same name (regardless of direction)
        let visited_before = visited_scopes
            .iter()
            .any(|tuple| tuple.0 == current_scope_id.clone() && tuple.1 == *name);

        // Check if this exact scope+name+direction has been visited.
        // Prevents infinite loops if traversing the same path again.
        if !visited_scopes.insert((
            current_scope_id.clone(),
            name.to_string(),
            resolving_direction_is_down,
        )) {
            return results;
        }

        if let Some(current_scope) = self.get_scope(current_scope_id) {
            let parts: Vec<&str> = name.split('.').collect();

            // Unqualified name (or symbol with a dot in its name): search in current scope
            if !visited_before {
                for (symbol_name, symbols_ids) in &current_scope.symbols {
                    for symbol_id in symbols_ids {
                        if name == symbol_name {
                            if let Some(symbol) = self.get_symbol(symbol_id) {
                                results.push(symbol);
                            }
                        }
                    }
                }
            }

            // Qualified name: check subscopes for a scope that matches the first component of the requested name
            if parts.len() > 1 && resolving_direction_is_down {
                let next_scope_name = parts[0];
                for subscope_id in &current_scope.subscopes {
                    if let Some(subscope) = self.get_scope(subscope_id) {
                        if subscope.name == next_scope_name {
                            let remaining_name = name[next_scope_name.len() + 1..].to_string();
                            results.extend(self.resolve_recursive(
                                subscope_id,
                                original_request,
                                &remaining_name,
                                resolving_direction_is_down,
                                visited_scopes,
                            ));
                        }
                    }
                }
            }

            // Continue search in anonymous subscopes (search downwards).
            // Anonymous scopes act like transparent containers; symbols within them are searched
            // as if they were in the parent, but only when searching downwards initially.
            if resolving_direction_is_down {
                for subscope_id in &current_scope.subscopes {
                    if let Some(subscope) = self.get_scope(subscope_id) {
                        if subscope.name.is_empty() {
                            results.extend(self.resolve_recursive(
                                subscope_id,
                                original_request,
                                name,
                                resolving_direction_is_down,
                                visited_scopes,
                            ));
                        }
                    }
                }
            }

            // Search upwards into the enclosing scope.
            if let Some(enclosing_scope_id) = &current_scope.enclosing_scope {
                let enclosing_scope_is_global_scope =
                    self.get_scope(enclosing_scope_id).unwrap().enclosing_scope.is_none();
                results.extend(self.resolve_recursive(
                    enclosing_scope_id,
                    original_request,
                    original_request, // Search full name in enclosing scope
                    enclosing_scope_is_global_scope,
                    visited_scopes,
                ));
            }
        }
        results
    }

    /// Accepts a [SymbolTableChecker] visitor to perform checks on the symbol table.
    ///
    /// This implements the Visitor pattern, allowing different sets of checks (CoCos)
    /// to be run over the fully constructed (and potentially linked) symbol table.
    /// The checker's methods are called in a specific order:
    /// 1. `before_checks`
    /// 2. `check_*` methods for each symbol type.
    /// 3. `after_checks`
    ///
    /// # Parameters
    ///
    /// * `checker`: A mutable reference to an object implementing the [SymbolTableChecker] trait.
    pub fn accept(&self, checker: &mut dyn SymbolTableChecker) {
        checker.before_checks(self);
        for symbol in self.symbols.values() {
            match symbol {
                SymbolKind::PlaceholderForTests(_) => { /* Intentionally skip checks for test placeholders */
                }
                SymbolKind::Topic(channel) => checker.check_channel(self, channel),
                SymbolKind::ServiceBundle(service_bundle) => {
                    checker.check_service_bundle(self, service_bundle)
                }
                SymbolKind::Publisher(publisher) => checker.check_publisher(self, publisher),
                SymbolKind::RpcChannel(rpc_channel) => checker.check_rpc_channel(self, rpc_channel),
                SymbolKind::Subscriber(subscriber) => checker.check_subscriber(self, subscriber),
                SymbolKind::RpcServer(rpc_server) => checker.check_rpc_server(self, rpc_server),
                SymbolKind::RpcClient(rpc_client) => checker.check_rpc_client(self, rpc_client),
                SymbolKind::ProtoMessage(message) => checker.check_proto_message(self, message),
                SymbolKind::ProtoEnum(enum_) => checker.check_proto_enum(self, enum_),
                SymbolKind::ProtoPublication(publication) => {
                    checker.check_proto_publication(self, publication)
                }
                SymbolKind::ProtoRpcService(rpc_service) => {
                    checker.check_proto_rpc_service(self, rpc_service)
                }
            }
        }
        checker.after_checks(self);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{symbol::SymbolKind, symbol_table_checker::MockSymbolTableChecker};

    const RESOLVE_FQN: bool = true;

    /// Creates a scope hierarchy "com" > "google" > "sdv", where "sdv" contains a symbol "Monitor"
    fn setup() -> (SymbolTable, ScopeId, ScopeId, ScopeId, ScopeId, SymbolId) {
        let mut symbol_table = SymbolTable::default();

        // Create scopes
        let global_scope_id = symbol_table.create_scope("".to_string());
        let com_scope_id = symbol_table.create_scope("com".to_string());
        let google_scope_id = symbol_table.create_scope("google".to_string());
        let sdv_scope_id = symbol_table.create_scope("sdv".to_string());
        let distraction_scope_id = symbol_table.create_scope("distraction".to_string());

        // Add subscopes
        symbol_table.add_subscope(&global_scope_id, &com_scope_id);
        symbol_table.add_subscope(&com_scope_id, &google_scope_id);
        symbol_table.add_subscope(&google_scope_id, &sdv_scope_id);
        symbol_table.add_subscope(&google_scope_id, &distraction_scope_id);

        // Create symbols
        let monitor_symbol_id = symbol_table.create_symbol(
            "Monitor".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        // The second_monitor_symbol_id serves a distraction with the same unqualified name
        // that shall never be found be the tests
        let second_monitor_symbol_id = symbol_table.create_symbol(
            "Monitor".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );

        // Add symbols to scopes
        symbol_table.add_symbol_to_scope(&sdv_scope_id, &monitor_symbol_id);
        symbol_table.add_symbol_to_scope(&distraction_scope_id, &second_monitor_symbol_id);

        (
            symbol_table,
            global_scope_id,
            com_scope_id,
            google_scope_id,
            sdv_scope_id,
            monitor_symbol_id,
        )
    }

    /// Sets up a complex symbol table for testing name resolution, especially with
    /// parallel anonymous scopes.
    ///
    /// The created scope hierarchy and the returned variables are as follows:
    /// ```
    /// global ("") `global_scope_id`
    /// └── com ("com") `com_scope_id`
    ///     ├── google ("google") `google_scope_id`
    ///     │   ├── "" (anonymous_scope_1) `anonymous_scope_1_id`
    ///     │   │   └── sdv ("sdv") `sdv_scope_id`
    ///     │   │       └── contains "Monitor" `monitor_symbol_id`
    ///     │   ├── "" (anonymous_scope_2) `anonymous_scope_2_id`
    ///     │   │   └── contains "Monitor2" `monitor2_symbol_id`
    ///     │   ├── sdv ("sdv_scope_2") `sdv_scope_2_id`
    ///     │   │   └── contains "Monitor.With.Dots" `monitor3_symbol_id`
    ///     │   └── very -> deep -> package
    ///     │       └── contains "Monitor" `monitor4_symbol_id`
    ///     └── contains "google.sdv.UnitTypeWithDot" `unit_type_with_dot_symbol_id`
    /// ```
    ///
    /// # Returns
    ///
    /// A tuple containing the constructed `SymbolTable` and all created `ScopeId`s and `SymbolId`s.
    fn setup_with_parallel_anonymous_scopes_and_symbols() -> (
        SymbolTable,
        ScopeId,
        ScopeId,
        ScopeId,
        ScopeId,
        ScopeId,
        ScopeId,
        ScopeId,
        SymbolId,
        SymbolId,
        SymbolId,
        SymbolId,
        SymbolId,
    ) {
        let mut symbol_table = SymbolTable::default();

        let global_scope_id = symbol_table.create_scope("".to_string());
        let com_scope_id = symbol_table.create_scope("com".to_string());
        let google_scope_id = symbol_table.create_scope("google".to_string());
        let anonymous_scope_1_id = symbol_table.create_scope("".to_string());
        let anonymous_scope_2_id = symbol_table.create_scope("".to_string());
        let sdv_scope_id = symbol_table.create_scope("sdv".to_string());
        let sdv_scope_2_id = symbol_table.create_scope("sdv".to_string());
        let very_scope_id = symbol_table.create_scope("very".to_string());
        let deep_scope_id = symbol_table.create_scope("deep".to_string());
        let package_scope_id = symbol_table.create_scope("package".to_string());

        // Add subscopes
        symbol_table.add_subscope(&global_scope_id, &com_scope_id);
        symbol_table.add_subscope(&com_scope_id, &google_scope_id);
        symbol_table.add_subscope(&google_scope_id, &anonymous_scope_1_id);
        symbol_table.add_subscope(&google_scope_id, &anonymous_scope_2_id);
        symbol_table.add_subscope(&google_scope_id, &sdv_scope_2_id);
        symbol_table.add_subscope(&anonymous_scope_1_id, &sdv_scope_id);
        symbol_table.add_subscope(&google_scope_id, &very_scope_id);
        symbol_table.add_subscope(&very_scope_id, &deep_scope_id);
        symbol_table.add_subscope(&deep_scope_id, &package_scope_id);

        // Create symbols
        let monitor_symbol_id = symbol_table.create_symbol(
            "Monitor".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        let monitor2_symbol_id = symbol_table.create_symbol(
            "Monitor2".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        let monitor3_symbol_id = symbol_table.create_symbol(
            "Monitor.With.Dots".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        let monitor4_symbol_id = symbol_table.create_symbol(
            "Monitor".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        let unit_type_with_dot_symbol_id = symbol_table.create_symbol(
            "google.sdv.UnitTypeWithDot".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );

        // Add symbol to scopes
        symbol_table.add_symbol_to_scope(&sdv_scope_id, &monitor_symbol_id);
        symbol_table.add_symbol_to_scope(&anonymous_scope_2_id, &monitor2_symbol_id);
        symbol_table.add_symbol_to_scope(&sdv_scope_2_id, &monitor3_symbol_id);
        symbol_table.add_symbol_to_scope(&package_scope_id, &monitor4_symbol_id);
        symbol_table.add_symbol_to_scope(&com_scope_id, &unit_type_with_dot_symbol_id);

        // Ensure symbols have different IDs
        let all_symbol_ids: HashSet<_> = vec![
            monitor_symbol_id.clone(),
            monitor2_symbol_id.clone(),
            monitor3_symbol_id.clone(),
            monitor4_symbol_id.clone(),
            unit_type_with_dot_symbol_id.clone(),
        ]
        .into_iter()
        .collect();
        assert_eq!(all_symbol_ids.len(), 5);

        (
            symbol_table,
            global_scope_id,
            com_scope_id,
            google_scope_id,
            anonymous_scope_1_id,
            anonymous_scope_2_id,
            sdv_scope_id,
            sdv_scope_2_id,
            monitor_symbol_id,
            monitor2_symbol_id,
            monitor3_symbol_id,
            monitor4_symbol_id,
            unit_type_with_dot_symbol_id,
        )
    }

    #[test]
    fn test_resolve_symbol_from_sdv_scope() {
        // GIVEN
        let (symbol_table, _, _, _, sdv_scope_id, monitor_symbol_id) = setup();

        // WHEN Resolve "Monitor" from the sdv scope.
        let resolved_from_sdv = symbol_table.resolve(&sdv_scope_id, "Monitor", !RESOLVE_FQN);

        // THEN Should resolve to the symbol defined in "sdv" scope.
        assert_eq!(resolved_from_sdv.len(), 1);
        assert_eq!(resolved_from_sdv[0].id(), monitor_symbol_id);
    }

    #[test]
    fn test_resolve_symbol_from_google_scope() {
        // GIVEN
        let (symbol_table, _, _, google_scope_id, _, monitor_symbol_id) = setup();

        // WHEN Resolve "sdv.Monitor" from the "google" scope.
        let resolved_from_google =
            symbol_table.resolve(&google_scope_id, "sdv.Monitor", !RESOLVE_FQN);

        // THEN Should resolve to the symbol defined in "sdv" subscope.
        assert_eq!(resolved_from_google.len(), 1);
        assert_eq!(resolved_from_google[0].id(), monitor_symbol_id);
    }

    #[test]
    fn test_resolve_symbol_from_com_scope() {
        // GIVEN
        let (symbol_table, _, com_scope_id, _, _, monitor_symbol_id) = setup();

        // WHEN Resolve "google.sdv.Monitor" from "com" scope.
        let resolved_from_com =
            symbol_table.resolve(&com_scope_id, "google.sdv.Monitor", !RESOLVE_FQN);

        // THEN Should resolve to the symbol defined in "google.sdv" subscope.
        assert_eq!(resolved_from_com.len(), 1);
        assert_eq!(resolved_from_com[0].id(), monitor_symbol_id);
    }

    #[test]
    fn test_resolve_symbol_from_global_scope() {
        // GIVEN
        let (symbol_table, global_scope_id, _, _, _, monitor_symbol_id) = setup();

        // WHEN Resolve "com.google.sdv.Monitor" from the global scope.
        let resolved_from_global =
            symbol_table.resolve(&global_scope_id, "com.google.sdv.Monitor", !RESOLVE_FQN);

        // THEN Should resolve to the symbol defined in "com.google.sdv" scope.
        assert_eq!(resolved_from_global.len(), 1);
        assert_eq!(resolved_from_global[0].id(), monitor_symbol_id);
    }

    #[test]
    fn test_resolve_symbol_from_sdv_scope_fqn() {
        // GIVEN
        let (symbol_table, _, _, _, sdv_scope_id, monitor_symbol_id) = setup();

        // WHEN Resolve "com.google.sdv.Monitor" from the "sdv" scope.
        let resolved_from_sdv_fqn =
            symbol_table.resolve(&sdv_scope_id, "com.google.sdv.Monitor", !RESOLVE_FQN);

        // THEN Should resolve to the symbol defined in "com.google.sdv" scope.
        assert_eq!(resolved_from_sdv_fqn.len(), 1);
        assert_eq!(resolved_from_sdv_fqn[0].id(), monitor_symbol_id);
    }

    #[test]
    fn test_resolve_partial_symbol_from_sdv_scope() {
        // GIVEN
        let (symbol_table, _, _, _, sdv_scope_id, _) = setup();

        // WHEN Resolve "sdv.Monitor" from the sdv scope
        let resolved_from_sdv_partial =
            symbol_table.resolve(&sdv_scope_id, "sdv.Monitor", !RESOLVE_FQN);

        // THEN should NOT resolve to the symbol defined in "sdv" scope, as we are already in this scope.
        assert_eq!(resolved_from_sdv_partial.len(), 0);
    }

    #[test]
    fn test_resolve_non_matching_symbol_from_google_scope() {
        // GIVEN
        let (symbol_table, _, _, google_scope_id, _, _) = setup();

        // WHEN Resolve "Monitor" from the google scope.
        let resolved_from_google_no_match =
            symbol_table.resolve(&google_scope_id, "Monitor", !RESOLVE_FQN);

        // THEN Should not resolve to anything, as there is no symbol in the global scope
        assert_eq!(resolved_from_google_no_match.len(), 0);
    }

    #[test]
    fn test_resolve_non_existing_symbol_from_global_scope() {
        // GIVEN
        let (symbol_table, global_scope_id, _, _, _, _) = setup();

        // WHEN Resolve "NonExistingSymbol" from the global scope.
        let resolved_non_existing =
            symbol_table.resolve(&global_scope_id, "NonExistingSymbol", !RESOLVE_FQN);

        // THEN Should not resolve to anything, as the symbol does not exist
        assert_eq!(resolved_non_existing.len(), 0);
    }

    #[test]
    fn test_resolve_monitor_from_first_anonymous_scope() {
        // GIVEN
        let (symbol_table, _, _, _, anonymous_scope_1_id, _, _, _, monitor_symbol_id, _, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN
        // Resolve "Monitor" from the first anonymous scope
        let resolved_from_anonymous_scope =
            symbol_table.resolve(&anonymous_scope_1_id, "sdv.Monitor", !RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_anonymous_scope.len(), 1);
        assert_eq!(resolved_from_anonymous_scope[0].id(), monitor_symbol_id);
    }

    #[test]
    fn test_resolve_monitor2_from_second_anonymous_scope() {
        // GIVEN
        let (symbol_table, _, _, _, _, anonymous_scope_2_id, _, _, _, monitor2_symbol_id, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN
        // Resolve "Monitor2" from the second anonymous scope
        let resolved_from_anonymous_scope =
            symbol_table.resolve(&anonymous_scope_2_id, "Monitor2", !RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_anonymous_scope.len(), 1);
        assert_eq!(resolved_from_anonymous_scope[0].id(), monitor2_symbol_id);
    }

    #[test]
    fn test_resolve_monitor_from_second_anonymous_scope_should_fail() {
        // GIVEN
        let (symbol_table, _, _, _, _, anonymous_scope_2_id, _, _, _, _, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN
        // Try to resolve "sdv.Monitor" from the second anonymous scope (should fail).
        let resolved_from_anonymous_scope =
            symbol_table.resolve(&anonymous_scope_2_id, "sdv.Monitor", !RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_anonymous_scope.len(), 0);
    }

    #[test]
    fn test_resolve_monitor2_from_first_anonymous_scope_without_parallel_scope_resolving() {
        // GIVEN
        let (symbol_table, _, _, _, anonymous_scope_1_id, _, _, _, _, _, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN resolve "Monitor2" from the first anonymous scope
        let resolved_from_anonymous_scope =
            symbol_table.resolve(&anonymous_scope_1_id, "Monitor2", !RESOLVE_FQN);

        // THEN symbol is not found because parallel anonymous scopes (and thus the first anonymous scope) are not considered
        assert_eq!(resolved_from_anonymous_scope.len(), 0);
    }

    #[test]
    fn test_resolve_monitor2_from_first_anonymous_scope() {
        // GIVEN
        let (symbol_table, _, _, _, anonymous_scope_1_id, _, _, _, _, monitor2_symbol_id, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN resolve "Monitor2" from the first anonymous scope
        let resolved_from_anonymous_scope =
            symbol_table.resolve(&anonymous_scope_1_id, "Monitor2", RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_anonymous_scope.len(), 1);
        assert_eq!(resolved_from_anonymous_scope[0].id(), monitor2_symbol_id);
    }

    #[test]
    fn test_resolve_monitor_from_google_scope_with_parallel_anonymous_scopes() {
        // GIVEN
        let (symbol_table, _, _, google_scope_id, _, _, _, _, monitor_symbol_id, _, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN
        // Resolve "Monitor" from the "google" scope
        let resolved_from_google =
            symbol_table.resolve(&google_scope_id, "sdv.Monitor", !RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_google.len(), 1);
        assert_eq!(resolved_from_google[0].id(), monitor_symbol_id);
    }

    #[test]
    fn test_resolve_monitor2_from_google_scope_with_parallel_anonymous_scopes() {
        // GIVEN
        let (symbol_table, _, _, google_scope_id, _, _, _, _, _, monitor2_symbol_id, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN
        // Resolve "Monitor2" from the "google" scope
        let resolved_from_google = symbol_table.resolve(&google_scope_id, "Monitor2", !RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_google.len(), 1);
        assert_eq!(resolved_from_google[0].id(), monitor2_symbol_id);
    }

    #[test]
    fn test_resolve_monitor_from_com_scope_with_parallel_anonymous_scopes() {
        // GIVEN
        let (symbol_table, _, com_scope_id, _, _, _, _, _, monitor_symbol_id, _, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN
        // Resolve "Monitor" from the "com" scope.
        let resolved_from_com =
            symbol_table.resolve(&com_scope_id, "google.sdv.Monitor", !RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_com.len(), 1);
        assert_eq!(resolved_from_com[0].id(), monitor_symbol_id);
    }

    #[test]
    fn test_resolve_monitor2_from_com_scope_with_parallel_anonymous_scopes() {
        // GIVEN
        let (symbol_table, _, com_scope_id, _, _, _, _, _, _, monitor2_symbol_id, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN Resolve "Monitor2" from the "com" scope.
        let resolved_from_com =
            symbol_table.resolve(&com_scope_id, "google.Monitor2", !RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_com.len(), 1);
        assert_eq!(resolved_from_com[0].id(), monitor2_symbol_id);
    }

    #[test]
    fn test_resolve_monitor_from_global_scope_with_parallel_anonymous_scopes() {
        // GIVEN
        let (symbol_table, global_scope_id, _, _, _, _, _, _, monitor_symbol_id, _, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN resolve "Monitor" from the global scope.
        let resolved_from_global =
            symbol_table.resolve(&global_scope_id, "com.google.sdv.Monitor", !RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_global.len(), 1);
        assert_eq!(resolved_from_global[0].id(), monitor_symbol_id);
    }

    #[test]
    fn test_resolve_monitor2_from_global_scope_with_parallel_anonymous_scopes() {
        // GIVEN
        let (symbol_table, global_scope_id, _, _, _, _, _, _, _, monitor2_symbol_id, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN resolve "Monitor2" from the global scope.
        let resolved_from_global =
            symbol_table.resolve(&global_scope_id, "com.google.Monitor2", !RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_global.len(), 1);
        assert_eq!(resolved_from_global[0].id(), monitor2_symbol_id);
    }

    #[test]
    fn test_resolve_monitor_with_dots_from_global_scope() {
        // GIVEN
        let (
            symbol_table,
            global_scope_id,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            monitor_with_dots_symbol_id,
            _,
            _,
        ) = setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN resolve "com.google.Monitor.With.Dots" from the global scope.
        let resolved_from_global = symbol_table.resolve(
            &global_scope_id,
            "com.google.sdv.Monitor.With.Dots",
            !RESOLVE_FQN,
        );

        // THEN
        assert_eq!(resolved_from_global.len(), 1);
        assert_eq!(resolved_from_global[0].id(), monitor_with_dots_symbol_id);
    }

    #[test]
    fn test_resolve_monitor_with_dots_fqn_from_sdv_scope() {
        // GIVEN
        let (
            symbol_table,
            _,
            _,
            _,
            _,
            _,
            _,
            sdv_scope_2_id,
            _,
            _,
            monitor_with_dots_symbol_id,
            _,
            _,
        ) = setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN resolve "com.google.Monitor.With.Dots" from the "sdv" scope.
        let resolved_from_sdv =
            symbol_table.resolve(&sdv_scope_2_id, "com.google.sdv.Monitor.With.Dots", !RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_sdv.len(), 1);
        assert_eq!(resolved_from_sdv[0].id(), monitor_with_dots_symbol_id);
    }

    #[test]
    fn test_resolve_monitor_with_dots_unqualified_from_sdv_scope() {
        // GIVEN
        let (
            symbol_table,
            _,
            _,
            _,
            _,
            _,
            _,
            sdv_scope_2_id,
            _,
            _,
            monitor_with_dots_symbol_id,
            _,
            _,
        ) = setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN resolve "Monitor.With.Dots" from the "sdv" scope.
        let resolved_from_sdv =
            symbol_table.resolve(&sdv_scope_2_id, "Monitor.With.Dots", !RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_sdv.len(), 1);
        assert_eq!(resolved_from_sdv[0].id(), monitor_with_dots_symbol_id);
    }

    #[test]
    fn test_resolve_monitor_deep_down_parallel_to_starting_scope() {
        // GIVEN
        let (symbol_table, _, _, _, _, anonymous_scope_2_id, _, _, _, _, _, monitor4_symbol_id, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN resolve "sdv.very.deep.package.Monitor" from the anonymous2 scope.
        let resolved_from_sdv =
            symbol_table.resolve(&anonymous_scope_2_id, "very.deep.package.Monitor", RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_sdv.len(), 1);
        assert_eq!(resolved_from_sdv[0].id(), monitor4_symbol_id);
    }

    #[test]
    fn test_resolve_unit_type_with_dot_from_sdv_scope_with_resolve_fqn() {
        // GIVEN
        let (
            symbol_table,
            _,
            _,
            _,
            _,
            _,
            sdv_scope_id,
            _,
            _,
            _,
            _,
            _,
            unit_type_with_dot_symbol_id,
        ) = setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN resolve "UnitTypeWithDot" from the sdv scope.
        let resolved_from_sdv = symbol_table.resolve(&sdv_scope_id, "UnitTypeWithDot", RESOLVE_FQN);

        // THEN
        assert_eq!(resolved_from_sdv.len(), 1);
        assert_eq!(resolved_from_sdv[0].id(), unit_type_with_dot_symbol_id);
    }

    #[test]
    fn test_resolve_unit_type_with_dot_from_sdv_scope_without_resolve_fqn() {
        // GIVEN
        let (symbol_table, _, _, _, _, _, sdv_scope_id, _, _, _, _, _, _) =
            setup_with_parallel_anonymous_scopes_and_symbols();

        // WHEN resolve "UnitTypeWithDot" from the sdv scope.
        let resolved_from_sdv =
            symbol_table.resolve(&sdv_scope_id, "UnitTypeWithDot", !RESOLVE_FQN);

        // THEN nothing can be found
        assert_eq!(resolved_from_sdv.len(), 0);
    }

    #[test]
    fn test_resolve_in_non_existent_scope() {
        // GIVEN an empty symbol table and non-existent scope
        let symbol_table = SymbolTable::default();
        let non_existent_scope_id = ScopeId(999);

        // WHEN trying to resolve in the non-existent scope
        let result =
            symbol_table.resolve(&non_existent_scope_id, "this.is.a.placeholder", RESOLVE_FQN);

        // THEN nothing can be found
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_resolve_in_non_existent_subscope() {
        // GIVEN a symbol table with a scope
        let mut symbol_table = SymbolTable::default();
        let scope_id = symbol_table.create_scope("parent".to_string());

        // GIVEN a deliberately non-existing subscope (impossible to create through symbol table public API)
        let scope = symbol_table.scopes.get_mut(&scope_id).unwrap();
        let non_existent_scope_id = ScopeId(999);
        scope.subscopes.push(non_existent_scope_id.clone());

        // WHEN trying to access the non-existing subscope
        let result = symbol_table.resolve(&scope_id, "parent.child", !RESOLVE_FQN);

        // THEN nothing can be found
        assert_eq!(result.len(), 0);
    }

    #[test]
    #[should_panic(expected = "Tried to get symbol 'Some(PlaceholderForTests(SymbolCommon")]
    fn test_get_symbol_macro_panic_wrong_type() {
        let mut symbol_table = SymbolTable::default();
        let symbol_id = symbol_table.create_symbol(
            "TestSymbol".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        // Try to get it as a ServiceBundleSymbol, which should panic.
        let _ = symbol_table.get_service_bundle_symbol(&symbol_id);
    }

    #[test]
    #[should_panic(expected = "Symbol table is used without being linked.")]
    fn test_panic_if_not_linked_when_not_created() {
        let symbol_table = SymbolTable::default();
        symbol_table.panic_if_not_linked();
    }

    #[test]
    #[should_panic(expected = "Symbol table is used without being linked.")]
    fn test_panic_if_not_linked_when_created_not_linked() {
        let mut symbol_table = SymbolTable::default();
        symbol_table.set_state(SymbolTableState::CreatedNotLinked);
        symbol_table.panic_if_not_linked();
    }

    #[test]
    fn test_add_subscope_non_existent_enclosing_scope() {
        let mut symbol_table = SymbolTable::default();
        let subscope_id = symbol_table.create_scope("sub".to_string());
        let non_existent_enclosing_scope_id = ScopeId(999);

        symbol_table.add_subscope(&non_existent_enclosing_scope_id, &subscope_id);

        // Verify subscope's enclosing_scope is still None
        assert!(symbol_table.get_scope(&subscope_id).unwrap().enclosing_scope.is_none());
    }

    #[test]
    fn test_add_subscope_non_existent_subscope() {
        let mut symbol_table = SymbolTable::default();
        let enclosing_scope_id = symbol_table.create_scope("enclosing".to_string());
        let non_existent_subscope_id = ScopeId(999);

        symbol_table.add_subscope(&enclosing_scope_id, &non_existent_subscope_id);

        // Verify enclosing_scope's subscopes list is empty or unchanged
        assert!(symbol_table.get_scope(&enclosing_scope_id).unwrap().subscopes.is_empty());
    }

    #[test]
    fn test_add_symbol_to_scope_non_existent_symbol() {
        let mut symbol_table = SymbolTable::default();
        let scope_id = symbol_table.create_scope("test_scope".to_string());
        let non_existent_symbol_id = SymbolId(999);

        symbol_table.add_symbol_to_scope(&scope_id, &non_existent_symbol_id);

        // Verify scope's symbols map is empty
        assert!(symbol_table.get_scope(&scope_id).unwrap().symbols.is_empty());
        // Verify symbols_fqn map is empty
        assert!(symbol_table.symbols_fqn.is_empty());
    }

    #[test]
    fn test_add_symbol_to_scope_non_existent_scope_id() {
        // GIVEN a symbol table with a single symbol
        let mut symbol_table = SymbolTable::default();
        let scope_id = symbol_table.create_scope("test_scope".to_string());
        let symbol_id = symbol_table.create_symbol(
            "TestSymbol".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        symbol_table.add_symbol_to_scope(&scope_id, &symbol_id);

        // WHEN adding it to a non-existent scope
        let non_existent_scope_id = ScopeId(999);
        symbol_table.add_symbol_to_scope(&non_existent_scope_id, &symbol_id);

        // THEN nothing should have changed
        assert_eq!(symbol_table.get_symbol(&symbol_id).unwrap().symbol().enclosing_scope, scope_id);
    }

    #[test]
    fn test_set_spanning_symbol_non_existent_symbol() {
        let mut symbol_table = SymbolTable::default();
        let scope_id = symbol_table.create_scope("test_scope".to_string());
        let non_existent_symbol_id = SymbolId(999);

        symbol_table.set_spanning_symbol(&scope_id, &non_existent_symbol_id);

        // Verify scope's spanning_symbol is None
        assert!(symbol_table.get_scope(&scope_id).unwrap().spanning_symbol.is_none());
    }

    #[test]
    fn test_get_subscope_by_name_empty_name() {
        let mut symbol_table = SymbolTable::default();
        let scope_id = symbol_table.create_scope("parent".to_string());
        let result = symbol_table.get_subscope_by_name(&scope_id, "");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_subscope_by_name_non_existent_scope() {
        let symbol_table = SymbolTable::default();
        let non_existent_scope_id = ScopeId(999);
        let result = symbol_table.get_subscope_by_name(&non_existent_scope_id, "subscope_name");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_subscope_by_name_non_existent_subscope() {
        // GIVEN a symbol table with a scope
        let mut symbol_table = SymbolTable::default();
        let scope_id = symbol_table.create_scope("parent".to_string());

        // GIVEN a deliberately non-existing subscope (impossible to create through symbol table public API)
        let scope = symbol_table.scopes.get_mut(&scope_id).unwrap();
        let non_existent_scope_id = ScopeId(999);
        scope.subscopes.push(non_existent_scope_id.clone());

        // WHEN trying to access the non-existing subscope
        let result = symbol_table.get_subscope_by_name(&scope_id, "subscope_name");

        // THEN nothing shall be found
        assert!(result.is_none());
    }

    #[test]
    fn test_get_all_symbols_empty_table() {
        let symbol_table = SymbolTable::default();
        assert!(symbol_table.get_all_symbols().is_empty());
    }

    #[test]
    fn test_get_all_symbols_with_data() {
        let mut symbol_table = SymbolTable::default();
        symbol_table.create_symbol(
            "Sym1".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        symbol_table.create_symbol(
            "Sym2".to_string(),
            None,
            SymbolKind::ServiceBundle(ServiceBundleSymbol::default()),
        );
        assert_eq!(symbol_table.get_all_symbols().len(), 2);
    }

    #[test]
    fn test_get_all_scopes_empty_table() {
        let symbol_table = SymbolTable::default();
        // Global scope is not created by default, so this should be empty
        assert!(symbol_table.get_all_scopes().is_empty());
    }

    #[test]
    fn test_get_all_scopes_with_data() {
        let mut symbol_table = SymbolTable::default();
        symbol_table.create_scope("Scope1".to_string());
        symbol_table.create_scope("Scope2".to_string());
        assert_eq!(symbol_table.get_all_scopes().len(), 2);
    }

    #[test]
    fn test_get_service_bundle_symbol_mut_non_existent_id() {
        let mut symbol_table = SymbolTable::default();
        let non_existent_id = SymbolId(999);
        assert!(symbol_table.get_service_bundle_symbol_mut(&non_existent_id).is_none());
    }

    #[test]
    fn test_get_service_bundle_symbol_mut_wrong_type() {
        let mut symbol_table = SymbolTable::default();
        let symbol_id = symbol_table.create_symbol(
            "Test".to_string(),
            None,
            SymbolKind::Publisher(PublisherSymbol::default()),
        );
        assert!(symbol_table.get_service_bundle_symbol_mut(&symbol_id).is_none());
    }

    #[test]
    fn test_resolve_fqn_shadowing_symbol_from_enclosing_scope() {
        // GIVEN a symbol table with a symbol shadowing the name
        // of a symbol from its enclosing scope
        let mut symbol_table = SymbolTable::default();

        // Create scopes: global -> "com" -> "google" -> "Monitor" (symbol)
        //                                -> "Monitor" (symbol, shadowed by com.google.Monitor)
        let global_scope_id = symbol_table.create_scope("".to_string());
        let com_scope_id = symbol_table.create_scope("com".to_string());
        let google_scope_id = symbol_table.create_scope("google".to_string());

        // Add subscopes
        symbol_table.add_subscope(&global_scope_id, &com_scope_id);
        symbol_table.add_subscope(&com_scope_id, &google_scope_id);

        // Create symbols
        let monitor_symbol_id = symbol_table.create_symbol(
            "Monitor".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        let shadowed_monitor_symbol_id = symbol_table.create_symbol(
            "Monitor".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );

        // Add symbol to scopes
        symbol_table.add_symbol_to_scope(&google_scope_id, &monitor_symbol_id);
        symbol_table.add_symbol_to_scope(&com_scope_id, &shadowed_monitor_symbol_id);

        // WHEN resolving "com.google.Monitor" from the global scope
        let resolved_symbols =
            symbol_table.resolve(&global_scope_id, "com.google.Monitor", !RESOLVE_FQN);

        // THEN Should have resolved only one symbol
        assert_eq!(resolved_symbols.len(), 1);
        assert_eq!(resolved_symbols[0].id(), monitor_symbol_id);
    }

    #[test]
    fn test_resolve_recursive_non_existent_scope() {
        let symbol_table = SymbolTable::default();
        let non_existent_scope_id = ScopeId(999);
        let mut visited_scopes = HashSet::new();
        let result = symbol_table.resolve_recursive(
            &non_existent_scope_id,
            "any_name",
            "any_name",
            true,
            &mut visited_scopes,
        );
        assert!(result.is_empty());
    }

    #[test]
    fn test_accept_placeholder_for_tests() {
        let mut symbol_table = SymbolTable::default();
        symbol_table.create_symbol(
            "Placeholder".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );

        let mut mock_checker = MockSymbolTableChecker::new();

        mock_checker.expect_check_service_bundle().never();
        mock_checker.expect_check_publisher().never();

        // before_checks and after_checks will still be called.
        mock_checker.expect_before_checks().once().return_const(());
        mock_checker.expect_after_checks().once().return_const(());

        symbol_table.accept(&mut mock_checker);
    }

    #[test]
    fn test_resolve_explicit_fqns() {
        // GIVEN a symbol table
        let (
            symbol_table,
            global_scope_id,
            com_scope_id,
            google_scope_id,
            anonymous_scope_1_id,
            anonymous_scope_2_id,
            sdv_scope_id,
            sdv_scope_2_id,
            monitor_symbol_id,
            monitor2_symbol_id,
            monitor3_symbol_id,
            monitor4_symbol_id,
            unit_type_with_dot_symbol_id,
        ) = setup_with_parallel_anonymous_scopes_and_symbols();

        let scopes = vec![
            global_scope_id,
            com_scope_id,
            google_scope_id,
            anonymous_scope_1_id,
            anonymous_scope_2_id,
            sdv_scope_id,
            sdv_scope_2_id,
        ];
        let symbols = vec![
            (monitor_symbol_id, ".com.google.sdv.Monitor"),
            (monitor2_symbol_id, ".com.google.Monitor2"),
            (monitor3_symbol_id, ".com.google.sdv.Monitor.With.Dots"),
            (monitor4_symbol_id, ".com.google.very.deep.package.Monitor"),
            (unit_type_with_dot_symbol_id, ".com.google.sdv.UnitTypeWithDot"),
        ];

        for scope_id in &scopes {
            for (symbol_id, fqn) in &symbols {
                // WHEN resolving enforced FQN (leading dot)
                let resolved_with_leading_dot = symbol_table.resolve(scope_id, fqn, !RESOLVE_FQN);

                // THEN it should resolve to the correct symbol
                assert_eq!(
                    resolved_with_leading_dot.len(),
                    1,
                    "Failed for scope '{scope_id:?}' and FQN '{fqn}'"
                );
                assert_eq!(
                    resolved_with_leading_dot[0].id(),
                    symbol_id.clone(),
                    "Wrong symbol resolved for scope '{scope_id:?}' and FQN '{fqn}'"
                );
            }
        }
    }

    #[test]
    fn test_get_symbol_at_location() {
        let mut symbol_table = SymbolTable::default();
        let file_path = PathBuf::from("/test.vsidl");
        let span = Span {
            file_path: file_path.to_str().unwrap().to_string(),
            start: 10,
            end: 20,
            special_fields: Default::default(),
        };
        let symbol_id = symbol_table.create_symbol(
            "TestSymbol".to_string(),
            Some(span),
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );

        let found_symbol = symbol_table.get_symbol_at_location(&file_path, 10).unwrap();
        assert_eq!(found_symbol.id(), symbol_id);

        assert!(symbol_table.get_symbol_at_location(&file_path, 9).is_none());
        assert!(symbol_table.get_symbol_at_location(&file_path, 99).is_none());
        assert!(symbol_table.get_symbol_at_location(&PathBuf::from("/other.vsidl"), 10).is_none());
    }

    #[test]
    fn test_resolve_exlicit_fqn_where_qualified_name_is_ambiguous() {
        // GIVEN a symbol table where from "com.google.Monitor" can refer to
        // two different symbols from the starting scope "com.google", but
        // ".com.google.Monitor" can only refer to a single symbol.
        let mut symbol_table = SymbolTable::default();

        // The created scope hierarchy and the variables are as follows:
        // ```
        // global ("") `global_scope_id`
        // └── com ("com") `com_scope_id`
        //     └── google ("google") `google_scope_id`
        //         ├── contains "Monitor" `monitor_symbol_id`
        //         └── com ("com") `com2_scope_id`
        //             └── google ("google") `google2_scope_id`
        //                 └── contains "Monitor" `monitor2_symbol_id`
        //
        // Starting from the `google_scope_id` scope, "com.google.Monitor" could refer
        // to either the symbol in the nested scope (`monitor2_symbol_id`) or to
        // `monitor_symbol_id` (when interpreting it as a fully qualified name).
        // ".com.google.monitor", however, uniquely identifies `monitor_symbol_id` as
        // resolving has to be started from the global scope due to the leading dot.
        let global_scope_id = symbol_table.create_scope("".to_string());
        let com_scope_id = symbol_table.create_scope("com".to_string());
        let google_scope_id = symbol_table.create_scope("google".to_string());
        let com2_scope_id = symbol_table.create_scope("com".to_string());
        let google2_scope_id = symbol_table.create_scope("google".to_string());

        // Add subscopes
        symbol_table.add_subscope(&global_scope_id, &com_scope_id);
        symbol_table.add_subscope(&com_scope_id, &google_scope_id);
        symbol_table.add_subscope(&google_scope_id, &com2_scope_id);
        symbol_table.add_subscope(&com2_scope_id, &google2_scope_id);

        // Create symbols
        let monitor_symbol_id = symbol_table.create_symbol(
            "Monitor".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );
        let monitor2_symbol_id = symbol_table.create_symbol(
            "Monitor".to_string(),
            None,
            SymbolKind::PlaceholderForTests(SymbolCommon::default()),
        );

        // Add symbol to scopes
        symbol_table.add_symbol_to_scope(&google_scope_id, &monitor_symbol_id);
        symbol_table.add_symbol_to_scope(&google2_scope_id, &monitor2_symbol_id);

        // WHEN resolving "com.google.Monitor" from "com.google"
        let resolved_from_com =
            symbol_table.resolve(&google_scope_id, "com.google.Monitor", !RESOLVE_FQN);

        // THEN Should have resolved both symbols
        assert_eq!(resolved_from_com.len(), 2);

        // WHEN resolving ".com.google.Monitor" (enforces fully qualified name)
        let resolved_from_com_fqn =
            symbol_table.resolve(&google_scope_id, ".com.google.Monitor", !RESOLVE_FQN);

        // THEN Should have resolved only the first symbol
        assert_eq!(resolved_from_com_fqn.len(), 1);
        assert_eq!(resolved_from_com_fqn[0].id(), monitor_symbol_id);
    }
}
