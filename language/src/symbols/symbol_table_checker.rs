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
    specific_symbols::{
        proto_enum_symbol::ProtoEnumSymbol, proto_message_symbol::ProtoMessageSymbol,
        proto_publication_symbol::ProtoPublicationSymbol,
        proto_rpc_service_symbol::ProtoRpcServiceSymbol, publisher_symbol::PublisherSymbol,
        rpc_client_symbol::RpcClientSymbol, rpc_server_symbol::RpcServerSymbol,
        service_bundle_symbol::ServiceBundleSymbol, subscriber_symbol::SubscriberSymbol,
        topic_symbol::TopicSymbol,
    },
    symbol_table::SymbolTable,
};
use mockall::automock;

/// A trait for implementing checks that validate the correctness and consistency
/// of symbols within a [SymbolTable].
///
/// Implementors of this trait can define checks that run at different stages
/// of symbol table processing (before checks, during symbol traversal, and after checks).
/// This is typically used by context conditions that operate on the symbol table.
#[automock]
pub trait SymbolTableChecker: AsDynChecker {
    /// Called once before traversing the symbol table and running specific checks.
    ///
    /// Implementations can use this method to initialize internal state needed for the checks.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table that is about to be checked.
    fn before_checks(&mut self, _symbol_table: &SymbolTable) {}

    /// Checks a specific [TopicSymbol].
    ///
    /// This method is called for each channel symbol found during the traversal
    /// of the symbol table.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table containing the symbol.
    /// * `_channel` - The channel symbol to check.
    fn check_channel(&mut self, _symbol_table: &SymbolTable, _channel: &TopicSymbol) {}

    /// Checks a specific [ServiceBundleSymbol].
    ///
    /// This method is called for each service bundle symbol found during the traversal
    /// of the symbol table.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table containing the symbol.
    /// * `_service_bundle` - The service bundle symbol to check.
    fn check_service_bundle(
        &mut self,
        _symbol_table: &SymbolTable,
        _service_bundle: &ServiceBundleSymbol,
    ) {
    }

    /// Checks a specific [PublisherSymbol].
    ///
    /// This method is called for each publisher symbol found during the traversal
    /// of the symbol table.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table containing the symbol.
    /// * `_publisher` - The publisher symbol to check.
    fn check_publisher(&mut self, _symbol_table: &SymbolTable, _publisher: &PublisherSymbol) {}

    /// Checks a specific [SubscriberSymbol].
    ///
    /// This method is called for each subscriber symbol found during the traversal
    /// of the symbol table.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table containing the symbol.
    /// * `_subscriber` - The subscriber symbol to check.
    fn check_subscriber(&mut self, _symbol_table: &SymbolTable, _subscriber: &SubscriberSymbol) {}

    /// Checks a specific [RpcServerSymbol].
    ///
    /// This method is called for each RPC server symbol found during the traversal
    /// of the symbol table.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table containing the symbol.
    /// * `_rpc_server` - The RPC server symbol to check.
    fn check_rpc_server(&mut self, _symbol_table: &SymbolTable, _rpc_server: &RpcServerSymbol) {}

    /// Checks a specific [RpcClientSymbol].
    ///
    /// This method is called for each RPC client symbol found during the traversal
    /// of the symbol table.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table containing the symbol.
    /// * `_rpc_client` - The RPC client symbol to check.
    fn check_rpc_client(&mut self, _symbol_table: &SymbolTable, _rpc_client: &RpcClientSymbol) {}

    /// Checks a specific [ProtoMessageSymbol].
    ///
    /// This method is called for each Protobuf message symbol found during the traversal
    /// of the symbol table.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table containing the symbol.
    /// * `_message` - The Protobuf message symbol to check.
    fn check_proto_message(&mut self, _symbol_table: &SymbolTable, _message: &ProtoMessageSymbol) {}

    /// Checks a specific [ProtoEnumSymbol].
    ///
    /// This method is called for each Protobuf enum symbol found during the traversal
    /// of the symbol table.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table containing the symbol.
    /// * `_enum` - The Protobuf enum symbol to check.
    fn check_proto_enum(&mut self, _symbol_table: &SymbolTable, _enum: &ProtoEnumSymbol) {}

    /// Checks a specific [ProtoPublicationSymbol].
    ///
    /// This method is called for each Protobuf publication symbol found during the traversal
    /// of the symbol table.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table containing the symbol.
    /// * `_publication` - The Protobuf publication symbol to check.
    fn check_proto_publication(
        &mut self,
        _symbol_table: &SymbolTable,
        _publication: &ProtoPublicationSymbol,
    ) {
    }

    /// Checks a specific [ProtoRpcServiceSymbol].
    ///
    /// This method is called for each Protobuf RPC service symbol found during the traversal
    /// of the symbol table.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table containing the symbol.
    /// * `_rpc_service` - The Protobuf RPC service symbol to check.
    fn check_proto_rpc_service(
        &mut self,
        _symbol_table: &SymbolTable,
        _rpc_service: &ProtoRpcServiceSymbol,
    ) {
    }

    /// Called once after traversing the symbol table and running all specific checks.
    ///
    /// Implementations can use this method to perform final validation based on
    /// collected information or to clean up internal state.
    ///
    /// # Parameters
    ///
    /// * `_symbol_table` - The symbol table that has been checked.
    fn after_checks(&mut self, _symbol_table: &SymbolTable) {}
}

/// Helper trait to allow casting implementors of [SymbolTableChecker] to `&mut dyn SymbolTableChecker`.
///
/// This is used internally to work with collections of different checker types.
pub trait AsDynChecker {
    /// Returns a mutable reference to the object as a dynamic [SymbolTableChecker] trait object.
    fn as_mut_dyn_checker(&mut self) -> &mut dyn SymbolTableChecker;
}

/// Blanket implementation of [AsDynChecker] for any type that implements [SymbolTableChecker].
impl<T: SymbolTableChecker> AsDynChecker for T {
    /// Returns `self` as a mutable reference to a dynamic [SymbolTableChecker].
    fn as_mut_dyn_checker(&mut self) -> &mut dyn SymbolTableChecker {
        self
    }
}
