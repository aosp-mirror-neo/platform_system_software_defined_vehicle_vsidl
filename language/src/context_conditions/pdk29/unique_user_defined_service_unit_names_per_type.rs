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

use crate::impl_context_condition_symtab;
use std::collections::{HashMap, HashSet};
use vsidl_v1::syntax::Span;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::{
    publisher_symbol::PublisherSymbol, rpc_server_symbol::RpcServerSymbol,
    symbol_table_checker::SymbolTableChecker, SymbolTable,
};

use crate::{FindingCollector, FindingId};

/// Checks that a user-defined service unit name is unique for every type, i.e.,
/// a user-defined name service unit name cannot be used for the same unit type
/// even if those service units are defined in different service bundles
#[derive(Debug, Default)]
pub struct UniqueUserDefinedServiceUnitNamesPerType {
    finding_collector: FindingCollector,
    // maps unit type names to all user-defined names that have been assigned to the type
    seen_rpc_names: HashMap<String, Vec<(String, Option<Span>)>>,
    // maps unit type names to all user-defined names that have been assigned to the type
    seen_pub_names: HashMap<String, Vec<(String, Option<Span>)>>,
}
impl_context_condition_symtab!(UniqueUserDefinedServiceUnitNamesPerType);
impl VsidlVisitor for UniqueUserDefinedServiceUnitNamesPerType {}

impl SymbolTableChecker for UniqueUserDefinedServiceUnitNamesPerType {
    fn check_rpc_server(&mut self, symbol_table: &SymbolTable, rpc_server: &RpcServerSymbol) {
        let server_service_name = rpc_server.service(symbol_table).full_name(symbol_table);

        let rpc_server_name = rpc_server.service_unit_name(symbol_table);
        if rpc_server.has_user_defined_name() {
            append(
                &mut self.seen_rpc_names,
                server_service_name.clone(),
                (rpc_server_name.clone(), rpc_server.span()),
            );
        }
    }

    fn check_publisher(&mut self, symbol_table: &SymbolTable, publisher: &PublisherSymbol) {
        let publication_type_name = publisher.publication(symbol_table).full_name(symbol_table);

        for channel_name in publisher.channels(symbol_table) {
            let publisher_name = publisher.service_unit_name(symbol_table, &channel_name);
            if publisher.has_user_defined_name() {
                append(
                    &mut self.seen_pub_names,
                    publication_type_name.clone(),
                    (publisher_name.clone(), publisher.span()),
                );
            }
        }
    }

    fn after_checks(&mut self, _symbol_table: &SymbolTable) {
        // Check if there are duplicated RPC servers
        for server_name in self.seen_rpc_names.keys() {
            if let Some((duplicate, spans)) =
                Self::find_duplicate_name(self.seen_rpc_names.get(server_name).unwrap())
            {
                self.finding_collector.error(
                    FindingId::UniqueUserDefinedServiceUnitNamesPerTypePublisher,
                    format!("'{duplicate}' is specified as the name of multiple rpc servers"),
                    spans,
                );
            }
        }

        // Check if there are duplicated publications
        for publisher_name in self.seen_pub_names.keys() {
            if let Some((duplicate, spans)) =
                Self::find_duplicate_name(self.seen_pub_names.get(publisher_name).unwrap())
            {
                self.finding_collector.error(
                    FindingId::UniqueUserDefinedServiceUnitNamesPerTypePublisher,
                    format!("'{duplicate}' is specified as the name of multiple publishers"),
                    spans,
                );
            }
        }
    }
}

impl UniqueUserDefinedServiceUnitNamesPerType {
    // Checks if the given vector contains duplicates and if so, returns the first found duplicate
    pub fn find_duplicate_name(
        elements: &[(String, Option<Span>)],
    ) -> Option<(String, Vec<Option<Span>>)> {
        let mut seen = HashSet::new();
        elements.iter().find_map(|(elem, _)| {
            if !seen.insert(elem.clone()) {
                let spans =
                    elements.iter().filter(|(n, _)| n == elem).map(|(_, s)| s.clone()).collect();
                Some((elem.clone(), spans))
            } else {
                None
            }
        })
    }
}

/// Add a value to the Vector for the given key. Creates a new Vec from the value if key is unknown in the map.
fn append(
    map: &mut HashMap<String, Vec<(String, Option<Span>)>>,
    key: String,
    value: (String, Option<Span>),
) {
    if let Some(existing_vector) = map.get_mut(&key) {
        existing_vector.push(value);
    } else {
        map.insert(key, vec![value]);
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::pdk29::UniqueUserDefinedServiceUnitNamesPerType;
    use crate::FindingSeverity;
    use vsidl_v1_common::{AstNode, Catalog};
    use vsidl_v1_symbols::SymbolTableCreator;
    use vsidl_v1_symbols::SymbolTableLinker;
    use vsidl_v1_symbols::TargetApi;

    #[test]
    fn test_should_reject_catalog_with_duplicated_rpc_name() {
        // Given: A catalog with a duplicated user-defined RPC name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_DUPLICATED_USER_DEFINED_RPC_NAME_PER_TYPE.clone());

        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // When: Executing context condition
        let mut check = UniqueUserDefinedServiceUnitNamesPerType::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // Then: context condition should find errors
        assert!(check.is_err());

        // Then: Error message contains the duplicated service name
        assert!(check.any_finding_contains(FindingSeverity::Error, "my-service"));
    }

    #[test]
    fn test_should_reject_catalog_with_duplicated_publisher_name() {
        // Given: A catalog with a duplicated user-defined publisher name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_DUPLICATED_USER_DEFINED_PUB_NAME_PER_TYPE.clone());

        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Pdk29);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // When: Executing context condition
        let mut check = UniqueUserDefinedServiceUnitNamesPerType::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // Then: context condition should find errors
        assert!(check.is_err());

        // Then: Error message contains the duplicated publisher name
        assert!(check.any_finding_contains(FindingSeverity::Error, "my-pub"));
    }
}
