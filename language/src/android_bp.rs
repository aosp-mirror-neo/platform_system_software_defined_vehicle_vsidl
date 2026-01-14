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

use lazy_static::lazy_static;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::fs_utils::find_matching_files_glob;
use crate::ProtoBuildTargetMetadata;

lazy_static! {
    /// Regex to capture the value of the `name` field in an Android.bp target.
    pub static ref NAME_RE: Regex = Regex::new(r#"[^_]name:\s*"([^"]*)""#).unwrap();
    /// Regex to capture the value of the `crate_name` field in a `rust_protobuf` target.
    pub static ref CRATE_NAME_RE: Regex = Regex::new(r#"crate_name:\s*"([^"]*)""#).unwrap();
    /// Regex to find the start of a `defaults` list.
    pub static ref DEFAULTS_RE: Regex = Regex::new(r#"defaults:\s*\["#).unwrap();
    /// Regex to capture the content block of a `rust_protobuf` target.
    pub static ref RUST_PROTOBUF_RE: Regex = Regex::new(r"rust_protobuf\s*\{([^}]*?)\}").unwrap();
    /// Regex to capture the list of protos within the `protos` field of a `rust_protobuf` target.
    pub static ref PROTOS_RE: Regex = Regex::new(r#"protos:\s*\[([^\]]*)\]"#).unwrap();
    /// Regex to capture the content block of a `filegroup` target.
    pub static ref FILEGROUP_RE: Regex = Regex::new(r"filegroup\s*\{([^}]*?)\}").unwrap();
    /// Regex to capture the list of source files within the `srcs` field of a `filegroup` target.
    pub static ref SRCS_RE: Regex = Regex::new(r#"srcs:\s*\[([^\]]*)\]"#).unwrap();
}

/// Represents the extracted information from a `rust_protobuf` target within an Android.bp file.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct RustProtobufInfo {
    /// The value of the `name` field in the `rust_protobuf` target.
    pub name: String,
    /// The value of the `crate_name` field in the `rust_protobuf` target.
    pub crate_name: String,
    /// A list of strings representing the entries in the `protos` field.
    /// These can be direct file paths, globs, or references to `filegroup` targets (prefixed with ':').
    pub protos: Vec<String>,
}

/// Represents the extracted information from a `filegroup` target within an Android.bp file.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct FilegroupInfo {
    /// The value of the `name` field in the `filegroup` target.
    pub name: String,
    /// A list of strings representing the entries in the `srcs` field.
    /// These are typically file paths or globs.
    pub srcs: Vec<String>,
}

/// Extracts the first capture group match for a given regex within a string slice.
///
/// # Parameters
///
/// * `content` - The string slice to search within.
/// * `regex` - The compiled regular expression to use for matching.
///
/// # Returns
///
/// An [Option<String>] containing the matched substring if found, otherwise [None].
pub fn get_first_match<'a>(content: &'a str, regex: &'a Regex) -> Option<String> {
    regex.captures(content).and_then(|caps| caps.get(1)).map(|m| m.as_str().to_string())
}

/// Extract information from all `filegroup` targets in an Android.bp file.
///
/// # Parameters
///
/// * `content` - The string content of the Android.bp file.
///
/// # Returns
///
/// A vector of [FilegroupInfo] structs, one for each `filegroup` target found.
pub fn extract_android_bp_filegroup(content: &str) -> Result<Vec<FilegroupInfo>, String> {
    let mut all_filegroup_info = Vec::new();
    for captures in FILEGROUP_RE.captures_iter(content) {
        let block_content = captures.get(1).unwrap().as_str();
        let name = get_first_match(block_content, &NAME_RE).unwrap_or_default();
        let srcs_string = get_first_match(block_content, &SRCS_RE);
        let srcs: Vec<String> = if let Some(ss) = srcs_string {
            ss.split(',').map(|s| s.trim().replace("\"", "")).filter(|s| !s.is_empty()).collect()
        } else {
            Vec::new()
        };
        all_filegroup_info.push(FilegroupInfo { name, srcs });
    }

    Ok(all_filegroup_info)
}

/// Extract information from all `rust_protobuf` targets in an Android.bp file.
///
/// # Parameters
///
/// * `content` - The string content of the Android.bp file.
///
/// # Returns
///
/// A vector of [RustProtobufInfo] structs, one for each `rust_protobuf` target found.
pub fn extract_android_bp_info(content: &str) -> Result<Vec<RustProtobufInfo>, String> {
    // TODO (b/396349696) handle operators
    let mut all_rust_protobuf_info = Vec::new();
    for captures in RUST_PROTOBUF_RE.captures_iter(content) {
        let block_content = captures.get(1).unwrap().as_str();

        // All errors handling are delegated to CoCo AndroidBpFields
        let name = get_first_match(block_content, &NAME_RE).unwrap_or_default();
        let crate_name = get_first_match(block_content, &CRATE_NAME_RE).unwrap_or_default();
        let protos_string = get_first_match(block_content, &PROTOS_RE);
        let protos: Vec<String> = if let Some(ps) = protos_string {
            ps.split(',')
                .map(|s| s.trim().replace("\"", "")) // removing double quotes "
                .filter(|s| !s.is_empty()) // removing empty names
                .collect()
        } else {
            Vec::new()
        };

        all_rust_protobuf_info.push(RustProtobufInfo { name, crate_name, protos });
    }

    Ok(all_rust_protobuf_info)
}

/// Represents an Android.bp file and its extracted information relevant to VSIDL processing.
#[derive(Debug, Clone)]
pub struct AndroidBpFile {
    /// The absolute path to the Android.bp file.
    pub file_path: PathBuf,
    /// The raw string content of the Android.bp file.
    pub content: String,
    /// A list of `rust_protobuf` targets found within the file.
    pub rust_protobuf_targets: Vec<RustProtobufInfo>,
    /// A list of `filegroup` targets found within the file.
    pub filegroup_targets: Vec<FilegroupInfo>,
}

/// Collects potential issues encountered while resolving proto files for a `rust_protobuf` target.
#[derive(Default)]
pub struct ProtoFileMatchingTargetResponse {
    /// List of `filegroup` names referenced in `protos` that could not be found.
    pub unknown_filegroup: Vec<String>,
    /// List of glob patterns or filegroup references in `protos` that did not match any files.
    pub empty_glob: Vec<String>,
    /// List of proto file paths that were included multiple times (potentially via different globs or filegroups).
    pub double_defined_protos: Vec<String>,
}

/// Resolves the `protos` list of a [RustProtobufInfo] into a set of absolute proto file paths.
///
/// This function handles direct file paths, glob patterns, and `filegroup` references (prefixed with ':').
/// It expands globs relative to the directory containing the Android.bp file and looks up filegroups
/// in the provided map.
///
/// # Parameters
///
/// * `rust_protobuf_info` - The extracted `rust_protobuf` target information.
/// * `rust_protobuf_folder` - The path to the directory containing the Android.bp file where `rust_protobuf_info` was defined.
/// * `file_groups` - A map where keys are `filegroup` names and values are lists of resolved absolute file paths belonging to that group.
///
/// # Returns
///
/// A tuple containing:
/// * A [HashSet<PathBuf>] of absolute paths to all resolved `.proto` files.
/// * A [ProtoFileMatchingTargetResponse] detailing any issues encountered during resolution.
///
/// # Panics
///
/// * If `rust_protobuf_folder` cannot be converted to a string.
/// * If glob pattern matching fails unexpectedly (though `glob` errors are generally handled).
pub fn get_all_proto_files_matching_target(
    rust_protobuf_info: &RustProtobufInfo,
    rust_protobuf_folder: &Path,
    file_groups: &HashMap<String, Vec<PathBuf>>,
) -> (HashSet<PathBuf>, ProtoFileMatchingTargetResponse) {
    let mut proto_files_per_target: HashSet<PathBuf> = HashSet::new();

    let mut proto_file_matching_target_response =
        ProtoFileMatchingTargetResponse { ..Default::default() };

    for proto in rust_protobuf_info.protos.iter() {
        let proto_files: Vec<PathBuf>;
        if let Some(file_group_name) = proto.strip_prefix(':') {
            match file_groups.get(file_group_name) {
                None => {
                    proto_file_matching_target_response.unknown_filegroup.push(proto.clone());
                    continue;
                }
                Some(globs) => {
                    proto_files = globs
                        .iter()
                        .filter_map(|g| {
                            g.extension().and_then(|ext| ext.to_str()).and_then(|ext_str| {
                                if ext_str == "proto" {
                                    Some(g.clone())
                                } else {
                                    None
                                }
                            })
                        })
                        .collect::<Vec<PathBuf>>();
                }
            }
        } else {
            proto_files =
                find_matching_files_glob(rust_protobuf_folder.to_str().unwrap(), proto).unwrap();
        }

        if proto_files.is_empty() {
            proto_file_matching_target_response.empty_glob.push(proto.clone());
        }
        for f in proto_files {
            if !proto_files_per_target.insert(f.clone()) {
                proto_file_matching_target_response
                    .double_defined_protos
                    .push(f.display().to_string());
            }
        }
    }

    (proto_files_per_target, proto_file_matching_target_response)
}

/// Expands all `filegroup` targets defined across multiple Android.bp files.
///
/// It iterates through each [AndroidBpFile], resolves the `srcs` for each `filegroup` target
/// relative to its Android.bp file's directory, and aggregates the results into the provided map.
///
/// # Parameters
///
/// * `android_bps` - A slice of [AndroidBpFile] structs representing the Android.bp files.
/// * `file_groups` - A mutable [HashMap] where keys are `filegroup` names and values are vectors
///   of absolute [PathBuf]s. This map will be populated with the expanded file lists.
pub fn expand_file_groups(
    android_bps: &[AndroidBpFile],
    file_groups: &mut HashMap<String, Vec<PathBuf>>,
) {
    for android_bp in android_bps {
        for (file_group_name, mut files) in expand_file_groups_per_android_bp(android_bp) {
            file_groups.entry(file_group_name).or_default().append(&mut files);
        }
    }
}

/// Expands all `filegroup` targets defined within a single Android.bp file.
///
/// For each `filegroup` target in the given [AndroidBpFile], this function resolves the file paths
/// or glob patterns listed in its `srcs` field relative to the directory of the Android.bp file.
///
/// # Parameters
///
/// * `android_bp` - The [AndroidBpFile] struct containing the `filegroup` targets to expand.
///
/// # Returns
///
/// A [HashMap] where keys are the names of the `filegroup` targets defined in the input `android_bp`,
/// and values are vectors of absolute [PathBuf]s representing the resolved files for each group.
///
/// # Panics
///
/// * If the parent directory of `android_bp.file_path` cannot be determined or converted to a string.
/// * If glob pattern matching fails unexpectedly (though `glob` errors are generally handled).
pub fn expand_file_groups_per_android_bp(
    android_bp: &AndroidBpFile,
) -> HashMap<String, Vec<PathBuf>> {
    let mut file_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();

    for file_group in &android_bp.filegroup_targets {
        for src in &file_group.srcs {
            file_groups.entry(file_group.name.clone()).or_default().append(
                &mut find_matching_files_glob(
                    android_bp.file_path.parent().unwrap().to_str().unwrap(),
                    src,
                )
                .unwrap(),
            );
        }
    }

    file_groups
}

/// Creates a map associating absolute proto file paths with their corresponding build target metadata.
///
/// This function processes a list of [AndroidBpFile]s. It first expands all `filegroup`
/// targets globally, then iterates through each `rust_protobuf` target in each file. For each
/// `rust_protobuf` target, it resolves its `protos` list (handling globs and filegroups) to
/// absolute proto file paths. Finally, it maps each unique absolute proto file path to the
/// [ProtoBuildTargetMetadata] derived from the `rust_protobuf` target it belongs to.
///
/// # Parameters
///
/// * `android_bp_files` - A slice of [AndroidBpFile] structs representing the Android.bp files.
///
/// # Returns
///
/// A [HashMap] where keys are absolute canonicalized [PathBuf]s of `.proto` files, and values are
/// the [ProtoBuildTargetMetadata] associated with the `rust_protobuf` target that includes the file.
/// If a proto file is included in multiple `rust_protobuf` targets, the behavior is currently
/// undefined (last one processed might win). Context condition checks should handle such conflicts.
///
/// # Panics
///
/// * If canonicalization of paths fails (e.g., file not found during processing, though `get_all_proto_files_matching_target` should handle initial resolution).
/// * If path manipulations (like getting parent directory) fail unexpectedly.
pub fn proto_file_to_metadata(
    android_bp_to_properties: &[AndroidBpFile],
) -> HashMap<PathBuf, ProtoBuildTargetMetadata> {
    let mut proto_file_to_property: HashMap<PathBuf, ProtoBuildTargetMetadata> = HashMap::new();

    let mut filegroups_to_files: HashMap<String, Vec<PathBuf>> = HashMap::new();
    expand_file_groups(android_bp_to_properties, &mut filegroups_to_files);

    for android_bp in android_bp_to_properties.iter() {
        for rust_protobuf_info in &android_bp.rust_protobuf_targets {
            let (proto_files_per_target, _) = get_all_proto_files_matching_target(
                rust_protobuf_info,
                android_bp.file_path.parent().unwrap(),
                &filegroups_to_files,
            );

            for proto_file in proto_files_per_target {
                let absolute_proto_file_path: PathBuf = proto_file.canonicalize().unwrap().clone();

                let metadata = ProtoBuildTargetMetadata {
                    folder: android_bp.file_path.parent().unwrap().canonicalize().unwrap(),
                    target_name: Some(rust_protobuf_info.name.clone()),
                    crate_name: Some(rust_protobuf_info.crate_name.clone()),
                };
                proto_file_to_property.insert(absolute_proto_file_path, metadata);
            }
        }
    }

    proto_file_to_property
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::{tempdir, TempDir};

    fn setup_tmp_android_bp_file() -> (TempDir, PathBuf, File) {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("Android.bp");
        let file = File::create(&file_path).unwrap();
        (dir, file_path, file)
    }

    #[test]
    fn test_extract_android_bp_info_basic() {
        // GIVEN Android.bp file with one rust_protobuf target
        let file_content = r#"
rust_protobuf {{
    name: "test_proto",
    crate_name: "test_crate",
    protos: ["proto1.proto", "proto2.proto"],
}}
            "#
        .to_string();

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_info(&file_content);

        // THEN the expected RustRotobufInfo struct is returned
        let expected = RustProtobufInfo {
            name: "test_proto".to_string(),
            crate_name: "test_crate".to_string(),
            protos: vec!["proto1.proto".to_string(), "proto2.proto".to_string()],
        };
        assert_eq!(result.unwrap(), vec![expected]);
    }

    #[test]
    fn test_extract_android_bp_info_regex() {
        // GIVEN Android.bp file with one rust_protobuf target
        let file_content = r#"
rust_protobuf {{
    name: "test_regex_name",
    crate_name: "test_regex_crate",
    protos: ["*.proto", "subfolder/*.proto", "**/*.proto", "**/*"],
}}
            "#
        .to_string();

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_info(&file_content);

        // THEN the expected RustRotobufInfo struct is returned
        let expected = RustProtobufInfo {
            name: "test_regex_name".to_string(),
            crate_name: "test_regex_crate".to_string(),
            protos: vec![
                "*.proto".to_string(),
                "subfolder/*.proto".to_string(),
                "**/*.proto".to_string(),
                "**/*".to_string(),
            ],
        };
        assert_eq!(result.unwrap(), vec![expected]);
    }

    #[test]
    fn test_extract_android_bp_info_multiple_blocks() {
        // GIVEN Android.bp file with two rust_protobuf target
        let file_content = r#"
rust_protobuf {{
    name: "proto_a",
    crate_name: "crate_a",
    protos: ["a.proto"],
}}
rust_protobuf {{
    name: "proto_b",
    crate_name: "crate_b",
    protos: ["b1.proto", "b2.proto"],
}}
            "#
        .to_string();

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_info(&file_content);

        // THEN two RustRotobufInfo are returned
        let expected_a = RustProtobufInfo {
            name: "proto_a".to_string(),
            crate_name: "crate_a".to_string(),
            protos: vec!["a.proto".to_string()],
        };
        let expected_b = RustProtobufInfo {
            name: "proto_b".to_string(),
            crate_name: "crate_b".to_string(),
            protos: vec!["b1.proto".to_string(), "b2.proto".to_string()],
        };
        assert_eq!(result.unwrap(), vec![expected_a, expected_b]);
    }

    #[test]
    fn test_extract_android_bp_info_empty_protos() {
        // GIVEN Android.bp file with empty list of proto
        let file_content = r#"
rust_protobuf {{
    name: "empty_proto",
    crate_name: "empty_crate",
    protos: [],
}}
            "#
        .to_string();

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_info(&file_content);

        // THEN RustProtobufInfo has empty list of protos
        assert_eq!(
            result.unwrap(),
            vec![RustProtobufInfo {
                name: "empty_proto".to_string(),
                crate_name: "empty_crate".to_string(),
                protos: vec![],
            }]
        );
    }

    #[test]
    fn test_extract_android_bp_info_no_protos_field() {
        // GIVEN Android.bp file with no protos field
        let file_content = r#"
rust_protobuf {{
    name: "no_protos",
    crate_name: "no_protos_crate",
}}
            "#
        .to_string();

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_info(&file_content);

        // THEN RustProtobufInfo has empty list of protos
        assert_eq!(
            result.unwrap(),
            vec![RustProtobufInfo {
                name: "no_protos".to_string(),
                crate_name: "no_protos_crate".to_string(),
                protos: vec![],
            }]
        );
    }

    #[test]
    fn test_extract_android_bp_empty_file() {
        // GIVEN Android.bp empty file
        let file_content = "".to_string();

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_info(&file_content);

        // THEN the expected RustRotobufInfo struct is returned
        assert_eq!(result.unwrap(), Vec::new());
    }

    #[test]
    fn test_proto_file_to_metadata_basic() {
        // GIVEN a map with Android.bp files and RustProtobufInfo
        let (dir, android_bp_file_path, mut _file) = setup_tmp_android_bp_file();
        let proto_file_path = dir.path().join("proto1.proto");
        File::create(&proto_file_path).unwrap();

        // GIVEN Android with the following RustProtobufInfo
        let rust_protobuf_info = RustProtobufInfo {
            name: "test_proto".to_string(),
            crate_name: "test_crate".to_string(),
            protos: vec!["proto1.proto".to_string()],
        };
        let android_bp_to_properties = vec![AndroidBpFile {
            file_path: android_bp_file_path.clone(),
            content: "".to_string(),
            rust_protobuf_targets: vec![rust_protobuf_info],
            filegroup_targets: Vec::new(),
        }];

        // WHEN extracting proto file metadata
        let result = proto_file_to_metadata(&android_bp_to_properties);

        // THEN the expected ProtoBuildTargetMetadata is returned
        let canonicalized_proto_file_path = proto_file_path.canonicalize().unwrap();
        let expected_metadata = ProtoBuildTargetMetadata {
            folder: android_bp_file_path.parent().unwrap().canonicalize().unwrap(),
            target_name: Some("test_proto".to_string()),
            crate_name: Some("test_crate".to_string()),
        };
        assert_eq!(result.len(), 1);
        assert_eq!(result.get(&canonicalized_proto_file_path), Some(&expected_metadata));
    }

    #[test]
    fn test_proto_file_to_metadata_multiple_proto_files() {
        // GIVEN a map with Android.bp files and RustProtobufInfo with multiple proto files
        let (dir, android_bp_file_path, mut _file) = setup_tmp_android_bp_file();
        let proto_file_path_1 = dir.path().join("proto1.proto");
        let proto_file_path_2 = dir.path().join("proto2.proto");
        File::create(&proto_file_path_1).unwrap();
        File::create(&proto_file_path_2).unwrap();

        // GIVEN Android with a RustProtobufInfo including proto1.proto and proto2.proto
        let rust_protobuf_info = RustProtobufInfo {
            name: "test_proto".to_string(),
            crate_name: "test_crate".to_string(),
            protos: vec!["proto1.proto".to_string(), "proto2.proto".to_string()],
        };
        let android_bp_to_properties = vec![AndroidBpFile {
            file_path: android_bp_file_path.clone(),
            content: "".to_string(),
            rust_protobuf_targets: vec![rust_protobuf_info],
            filegroup_targets: Vec::new(),
        }];

        // WHEN extracting proto file metadata
        let result = proto_file_to_metadata(&android_bp_to_properties);

        // THEN the expected ProtoBuildTargetMetadata is returned for each proto file
        let canonicalized_proto_file_path_1 = proto_file_path_1.canonicalize().unwrap();
        let canonicalized_proto_file_path_2 = proto_file_path_2.canonicalize().unwrap();
        let expected_metadata = ProtoBuildTargetMetadata {
            folder: android_bp_file_path.parent().unwrap().canonicalize().unwrap(),
            target_name: Some("test_proto".to_string()),
            crate_name: Some("test_crate".to_string()),
        };
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(&canonicalized_proto_file_path_1), Some(&expected_metadata));
        assert_eq!(result.get(&canonicalized_proto_file_path_2), Some(&expected_metadata));
    }

    #[test]
    fn test_proto_file_to_metadata_multiple_android_bp() {
        // GIVEN a map with Android.bp files and RustProtobufInfo with multiple proto files
        let (dir_1, android_bp_file_path_1, mut _file) = setup_tmp_android_bp_file();
        let (dir_2, android_bp_file_path_2, mut _file) = setup_tmp_android_bp_file();
        let proto_file_path_1 = dir_1.path().join("proto1.proto");
        let proto_file_path_2 = dir_2.path().join("proto2.proto");
        File::create(&proto_file_path_1).unwrap();
        File::create(&proto_file_path_2).unwrap();

        // GIVEN Android with a RustProtobufInfo including proto1.proto and proto2.proto
        let rust_protobuf_info_1 = RustProtobufInfo {
            name: "test_proto1".to_string(),
            crate_name: "test_crate1".to_string(),
            protos: vec!["proto1.proto".to_string()],
        };
        let rust_protobuf_info_2 = RustProtobufInfo {
            name: "test_proto2".to_string(),
            crate_name: "test_crate2".to_string(),
            protos: vec!["proto2.proto".to_string()],
        };
        let android_bp_to_properties = vec![
            AndroidBpFile {
                file_path: android_bp_file_path_1.clone(),
                content: "".to_string(),
                rust_protobuf_targets: vec![rust_protobuf_info_1],
                filegroup_targets: Vec::new(),
            },
            AndroidBpFile {
                file_path: android_bp_file_path_2.clone(),
                content: "".to_string(),
                rust_protobuf_targets: vec![rust_protobuf_info_2],
                filegroup_targets: Vec::new(),
            },
        ];

        // WHEN extracting proto file metadata
        let result = proto_file_to_metadata(&android_bp_to_properties);

        // THEN the expected ProtoBuildTargetMetadata is returned for each proto file
        let canonicalized_proto_file_path_1 = proto_file_path_1.canonicalize().unwrap();
        let canonicalized_proto_file_path_2 = proto_file_path_2.canonicalize().unwrap();
        let expected_metadata_1 = ProtoBuildTargetMetadata {
            folder: android_bp_file_path_1.parent().unwrap().canonicalize().unwrap(),
            target_name: Some("test_proto1".to_string()),
            crate_name: Some("test_crate1".to_string()),
        };
        let expected_metadata_2 = ProtoBuildTargetMetadata {
            folder: android_bp_file_path_2.parent().unwrap().canonicalize().unwrap(),
            target_name: Some("test_proto2".to_string()),
            crate_name: Some("test_crate2".to_string()),
        };
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(&canonicalized_proto_file_path_1), Some(&expected_metadata_1));
        assert_eq!(result.get(&canonicalized_proto_file_path_2), Some(&expected_metadata_2));
    }

    #[test]
    fn test_proto_file_to_metadata_multiple_rust_protobuf_info() {
        // GIVEN a map with Android.bp files and RustProtobufInfo with multiple proto files
        let (dir, android_bp_file_path, mut _file) = setup_tmp_android_bp_file();
        let proto_file_path_1 = dir.path().join("proto1.proto");
        let proto_file_path_2 = dir.path().join("proto2.proto");
        File::create(&proto_file_path_1).unwrap();
        File::create(&proto_file_path_2).unwrap();

        // GIVEN Android with a RustProtobufInfo including proto1.proto and proto2.proto
        let rust_protobuf_info_1 = RustProtobufInfo {
            name: "test_proto1".to_string(),
            crate_name: "test_crate1".to_string(),
            protos: vec!["proto1.proto".to_string()],
        };
        let rust_protobuf_info_2 = RustProtobufInfo {
            name: "test_proto2".to_string(),
            crate_name: "test_crate2".to_string(),
            protos: vec!["proto2.proto".to_string()],
        };
        let android_bp_to_properties = vec![AndroidBpFile {
            file_path: android_bp_file_path.clone(),
            content: "".to_string(),
            rust_protobuf_targets: vec![rust_protobuf_info_1, rust_protobuf_info_2],
            filegroup_targets: Vec::new(),
        }];

        // WHEN extracting proto file metadata
        let result = proto_file_to_metadata(&android_bp_to_properties);

        // THEN the expected ProtoBuildTargetMetadata is returned for each proto file
        let canonicalized_proto_file_path_1 = proto_file_path_1.canonicalize().unwrap();
        let canonicalized_proto_file_path_2 = proto_file_path_2.canonicalize().unwrap();
        let expected_metadata_1 = ProtoBuildTargetMetadata {
            folder: android_bp_file_path.parent().unwrap().canonicalize().unwrap(),
            target_name: Some("test_proto1".to_string()),
            crate_name: Some("test_crate1".to_string()),
        };
        let expected_metadata_2 = ProtoBuildTargetMetadata {
            folder: android_bp_file_path.parent().unwrap().canonicalize().unwrap(),
            target_name: Some("test_proto2".to_string()),
            crate_name: Some("test_crate2".to_string()),
        };
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(&canonicalized_proto_file_path_1), Some(&expected_metadata_1));
        assert_eq!(result.get(&canonicalized_proto_file_path_2), Some(&expected_metadata_2));
    }

    #[test]
    fn test_proto_file_to_metadata_rust_protobuf_info_regex() {
        // GIVEN a map with Android.bp files and RustProtobufInfo with multiple proto files
        let (dir, android_bp_file_path, mut _file) = setup_tmp_android_bp_file();
        let proto_file_path_1 = dir.path().join("proto1.proto");
        let proto_file_path_2 = dir.path().join("proto2.proto");
        File::create(&proto_file_path_1).unwrap();
        File::create(&proto_file_path_2).unwrap();

        // GIVEN Android with a RustProtobufInfo including proto1.proto and proto2.proto
        let rust_protobuf_info = RustProtobufInfo {
            name: "test_proto".to_string(),
            crate_name: "test_crate".to_string(),
            protos: vec!["*.proto".to_string()],
        };
        let android_bp_to_properties = vec![AndroidBpFile {
            file_path: android_bp_file_path.clone(),
            content: "".to_string(),
            rust_protobuf_targets: vec![rust_protobuf_info],
            filegroup_targets: Vec::new(),
        }];

        // WHEN extracting proto file metadata
        let result = proto_file_to_metadata(&android_bp_to_properties);

        // THEN the expected ProtoBuildTargetMetadata is returned for each proto file
        let canonicalized_proto_file_path_1 = proto_file_path_1.canonicalize().unwrap();
        let canonicalized_proto_file_path_2 = proto_file_path_2.canonicalize().unwrap();
        let expected_metadata = ProtoBuildTargetMetadata {
            folder: android_bp_file_path.parent().unwrap().canonicalize().unwrap(),
            target_name: Some("test_proto".to_string()),
            crate_name: Some("test_crate".to_string()),
        };
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(&canonicalized_proto_file_path_1), Some(&expected_metadata));
        assert_eq!(result.get(&canonicalized_proto_file_path_2), Some(&expected_metadata));
    }

    #[test]
    fn test_proto_file_to_metadata_filegroup_not_found() {
        // GIVEN a map with Android.bp files and RustProtobufInfo with multiple proto files
        let (dir, android_bp_file_path, mut _file) = setup_tmp_android_bp_file();
        let proto_file_path_1 = dir.path().join("proto1.proto");
        File::create(&proto_file_path_1).unwrap();

        // GIVEN Android with a RustProtobufInfo including proto1.proto and proto2.proto
        let rust_protobuf_info = RustProtobufInfo {
            name: "test_proto".to_string(),
            crate_name: "test_crate".to_string(),
            protos: vec![":test_filegroup".to_string()],
        };
        let android_bp_to_properties = vec![AndroidBpFile {
            file_path: android_bp_file_path.clone(),
            content: "".to_string(),
            rust_protobuf_targets: vec![rust_protobuf_info],
            filegroup_targets: Vec::new(),
        }];

        // WHEN extracting proto file metadata
        let result = proto_file_to_metadata(&android_bp_to_properties);

        // THEN the expected ProtoBuildTargetMetadata is returned for nothing
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_proto_file_to_metadata_filegroup_with_proto() {
        // GIVEN a map with Android.bp files and RustProtobufInfo with multiple proto files
        let (dir, android_bp_file_path, mut _file) = setup_tmp_android_bp_file();
        let proto_file_path_1 = dir.path().join("proto1.proto");
        let proto_file_path_2 = dir.path().join("proto2.proto");
        File::create(&proto_file_path_1).unwrap();
        File::create(&proto_file_path_2).unwrap();

        // GIVEN Android with a RustProtobufInfo including a filegroup
        let rust_protobuf_info = RustProtobufInfo {
            name: "test_proto".to_string(),
            crate_name: "test_crate".to_string(),
            protos: vec![":test_filegroup".to_string()],
        };

        // GIVEN a filegroup with some protos
        let filegroup_info = FilegroupInfo {
            name: "test_filegroup".to_string(),
            srcs: vec!["proto1.proto".to_string(), "proto2.proto".to_string()],
        };

        let android_bp_to_properties = vec![AndroidBpFile {
            file_path: android_bp_file_path.clone(),
            content: "".to_string(),
            rust_protobuf_targets: vec![rust_protobuf_info],
            filegroup_targets: vec![filegroup_info], // Include the filegroup here
        }];

        // WHEN extracting proto file metadata
        let result = proto_file_to_metadata(&android_bp_to_properties);

        // THEN the expected ProtoBuildTargetMetadata is returned for both proto files
        let canonicalized_proto_file_path_1 = proto_file_path_1.canonicalize().unwrap();
        let canonicalized_proto_file_path_2 = proto_file_path_2.canonicalize().unwrap();
        let expected_metadata = ProtoBuildTargetMetadata {
            folder: android_bp_file_path.parent().unwrap().canonicalize().unwrap(),
            target_name: Some("test_proto".to_string()),
            crate_name: Some("test_crate".to_string()),
        };
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(&canonicalized_proto_file_path_1), Some(&expected_metadata));
        assert_eq!(result.get(&canonicalized_proto_file_path_2), Some(&expected_metadata));
    }

    #[test]
    fn test_proto_file_to_metadata_rust_protobuf_file_matched_multiple_times() {
        // GIVEN a map with Android.bp files and RustProtobufInfo with multiple proto files
        let (dir, android_bp_file_path, mut _file) = setup_tmp_android_bp_file();
        let proto_file_path_1 = dir.path().join("proto1.proto");
        File::create(&proto_file_path_1).unwrap();

        // GIVEN Android with a RustProtobufInfo including proto1.proto and proto2.proto
        let rust_protobuf_info = RustProtobufInfo {
            name: "test_proto".to_string(),
            crate_name: "test_crate".to_string(),
            protos: vec!["*.proto".to_string(), "proto1.proto".to_string()],
        };
        let android_bp_to_properties = vec![AndroidBpFile {
            file_path: android_bp_file_path.clone(),
            content: "".to_string(),
            rust_protobuf_targets: vec![rust_protobuf_info],
            filegroup_targets: Vec::new(),
        }];

        // WHEN extracting proto file metadata
        let result = proto_file_to_metadata(&android_bp_to_properties);

        // THEN the expected ProtoBuildTargetMetadata is returned for each proto file
        let canonicalized_proto_file_path_1 = proto_file_path_1.canonicalize().unwrap();
        let expected_metadata = ProtoBuildTargetMetadata {
            folder: android_bp_file_path.parent().unwrap().canonicalize().unwrap(),
            target_name: Some("test_proto".to_string()),
            crate_name: Some("test_crate".to_string()),
        };
        assert_eq!(result.len(), 1);
        assert_eq!(result.get(&canonicalized_proto_file_path_1), Some(&expected_metadata));
    }

    #[test]
    fn test_proto_file_to_metadata_filegroup_without_proto() {
        // GIVEN a map with Android.bp files and RustProtobufInfo with multiple proto files
        let (dir, android_bp_file_path, mut _file) = setup_tmp_android_bp_file();
        let file_path_1 = dir.path().join("file1.txt");
        File::create(&file_path_1).unwrap();

        // GIVEN Android with a RustProtobufInfo including a filegroup
        let rust_protobuf_info = RustProtobufInfo {
            name: "test_proto".to_string(),
            crate_name: "test_crate".to_string(),
            protos: vec![":test_filegroup".to_string()],
        };

        // GIVEN a filegroup with some protos
        let filegroup_info = FilegroupInfo {
            name: "test_filegroup".to_string(),
            srcs: vec!["file1.txt".to_string()],
        };

        let android_bp_to_properties = vec![AndroidBpFile {
            file_path: android_bp_file_path.clone(),
            content: "".to_string(),
            rust_protobuf_targets: vec![rust_protobuf_info],
            filegroup_targets: vec![filegroup_info], // Include the filegroup here
        }];

        // WHEN extracting proto file metadata
        let result = proto_file_to_metadata(&android_bp_to_properties);

        // THEN the no result is returned because no protofile is defineds
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_extract_android_bp_filegroup_info_basic() {
        // GIVEN Android.bp file with one filegroup target
        let file_content = r#"
filegroup {
    name: "test_filegroup",
    srcs: ["file1.proto", "file2.proto"],
}
            "#;

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_filegroup(file_content);

        // THEN the expected FilegroupInfo is returned
        let expected = FilegroupInfo {
            name: "test_filegroup".to_string(),
            srcs: vec!["file1.proto".to_string(), "file2.proto".to_string()],
        };
        assert_eq!(result.unwrap(), vec![expected]);
    }

    #[test]
    fn test_extract_android_bp_filegroup_info_multiple_blocks() {
        // GIVEN Android.bp file with two filegroup targets
        let file_content = r#"
filegroup {
    name: "group_a",
    srcs: ["a1.proto"],
}

filegroup {
    name: "group_b",
    srcs: ["b1.proto", "b2.proto"],
}
            "#;

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_filegroup(file_content);

        // THEN two FilegroupInfo are returned
        let expected_a =
            FilegroupInfo { name: "group_a".to_string(), srcs: vec!["a1.proto".to_string()] };
        let expected_b = FilegroupInfo {
            name: "group_b".to_string(),
            srcs: vec!["b1.proto".to_string(), "b2.proto".to_string()],
        };
        assert_eq!(result.unwrap(), vec![expected_a, expected_b]);
    }

    #[test]
    fn test_extract_android_bp_filegroup_info_empty_srcs() {
        // GIVEN Android.bp file with an empty list of srcs
        let file_content = r#"
filegroup {
    name: "empty_group",
    srcs: [],
}
            "#;

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_filegroup(file_content).unwrap();

        // THEN FilegroupInfo has an empty list of srcs
        assert_eq!(result, vec![FilegroupInfo { name: "empty_group".to_string(), srcs: vec![] }]);
    }

    #[test]
    fn test_extract_android_bp_filegroup_info_no_srcs_field() {
        // GIVEN Android.bp file with no srcs field
        let file_content = r#"
filegroup {
    name: "no_srcs_group",
}
            "#;

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_filegroup(file_content).unwrap();

        // THEN FilegroupInfo has an empty list of srcs
        assert_eq!(result, vec![FilegroupInfo { name: "no_srcs_group".to_string(), srcs: vec![] }]);
    }

    #[test]
    fn test_extract_android_bp_filegroup_info_empty_file() {
        // GIVEN an empty Android.bp file
        let file_content = "";

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_filegroup(file_content).unwrap();

        // THEN an empty vec is returned
        assert_eq!(result, Vec::<FilegroupInfo>::new());
    }

    #[test]
    fn test_extract_android_bp_filegroup_info_missing_name() {
        // GIVEN an Android.bp file with a filegroup missing the name field
        let file_content = r#"
filegroup {{
    srcs: ["file1.proto"],
}}
            "#;

        // WHEN extracting information from Android.bp
        let result = extract_android_bp_filegroup(file_content).unwrap();
        let expected =
            FilegroupInfo { name: "".to_string(), srcs: vec!["file1.proto".to_string()] };

        // THEN an error is returned
        assert_eq!(result, vec![expected]);
    }

    #[test]
    fn test_expand_file_groups() {
        // GIVEN two Android.bp files with filegroups
        let (dir1, bp_path1, _file1) = setup_tmp_android_bp_file();
        let (dir2, bp_path2, _file2) = setup_tmp_android_bp_file();

        let bp1 = AndroidBpFile {
            file_path: bp_path1,
            content: String::new(),
            rust_protobuf_targets: vec![],
            filegroup_targets: vec![FilegroupInfo {
                name: "group1".to_string(),
                srcs: vec!["*.proto".to_string()],
            }],
        };
        let bp2 = AndroidBpFile {
            file_path: bp_path2,
            content: String::new(),
            rust_protobuf_targets: vec![],
            filegroup_targets: vec![
                FilegroupInfo { name: "group1".to_string(), srcs: vec!["a.proto".to_string()] },
                FilegroupInfo { name: "group2".to_string(), srcs: vec!["b*.proto".to_string()] },
            ],
        };

        // Create some dummy files
        File::create(dir1.path().join("file1.proto")).unwrap();
        File::create(dir1.path().join("file2.proto")).unwrap();
        File::create(dir2.path().join("a.proto")).unwrap();
        File::create(dir2.path().join("ba.proto")).unwrap();
        File::create(dir2.path().join("bb.proto")).unwrap();

        // WHEN expanding file groups across both files
        let mut file_groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
        expand_file_groups(&[bp1, bp2], &mut file_groups);

        // THEN the file groups should be correctly expanded and aggregated
        assert_eq!(file_groups.len(), 2);

        let group1_files: HashSet<_> = file_groups.get("group1").unwrap().iter().cloned().collect();
        let expected_group1: HashSet<_> = [
            dir1.path().join("file1.proto"),
            dir1.path().join("file2.proto"),
            dir2.path().join("a.proto"),
        ]
        .into_iter()
        .collect();
        assert_eq!(group1_files, expected_group1);

        let group2_files: HashSet<_> = file_groups.get("group2").unwrap().iter().cloned().collect();
        let expected_group2: HashSet<_> =
            [dir2.path().join("ba.proto"), dir2.path().join("bb.proto")].into_iter().collect();
        assert_eq!(group2_files, expected_group2);
    }
}
