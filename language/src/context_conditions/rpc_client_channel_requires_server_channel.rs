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
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::specific_symbols::rpc_client_symbol::RpcClientSymbol;
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

#[derive(Default)]
pub struct RpcClientChannelRequiresServerChannel {
    finding_collector: FindingCollector,
}

impl_context_condition_symtab!(RpcClientChannelRequiresServerChannel);

impl VsidlVisitor for RpcClientChannelRequiresServerChannel {}

impl SymbolTableChecker for RpcClientChannelRequiresServerChannel {
    fn check_rpc_client(&mut self, symbol_table: &SymbolTable, rpc_client: &RpcClientSymbol) {
        let channel = rpc_client.channel(symbol_table);
        let requested_service = rpc_client.service(symbol_table);

        let has_server_on_channel = channel
            .servers(symbol_table)
            .iter()
            .any(|server| server.service(symbol_table).id() == requested_service.id());

        if !has_server_on_channel {
            self.finding_collector.error(
                FindingId::RpcClientChannelRequiresServerChannel,
                format!(
                    "RPC client '{}' uses channel '{}' for service '{}', but no server for this service is defined on this channel",
                    rpc_client.full_name(symbol_table),
                    channel.name(),
                    requested_service.full_name(symbol_table)
                ),
                vec![rpc_client.span.clone().into_option()],
            );
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
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;
    use vsidl_v1_symbols::TargetApi;

    #[test]
    fn test_should_reject_client_without_server_on_same_channel() {
        // GIVEN A catalog where a client uses "channel-a" but server is on "channel-b"
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);

        let mut vsidl_entry = vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.1.clone();
        // Client uses channel-a
        vsidl_entry.service_bundle[0].client[0].channel = "channel-a".to_string();
        vsidl_entry.service_bundle[0].client[1].channel = "other-channel".to_string();
        // Server uses channel-b
        vsidl_entry.service_bundle[1].server[0].channel = "channel-b".to_string();
        vsidl_entry.service_bundle[1].server[1].channel = "other-channel".to_string();

        catalog
            .catalog_vsidl_files
            .push((vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.0.clone(), vsidl_entry));

        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = RpcClientChannelRequiresServerChannel::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN Should have found an error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "uses channel 'channel-a'"));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "no server for this service is defined on this channel"
        ));
    }

    #[test]
    fn test_should_accept_client_with_server_on_same_channel() {
        // GIVEN A catalog where both client and server use the same channel
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO_NO_ANNOTATIONS.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);

        let mut vsidl_entry = vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.1.clone();
        vsidl_entry.service_bundle[1].server[0].channel = "main".to_string();
        vsidl_entry.service_bundle[1].server[1].channel = "main2".to_string();
        vsidl_entry.service_bundle[0].client[0].channel = "main".to_string();
        vsidl_entry.service_bundle[0].client[1].channel = "main2".to_string();

        catalog
            .catalog_vsidl_files
            .push((vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.0.clone(), vsidl_entry));

        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = RpcClientChannelRequiresServerChannel::default();
        check.check_symbol_table(linker.symbol_table());

        // THEN Should be OK
        assert!(check.is_ok());
        assert!(check.finding_collector().findings().is_empty());
    }
}
