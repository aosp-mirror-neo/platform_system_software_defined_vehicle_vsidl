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
use vsidl_v1::syntax::Span;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct UserDefinedTargetNameConvention {
    finding_collector: FindingCollector,
}
impl_context_condition!(UserDefinedTargetNameConvention);

impl VsidlVisitor for UserDefinedTargetNameConvention {
    fn start_visit_service_bundle(&mut self, node: &vsidl_v1::syntax::ServiceBundle) {
        self.check_name(
            &node.build_cfg.target_name.clone().unwrap_or_default(),
            node.build_cfg.target_name_span.clone().into_option(),
        );
    }
}

/// Checks that user-defined target name follow naming guideline
impl UserDefinedTargetNameConvention {
    fn check_name(&mut self, target_name: &String, span: Option<Span>) {
        if target_name.is_empty() {
            // no target name defined. Nothing to check
            return;
        }

        // All characters must be lowercase alphanumeric or a underscore [a-z0-9_]
        if !target_name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            self.finding_collector.error(
                FindingId::UserDefinedTargetNameConventionCharset,
                format!(
                    "target_name '{target_name}' must be lower alphanumeric or a underscore [a-z0-9_]"
                ),
                vec![span.clone()],
            );
        }

        // Underscore in the name must not appear consecutively more than once
        if target_name.contains("__") {
            self.finding_collector.error(
                FindingId::UserDefinedTargetNameConventionConsecutiveUnderscore,
                format!("target_name '{target_name}' must not contain consecutive underscores"),
                vec![span.clone()],
            );
        }

        // Target name cannot ends with _.
        if target_name.ends_with("_") {
            self.finding_collector.error(
                FindingId::UserDefinedTargetNameConventionLastChar,
                format!("target_name '{target_name}' must not end with underscore"),
                vec![span.clone()],
            );
        }

        // Target name cannot starts with _.
        if target_name.starts_with("_") {
            self.finding_collector.error(
                FindingId::UserDefinedTargetNameConventionFirstChar,
                format!("target_name '{target_name}' must not start with underscore"),
                vec![span.clone()],
            );
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::context_condition::ContextCondition;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_should_reject_wrongly_defined_target_names() {
        // Given: a catalog a mix of valid and invalid snake_case
        let mut catalog = Catalog::new();
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_USER_DEFINED_INVALID_LIBRARY_NAME.clone());

        // When: Executing context condition
        let mut check = UserDefinedTargetNameConvention::default();
        check.check_catalog(&catalog);

        // Then: context condition should fail
        assert_eq!(check.finding_collector().findings().len(), 4);

        // INVALID
        assert!(check.any_finding_contains(FindingSeverity::Error, "service_bundle_target__name"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "service_bundle_target_name_"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "Service_bundle_target_name"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "_service_bundle_target_name"));

        // VALID
        assert!(
            !check.any_finding_contains(FindingSeverity::Error, "8ervice_bundle_target_name_valid")
        );
        assert!(!check
            .any_finding_contains(FindingSeverity::Error, "service_bund7le_target_name_valid"));
    }

    #[test]
    fn test_should_accept_valid_service_bundle() {
        // Given: A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());

        // When: Executing context condition
        let mut check = UserDefinedTargetNameConvention::default();
        check.check_catalog(&catalog);

        // Then: Should not have found any errors
        assert!(check.is_ok());
    }
}
