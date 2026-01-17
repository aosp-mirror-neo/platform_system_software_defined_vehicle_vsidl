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
use vsidl_v1_symbols::specific_symbols::service_bundle_symbol::ServiceBundleSymbol;
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

/// Checks that there are no two service bundles with the same fully-qualified name
#[derive(Debug, Default)]
pub struct ServiceBundleFqnDuplicate {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(ServiceBundleFqnDuplicate);
impl VsidlVisitor for ServiceBundleFqnDuplicate {}

impl SymbolTableChecker for ServiceBundleFqnDuplicate {
    fn check_service_bundle(
        &mut self,
        symbol_table: &SymbolTable,
        service_bundle: &ServiceBundleSymbol,
    ) {
        let fqn = service_bundle.full_name(symbol_table);
        let bundles = symbol_table.resolve_service_bundle_fqn(fqn.clone());

        if bundles.len() > 1 {
            self.finding_collector.error(
                FindingId::ServiceBundleFqnDuplicate,
                format!("There are multiple service bundles with the fully-qualified name '{fqn}'"),
                vec![service_bundle.name_span.clone().into_option()],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::context_condition::ContextCondition;
    use crate::service_bundle_fqn_duplicate::ServiceBundleFqnDuplicate;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;

    #[test]
    fn test_shall_reject_two_service_bundles_with_same_fqn() {
        // GIVEN a Catalog and SymbolTable with a name conflict
        let mut catalog = Catalog::new();
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SERVICE_BUNDLES_WITH_SAME_FQN.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = ServiceBundleFqnDuplicate::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "There are multiple service bundles with the fully-qualified name 'sdv.test.Source'"
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
        let mut check = ServiceBundleFqnDuplicate::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }
}
