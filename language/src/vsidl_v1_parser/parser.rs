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
//! Extract all information from the catalog to be used for code-generation
use crate::symbol_table_creator::SymbolTableCreator;
use crate::symbol_table_linker::SymbolTableLinker;
use crate::visitors::keyword_converter::KeywordConverter;
use crate::visitors::visitor::AstNode;
use crate::Catalog;
use crate::SymbolTable;
use crate::TargetApi;
use heck::ToKebabCase;
use vsidl_v1_context_conditions::*;

/// Construct a unit name from protobuf FQN
pub fn get_rpc_unit_name(fqn: &str) -> anyhow::Result<String> {
    let name = fqn.replace('.', "-").to_kebab_case();
    Ok(name)
}

/// Validate a catalog and create a symbol table from it
pub fn create_symbol_table(
    catalog: &Catalog,
    mut coco_checker: ContextConditionChecker,
    target_api: TargetApi,
) -> anyhow::Result<SymbolTable> {
    let mut catalog = catalog.clone();
    KeywordConverter::traverse(&mut catalog);
    coco_checker.check_ast(&catalog)?;
    let mut creator = SymbolTableCreator::new();
    creator.set_target_api(target_api);
    catalog.accept(&mut creator);
    coco_checker.check_symbol_table(creator.symbol_table())?;
    let mut linker = SymbolTableLinker::from(creator);
    linker.execute();
    coco_checker.check_linked_symbol_table(linker.symbol_table())?;
    Ok(linker.symbol_table().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_rpc_unit_name_simple_fqn() {
        let fqn = "com.example.MyService";
        let expected = "com-example-my-service";
        assert_eq!(get_rpc_unit_name(fqn).unwrap(), expected);
    }

    #[test]
    fn test_get_rpc_unit_name_already_kebab_case_with_dots() {
        let fqn = "com.example.my-service";
        let expected = "com-example-my-service";
        assert_eq!(get_rpc_unit_name(fqn).unwrap(), expected);
    }

    #[test]
    fn test_get_rpc_unit_name_mixed_casing() {
        let fqn = "Com.Example.AnotherService";
        let expected = "com-example-another-service";
        assert_eq!(get_rpc_unit_name(fqn).unwrap(), expected);
    }

    #[test]
    fn test_get_rpc_unit_name_empty_fqn() {
        let fqn = "";
        let expected = "";
        assert_eq!(get_rpc_unit_name(fqn).unwrap(), expected);
    }

    #[test]
    fn test_get_rpc_unit_name_no_dots() {
        let fqn = "MyService";
        let expected = "my-service";
        assert_eq!(get_rpc_unit_name(fqn).unwrap(), expected);
    }

    #[test]
    fn test_get_rpc_unit_name_with_underscores() {
        let fqn = "com.example_service.My_Service";
        let expected = "com-example-service-my-service"; // underscores are treated as word separators by to_kebab_case
        assert_eq!(get_rpc_unit_name(fqn).unwrap(), expected);
    }

    #[test]
    fn test_get_rpc_unit_name_with_numbers() {
        let fqn = "com.example.Service123.V4";
        let expected = "com-example-service123-v4";
        assert_eq!(get_rpc_unit_name(fqn).unwrap(), expected);
    }

    #[test]
    fn test_create_symbol_table() {
        // GIVEN a catalog with a service bundle, an RPC server and a Proto RPC service
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());

        // GIVEN the default CoCo checker
        let coco_checker = ContextConditionChecker::default();

        // WHEN creating a symbol table
        let symbol_table = create_symbol_table(&catalog, coco_checker, TargetApi::Pdk29).unwrap();

        // THEN the symbol table contains the expected symbols
        // Note: These are not all symbols, but just a short sanity check.
        // The completeness is tested in SymbolTableCreator's and SymbolTableLinker's tests.
        assert!(!symbol_table.resolve_service_bundle_fqn("sdv.test.ServiceBundleOne").is_empty());
        assert!(!symbol_table.resolve_service_bundle_fqn("sdv.test.ServiceBundleTwo").is_empty());
        assert!(!symbol_table.resolve_rpc_server_fqn("sdv.test.DummyRpc").is_empty());
        assert!(!symbol_table.resolve_rpc_server_fqn("sdv.test.AnotherDummyRpc").is_empty());
        assert!(!symbol_table.resolve_rpc_client_fqn("sdv.test.DummyRpc").is_empty());
        assert!(!symbol_table.resolve_rpc_client_fqn("sdv.test.AnotherDummyRpc").is_empty());
    }
}
