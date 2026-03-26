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

use super::print_list_of_element;
use super::{impl_context_condition, FindingCollector, FindingId};
use std::collections::HashMap;
use std::path::PathBuf;
use vsidl_v1_common::android_bp::AndroidBpFile;
use vsidl_v1_common::expand_file_groups_per_android_bp;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_common::Catalog;

#[derive(Default)]
pub struct AndroidBpFilegroups {
    finding_collector: FindingCollector,
    filegroup_to_android_bp: HashMap<String, Vec<PathBuf>>,
}
impl_context_condition!(AndroidBpFilegroups);

impl AndroidBpFilegroups {
    fn register_filegroups(&mut self, android_bp_file: &AndroidBpFile) {
        for filegroup in &android_bp_file.filegroup_targets {
            self.filegroup_to_android_bp
                .entry(filegroup.name.clone())
                .or_default()
                .push(android_bp_file.file_path.clone());
        }
    }

    fn detect_empty_filegroup(&mut self, android_bp_file: &AndroidBpFile) {
        for (filegroup, files) in expand_file_groups_per_android_bp(android_bp_file) {
            if files.is_empty() {
                self.finding_collector.warn(
                    FindingId::AndroidBpFilegroupEmptyGlob,
                    format!(
                        "Android.bp file at '{}' contains filegroup '{}' with srcs that doesn't match any file",
                        android_bp_file.file_path.display(),
                        filegroup,
                    ),
                    None, Vec::new(),
                );
            }
        }
    }

    fn detect_duplicated_filegroup(&mut self) {
        for (filegroup, files) in self.filegroup_to_android_bp.iter() {
            if files.len() > 1 {
                self.finding_collector.error(
                    FindingId::AndroidBpFieldsFilegroupDuplicated,
                    format!(
                        "filegroup '{}' is defined in multiple Android.bp files: {}",
                        filegroup,
                        print_list_of_element(files, 2, |p| p.display().to_string())
                    ),
                    None,
                    Vec::new(),
                );
            }
        }
    }
}

impl VsidlVisitor for AndroidBpFilegroups {
    /// start visiting node Android.bp
    fn start_visit_android_bp_file(&mut self, android_bp_file: &AndroidBpFile) {
        self.detect_empty_filegroup(android_bp_file);
        self.register_filegroups(android_bp_file);
    }

    fn end_visit_catalog(&mut self, _catalog: &Catalog) {
        self.detect_duplicated_filegroup();
    }
}

#[cfg(test)]
mod tests {
    use crate::android_bp_filegroups::AndroidBpFilegroups;
    use crate::context_condition::ContextCondition;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_catalog_with_valid_filegroup() {
        // GIVEN an Android.bp file with filegroup defined
        let mut catalog = Catalog::new();
        catalog.android_bp_files.push(vsidl_v1_test_data::ANDROID_BP_FILEGROUP_PROTOS.clone());

        // WHEN AndroidBpFilegroups is validate
        let mut check = AndroidBpFilegroups::default();
        check.check_catalog(&catalog);

        // THEN a warning is returned
        assert!(check.is_ok());
    }

    #[test]
    fn test_detect_duplicated_filegroup_same_file() {
        // GIVEN: multiple Android.bp files defining the same filegroup
        let mut catalog = Catalog::new();
        catalog
            .android_bp_files
            .push(vsidl_v1_test_data::ANDROID_BP_DUPLICATED_FILEGROUP_SAME_FILE.clone());

        // WHEN: check_catalog is called
        let mut check = AndroidBpFilegroups::default();
        check.check_catalog(&catalog);

        // THEN: An error should be reported.
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "filegroup 'filegroup_a' is defined in multiple Android.bp files: [src/tests_vsidl_v1/android_bp/duplicated_filegroup_a.bp,src/tests_vsidl_v1/android_bp/duplicated_filegroup_a.bp]"
        ));
    }

    #[test]
    fn test_detect_duplicated_filegroup_different_file() {
        // GIVEN: multiple Android.bp files defining the same filegroup
        let mut catalog = Catalog::new();
        catalog
            .android_bp_files
            .push(vsidl_v1_test_data::ANDROID_BP_DUPLICATED_FILEGROUP_SAME_FILE.clone());
        catalog
            .android_bp_files
            .push(vsidl_v1_test_data::ANDROID_BP_DUPLICATED_FILEGROUP_DIFFERENT_FILE.clone());

        // WHEN: check_catalog is called
        let mut check = AndroidBpFilegroups::default();
        check.check_catalog(&catalog);

        // THEN: An error should be reported.
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "filegroup 'filegroup_b' is defined in multiple Android.bp files: [src/tests_vsidl_v1/android_bp/duplicated_filegroup_a.bp,src/tests_vsidl_v1/android_bp/duplicated_filegroup_b.bp]"
        ));
    }

    #[test]
    fn test_detect_empty_filegroup() {
        // GIVEN: an Android.bp file with an empty filegroup
        let mut catalog = Catalog::new();
        catalog
            .android_bp_files
            .push(vsidl_v1_test_data::ANDROID_BP_FILEGROUP_NOT_MATCHING_GLOB.clone());

        // WHEN: check_catalog is called
        let mut check = AndroidBpFilegroups::default();
        check.check_catalog(&catalog);

        // THEN: a warning should be reported
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Android.bp file at 'src/tests_vsidl_v1/android_bp/empty_filegroup.bp' contains filegroup 'empty_filegroup_sample' with srcs that doesn't match any file"
        ));
    }
}
