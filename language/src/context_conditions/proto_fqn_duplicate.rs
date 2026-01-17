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

use super::impl_context_condition_symtab;
use super::{FindingCollector, FindingId};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::proto_message_symbol::ProtoMessageSymbol;
use vsidl_v1_symbols::proto_rpc_service_symbol::ProtoRpcServiceSymbol;
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

/// Checks that there are no two service bundles with the same fully-qualified name
#[derive(Debug, Default)]
pub struct ProtoFqnDuplicate {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(ProtoFqnDuplicate);
impl VsidlVisitor for ProtoFqnDuplicate {}

impl SymbolTableChecker for ProtoFqnDuplicate {
    fn check_proto_message(&mut self, symbol_table: &SymbolTable, message: &ProtoMessageSymbol) {
        let fqn = message.full_name(symbol_table);
        let messages = symbol_table.resolve_proto_message_fqn(fqn.clone());

        if messages.len() > 1 {
            self.finding_collector.warn(
                FindingId::ProtoFqnDuplicate,
                format!(
                    "There are multiple Protobuf messages with the fully-qualified name '{fqn}'"
                ),
                vec![message.span()],
            );
        }
    }

    fn check_proto_rpc_service(
        &mut self,
        symbol_table: &SymbolTable,
        rpc_service: &ProtoRpcServiceSymbol,
    ) {
        let fqn = rpc_service.full_name(symbol_table);
        let services = symbol_table.resolve_proto_rpc_service_fqn(fqn.clone());

        if services.len() > 1 {
            self.finding_collector.warn(
                FindingId::ProtoFqnDuplicate,
                format!("There are multiple Protobuf RPC services with the fully-qualified name '{fqn}'"),
                vec![rpc_service.span()],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::context_condition::ContextCondition;
    use crate::proto_fqn_duplicate::ProtoFqnDuplicate;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;

    #[test]
    fn test_shall_reject_two_proto_messages_with_same_fqn() {
        // GIVEN a Catalog and SymbolTable with a name conflict
        let mut catalog = Catalog::new();
        let mut creator = SymbolTableCreator::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(), // Duplicated messages
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
        ]);
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = ProtoFqnDuplicate::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "There are multiple Protobuf messages with the fully-qualified name 'sdv.test.BatteryLevel'"
        ));
    }

    #[test]
    fn test_shall_reject_two_proto_services_with_same_fqn() {
        // GIVEN a Catalog and SymbolTable with a name conflict
        let mut catalog = Catalog::new();
        let mut creator = SymbolTableCreator::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(), // Duplicated RPC services
        ]);
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = ProtoFqnDuplicate::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "There are multiple Protobuf RPC services with the fully-qualified name 'sdv.test.DummyRpc'"
        ));
    }

    #[test]
    fn test_should_accept_valid_catalog() {
        // GIVEN A valid catalog
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

        // WHEN Executing context condition
        let mut check = ProtoFqnDuplicate::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }
}
