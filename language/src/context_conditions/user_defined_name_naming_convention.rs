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

use crate::impl_context_condition;
use sdv_comms_id_validator::service_unit_name_validation::{
    validate_consecutive_hyphens, validate_first_char, validate_last_char, validate_valid_chars,
};
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::Event;
use vsidl_v1::span::Span;
use vsidl_v1::syntax::service_bundle::{Publisher, Server};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

use super::{FindingCollector, FindingId};

#[derive(Debug, Default)]
pub struct UserDefinedNameNamingConvention {
    finding_collector: FindingCollector,
}
impl_context_condition!(UserDefinedNameNamingConvention);

/// Checks that user-defined service unit names follow naming guideline
impl UserDefinedNameNamingConvention {
    fn check_name(&mut self, service_unit_name: &String, span: Option<Span>) {
        // The name must start with a letter [a-z]
        let result = validate_first_char(service_unit_name);
        if let Err(err) = result {
            self.finding_collector.error(
                FindingId::UserDefinedNameNamingConventionFirstChar,
                format!("Service unit name '{}' {}", service_unit_name, err),
                vec![span.clone()],
            );
        }

        // All characters must be lowercase alphanumeric or a hyphen [a-z0-9\-]
        let result = validate_valid_chars(service_unit_name);
        if let Err(err) = result {
            self.finding_collector.error(
                FindingId::UserDefinedNameNamingConventionCharset,
                format!("Service unit name '{}' {}", service_unit_name, err),
                vec![span.clone()],
            );
        }

        // Hyphens in the name must not appear consecutively more than once
        let result = validate_consecutive_hyphens(service_unit_name);
        if let Err(err) = result {
            self.finding_collector.error(
                FindingId::UserDefinedNameNamingConventionConsecutiveHyphen,
                format!("Service unit name '{}' {}", service_unit_name, err),
                vec![span.clone()],
            );
        }

        // The name must not end with a hyphen
        let result = validate_last_char(service_unit_name);
        if let Err(err) = result {
            self.finding_collector.error(
                FindingId::UserDefinedNameNamingConventionLastChar,
                format!("Service unit name '{}' {}", service_unit_name, err),
                vec![span.clone()],
            );
        }
    }
}

impl VsidlVisitor for UserDefinedNameNamingConvention {
    fn start_visit_publisher(&mut self, node: &Publisher) {
        self.check_name(&node.service_unit_name, node.service_unit_name_span.clone().into_option());
    }

    fn start_visit_server(&mut self, node: &Server) {
        self.check_name(&node.service_unit_name, node.service_unit_name_span.clone().into_option());
    }

    fn start_visit_diagnostics_event(&mut self, node: &Event) {
        self.check_name(&node.service_unit_name, None);
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::user_defined_name_naming_convention::UserDefinedNameNamingConvention;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_should_reject_invalid_user_defined_name() {
        // Given: A catalog with a publisher that has two channels and a user-defined name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_USER_DEFINED_NAME_NOT_NAMING_CONVENTION.clone());

        // When: Executing context condition
        let mut check = UserDefinedNameNamingConvention::default();
        check.check_catalog(&catalog);

        // Then: context condition should fail
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "11-test"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "Test-test"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "test--test"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "test-"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "MyPublisher"));
    }

    #[test]
    fn test_should_reject_invalid_user_defined_name_id_diagnostics() {
        // Given: A catalog with a publisher that has two channels and a user-defined name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog.catalog_vsidl_files.push(
            vsidl_v1_test_data::VSIDL_USER_DEFINED_NAME_NOT_NAMING_CONVENTION_IN_DIAGNOSTICS
                .clone(),
        );

        // When: Executing context condition
        let mut check = UserDefinedNameNamingConvention::default();
        check.check_catalog(&catalog);

        // Then: context condition should fail
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "11-test"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "Test-test"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "test--test"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "test-"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "MyPublisher"));
    }
}
