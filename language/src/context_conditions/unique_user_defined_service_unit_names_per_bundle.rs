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
use crate::impl_context_condition;
use std::collections::HashSet;
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::Event;
use vsidl_v1::syntax::service_bundle::{Publisher, Server};
use vsidl_v1::syntax::ServiceBundle;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct UniqueUserDefinedServiceUnitNamesPerBundle {
    finding_collector: FindingCollector,
    seen_rpc_names: HashSet<String>,
    seen_pub_names: HashSet<String>,
}
impl_context_condition!(UniqueUserDefinedServiceUnitNamesPerBundle);

impl VsidlVisitor for UniqueUserDefinedServiceUnitNamesPerBundle {
    // Reset collections for every service bundle
    fn start_visit_service_bundle(&mut self, _node: &ServiceBundle) {
        self.seen_rpc_names = HashSet::new();
        self.seen_pub_names = HashSet::new();
    }

    // Collect user-defined publisher names
    fn start_visit_publisher(&mut self, node: &Publisher) {
        // Ignore service units without a user-defined name
        if node.service_unit_name.is_empty() {
            return;
        }

        if !self.seen_pub_names.insert(node.service_unit_name.clone()) {
            self.finding_collector.error(
                FindingId::UniqueUserDefinedServiceUnitNamesPerBundlePublisher,
                format!(
                    "'{}' is specified as the name of multiple publishers",
                    node.service_unit_name
                ),
                vec![node.service_unit_name_span.clone().into_option()],
            );
        }
    }

    // Collect user-defined server names
    fn start_visit_server(&mut self, node: &Server) {
        // Ignore service units without a user-defined name
        if node.service_unit_name.is_empty() {
            return;
        }

        if !self.seen_rpc_names.insert(node.service_unit_name.clone()) {
            self.finding_collector.error(
                FindingId::UniqueUserDefinedServiceUnitNamesPerBundleRpc,
                format!(
                    "'{}' is specified as the name of multiple RPC servers",
                    node.service_unit_name
                ),
                vec![node.service_unit_name_span.clone().into_option()],
            );
        }
    }

    fn start_visit_diagnostics_event(&mut self, node: &Event) {
        // Ignore service units without a user-defined name
        if node.service_unit_name.is_empty() {
            return;
        }

        if !self.seen_pub_names.insert(node.service_unit_name.clone()) {
            self.finding_collector.error(
                FindingId::UniqueUserDefinedServiceUnitNamesPerBundlePublisher,
                format!(
                    "'{}' is specified as the name of multiple publishers",
                    node.service_unit_name
                ),
                Vec::new(),
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::unique_user_defined_service_unit_names_per_bundle::UniqueUserDefinedServiceUnitNamesPerBundle;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_should_accept_catalog_with_rpc_and_pub_using_same_user_defined_name() {
        // Given: A catalog where an RPC server and a publisher use the same user-defined name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_USER_DEFINED_NAME_SAME_FOR_RPC_AND_PUB.clone());

        // When: Executing context condition
        let mut check = UniqueUserDefinedServiceUnitNamesPerBundle::default();
        check.check_catalog(&catalog);

        // Then: context condition should not find any errors
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_reject_catalog_with_duplicated_rpc_name() {
        // Given: A catalog with a duplicated user-defined RPC name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_DUPLICATED_USER_DEFINED_RPC_NAME.clone());

        // When: Executing context condition
        let mut check = UniqueUserDefinedServiceUnitNamesPerBundle::default();
        check.check_catalog(&catalog);

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
            .push(vsidl_v1_test_data::VSIDL_DUPLICATED_USER_DEFINED_PUB_NAME.clone());

        // When: Executing context condition
        let mut check = UniqueUserDefinedServiceUnitNamesPerBundle::default();
        check.check_catalog(&catalog);

        // Then: context condition should find errors
        assert!(!check.finding_collector().findings().is_empty());

        // Then: Error message contains the duplicated publisher name
        assert!(check.any_finding_contains(FindingSeverity::Error, "my-pub"));
    }

    #[test]
    fn test_should_reject_catalog_with_duplicated_publisher_name_id_diagnostics() {
        // Given: A catalog with a duplicated user-defined publisher name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog.catalog_vsidl_files.push(
            vsidl_v1_test_data::VSIDL_DUPLICATED_USER_DEFINED_PUB_NAME_IN_DIAGNOSTICS.clone(),
        );

        // When: Executing CoCo
        let mut check = UniqueUserDefinedServiceUnitNamesPerBundle::default();
        check.check_catalog(&catalog);

        // Then: CoCo should find errors
        assert!(!check.finding_collector().findings().is_empty());

        // Then: Error message contains the duplicated publisher name
        assert!(check.any_finding_contains(FindingSeverity::Error, "my-pub"));
    }
}
