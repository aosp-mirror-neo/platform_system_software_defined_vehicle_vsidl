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

//! This crate provides functionality for validating VSIDLC
//! CLI arguments and generate catalogs and symbol tables.

use anyhow::anyhow;
use std::path::PathBuf;

use vsidl_v1_parser::{AndroidBpDanglingFiles, Catalog, FindingId, SymbolTable, SymbolTableState};

const BOLD_RED: &str = "\x1b[1;31m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// VSIDL (Vehicle Service Interface Description Language) compiler Command Line Interface
#[derive(Default)]
pub struct VsidlcCommonCli {
    /// Path to catalog directory, i.e., the directory that contains .vsidl files
    pub catalog_path: PathBuf,

    /// Path to parent directory of `generated_rs` folder in which generated files are placed.
    pub output_path: PathBuf,

    /// Remove the `output_path/generated_rs` directory before generating new files.
    pub purge: bool,

    /// Ignore context condition findings with the given IDs
    pub ignore: Vec<String>,

    /// Use the Protobuf parser instead of the Pest parser to parse `.vsidl` files
    pub no_pest: bool,

    /// Optional. Specifies the directories of the dependent protos outside of the `catalog_path`. Each directory must contain an `Android.bp` file with a `rust_protobuf` build target to generate Rust API for the dependent protos. In the case of multi-layered dependency graph, please pass in one list with all the directories in the graph.
    pub dependency_catalog_path: Vec<PathBuf>,

    /// Suppress all compiler warnings. Errors stay enabled.
    pub no_warn: bool,

    /// Optional. Enable validation of ContextCondition related to Genrule. Default false.
    pub genrule: bool,
}

impl VsidlcCommonCli {
    /// Reads and parses a VSIDL catalog and its dependencies.
    ///
    /// This function reads the VSIDL catalog located at the `catalog_path`, along with any
    /// specified dependencies in `dependency_catalog_path`. It uses the `vsidl_v1_parser` crate
    /// to parse the catalog and construct an internal representation.
    ///
    /// # Arguments
    ///
    /// * `override_vsidl_path` - Optional path to the VSIDL syntax definition files. If provided,
    ///   this path will be used instead of the default location. This is useful for testing or
    ///   when the VSIDL syntax files are not in the standard location.
    ///
    /// # Returns
    ///
    /// * `Ok(vsidl_v1_parser::Catalog)` - The parsed VSIDL catalog.
    /// * `Err(anyhow::Error)` - If an error occurs during catalog reading or parsing.
    pub fn create_catalog(&self, override_vsidl_path: Option<PathBuf>) -> anyhow::Result<Catalog> {
        let dependencies: Vec<&std::path::Path> =
            self.dependency_catalog_path.iter().map(PathBuf::as_path).collect();
        vsidl_v1_parser::read_catalog(
            self.catalog_path.as_path(),
            &dependencies,
            override_vsidl_path
                .unwrap_or_else(|| vsidl_v1_parser::get_vsidl_syntax_default_path().unwrap())
                .as_path(),
        )
    }

    /// Same as create_catalog but uses Pest parser internally
    pub fn create_catalog_using_pest(
        &self,
        override_vsidl_path: Option<PathBuf>,
    ) -> anyhow::Result<Catalog> {
        let dependencies: Vec<&std::path::Path> =
            self.dependency_catalog_path.iter().map(PathBuf::as_path).collect();
        vsidl_v1_parser::read_catalog_using_pest(
            self.catalog_path.as_path(),
            &dependencies,
            override_vsidl_path
                .unwrap_or_else(|| vsidl_v1_parser::get_vsidl_syntax_default_path().unwrap())
                .as_path(),
        )
    }

    /// Creates a [SymbolTable] table from a VSIDL [Catalog].
    ///
    /// This function takes a parsed VSIDL [Catalog] and creates a [SymbolTable] table,
    /// defined from the catalog. It utilizes the `vsidl_v1_parser` crate for symbol table creation.
    /// Additionally, it disables context condition findings based on the IDs provided in the `ignore` field
    /// of the [VsidlcCli] struct.
    ///
    /// # Arguments
    ///
    /// * `catalog` - A reference to the [Catalog] from which to create the symbol table.
    ///
    /// # Returns
    ///
    /// * `Ok(SymbolTable)` - The created symbol table.
    /// * `Err(anyhow::Error)` - If an error occurs during symbol table creation.
    pub fn create_symbol_table_from_catalog(
        &self,
        catalog: &Catalog,
    ) -> anyhow::Result<SymbolTable> {
        let mut coco_checker = vsidl_v1_parser::ContextConditionChecker::default();
        if !self.no_warn {
            coco_checker.enable_warnings();
        }
        if self.genrule {
            coco_checker.add_context_condition(
                SymbolTableState::NotCreated,
                Box::new(AndroidBpDanglingFiles::default()),
            );
        }
        for ignored_id in &self.ignore {
            let finding_id: FindingId = FindingId::try_from(ignored_id.as_ref())
                .map_err(|_| anyhow!("Invalid FindingId: {}", ignored_id))?;
            eprintln!("{BOLD_RED}WARNING{RESET}{BOLD}: Manually disabled input validation {ignored_id}. Generated code may be invalid.{RESET}");
            coco_checker.disable_finding_id(finding_id);
        }
        vsidl_v1_parser::create_symbol_table(catalog, coco_checker)
    }

    /// Reads a VSIDL catalog and creates [Catalog] and [SymbolTable] object if possible.
    ///
    /// This function provide a convenient way to both parse a VSIDL catalog
    /// and create its corresponding symbol table.
    ///
    /// # Arguments
    ///
    /// * `override_vsidl_path` - Optional path to the VSIDL syntax definition files. If provided,
    ///   this path will be used instead of the default location. This is useful for testing or
    ///   when the VSIDL syntax files are not in the standard location.
    ///
    /// # Returns
    ///
    /// * `Ok((Catalog, SymbolTable))` - A tuple containing the parsed [Catalog] and its [SymbolTable].
    /// * `Err(anyhow::Error)` - If an error occurs during catalog reading, parsing, or symbol table creation.
    pub fn create_symbol_table(
        &self,
        override_vsidl_path: Option<PathBuf>,
    ) -> anyhow::Result<SymbolTable> {
        let catalog = if self.no_pest {
            self.create_catalog(override_vsidl_path)?
        } else {
            self.create_catalog_using_pest(override_vsidl_path)?
        };
        self.create_symbol_table_from_catalog(&catalog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn setup_test_catalog() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a dummy VSIDL file
        let vsidl_path = temp_dir.path().join("test.vsidl");
        let mut vsidl_file = File::create(vsidl_path).expect("Failed to create vsidl file");
        vsidl_file
            .write_all(
                b"
# proto-file: //system/software_defined_vehicle/vsidl/language/src/protos/sdv/vsidl/v1/syntax.proto
# proto-message: VsidlEntry

package: \"sdv.test\"

sdv_service_bundle {
    name: \"test\"
}
",
            )
            .expect("Failed to write to vsidl file");

        // Create a dummy proto file (required for Android.bp)
        let proto_dir = temp_dir.path().join("protos");
        std::fs::create_dir(&proto_dir).expect("Failed to create protos dir");
        let proto_path = proto_dir.join("test.proto");
        let mut proto_file = File::create(proto_path).expect("Failed to create proto file");
        proto_file
            .write_all(b"syntax = \"proto3\";\npackage test;\n")
            .expect("Failed to write to proto file");

        // Create a dummy Android.bp file
        let android_bp_path = temp_dir.path().join("Android.bp");
        let mut android_bp_file =
            File::create(android_bp_path).expect("Failed to create Android.bp");
        android_bp_file
            .write_all(
                b"rust_protobuf {
        name: \"test_proto\",
        protos: [\"protos/test.proto\"],
        crate_name: \"test_proto\",
    }",
            )
            .expect("Failed to write to Android.bp");

        temp_dir
    }

    #[test]
    fn test_create_catalog() {
        // GIVEN a VsidlcCommonCli instance
        let temp = setup_test_catalog();
        let cli = VsidlcCommonCli {
            catalog_path: PathBuf::from(temp.path()),
            output_path: PathBuf::from("output"),
            purge: false,
            ignore: vec![],
            no_pest: false,
            dependency_catalog_path: vec![],
            no_warn: false,
            genrule: false,
        };

        // WHEN creating a catalog WITH a custom syntax path
        let result = cli.create_catalog(Some(".".into()));

        // THEN the catalog is successfully created
        println!("result {result:?}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_catalog_without_custom_syntax_path() {
        // GIVEN a VsidlcCommonCli instance
        let temp = setup_test_catalog();
        let cli = VsidlcCommonCli {
            catalog_path: PathBuf::from(temp.path()),
            output_path: PathBuf::from("output"),
            purge: false,
            ignore: vec![],
            no_pest: false,
            dependency_catalog_path: vec![],
            no_warn: false,
            genrule: false,
        };

        // WHEN creating a catalog WITHOUT passing a custom syntax path
        let result = cli.create_catalog(None);

        // THEN the catalog is successfully created
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_symbol_table() {
        // GIVEN a VsidlcCommonCli instance
        let temp = setup_test_catalog();
        let cli = VsidlcCommonCli {
            catalog_path: PathBuf::from(temp.path()),
            output_path: PathBuf::from("output"),
            purge: false,
            ignore: vec![],
            no_pest: false,
            dependency_catalog_path: vec![],
            no_warn: false,
            genrule: false,
        };

        // WHEN creating symbol table WITH custom syntax path
        let result = cli.create_symbol_table(Some(".".into()));

        // THEN symbol table is successfully created
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_symbol_table_from_catalog() {
        // GIVEN a VsidlcCommonCli instance
        let temp = setup_test_catalog();
        let cli = VsidlcCommonCli {
            catalog_path: PathBuf::from(temp.path()),
            output_path: PathBuf::from("output"),
            purge: false,
            ignore: vec![],
            no_pest: false,
            dependency_catalog_path: vec![],
            no_warn: false,
            genrule: false,
        };

        // WHEN creating a catalog WITH a custom syntax path
        let catalog = cli.create_catalog(Some(".".into())).unwrap();
        // and WHEN creating symbol table from the generated catalog
        let result = cli.create_symbol_table_from_catalog(&catalog);

        // THEN symbol table is successfully created
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_symbol_table_from_catalog_with_ignore_values() {
        // GIVEN a VsidlcCommonCli instance WITH ignore Finding
        let temp = setup_test_catalog();
        let cli = VsidlcCommonCli {
            catalog_path: PathBuf::from(temp.path()),
            output_path: PathBuf::from("output"),
            purge: false,
            ignore: vec!["E100".into()], // Ignore E100 Finding
            no_pest: false,
            dependency_catalog_path: vec![],
            no_warn: false,
            genrule: false,
        };

        // WHEN creating a catalog WITH a custom syntax path
        let catalog = cli.create_catalog(Some(".".into())).unwrap();
        // and WHEN creating symbol table from the generated catalog
        let result = cli.create_symbol_table_from_catalog(&catalog);

        // THEN symbol table is successfully created
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_symbol_table_from_catalog_with_invalid_ignore_values() {
        // GIVEN a VsidlcCommonCli instance WITH ignore Finding
        let temp = setup_test_catalog();
        let cli = VsidlcCommonCli {
            catalog_path: PathBuf::from(temp.path()),
            output_path: PathBuf::from("output"),
            purge: false,
            ignore: vec!["".into()], // ignore invalid Finding
            no_pest: false,
            dependency_catalog_path: vec![],
            no_warn: false,
            genrule: false,
        };

        // WHEN creating a catalog WITH a custom syntax path
        // THEN catalog object is created
        let catalog = cli.create_catalog(Some(".".into())).unwrap();

        // WHEN creating the symbol table.
        // THEN assert symbol table result is an Error.
        let result = cli.create_symbol_table_from_catalog(&catalog);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_values() {
        // Create a VsidlcCommonCli instance
        let cli = VsidlcCommonCli::default();

        // Assert the default values
        assert_eq!(cli.catalog_path, PathBuf::default());
        assert_eq!(cli.output_path, PathBuf::default());
        assert!(!cli.purge);
        assert_eq!(cli.ignore, Vec::<String>::default());
        assert_eq!(cli.dependency_catalog_path, Vec::<PathBuf>::default());
    }
}
