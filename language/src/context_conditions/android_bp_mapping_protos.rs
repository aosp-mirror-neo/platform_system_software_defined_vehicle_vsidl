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
use super::{FindingCollector, FindingId};
use std::collections::HashMap;
use std::path::PathBuf;
use vsidl_v1_common::android_bp::RustProtobufInfo;
use vsidl_v1_common::android_bp::{
    expand_file_groups, get_all_proto_files_matching_target, AndroidBpFile,
};
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_common::Catalog;
use vsidl_v1_common::ProtoBuildTargetMetadata;

#[derive(Default)]
pub struct AndroidBpMappingProtos {
    finding_collector: FindingCollector,
    android_bp_to_properties: HashMap<PathBuf, Vec<RustProtobufInfo>>,
    file_group_name_to_file: HashMap<String, Vec<PathBuf>>,
}
impl_context_condition!(AndroidBpMappingProtos);

impl VsidlVisitor for AndroidBpMappingProtos {
    /// start visiting node Android.bp
    fn start_visit_android_bp_file(&mut self, android_bp_file: &AndroidBpFile) {
        self.android_bp_to_properties.insert(
            android_bp_file.file_path.clone(),
            android_bp_file.rust_protobuf_targets.clone(),
        );
        expand_file_groups(
            std::slice::from_ref(android_bp_file),
            &mut self.file_group_name_to_file,
        );
    }

    fn end_visit_catalog(&mut self, _catalog: &Catalog) {
        let mut proto_file_to_property: HashMap<PathBuf, ProtoBuildTargetMetadata> = HashMap::new();
        for (android_bp_file, rust_protobuf_targets) in self.android_bp_to_properties.iter() {
            for rust_protobuf_info in rust_protobuf_targets {
                let (proto_files_per_target, response) = get_all_proto_files_matching_target(
                    rust_protobuf_info,
                    android_bp_file.parent().unwrap(),
                    &self.file_group_name_to_file,
                );

                for unknown_filegroup in response.unknown_filegroup {
                    self.finding_collector.error(
                        FindingId::AndroidBpMappingProtosFilegroupDoesNotExist,
                        format!(
                            "Android.bp file at '{}' contains rust_protobuf '{}' with a reference to a not defined filegroup '{}'",
                            android_bp_file.display(),
                            rust_protobuf_info.name,
                            unknown_filegroup,
                        ),
                        None, Vec::new(),
                    );
                }

                for empty_glob in response.empty_glob {
                    self.finding_collector.warn(
                        FindingId::AndroidBpMappingEmptyGlob,
                        format!(
                            "Android.bp file at '{}' contains rust_protobuf with protos '{}' that doesn't match any Proto file",
                            android_bp_file.display(),
                            empty_glob
                        ),
                        None, Vec::new(),
                    );
                }

                for proto_file in response.double_defined_protos {
                    self.finding_collector.warn(
                        FindingId::AndroidBpMappingDuplicateProtoFileSameLibrary,
                        format!(
                            "Android.bp file at '{}' contains rust_protobuf '{}' which include proto file '{}' multiple times.",
                            android_bp_file.display(),
                            rust_protobuf_info.name,
                            proto_file
                        ),
                        None, Vec::new(),
                    );
                }

                for proto_file in proto_files_per_target {
                    let relative_proto_file_path: PathBuf =
                        proto_file.canonicalize().unwrap().clone();

                    let metadata = ProtoBuildTargetMetadata {
                        folder: android_bp_file.parent().unwrap().canonicalize().unwrap(),
                        target_name: Some(rust_protobuf_info.name.clone()),
                        crate_name: Some(rust_protobuf_info.crate_name.clone()),
                    };
                    if proto_file_to_property.insert(relative_proto_file_path, metadata).is_some() {
                        self.finding_collector.warn(
                            FindingId::AndroidBpMappingDuplicateProtoFileMultipleLibraries,
                            format!(
                                "Multiple rust_protobuf points to '{}'; ensure unique definitions to avoid random selection.",
                                proto_file.display()
                            ),
                            None, Vec::new(),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::context_condition::ContextCondition;
    use crate::AndroidBpMappingProtos;
    use crate::FindingSeverity;
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_catalog_with_valid_filegroup() {
        // GIVEN an Android.bp file with filegroup defined
        let mut catalog = Catalog::new();
        catalog.android_bp_files.push(vsidl_v1_test_data::ANDROID_BP_FILEGROUP_PROTOS.clone());

        // WHEN AndroidBpMappingProtos is validate
        let mut check = AndroidBpMappingProtos::default();
        check.check_catalog(&catalog);

        // THEN a warning is returned
        assert!(check.is_ok());
    }

    #[test]
    fn test_catalog_with_undefined_filegroup() {
        // GIVEN an Android.bp file with filegroup defined
        let mut catalog = Catalog::new();
        catalog
            .android_bp_files
            .push(vsidl_v1_test_data::ANDROID_BP_UNDEFINED_FILEGROUP_PROTOS.clone());

        // WHEN AndroidBpMappingProtos is validate
        let mut check = AndroidBpMappingProtos::default();
        check.check_catalog(&catalog);

        // THEN a warning is returned
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Error, "Android.bp file at 'src/tests_vsidl_v1/android_bp/undefined_filegroup.bp' contains rust_protobuf 'libtest_rust_proto' with a reference to a not defined filegroup ':undefined_filegroup'"));
    }

    #[test]
    fn test_empty_proto_file() {
        // GIVEN an Android.bp file with filegroup defined
        let mut catalog = Catalog::new();
        catalog.android_bp_files.push(vsidl_v1_test_data::MISSING_CRATE_NAME.clone());

        // WHEN AndroidBpMappingProtos is validate
        let mut check = AndroidBpMappingProtos::default();
        check.check_catalog(&catalog);

        // THEN a warning is returned
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Warn, "Android.bp file at 'src/tests_vsidl_v1/android_bp/missing_crate.bp' contains rust_protobuf with protos '**/*.proto' that doesn't match any Proto file"));
    }

    #[test]
    fn test_duplicate_proto_file_same_library() {
        // GIVEN an Android.bp file with filegroup defined
        let mut catalog = Catalog::new();
        catalog.android_bp_files.push(vsidl_v1_test_data::ANDROID_BP_DUPLICATED_PROTOS.clone());

        // WHEN AndroidBpMappingProtos is validate
        let mut check = AndroidBpMappingProtos::default();
        check.check_catalog(&catalog);

        // THEN a warning is returned
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Warn, "Android.bp file at 'src/tests_vsidl_v1/duplicated_protos_definition.bp' contains rust_protobuf 'libtest_protos' which include proto file 'src/tests_vsidl_v1/protos/messages.proto' multiple times."));
    }

    #[test]
    fn test_multiple_libraries_linking_the_same_proto() {
        // GIVEN an Android.bp file with filegroup defined
        let mut catalog = Catalog::new();
        catalog.android_bp_files.push(vsidl_v1_test_data::ANDROID_BP_DUPLICATED_PROTOS.clone());
        catalog.android_bp_files.push(vsidl_v1_test_data::ANDROID_BP_MESSAGES_PROTOS.clone());

        // WHEN AndroidBpMappingProtos is validate
        let mut check = AndroidBpMappingProtos::default();
        check.check_catalog(&catalog);

        // THEN a warning is returned
        assert!(check.has_findings());
        assert!(check.any_finding_contains(FindingSeverity::Warn, "Multiple rust_protobuf points to 'src/tests_vsidl_v1/protos/messages.proto'; ensure unique definitions to avoid random selection."));
    }
}
