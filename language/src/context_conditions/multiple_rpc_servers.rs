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
use std::collections::HashSet;
use vsidl_v1_common::VsidlVisitor;
use vsidl_v1_symbols::{
    service_bundle_symbol::ServiceBundleSymbol, SymbolTable, SymbolTableChecker,
};

#[derive(Debug, Default)]
pub struct MultipleRpcServersForSameInterface {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(MultipleRpcServersForSameInterface);
impl VsidlVisitor for MultipleRpcServersForSameInterface {}

/// Issues a warning if a service bundle contains multiple servers serving the same service
impl SymbolTableChecker for MultipleRpcServersForSameInterface {
    fn check_service_bundle(
        &mut self,
        symbol_table: &SymbolTable,
        service_bundle: &ServiceBundleSymbol,
    ) {
        // List of all unit types used by servers in the same service bundle
        let mut services: HashSet<String> = HashSet::new();

        for server in &service_bundle.servers(symbol_table) {
            let service_name = server.service(symbol_table).full_name(symbol_table);
            // If we've already seen the same service name, it's not unique
            if services.contains(&service_name.clone()) {
                self.finding_collector.warn(
                    FindingId::MultipleRpcServersForSameInterface,
                    format!(
                        "Service bundle '{}' contains multiple servers for service '{}'",
                        service_bundle.full_name(symbol_table),
                        server.service
                    ),
                    Some("Service bundle contains multiple servers for this service".to_string()),
                    vec![server.service_span.clone().into_option()],
                );
            }

            // Remember that the current service name has already been already seen
            services.insert(service_name);
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::multiple_rpc_servers::MultipleRpcServersForSameInterface;
    use crate::FindingSeverity;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::SymbolTableCreator;
    use vsidl_v1_symbols::SymbolTableLinker;

    #[test]
    fn test_should_reject_catalog_with_duplicated_rpc_name() {
        // Given: A catalog with a duplicated user-defined RPC name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_DUPLICATED_UNNAMED_SERVER.clone());

        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // When: Executing context condition
        let mut check = MultipleRpcServersForSameInterface::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // Then: Should have found warning
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Warn, "DummyRpc"));
    }
}
