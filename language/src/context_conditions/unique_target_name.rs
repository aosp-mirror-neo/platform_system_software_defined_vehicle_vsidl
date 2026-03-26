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

use crate::{impl_context_condition, FindingCollector, FindingId};
use std::collections::HashSet;
use vsidl_v1::syntax::{ServiceBundle, VsidlEntry};
use vsidl_v1_common::get_crate_and_library_name;
use vsidl_v1_common::VsidlVisitor;

#[derive(Debug, Default)]
pub struct UniqueTargetName {
    finding_collector: FindingCollector,
    current_package: Option<String>,
    target_names: HashSet<String>,
}
impl_context_condition!(UniqueTargetName);

/// User-defined target name in build_cfg must not conflict with generated names
impl VsidlVisitor for UniqueTargetName {
    fn start_visit_vsidl_entry(&mut self, _node: &VsidlEntry) {
        self.current_package = Some(_node.package.clone());
    }

    fn end_visit_vsidl_entry(&mut self, _node: &VsidlEntry) {
        self.current_package = None;
    }

    fn start_visit_service_bundle(&mut self, node: &ServiceBundle) {
        let service_bundle_crate_name = get_crate_and_library_name(
            self.current_package.as_ref().unwrap(),
            &node.name,
            &node.build_cfg.target_name.clone().unwrap_or_default(),
        );
        if !self.target_names.insert(service_bundle_crate_name.clone()) {
            self.finding_collector.error(
                FindingId::UniqueTargetName,
                format!("'{service_bundle_crate_name}' is defined in multiple build target names"),
                None,
                vec![node.span.clone().into_option()],
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::unique_target_name::UniqueTargetName;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_should_reject_generated_name_clash() {
        // Given: A catalog with two service bundles that have the same generated target name
        let mut catalog = Catalog::new();
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_DUPLICATED_GENERATED_TARGET_NAME.clone());

        // When: Executing context condition
        let mut check = UniqueTargetName::default();
        check.check_catalog(&catalog);

        // Then: context condition should find errors
        assert!(check.is_err());

        // Then: Error message contains the duplicated service name
        assert!(
            check.any_finding_contains(FindingSeverity::Error, "sdv_test_duplicated_target_name")
        );
    }

    #[test]
    fn test_should_reject_user_defined_name_clashes_with_generated_name() {
        // Given: A catalog with two service bundles have names that are not in conflict,
        // but one of them chooses a user-defined target name that is in conflict with the
        // generated name of the other service bundle
        let mut catalog = Catalog::new();
        catalog.catalog_vsidl_files.push(
            vsidl_v1_test_data::VSIDL_USER_DEFINED_TARGET_NAME_CLASHES_WITH_GENERATED_NAME.clone(),
        );

        // When: Executing context condition
        let mut check = UniqueTargetName::default();
        check.check_catalog(&catalog);

        // Then: context condition should find errors
        assert!(check.is_err());

        // Then: Error message contains the duplicated service name
        assert!(
            check.any_finding_contains(FindingSeverity::Error, "sdv_test_duplicated_target_name")
        );
    }

    #[test]
    fn test_should_accept_user_defined_name_resolves_conflict_with_generated_name() {
        // Given: A catalog where two services bundles have conflicting generated target names
        // but the conflict is resolved by manually setting a different target name
        let mut catalog = Catalog::new();
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_USER_DEFINED_TARGET_NAME_RESOLVES_CONFLICT.clone());

        // When: Executing context condition
        let mut check = UniqueTargetName::default();
        check.check_catalog(&catalog);

        // Then: context condition should not find errors
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_accept_valid_service_bundle() {
        // GIVEN A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());

        // When: Executing context condition
        let mut check = UniqueTargetName::default();
        check.check_catalog(&catalog);

        // Then: context condition should not find errors
        assert!(check.is_ok());
    }
}
