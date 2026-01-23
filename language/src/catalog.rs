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
//! The module compiles VSIDL catalog and its dependencies into in-memory objects.

use crate::file_provider::FileProvider;
use anyhow::anyhow;
use anyhow::bail;
use lazy_static::lazy_static;
use protobuf::descriptor::FileDescriptorProto;
use protobuf::text_format;
use protobuf_parse::Parser as ProtobufParser;
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use vsidl_v1::syntax::VsidlEntry;
use walkdir::WalkDir;

use crate::android_bp::{
    extract_android_bp_filegroup, extract_android_bp_info, proto_file_to_metadata, AndroidBpFile,
};
use pest::Parser;
use vsidl_v1_pest_parser::{
    converter,
    pest_parser::{Rule, VsidlParser},
};

lazy_static! {
    /// Lazily initialized static vector holding metadata and file descriptors for implicitly included protobuf files.
    /// These include well-known protobuf types and VSIDL annotations.
    pub(crate) static ref IMPLICIT_DEPENDENCIES_PROTO_FILE_DESCRIPTORS: Vec<(ProtoBuildTargetMetadata, FileDescriptorProto)> = {
        let mut fds: Vec<_> = sdv_vsidl_common::WELLKNOWN_PROTOS_TO_DESCRIPTOR
            .values()
            .map(|fd| {
                (
                    ProtoBuildTargetMetadata {
                        folder: PathBuf::from(""),
                        target_name: Some("libprotobuf".to_string()),
                        crate_name: Some("protobuf::well_known_types".to_string()),
                    },
                    fd.proto().clone(),
                )
            })
            .collect();
        fds.push((
            ProtoBuildTargetMetadata {
                folder: PathBuf::from(""),
                target_name: Some("libvsidl_v1_stdlib_proto_rs".to_string()),
                crate_name: Some("vsidl_v1_stdlib".to_string()),
            },
            vsidl_v1::annotations::file_descriptor().proto().clone(),
        ));
        fds
    };
}

#[derive(Clone, Debug, PartialEq)]
/// Metadata for protobuf build target (required to generate Rust MW).
///
/// This struct holds information about the build context of a protobuf file,
/// such as its location and associated build target names.
pub struct ProtoBuildTargetMetadata {
    /// The root folder of the catalog or dependency where the proto file resides.
    pub folder: PathBuf,
    /// The name of the build target (`rust_protobuf` name in `Android.bp`).
    pub target_name: Option<String>,
    /// The Rust crate name associated with the `rust_protobuf` target.
    pub crate_name: Option<String>,
}

#[derive(Clone, Debug)]
/// Compiled in-memory representation of a VSIDL catalog and its dependencies.
///
/// This struct aggregates all the parsed information from `.vsidl`, `.proto`,
/// and `Android.bp` files found within the main catalog directory and any specified
/// dependency directories.
pub struct Catalog {
    /// A collection of protobuf [FileDescriptorProto] objects, each paired with its
    /// corresponding [ProtoBuildTargetMetadata]. This includes descriptors from the
    /// main catalog, dependencies, and implicitly included well-known types.
    pub proto_descriptors: Vec<(ProtoBuildTargetMetadata, FileDescriptorProto)>,
    /// A list of parsed [VsidlEntry] objects representing the VSIDL definitions
    /// found in dependency catalogs.
    pub dependencies_vsidl_files: Vec<VsidlEntry>,
    /// A list of pairs, where each pair contains the relative path (within the catalog directory)
    /// and the parsed [VsidlEntry] for a VSIDL file found in the main catalog directory.
    pub catalog_vsidl_files: Vec<(PathBuf, VsidlEntry)>,
    /// A list of parsed [AndroidBpFile] objects representing the `Android.bp` files
    /// found within the catalog and dependency directories.
    pub android_bp_files: Vec<AndroidBpFile>,
    /// The absolute path to the root of the main catalog.
    pub catalog_path: Option<PathBuf>,
    /// The list of dependencies path
    pub dependencies_path: Vec<PathBuf>,
}

impl Catalog {
    /// Creates a new, empty [Catalog] instance, pre-populated with implicit protobuf dependencies.
    ///
    /// The implicit dependencies include standard Google well-known types and VSIDL annotations.
    ///
    /// # Returns
    ///
    /// An initialized [Catalog] with empty lists for catalog/dependency files and
    /// pre-filled implicit protobuf descriptors.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            proto_descriptors: IMPLICIT_DEPENDENCIES_PROTO_FILE_DESCRIPTORS.clone(),
            dependencies_vsidl_files: Vec::new(),
            catalog_vsidl_files: Vec::new(),
            android_bp_files: Vec::new(),
            catalog_path: None,
            dependencies_path: Vec::new(),
        }
    }
}

/// Recursively finds all files within a directory that have a specific extension.
///
/// # Parameters
///
/// * `path` - The directory path to search within.
/// * `target_extension` - The desired file extension (e.g., "proto", "vsidl"). Must not be empty.
///
/// # Returns
///
/// A `Result` containing a vector of [PathBuf] for each file found with the matching extension,
/// or an error if the path is not a directory or the extension is empty.
///
/// # Errors
///
/// * If `target_extension` is empty.
/// * If there are issues accessing the file system during the walk (e.g., permission errors).
fn list_files_with_extension<F: FileProvider>(
    file_provider: &F,
    path: &Path,
    target_extension: &str,
) -> anyhow::Result<Vec<PathBuf>> {
    file_provider.list_files_with_extension(path, target_extension)
}

/// Reads and parses all `.vsidl` files found within the specified directory.
///
/// # Parameters
///
/// * `path` - The directory path containing the `.vsidl` files.
///
/// # Returns
///
/// A vector of tuples. Each tuple consists of:
///   - The relative path of the `.vsidl` file within the `path` directory.
///   - The parsed [VsidlEntry] object representing the content of the file.
///
/// # Errors
///
/// * If any file listed by [list_files_with_extension] cannot be read.
/// * If the text content of a `.vsidl` file cannot be parsed into a [VsidlEntry]
///   (e.g., due to syntax errors). The error message will include the file path.
fn read_vsidl_files<F: FileProvider>(
    file_provider: &F,
    path: &Path,
) -> anyhow::Result<Vec<(PathBuf, VsidlEntry)>> {
    const FILE_EXTENSION_VSIDL: &str = "vsidl";
    let mut contents = Vec::new();
    for file in list_files_with_extension(file_provider, path, FILE_EXTENSION_VSIDL)? {
        let textproto = file_provider.read_to_string(&file)?;
        let entry = text_format::parse_from_str::<VsidlEntry>(&textproto).map_err(
            |e: text_format::ParseError| {
                anyhow!(file.to_str().unwrap().to_owned() + ": " + &e.to_string())
            },
        )?;
        contents.push((file.strip_prefix(path).unwrap().to_path_buf(), entry));
    }
    Ok(contents)
}

/// Reads and parses all `.vsidl` files found within the specified directory using Pest.
///
/// # Parameters
///
/// * `path` - The directory path containing the `.vsidl` files.
///
/// # Returns
///
/// A vector of tuples. Each tuple consists of:
///   - The relative path of the `.vsidl` file within the `path` directory.
///   - The parsed [VsidlEntry] object representing the content of the file.
///
/// # Errors
///
/// * If any file listed by [list_files_with_extension] cannot be read.
/// * If the text content of a `.vsidl` file cannot be parsed into a [VsidlEntry]
///   (e.g., due to syntax errors). The error message will include the file path.
fn read_vsidl_files_using_pest<F: FileProvider>(
    file_provider: &F,
    path: &Path,
) -> anyhow::Result<Vec<(PathBuf, VsidlEntry)>> {
    const FILE_EXTENSION_VSIDL: &str = "vsidl";
    let mut contents = Vec::new();
    for file in list_files_with_extension(file_provider, path, FILE_EXTENSION_VSIDL)? {
        let vsidl_content = file_provider.read_to_string(&file)?;
        let parsed_file = VsidlParser::parse(Rule::vsidl_file, &vsidl_content);
        if parsed_file.is_err() {
            return Err(parsed_file
                .clone()
                .err()
                .unwrap()
                .with_path(file.to_str().unwrap())
                .into());
        }
        let vsidl_entry = converter::convert_to_vsidl_entry(
            parsed_file.unwrap().next().unwrap(),
            file.to_str().unwrap(),
        );
        contents.push((file.strip_prefix(path).unwrap().to_path_buf(), vsidl_entry.clone()));
    }
    Ok(contents)
}

/// Reads and parses all `.proto` files found within the specified directory, using provided include paths.
///
/// It uses the `protobuf-parse` crate to handle proto parsing and dependency resolution.
///
/// # Parameters
///
/// * `path` - The directory path containing the `.proto` files.
/// * `includes` - A slice of paths to be used as include directories for the protobuf parser.
/// * `vsidl_syntax_path` - The path to the directory containing the VSIDL syntax definition protos (e.g., `annotations.proto`).
/// * `mpl` - A boolean flag indicating whether Multiple Protobuf Libraries (MPL) mode is enabled.
///   If true, metadata is initially generic and refined later. If false, it attempts
///   to find metadata from a single `Android.bp` in the `path`.
///
/// # Returns
///
/// A vector of tuples. Each tuple consists of:
///   - The determined [ProtoBuildTargetMetadata] for the proto file.
///   - The parsed [FileDescriptorProto] object representing the content of the file.
///
/// # Errors
///
/// * If `list_files_with_extension` fails.
/// * If the `protobuf_parse::Parser` fails to produce a file descriptor set.
/// * If `mpl` is false and [find_and_extract_proto_build_target_metadata] fails (e.g., no `Android.bp` found).
/// * If path canonicalization fails.

fn read_protobuf_files<F: FileProvider>(
    file_provider: &F,
    path: &Path,
    includes: &[&Path],
    vsidl_syntax_path: &Path,
) -> anyhow::Result<Vec<(ProtoBuildTargetMetadata, FileDescriptorProto)>> {
    const FILE_EXTENSION_PROTO: &str = "proto";

    let proto_files = list_files_with_extension(file_provider, path, FILE_EXTENSION_PROTO)?;
    if proto_files.is_empty() {
        return Ok(Vec::new());
    }
    let mut proto_parser = ProtobufParser::new();
    proto_parser.include(path);
    proto_parser.include(vsidl_syntax_path);
    proto_parser.includes(includes);
    proto_parser.inputs(proto_files);

    let metadata = ProtoBuildTargetMetadata {
        folder: path.canonicalize().unwrap(),
        target_name: None,
        crate_name: None,
    };

    Ok(proto_parser
        .file_descriptor_set()?
        .file
        .into_iter()
        .map(|descriptor| (metadata.clone(), descriptor))
        .collect::<Vec<_>>())
}

/// Determines the default path to the directory containing VSIDL syntax definition proto files.
///
/// This function uses the `ANDROID_BUILD_TOP` environment variable to locate the standard
/// path within the Android source tree. As a fallback for testing environments (like sandboxes
/// where `ANDROID_BUILD_TOP` might not be set), it assumes the syntax files are present
/// in the current working directory (`.`).
///
/// # Returns
///
/// The [PathBuf] to the VSIDL syntax directory.
pub fn get_vsidl_syntax_default_path() -> anyhow::Result<PathBuf> {
    // Hack to enable sdv_core_vsidl_unit_vsidlc_golden_test and future tests running in sandboxes.
    // When a user run vsidlc, the assumption for now is that ANDROID_BUILD_TOP should always be set. This allows the correct
    // retrieval of VSIDL syntax protos. However tests run into a sandbox, therefore, for now, we copy the syntax proto definition manually into the sanbox.
    // This function assumes that if ANDROID_BUILD_TOP is not set, a test is running and the syntax definition is
    // available in the current folder.
    if let Some(android_build_top) = env::var_os("ANDROID_BUILD_TOP") {
        // User is running vsidlc
        Ok(PathBuf::from(android_build_top)
            .join("system/software_defined_vehicle/vsidl/language/src/protos/"))
    } else {
        // a test is running vsidlc
        Ok(PathBuf::from("."))
    }
}

/// Reads an `Android.bp` file and parses relevant build target information.
///
/// Extracts `rust_protobuf` target details (if `extract_protobuf_targets` is true)
/// and `filegroup` target details.
///
/// # Parameters
///
/// * `f` - The path to the `Android.bp` file.
/// * `extract_protobuf_targets` - If true, attempts to parse `rust_protobuf` targets.
///
/// # Returns
///
/// An [AndroidBpFile] struct containing the file path, content, and extracted target information.
///
/// # Panics
///
/// * If the file `f` cannot be read to a string.
/// * If parsing the `Android.bp` content fails (errors from `extract_...` functions are unwrapped).
fn android_bp_file_from_file<F: FileProvider>(
    file_provider: &F,
    f: &Path,
    extract_protobuf_targets: bool,
) -> AndroidBpFile {
    let android_content = file_provider.read_to_string(f).unwrap();
    let rust_protobuf_info = match extract_protobuf_targets {
        true => extract_android_bp_info(&android_content).unwrap(),
        false => Vec::new(),
    };
    let rust_filegroup_info = extract_android_bp_filegroup(&android_content).unwrap();
    AndroidBpFile {
        file_path: f.to_path_buf(),
        content: android_content,
        rust_protobuf_targets: rust_protobuf_info,
        filegroup_targets: rust_filegroup_info,
    }
}

/// Finds and parses all `Android.bp` files within a given directory and its subdirectories.
///
/// # Parameters
///
/// * `dir` - The root directory to search within.
///
/// # Returns
///
/// A vector of [AndroidBpFile] structs, one for each `Android.bp` file found.
/// Protobuf targets are extracted for each file found.
fn get_android_bp_files<F: FileProvider>(file_provider: &F, dir: &Path) -> Vec<AndroidBpFile> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|f| {
            (f.file_type().is_file() || f.file_type().is_symlink())
                && f.file_name().to_string_lossy() == "Android.bp"
        })
        .map(|e| e.into_path())
        .map(|f| {
            android_bp_file_from_file(file_provider, &f, /* extract_protobuf_targets */ true)
        })
        .collect()
}

/// Searches for `Android.bp` files starting from a given path and traversing up through its parent directories.
///
/// # Parameters
///
/// * `path` - The starting path (file or directory).
///
/// # Returns
///
/// A vector of [PathBuf] objects, each representing the path to an `Android.bp` file found
/// in the starting directory or any of its ancestors. The order is from the starting path upwards.
fn find_ancestor_android_bps(path: &Path) -> Vec<PathBuf> {
    let mut current_path = path;
    let mut android_bp_paths = Vec::new();
    loop {
        if let Some(android_bp_path) = find_android_bp(current_path) {
            android_bp_paths.push(android_bp_path);
        }
        if let Some(parent) = current_path.parent() {
            current_path = parent;
        } else {
            break; // Reached the root
        }
    }
    android_bp_paths
}

/// Adds `Android.bp` files found in parent directories of the catalog and dependencies to the [Catalog].
///
/// This is used to find `filegroup` targets defined in ancestor `Android.bp` files, which
/// might be needed for resolving proto imports.
/// It avoids adding duplicate entries if an `Android.bp` file was already found directly within
/// a catalog or dependency directory.
///
/// # Parameters
///
/// * `catalog` - A mutable reference to the [Catalog] being built.
/// * `catalog_path` - The path to the main catalog directory.
/// * `dependencies` - A slice of paths to the dependency directories.
///
/// # Panics
///
/// * If path canonicalization fails for the catalog path or any dependency path.
/// * If [android_bp_file_from_file] panics (e.g., due to file read errors).
fn add_filegroups_from_parent_android_bp<F: FileProvider>(
    file_provider: &F,
    catalog: &mut Catalog,
    catalog_path: &Path,
    dependencies: &[&Path],
) {
    let mut existing_android_bp_files: HashSet<PathBuf> = catalog
        .android_bp_files
        .iter()
        .map(|a| a.file_path.canonicalize().unwrap().clone())
        .collect();

    for path in [&[catalog_path], dependencies].concat() {
        for path_android_bp in find_ancestor_android_bps(&path.canonicalize().unwrap()) {
            if existing_android_bp_files.contains(&path_android_bp)
                || existing_android_bp_files
                    .contains(&path_android_bp.canonicalize().unwrap_or(path_android_bp.clone()))
            {
                continue;
            }

            existing_android_bp_files.insert(path_android_bp.clone());
            catalog.android_bp_files.push(android_bp_file_from_file(
                file_provider,
                &path_android_bp,
                /* extract_protobuf_targets */ false,
            ));
        }
    }
}

/// Reads all VSIDL, Protobuf, and Android.bp files from a catalog directory and its dependencies.
///
/// This is the main entry point for parsing a VSIDL project structure from the filesystem.
/// It orchestrates the reading and parsing of different file types and populates a [Catalog] struct.
/// Handles both single `rust_protobuf` target mode and MPL (Multiple Protobuf Libraries) mode.
///
/// # Parameters
///
/// * `path` - The path to the root directory of the main VSIDL catalog.
/// * `dependencies` - A slice of paths to directories containing dependency VSIDL/proto files.
/// * `vsidl_syntax_path` - The path to the directory containing VSIDL syntax definition protos.
/// * `mpl` - If true, enables MPL mode, allowing multiple `rust_protobuf` targets across
///   the catalog and dependencies, and searches for ancestor `Android.bp` files.
///   If false, expects a single `rust_protobuf` target in the main catalog's `Android.bp`.
///
/// # Returns
///
/// A `Result` containing the populated [Catalog] struct on success.
///
/// # Errors
///
/// * If `path` is not a directory.
/// * If any file reading or parsing operation fails (e.g., [read_protobuf_files], [read_vsidl_files]).
/// * If MPL mode is false (`mpl = false`) and the catalog uses proto files or unit types, but no `Android.bp`
///   with a valid `rust_protobuf` target is found in the root of the `path` directory.
/// * If MPL mode is false (`mpl = false`) and [find_and_extract_proto_build_target_metadata] fails.
/// * If MPL mode is true (`mpl = true`) and path canonicalization fails during ancestor `Android.bp` search.
pub fn read_catalog(
    path: &Path,
    dependencies: &[&Path],
    vsidl_syntax_path: &Path,
) -> anyhow::Result<Catalog> {
    read_catalog_internal(
        &crate::file_provider::DiskFileProvider,
        path,
        dependencies,
        vsidl_syntax_path,
        false,
    )
}

pub fn read_catalog_using_pest(
    path: &Path,
    dependencies: &[&Path],
    vsidl_syntax_path: &Path,
) -> anyhow::Result<Catalog> {
    read_catalog_internal(
        &crate::file_provider::DiskFileProvider,
        path,
        dependencies,
        vsidl_syntax_path,
        true,
    )
}

pub fn read_catalog_internal<F: FileProvider>(
    file_provider: &F,
    path: &Path,
    dependencies: &[&Path],
    vsidl_syntax_path: &Path,
    use_pest_parser: bool,
) -> anyhow::Result<Catalog> {
    if !file_provider.is_dir(path) {
        bail!("`{path:?}` is not a folder.");
    }
    let mut read_catalogs = std::collections::HashSet::new();
    let mut catalog = Catalog::new();
    catalog.catalog_path = file_provider.canonicalize(path).ok();

    // Parse .proto files
    catalog.proto_descriptors.extend(read_protobuf_files(
        file_provider,
        path,
        dependencies,
        vsidl_syntax_path,
    )?);

    // Gather Android.bp files from catalog
    catalog.android_bp_files.extend(get_android_bp_files(file_provider, path));

    read_catalogs.insert(path);

    // Parse .proto and .vsidl dependencies
    for dir in dependencies {
        if read_catalogs.contains(dir) {
            eprintln!("Catalog {dir:?} is listed more than once as a dependency.");
            continue;
        }
        read_catalogs.insert(dir);
        catalog.dependencies_path.push(dir.to_path_buf());
        catalog.proto_descriptors.extend(read_protobuf_files(
            file_provider,
            dir,
            dependencies,
            vsidl_syntax_path,
        )?);

        // Gather Android.bp files from dependencies
        catalog.android_bp_files.extend(get_android_bp_files(file_provider, dir));

        if use_pest_parser {
            catalog.dependencies_vsidl_files.extend(
                read_vsidl_files_using_pest(file_provider, dir)?
                    .into_iter()
                    .map(|(_, entry)| entry),
            );
        } else {
            catalog
                .dependencies_vsidl_files
                .extend(read_vsidl_files(file_provider, dir)?.into_iter().map(|(_, entry)| entry));
        }
    }

    add_filegroups_from_parent_android_bp(file_provider, &mut catalog, path, dependencies);

    // update proto descriptor metadata
    find_and_extract_proto_build_target_metadata_mpl(&mut catalog);

    // Parse .vsidl files
    if use_pest_parser {
        catalog.catalog_vsidl_files = read_vsidl_files_using_pest(file_provider, path)?;
    } else {
        catalog.catalog_vsidl_files = read_vsidl_files(file_provider, path)?;
    }
    Ok(catalog)
}

/// Finds a file named "Android.bp" directly within the specified directory (non-recursive).
///
/// # Parameters
///
/// * `directory` - The path to the directory to search in.
///
/// # Returns
///
/// An `Option<PathBuf>` containing the path to the "Android.bp" file if found,
/// otherwise `None`. Returns `None` if `directory` is not a valid directory.
fn find_android_bp(directory: &Path) -> Option<PathBuf> {
    if !directory.is_dir() {
        return None;
    }
    for entry in WalkDir::new(directory).max_depth(1).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.file_name() == Some(std::ffi::OsStr::new("Android.bp")) {
            return Some(path.to_owned());
        }
    }
    None
}

/// Updates the [ProtoBuildTargetMetadata] for each proto descriptor in the catalog using MPL mode logic.
///
/// In MPL mode, proto descriptors might initially have generic metadata. This function
/// iterates through all collected `Android.bp` files, builds a map from absolute proto file paths
/// to their specific build target metadata, and then updates the metadata for each
/// descriptor in the `catalog.proto_descriptors` list based on this map.
///
/// # Parameters
///
/// * `catalog` - A mutable reference to the [Catalog] being processed.
///
/// # Note
///
/// Errors related to missing metadata for a proto file (i.e., a proto file isn't covered by any
/// `rust_protobuf` target in the collected `Android.bp` files) are not handled here but are expected
/// to be caught by later processing stages, such as context condition checks ([ProtoMetadata CoCo]).
fn find_and_extract_proto_build_target_metadata_mpl(catalog: &mut Catalog) {
    let proto_to_metadata = proto_file_to_metadata(&catalog.android_bp_files);

    for proto_descriptor in &mut catalog.proto_descriptors {
        let absolute_file_name =
            proto_descriptor.0.folder.clone().join(proto_descriptor.1.name.clone().unwrap());

        // Canonicalize the path to resolve any symlinks (like those in Shadow Workspace)
        // so it matches the keys in proto_to_metadata.
        let lookup_key = absolute_file_name.canonicalize().unwrap_or(absolute_file_name);

        if let Some(metadata) = proto_to_metadata.get(&lookup_key) {
            proto_descriptor.0.target_name = metadata.target_name.clone();
            proto_descriptor.0.crate_name = metadata.crate_name.clone();
        }

        // Error for this scenario are handled by ProtoMetadata CoCo.
    }
}

#[cfg(test)]
mod test {
    use crate::catalog::find_android_bp;
    use crate::catalog::list_files_with_extension;
    use crate::catalog::read_catalog;
    use crate::catalog::read_vsidl_files;
    use crate::file_provider::DiskFileProvider;
    use crate::get_vsidl_syntax_default_path;
    use googletest::assert_that;
    use googletest::matchers::unordered_elements_are;
    use googletest::prelude::eq;
    use std::env;
    use std::fs::File;
    use std::path::Path;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_read_valid_catalog_succeeds() {
        let catalog =
            read_catalog(Path::new("src/tests_vsidl_v1/full_catalog/"), &[], Path::new("./"));
        assert!(catalog.is_ok());
        let catalog = catalog.unwrap();
        let expected_proto_files = ["proto/messages.proto", "proto/rpc.proto"];
        assert!(expected_proto_files.iter().all(|f| catalog
            .proto_descriptors
            .iter()
            .any(|(_, fd)| fd.name.as_ref().unwrap() == f)));
        assert_that!(
            catalog
                .catalog_vsidl_files
                .iter()
                .map(|item| item.0.display().to_string())
                .collect::<Vec<_>>(),
            unordered_elements_are![
                eq("vsidl/test_service.vsidl"),
                eq("vsidl/test_someip_mapping.vsidl")
            ]
        );
        assert!(catalog.dependencies_vsidl_files.is_empty());
    }

    #[test]
    fn test_read_valid_catalog_with_dependency_succeeds() {
        let catalog = read_catalog(
            Path::new("src/tests_vsidl_v1/full_catalog/"),
            &[Path::new("src/tests_vsidl_v1/dependent_catalog/")],
            Path::new("./"),
        );
        assert!(catalog.is_ok());
        let catalog = catalog.unwrap();
        let expected_proto_files = ["dep.proto"];
        assert!(expected_proto_files.iter().all(|f| catalog
            .proto_descriptors
            .iter()
            .any(|(_, fd)| fd.name.as_ref().unwrap() == f)));
        assert!(catalog.dependencies_vsidl_files.len() == 2);
    }

    #[test]
    fn test_read_valid_catalog_with_dependency_duplicated_succeeds() {
        let catalog = read_catalog(
            Path::new("src/tests_vsidl_v1/full_catalog/"),
            &[
                Path::new("src/tests_vsidl_v1/dependent_catalog/"),
                Path::new("src/tests_vsidl_v1/dependent_catalog/"),
            ],
            Path::new("./"),
        );
        assert!(catalog.is_ok());
        let catalog = catalog.unwrap();
        assert!(catalog.dependencies_vsidl_files.len() == 2);
    }

    #[test]
    fn test_parse_catalog_without_android_bp_succeeds() {
        // Given
        let catalog_path = Path::new("src/tests_vsidl_v1/no_android_bp/");
        let vsidl_syntax_path = Path::new("./");

        // When (read in files == syntax is correct)
        let res = read_catalog(
            catalog_path,
            &[], // dependencies
            vsidl_syntax_path,
        );

        // Then
        assert!(res.is_ok());
    }

    #[test]
    fn test_parse_catalog_only_using_types_from_dependencies_succeeds() {
        // Given
        let catalog_path = Path::new("src/tests_vsidl_v1/type_usage_from_dependency/");
        let vsidl_syntax_path = Path::new("./");

        // When
        let res = read_catalog(
            catalog_path,
            &[Path::new("src/tests_vsidl_v1/dependent_catalog/")], // dependencies
            vsidl_syntax_path,
        );

        // Then
        assert!(res.is_ok());
    }

    #[test]
    fn test_parse_catalog_with_mpl_add_parent_android_bp() {
        // Given
        let catalog_path = Path::new("src/tests_vsidl_v1/no_android_bp/");
        let vsidl_syntax_path = Path::new("./");

        // When
        let res = read_catalog(
            catalog_path,
            &[], // dependencies
            vsidl_syntax_path,
        );

        // Then
        assert!(res.is_ok());
        assert!(!res.unwrap().android_bp_files.is_empty());
    }

    #[test]
    fn test_list_files_with_extension_empty_extension() {
        let temp_dir = tempdir().unwrap();
        let file_provider = DiskFileProvider {};
        let result = list_files_with_extension(&file_provider, temp_dir.path(), "");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Extension must not be empty");
    }

    #[test]
    fn test_list_files_with_extension_success() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path).unwrap();
        let file_path = temp_dir.path().join("test_without_extension");
        File::create(&file_path).unwrap();
        let file_provider = DiskFileProvider {};
        let result = list_files_with_extension(&file_provider, temp_dir.path(), "txt");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_read_vsidl_files_parse_error() {
        let temp_dir = tempdir().unwrap();
        let malformed_vsidl_path = temp_dir.path().join("malformed.vsidl");
        std::fs::write(&malformed_vsidl_path, "this is not valid vsidl textproto").unwrap();

        let file_provider = DiskFileProvider {};
        let result = read_vsidl_files(&file_provider, temp_dir.path());
        assert!(result.is_err());
        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains(malformed_vsidl_path.to_str().unwrap()));
    }

    #[test]
    fn test_get_vsidl_syntax_default_path() {
        // GIVEN the ANDROID_BUILD_TOP environment variable is set
        let temp_dir = tempdir().unwrap();
        env::set_var("ANDROID_BUILD_TOP", temp_dir.path().to_str().unwrap());

        // WHEN getting the VSIDL syntax default path
        let result = get_vsidl_syntax_default_path();

        // THEN the result should be the expected path based on ANDROID_BUILD_TOP
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            temp_dir.path().join("system/software_defined_vehicle/vsidl/language/src/protos/")
        );

        // GIVEN the ANDROID_BUILD_TOP environment variable is unset
        env::remove_var("ANDROID_BUILD_TOP");

        // WHEN getting the VSIDL syntax default path again
        let result = get_vsidl_syntax_default_path();

        // THEN the result should be the fallback path "."
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("."));
    }

    #[test]
    fn test_read_catalog_path_is_not_dir() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let result = read_catalog(temp_file.path(), &[], Path::new("./"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("is not a folder"));
    }

    #[test]
    fn test_find_android_bp_path_is_not_dir() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        assert!(find_android_bp(temp_file.path()).is_none());
    }
}
