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

use super::impl_context_condition;
use unicode_xid::UnicodeXID;
use vsidl_v1::syntax::{ServiceBundle, VsidlEntry};
use vsidl_v1_common::constants::{RUST_RESERVED_KEYWORDS, RUST_STRICT_KEYWORDS};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_common::{CPP_RESERVED_KEYWORDS, JAVA_CONTEXTUAL_KEYWORDS, JAVA_RESERVED_KEYWORDS};

use super::{FindingCollector, FindingId};

#[derive(Debug, Default)]
pub struct ServiceBundleNamingConvention {
    finding_collector: FindingCollector,
    package: String,
}
impl_context_condition!(ServiceBundleNamingConvention);

/// Checks that service bundle names follow naming guideline
impl VsidlVisitor for ServiceBundleNamingConvention {
    fn start_visit_vsidl_entry(&mut self, node: &VsidlEntry) {
        self.package = node.package.clone();
    }

    fn start_visit_service_bundle(&mut self, node: &ServiceBundle) {
        let mut service_bundle_name_chars = node.name.chars();

        // The name must not be empty
        if node.name.is_empty() {
            self.finding_collector.error(
                FindingId::ServiceBundleNamingConventionEmptyName,
                format!("Package '{}' contains a service bundle with an empty name.", self.package),
                Some("Service bundle name may not be empty".to_string()),
                vec![node.span.clone().into_option()],
            );
        }

        // The name must start with a Unicode XID start char
        if let Some(first_char) = service_bundle_name_chars.next() {
            if !UnicodeXID::is_xid_start(first_char) {
                self.finding_collector.error(
                    FindingId::ServiceBundleNamingConventionFirstChar,
                    format!(
                        "Character '{}' in service bundle name '{}' is not a valid first character. The name must start with unicode XID start character. See: https://util.unicode.org/UnicodeJsps/list-unicodeset.jsp?a=%5B%3AXID_Start%3A%5D&abb=on&g=&i=",
                        first_char,
                        node.name.clone()
                    ),
                    Some(format!("Character '{}' is not a valid first character. Must start with unicode XID start character.", first_char)),
                    vec![node.name_span.clone().into_option()],
                );
            }
        }

        // All characters must be Unicode XID continue char
        for current_char in service_bundle_name_chars {
            if !UnicodeXID::is_xid_continue(current_char) {
                self.finding_collector.error(
                    FindingId::ServiceBundleNamingConventionContinueChar,
                    format!(
                        "Character '{}' in service bundle name '{}' is invalid. The name must only contain unicode XID continue characters. See: https://util.unicode.org/UnicodeJsps/list-unicodeset.jsp?a=%5B%3AXID_Continue%3A%5D&abb=on&g=&i=",
                        current_char,
                        node.name.clone()
                    ),
                    Some(format!("Character '{}' is invalid. Must only contain unicode XID continue characters.", current_char)),
                    vec![node.name_span.clone().into_option()],
                );
            }
        }

        // The name must not be a Rust strict or reserved keyword
        if RUST_STRICT_KEYWORDS.contains(&node.name.as_str())
            || RUST_RESERVED_KEYWORDS.contains(&node.name.as_str())
        {
            self.finding_collector.error(
                FindingId::ServiceBundleNamingConventionForbiddenName,
                format!(
                    "Service bundle name '{}' is a reserved Rust keyword and cannot be used. See: https://doc.rust-lang.org/reference/keywords.html",
                    node.name
                ),
                Some(format!("'{}' is a reserved Rust keyword", node.name)),
                vec![node.name_span.clone().into_option()],
            );
        }

        // The name must not be a Java reserved or contextual keyword
        if JAVA_RESERVED_KEYWORDS.contains(&node.name.as_str())
            || JAVA_CONTEXTUAL_KEYWORDS.contains(&node.name.as_str())
        {
            self.finding_collector.error(
                FindingId::ServiceBundleNamingConventionForbiddenName,
                format!(
                    "Service bundle name '{}' is a reserved Java keyword and cannot be used. See: https://docs.oracle.com/javase/specs/jls/se21/html/jls-3.html#jls-Keyword",
                    node.name
                ),
                Some(format!("'{}' is a reserved Java keyword", node.name)),
                vec![node.name_span.clone().into_option()],
            );
        }

        // The name must not be a C++ reserved keyword
        if CPP_RESERVED_KEYWORDS.contains(&node.name.as_str()) {
            self.finding_collector.error(
                FindingId::ServiceBundleNamingConventionForbiddenName,
                format!(
                    "Service bundle name '{}' is a reserved C++ keyword and cannot be used. See: https://en.cppreference.com/w/cpp/keyword",
                    node.name
                ),
                Some(format!("'{}' is a reserved C++ keyword", node.name)),
                vec![node.name_span.clone().into_option()],
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::service_bundle_naming_convention::ServiceBundleNamingConvention;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_should_reject_long_user_defined_name() {
        // GIVEN A catalog with service bundles using invalid names
        let mut catalog = Catalog::new();
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_SERVICE_BUNDLE_INVALID_NAME.clone());

        // WHEN Executing context condition
        let mut check = ServiceBundleNamingConvention::default();
        check.check_catalog(&catalog);

        // THEN context condition should fail
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Package 'sdv.test' contains a service bundle with an empty name"
        ));
        assert!(check.any_finding_contains(FindingSeverity::Error, "Character '2' in service bundle name '2A' is not a valid first character. The name must start with unicode XID start character."));
        assert!(check.any_finding_contains(FindingSeverity::Error, "Character '.' in service bundle name 'A.B' is invalid. The name must only contain unicode XID continue characters."));
        assert!(check.any_finding_contains(FindingSeverity::Error, "Character '{' in service bundle name 'A {} fn main() { evil_code(); } impl A' is invalid. The name must only contain unicode XID continue characters."));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Service bundle name 'impl' is a reserved Rust keyword and cannot be used."
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Service bundle name 'instanceof' is a reserved Java keyword and cannot be used."
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Service bundle name 'decltype' is a reserved C++ keyword and cannot be used."
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

        // WHEN Executing context condition
        let mut check = ServiceBundleNamingConvention::default();
        check.check_catalog(&catalog);

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }
}
