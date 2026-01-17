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
use std::collections::HashSet;
use std::path::PathBuf;
use vsidl_v1_common::android_bp::expand_file_groups;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_common::Catalog;
use walkdir::WalkDir;

#[derive(Default)]
pub struct AndroidBpDanglingFiles {
    finding_collector: FindingCollector,
}
impl_context_condition!(AndroidBpDanglingFiles);

impl AndroidBpDanglingFiles {
    fn check_dangling_files_in_path(
        &mut self,
        path: &PathBuf,
        android_bp_files: &[vsidl_v1_common::AndroidBpFile],
    ) {
        // 1. Get all relevant files on disk within the given path.
        let files_on_disk: HashSet<PathBuf> = WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .filter(|p| {
                matches!(p.extension().and_then(|s| s.to_str()), Some("vsidl") | Some("proto"))
                    || p.file_name().and_then(|s| s.to_str()) == Some("Android.bp")
            })
            .map(|p| p.canonicalize().unwrap())
            .collect();

        // 2. Get all files referenced in filegroups from all Android.bp files.
        let mut file_groups_map = std::collections::HashMap::new();
        expand_file_groups(android_bp_files, &mut file_groups_map);

        let files_in_filegroups: HashSet<PathBuf> =
            file_groups_map.into_values().flatten().map(|p| p.canonicalize().unwrap()).collect();

        // 3. Find the difference and report warnings.
        for dangling_file in files_on_disk.difference(&files_in_filegroups) {
            if dangling_file.file_name().and_then(|s| s.to_str()) == Some("Android.bp") {
                // Special handling for Android.bp files.
                // Trigger error only when Android.bp defines rust_protobuf
                // or filegroups targets
                let is_truly_dangling = android_bp_files.iter().any(|bp_file| {
                    bp_file.file_path.canonicalize().unwrap() == *dangling_file
                        && !bp_file.rust_protobuf_targets.is_empty()
                        && !bp_file.filegroup_targets.is_empty()
                });

                if !is_truly_dangling {
                    continue;
                }
            }
            self.finding_collector.error(
                FindingId::AndroidBpDanglingFile,
                format!(
                    "File '{}' is not referenced in any filegroup in an Android.bp file.",
                    dangling_file.display()
                ),
                Vec::new(),
            );
        }
    }
}

impl VsidlVisitor for AndroidBpDanglingFiles {
    fn end_visit_catalog(&mut self, catalog: &Catalog) {
        let Some(catalog_path) = &catalog.catalog_path else {
            // If there's no catalog path, we can't check for dangling files.
            return;
        };

        // Check dangling files in the main catalog path
        self.check_dangling_files_in_path(catalog_path, &catalog.android_bp_files);

        // Check dangling files in all dependency paths
        for dep_catalog in &catalog.dependencies_path {
            self.check_dangling_files_in_path(dep_catalog, &catalog.android_bp_files);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_condition::ContextCondition;
    use crate::FindingSeverity;
    use std::fs::{self, File};
    use tempfile::tempdir;
    use vsidl_v1_common::android_bp::{extract_android_bp_filegroup, extract_android_bp_info};
    use vsidl_v1_common::AndroidBpFile;

    #[test]
    fn test_dangling_files_are_detected() {
        // GIVEN a temporary directory with a catalog structure
        let dir = tempdir().unwrap();
        let catalog_path = dir.path();

        let bp_path = catalog_path.join("Android.bp");
        let vsidl_path = catalog_path.join("a.vsidl");
        let proto_path = catalog_path.join("b.proto");
        let dangling_vsidl_path = catalog_path.join("dangling.vsidl");

        fs::write(
            &bp_path,
            r#"
filegroup {
    name: "my_sources",
    srcs: [
        "a.vsidl",
        "b.proto",
        "Android.bp",
    ],
}
"#,
        )
        .unwrap();
        File::create(&vsidl_path).unwrap();
        File::create(&proto_path).unwrap();
        File::create(&dangling_vsidl_path).unwrap();

        // GIVEN a catalog that represents this structure
        let bp_content = fs::read_to_string(&bp_path).unwrap();
        let android_bp_file = AndroidBpFile {
            file_path: bp_path.clone(),
            content: bp_content.clone(),
            rust_protobuf_targets: extract_android_bp_info(&bp_content).unwrap(),
            filegroup_targets: extract_android_bp_filegroup(&bp_content).unwrap(),
        };

        let mut catalog = Catalog::new();
        catalog.catalog_path = Some(catalog_path.to_path_buf());
        catalog.android_bp_files.push(android_bp_file);
        // The check walks the filesystem, so we don't need to populate the other catalog fields.

        // WHEN executing the context condition
        let mut check = AndroidBpDanglingFiles::default();
        check.check_catalog(&catalog);

        // THEN a warning for the dangling file should be found
        assert!(check.has_findings());
        assert_eq!(check.finding_collector.findings().len(), 1);
        let canonical_dangling_path = dangling_vsidl_path.canonicalize().unwrap();
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            &format!(
                "File '{}' is not referenced in any filegroup in an Android.bp file.",
                canonical_dangling_path.display()
            )
        ));
    }

    #[test]
    fn test_no_dangling_files() {
        // GIVEN a temporary directory with a catalog structure where all files are referenced
        let dir = tempdir().unwrap();
        let catalog_path = dir.path();

        let bp_path = catalog_path.join("Android.bp");
        let vsidl_path = catalog_path.join("a.vsidl");
        let proto_path = catalog_path.join("b.proto");

        fs::write(
            &bp_path,
            r#"
filegroup {
    name: "my_sources",
    srcs: [
        "**/*.vsidl",
        "**/*.proto",
        "Android.bp",
    ],
}
"#,
        )
        .unwrap();
        File::create(&vsidl_path).unwrap();
        File::create(&proto_path).unwrap();

        // GIVEN a catalog that represents this structure
        let bp_content = fs::read_to_string(&bp_path).unwrap();
        let android_bp_file = AndroidBpFile {
            file_path: bp_path.clone(),
            content: bp_content.clone(),
            rust_protobuf_targets: extract_android_bp_info(&bp_content).unwrap(),
            filegroup_targets: extract_android_bp_filegroup(&bp_content).unwrap(),
        };

        let mut catalog = Catalog::new();
        catalog.catalog_path = Some(catalog_path.to_path_buf());
        catalog.android_bp_files.push(android_bp_file);

        // WHEN executing the context condition
        let mut check = AndroidBpDanglingFiles::default();
        check.check_catalog(&catalog);

        // THEN no findings should be reported
        assert!(!check.has_findings());
    }
}
