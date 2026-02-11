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

use super::{FindingCollector, FindingId};
use crate::impl_context_condition_symtab;
use std::collections::{HashMap, HashSet};
use vsidl_v1::syntax::Span;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::specific_symbols::publisher_symbol::PublisherSymbol;
use vsidl_v1_symbols::symbol::{SymbolId, SymbolKind};
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

#[derive(Default)]
pub struct ChannelNameUnique {
    finding_collector: FindingCollector,
    // Maps channel name to the unique message types used with it
    channel_to_messages: HashMap<String, HashSet<SymbolId>>,
    // Maps channel name to the spans of all publishers using it
    channel_to_spans: HashMap<String, Vec<Option<Span>>>,
}
impl_context_condition_symtab!(ChannelNameUnique);
impl VsidlVisitor for ChannelNameUnique {}

/// Issues an error if there are publishers with different message symbols but same channel name
impl SymbolTableChecker for ChannelNameUnique {
    fn check_publisher(&mut self, symbol_table: &SymbolTable, publisher: &PublisherSymbol) {
        let message_type = &publisher.ast_node.message;
        let enclosing_scope_id = &publisher.symbol.enclosing_scope;

        // Resolve the message name manually since the symbol table might not be linked
        let resolved_messages: Vec<_> = symbol_table
            .resolve(enclosing_scope_id, message_type, false)
            .into_iter()
            .filter(|symbol| matches!(symbol, SymbolKind::ProtoMessage(_)))
            .collect();

        if resolved_messages.is_empty() {
            // Message resolution errors are handled by another CoCo
            return;
        }
        let message_id = resolved_messages[0].id().clone();

        // Collect message types and spans for each channel used by this publisher
        for channel in &publisher.ast_node.channel {
            self.channel_to_messages.entry(channel.clone()).or_default().insert(message_id.clone());
            self.channel_to_spans
                .entry(channel.clone())
                .or_default()
                .push(publisher.symbol.span.clone());
        }
    }

    fn after_checks(&mut self, symbol_table: &SymbolTable) {
        let mut sorted_channels: Vec<_> = self.channel_to_messages.keys().collect();
        sorted_channels.sort();

        for channel in sorted_channels {
            let messages = self.channel_to_messages.get(channel).unwrap();
            if messages.len() > 1 {
                let mut message_names: Vec<String> = messages
                    .iter()
                    .map(|id| symbol_table.get_symbol(id).unwrap().full_name(symbol_table))
                    .collect();
                message_names.sort();

                let spans = self.channel_to_spans.get(channel).unwrap().clone();

                self.finding_collector.error(
                    FindingId::ChannelNameUnique,
                    format!(
                        "Channel name '{}' is used for {} different message types: {}",
                        channel,
                        messages.len(),
                        message_names.join(", ")
                    ),
                    spans,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_condition::ContextCondition;
    use crate::FindingSeverity;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;

    #[test]
    fn test_should_reject_conflicting_channel_names() {
        // GIVEN a catalog with conflicting channel names for different types
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS.clone()]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_CHANNEL_NAME_CONFLICT.clone());

        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = ChannelNameUnique::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found errors
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Channel name 'CONFLICT' is used for 2 different message types:"
        ));
        assert!(check.any_finding_contains(FindingSeverity::Error, "sdv.test.FuelLevel"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "sdv.test.Pressure"));
    }

    #[test]
    fn test_should_accept_unique_channel_names() {
        // GIVEN A valid catalog with unique channel names
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = ChannelNameUnique::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }
}
