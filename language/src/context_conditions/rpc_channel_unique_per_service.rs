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
use vsidl_v1::span::Span;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::specific_symbols::rpc_server_symbol::RpcServerSymbol;
use vsidl_v1_symbols::symbol::{SymbolId, SymbolKind};
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

#[derive(Default)]
pub struct RpcChannelUniquePerService {
    finding_collector: FindingCollector,
    // Maps channel names to the unique service symbols used with it
    channel_to_services: HashMap<String, HashSet<SymbolId>>,
    // Maps channel names to the spans of all servers using it
    channel_to_spans: HashMap<String, Vec<Option<Span>>>,
}

impl_context_condition_symtab!(RpcChannelUniquePerService);
impl VsidlVisitor for RpcChannelUniquePerService {}

impl SymbolTableChecker for RpcChannelUniquePerService {
    fn check_rpc_server(&mut self, symbol_table: &SymbolTable, server: &RpcServerSymbol) {
        let service_name = &server.ast_node.service;
        let enclosing_scope_id = &server.symbol.enclosing_scope;

        // Resolve the service name manually since the symbol table might not be linked
        let resolved_services: Vec<_> = symbol_table
            .resolve(enclosing_scope_id, service_name, false)
            .into_iter()
            .filter(|symbol| matches!(symbol, SymbolKind::ProtoRpcService(_)))
            .collect();

        if resolved_services.is_empty() {
            // Service resolution errors are handled by another CoCo
            return;
        }
        let service_id = resolved_services[0].id().clone();
        let channel_name = server.ast_node.channel.clone();

        if !channel_name.is_empty() {
            self.channel_to_services.entry(channel_name.clone()).or_default().insert(service_id);
            self.channel_to_spans.entry(channel_name).or_default().push(server.symbol.span.clone());
        }
    }

    fn after_checks(&mut self, symbol_table: &SymbolTable) {
        let mut sorted_channels: Vec<_> = self.channel_to_services.keys().collect();
        sorted_channels.sort();

        for channel in sorted_channels {
            let services = self.channel_to_services.get(channel).unwrap();
            if services.len() > 1 {
                let mut service_names: Vec<String> = services
                    .iter()
                    .map(|id| symbol_table.get_symbol(id).unwrap().full_name(symbol_table))
                    .collect();
                service_names.sort();

                let spans = self.channel_to_spans.get(channel).unwrap().clone();

                self.finding_collector.error(
                    FindingId::RpcChannelUniquePerService,
                    format!(
                        "RPC channel '{}' is used for {} different services: {}",
                        channel,
                        services.len(),
                        service_names.join(", ")
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
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::TargetApi;

    #[test]
    fn test_should_reject_conflicting_channel_services() {
        // GIVEN a catalog where two different services use the same channel
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);

        let mut vsidl_entry = vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.1.clone();

        // Service 1 uses channel "main"
        vsidl_entry.service_bundle[1].server[0].channel = "main".to_string();
        vsidl_entry.service_bundle[0].client[0].channel = "main".to_string();
        // Service 2 also uses channel "main" -> conflict
        vsidl_entry.service_bundle[1].server[1].channel = "main".to_string();
        vsidl_entry.service_bundle[0].client[1].channel = "main".to_string();

        catalog
            .catalog_vsidl_files
            .push((vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.0.clone(), vsidl_entry));

        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = RpcChannelUniquePerService::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found errors
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "RPC channel 'main' is used for 2 different services:"
        ));
        assert!(check.any_finding_contains(FindingSeverity::Error, "sdv.test.AnotherDummyRpc"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "sdv.test.DummyRpc"));
    }

    #[test]
    fn test_should_accept_unique_channel_services() {
        // GIVEN A valid catalog with unique channel per service
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);

        let mut vsidl_entry = vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.1.clone();
        // Set mandatory channels for all elements
        vsidl_entry.service_bundle[0].client[0].channel = "main1".to_string();
        vsidl_entry.service_bundle[0].client[1].channel = "main2".to_string();
        vsidl_entry.service_bundle[1].server[0].channel = "main1".to_string();
        vsidl_entry.service_bundle[1].server[1].channel = "main2".to_string();

        catalog
            .catalog_vsidl_files
            .push((vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.0.clone(), vsidl_entry));

        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = RpcChannelUniquePerService::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }
}
