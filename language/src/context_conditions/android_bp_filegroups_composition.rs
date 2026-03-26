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
use std::collections::HashMap;
use std::path::PathBuf;
use vsidl_v1_common::android_bp::AndroidBpFile;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_common::Catalog;

#[derive(Default)]
pub struct AndroidBpFilegroupsComposition {
    finding_collector: FindingCollector,
    filegroup_referencing_filegroups: HashMap<String, Vec<PathBuf>>,
    used_filegroup_to_proto_library: HashMap<String, String>,
}
impl_context_condition!(AndroidBpFilegroupsComposition);

impl AndroidBpFilegroupsComposition {
    fn register_used_filegroups(&mut self, android_bp_file: &AndroidBpFile) {
        for target in android_bp_file.rust_protobuf_targets.iter() {
            for proto_file in target.protos.iter() {
                if proto_file.starts_with(":") {
                    self.used_filegroup_to_proto_library
                        .insert(proto_file.to_string(), target.name.clone());
                }
            }
        }
    }

    fn register_filegroup_using_composition(&mut self, android_bp_file: &AndroidBpFile) {
        for filegroup in android_bp_file.filegroup_targets.iter() {
            for src in filegroup.srcs.iter() {
                if src.starts_with(":") {
                    self.filegroup_referencing_filegroups
                        .entry(filegroup.name.clone())
                        .or_default()
                        .push(android_bp_file.file_path.clone());
                }
            }
        }
    }

    fn detect_filegroup_composition(&mut self) {
        for (filegroup, library) in &self.used_filegroup_to_proto_library {
            if self.filegroup_referencing_filegroups.contains_key(&filegroup[1..]) {
                self.finding_collector.error(
                    FindingId::AndroidBpFilegroupsCompositionSrcsPointsToFilegroup,
                    format!(
                        "rust_protobuf library '{library}' uses filegroup '{filegroup}' which reference to another filegroup. Not supported",
                    ),
                    None, Vec::new(),
                );
            }
        }
    }
}

impl VsidlVisitor for AndroidBpFilegroupsComposition {
    /// start visiting node Android.bp
    fn start_visit_android_bp_file(&mut self, android_bp_file: &AndroidBpFile) {
        self.register_filegroup_using_composition(android_bp_file);
        self.register_used_filegroups(android_bp_file);
    }

    fn end_visit_catalog(&mut self, _catalog: &Catalog) {
        self.detect_filegroup_composition();
    }
}

#[cfg(test)]
mod tests {
    use crate::android_bp_filegroups_composition::AndroidBpFilegroupsComposition;
    use crate::context_condition::ContextCondition;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_detect_nested_filegroups_in_required_filegroups() {
        // GIVEN: multiple Android.bp files defining the same filegroup
        let mut catalog = Catalog::new();
        catalog
            .android_bp_files
            .push(vsidl_v1_test_data::ANDROID_BP_USED_FILEGROUP_REFERENCE_TO_ANOTHER.clone());

        // WHEN: check_catalog is called
        let mut check = AndroidBpFilegroupsComposition::default();
        check.check_catalog(&catalog);

        // THEN: an error should be reported.
        check.finding_collector.print_findings();
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "rust_protobuf library 'libusing_filegroups' uses filegroup ':filegroup_c' which reference to another filegroup. Not supported"
        ));
    }

    #[test]
    fn test_do_not_detect_nested_filegroups_in_unused_filegroups() {
        // GIVEN: multiple Android.bp files defining the same filegroup
        let mut catalog = Catalog::new();
        catalog
            .android_bp_files
            .push(vsidl_v1_test_data::ANDROID_BP_FILEGROUP_REFERENCE_TO_ANOTHER.clone());

        // WHEN: check_catalog is called
        let mut check = AndroidBpFilegroupsComposition::default();
        check.check_catalog(&catalog);

        // THEN: No findings are detected.
        check.finding_collector.print_findings();
        assert!(!check.has_findings());
    }
}
