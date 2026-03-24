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

use super::{impl_context_condition, FindingCollector, FindingId};
use vsidl_v1_common::android_bp::{
    get_first_match, AndroidBpFile, CRATE_NAME_RE, DEFAULTS_RE, FILEGROUP_RE, NAME_RE,
    RUST_PROTOBUF_RE, SRCS_RE,
};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Default)]
pub struct AndroidBpFields {
    finding_collector: FindingCollector,
}
impl_context_condition!(AndroidBpFields);

impl AndroidBpFields {
    fn validate_rust_protobuf_content(&mut self, content: &str, android_bp: &AndroidBpFile) {
        let name = get_first_match(content, &NAME_RE).unwrap_or_default();
        let crate_name = get_first_match(content, &CRATE_NAME_RE).unwrap_or_default();

        if name.is_empty() {
            self.finding_collector.error(
                FindingId::AndroidBpFieldsRustProtobufNameNotExist,
                format!(
                    "missing 'name' attribute in rust_protobuf target in '{}'{}",
                    android_bp.file_path.display(),
                    if !crate_name.is_empty() {
                        format!(" with crate_name '{crate_name}'")
                    } else {
                        String::new()
                    }
                ),
                None,
                Vec::new(),
            );
        }

        if crate_name.is_empty() {
            self.finding_collector.error(
                FindingId::AndroidBpFieldsRustProtobufCrateNameNotExist,
                format!(
                    "missing 'crate_name' attribute in rust_protobuf target in '{}'{}",
                    android_bp.file_path.display(),
                    if !name.is_empty() { format!(" with name '{name}'") } else { String::new() }
                ),
                None,
                Vec::new(),
            );
        }

        if DEFAULTS_RE.is_match(content) {
            self.finding_collector.error(
                FindingId::AndroidBpFieldsRustProtobufDefaultsDefined,
                format!(
                    "Found NOT SUPPORTED 'defaults' attribute in rust_protobuf in '{}'{}{}",
                    android_bp.file_path.display(),
                    if !name.is_empty() { format!(" with name '{name}'") } else { String::new() },
                    if !crate_name.is_empty() {
                        format!(" with crate_name '{crate_name}'")
                    } else {
                        String::new()
                    },
                ),
                None,
                Vec::new(),
            );
        }
    }

    fn validate_filegroup_content(&mut self, content: &str, android_bp: &AndroidBpFile) {
        let name = get_first_match(content, &NAME_RE).unwrap_or_default();
        if name.is_empty() {
            self.finding_collector.error(
                FindingId::AndroidBpFieldsFilegroupNameDoesNotExist,
                format!(
                    "missing 'name' attribute in filegroup target in '{}'",
                    android_bp.file_path.display(),
                ),
                None,
                Vec::new(),
            );
        }

        let srcs_string = get_first_match(content, &SRCS_RE);
        if srcs_string.is_none() {
            self.finding_collector.error(
                FindingId::AndroidBpFieldsFilegroupSrcsDoesNotExist,
                format!(
                    "missing 'srcs' attribute in filegroup target in '{}'{}",
                    android_bp.file_path.display(),
                    if !name.is_empty() { format!(" with name '{name}'") } else { String::new() },
                ),
                None,
                Vec::new(),
            );
        }
    }
}

impl VsidlVisitor for AndroidBpFields {
    /// start visiting node Android.bp
    fn start_visit_android_bp_file(&mut self, android_bp_file: &AndroidBpFile) {
        for captures in RUST_PROTOBUF_RE.captures_iter(&android_bp_file.content) {
            // Capture the entire rust_protobuf block
            let block_content = captures.get(1).unwrap().as_str();
            self.validate_rust_protobuf_content(block_content, android_bp_file);
        }
        for captures in FILEGROUP_RE.captures_iter(&android_bp_file.content) {
            self.validate_filegroup_content(captures.get(1).unwrap().as_str(), android_bp_file);
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::AndroidBpFields;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_should_reject_rust_protobuf_without_name() {
        // GIVEN A catalog with a service bundle that contains a client with a non existing rpc type
        let mut catalog = Catalog::new();
        catalog.android_bp_files.push(vsidl_v1_test_data::MISSING_NAME.clone());

        // WHEN Executing context condition
        let mut check = AndroidBpFields::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Error, "missing 'name' attribute in rust_protobuf target in 'src/tests_vsidl_v1/android_bp/missing_name.bp' with crate_name 'missing_name_crate'"));
    }

    #[test]
    fn test_should_reject_rust_protobuf_without_crate_name() {
        // GIVEN A catalog with a service bundle that contains a client with a non existing rpc type
        let mut catalog = Catalog::new();
        catalog.android_bp_files.push(vsidl_v1_test_data::MISSING_CRATE_NAME.clone());

        // WHEN Executing context condition
        let mut check = AndroidBpFields::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Error, "missing 'crate_name' attribute in rust_protobuf target in 'src/tests_vsidl_v1/android_bp/missing_crate.bp' with name 'missing_crate'"));
    }

    #[test]
    fn test_should_reject_rust_protobuf_without_name_and_crate_name() {
        // GIVEN A catalog with a service bundle that contains a client with a non existing rpc type
        let mut catalog = Catalog::new();
        catalog.android_bp_files.push(vsidl_v1_test_data::MISSING_NAME_AND_CRATE.clone());

        // WHEN Executing context condition
        let mut check = AndroidBpFields::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Error, "missing 'name' attribute in rust_protobuf target in 'src/tests_vsidl_v1/android_bp/missing_name_and_crate.bp'"));
        assert!(check.any_finding_contains(FindingSeverity::Error, "missing 'crate_name' attribute in rust_protobuf target in 'src/tests_vsidl_v1/android_bp/missing_name_and_crate.bp'"));
    }

    #[test]
    fn test_should_reject_rust_protobuf_with_default() {
        // GIVEN A catalog with a service bundle that contains a client with a non existing rpc type
        let mut catalog = Catalog::new();
        catalog.android_bp_files.push(vsidl_v1_test_data::DEFAULTS_DEFINED.clone());

        // WHEN Executing context condition
        let mut check = AndroidBpFields::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Error, "Found NOT SUPPORTED 'defaults' attribute in rust_protobuf in 'src/tests_vsidl_v1/android_bp/defaults_defined.bp'"));
    }

    #[test]
    fn test_should_reject_rust_protobuf_with_default_and_attributes() {
        // GIVEN A catalog with a service bundle that contains a client with a non existing rpc type
        let mut catalog = Catalog::new();
        catalog.android_bp_files.push(vsidl_v1_test_data::DEFAULTS_DEFINED_WITH_ATTRIBUTES.clone());

        // WHEN Executing context condition
        let mut check = AndroidBpFields::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Error, "Found NOT SUPPORTED 'defaults' attribute in rust_protobuf in 'src/tests_vsidl_v1/android_bp/defaults_defined_with_attributes.bp' with name 'defaults_name' with crate_name 'defaults_crate'"));
    }

    #[test]
    fn test_should_reject_filegroup_without_name() {
        // GIVEN A catalog with a service bundle that contains a filegroup without a name
        let mut catalog = Catalog::new();
        catalog
            .android_bp_files
            .push(vsidl_v1_test_data::ANDROID_BP_MISSING_FILEGROUP_NAME.clone());

        // WHEN Executing context condition
        let mut check = AndroidBpFields::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Error, "missing 'name' attribute in filegroup target in 'src/tests_vsidl_v1/android_bp/missing_filegroup_name.bp'"));
    }

    #[test]
    fn test_should_reject_filegroup_without_srcs() {
        // GIVEN A catalog with a service bundle that contains a filegroup without a name
        let mut catalog = Catalog::new();
        catalog
            .android_bp_files
            .push(vsidl_v1_test_data::ANDROID_BP_MISSING_FILEGROUP_SRCS.clone());

        // WHEN Executing context condition
        let mut check = AndroidBpFields::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Error, "missing 'srcs' attribute in filegroup target in 'src/tests_vsidl_v1/android_bp/missing_filegroup_srcs.bp' with name 'no_srcs_filegroup'"));
    }
}
