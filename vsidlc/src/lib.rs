// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! VSIDL compiler libraries
//! Define function to reads and parses a VSIDL catalog and its dependencies.
//! Outputs middleware code for a target catalog.

/// module responsible to define command line argument of vsidlc
mod cli;
pub use cli::Cli;
/// module responsible for LSP server
mod lsp;
/// module responsible to generated rust code
mod rust;
/// module responsible for generator utilities
pub mod util;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use vsidl_v1_parser::SymbolTable;
use vsidl_v1_parser::{get_explanation_doc, AndroidBpFile};
use vsidlc_cli::VsidlcCommonCli;

/// Generates the middleware code from the symbol table and writes it to the
/// output path.
///
/// # Arguments
///
/// * `symbol_table` - The [SymbolTable] containing the symbols for which
///   middleware code needs to be generated.
/// * `vsidlc_common_cli` - The [VsidlcCommonCli] struct containing common CLI
///   arguments such as output path and purge option.
/// * `rust_formatter` - The [Format] enum specifying the tool to be used for
///   formatting the generated Rust code.
///
/// # Returns
///
/// *   `Ok(())` - If the middleware code is successfully generated and written
///     to the output path.
/// *   `Err(anyhow::Error)` - If an error occurs during directory removal or
///     middleware generation.
fn generate(
    symbol_table: SymbolTable,
    vsidlc_common_cli: &VsidlcCommonCli,
    cli: Cli,
    filegroups: AndroidBpFileGroups,
) -> anyhow::Result<()> {
    let mut generated_sources_path = vsidlc_common_cli.output_path.clone();
    generated_sources_path.push(crate::rust::GENERATED_RS_SOURCES_DIR);
    if generated_sources_path.exists() && vsidlc_common_cli.purge && generated_sources_path.is_dir()
    {
        // Remove the target directory and its contents
        fs::remove_dir_all(generated_sources_path.clone())?;
    }

    crate::rust::generate_middleware(symbol_table, &cli, filegroups)
}

/// Explains a given error code by printing its documentation.
///
/// If the `less` command is available, the documentation is piped to it for better readability.
///
/// # Arguments
///
/// * `code` - The error code to explain, e.g., "E102".
///
/// # Returns
///
/// * `Ok(())` - If the explanation is successfully printed.
/// * `Err(anyhow::Error)` - If the error code is unknown.
fn explain(code: &str) -> anyhow::Result<()> {
    let doc = match get_explanation_doc(code) {
        Some(d) => d,
        None => {
            // If get_explanation_doc returns None, we bail out of the explain function immediately.
            anyhow::bail!(
                "Unknown error code: {code}. Please provide a valid error code, e.g. E102."
            );
        }
    };

    if is_less_available() {
        let mut child =
            Command::new("less").stdin(Stdio::piped()).spawn().expect("Failed to spawn less");
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin.write_all(doc.as_bytes()).expect("Failed to write to stdin");
        child.wait().expect("less exited with error");
    } else {
        println!("{doc}");
    }
    Ok(())
}

/// Checks if the `less` command is available on the system.
///
/// # Returns
///
/// * `true` - If the `less` command is available.
/// * `false` - Otherwise.
fn is_less_available() -> bool {
    Command::new("which")
        .arg("less")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[derive(Clone, Debug)]
struct AndroidBpFileGroups {
    catalog: Vec<String>,
    dependencies: Vec<String>,
    // This group contains filegroups from Android.bp files located in parent directories
    // of the catalog path. It's currently populated but not used in code generation.
    #[allow(dead_code)]
    upper_folder: Vec<String>,
}

/// Splits Android.bp filegroup names into catalog and dependency groups.
///
/// This function iterates through a list of [AndroidBpFile] objects and
/// categorizes the filegroup targets defined within them. It determines whether
/// a filegroup belongs to the main catalog or a dependency by comparing its
/// file path against the provided `catalog_path`.
///
/// # Arguments
///
/// *   `catalog_path` - The path to the root directory of the main VSIDL
///     catalog.
/// *   `android_bp` - A slice of [AndroidBpFile] objects to be processed.
///
/// # Returns
///
/// *   An [AndroidBpFileGroups] struct containing three vectors of strings:
///     `catalog` for filegroups found within the catalog path, `dependencies`
///     for filegroups found outside of it, and `upper_folder` for filegroups
///     in parent directories of the catalog path.
fn get_catalog_android_bp(
    catalog_path: &Path,
    android_bp: &[AndroidBpFile],
) -> AndroidBpFileGroups {
    let mut catalog = Vec::new();
    let mut dependencies = Vec::new();
    let mut upper_folder = Vec::new();

    let canonical_catalog_path = catalog_path.canonicalize().unwrap();

    for bp in android_bp {
        let filegroup_names = bp.filegroup_targets.iter().map(|fg| fg.name.clone());
        let Ok(canonical_bp_path) = bp.file_path.canonicalize() else {
            eprintln!("unable to canonicalize file path: {}. Skipping.", bp.file_path.display());
            continue;
        };
        let Some(bp_dir) = canonical_bp_path.parent() else {
            eprintln!(
                "unable to get parent of file path: {}. Skipping.",
                canonical_bp_path.display()
            );
            continue;
        };

        if canonical_catalog_path.starts_with(bp_dir) && *bp_dir != canonical_catalog_path {
            upper_folder.extend(filegroup_names);
        } else if canonical_bp_path.starts_with(&canonical_catalog_path) {
            catalog.extend(filegroup_names);
        } else {
            dependencies.extend(filegroup_names);
        }
    }

    AndroidBpFileGroups { catalog, dependencies, upper_folder }
}

/// Executes the VSIDLC (Vehicle Service Interface Description Language
/// Compiler) with the provided command-line arguments.
///
/// This function performs the following steps creates a [VsidlcCommonCli]
/// struct from the parsed [Cli] arguments. This is required to create a
/// symbol table from the input catalog. When the [SymbolTable] is created
/// this is used to generate rust code.
///
/// # Arguments
///
/// *   `cli` - The `Cli` struct containing the command-line arguments.
///
/// # Returns
///
/// *   `Ok(())` - If the VSIDLC execution completes successfully.
/// *   `Err(anyhow::Error)` - If an error occurs during any of the steps, such
///     as catalog parsing, symbol table creation, or middleware generation.
pub fn execute(cli: Cli) -> anyhow::Result<()> {
    if let Some(code) = &cli.explain {
        return explain(code);
    }

    if cli.lsp {
        return lsp::lsp_server_start(cli.clone());
    }

    let catalog_path: PathBuf = cli.catalog_path.clone().unwrap_or_default();
    eprintln!("Parsing catalog from folder '{}'.", &catalog_path.display());

    let vsidlc_common_cli = VsidlcCommonCli::from(cli.clone());
    let catalog = if cli.no_pest {
        vsidlc_common_cli.create_catalog(None)?
    } else {
        vsidlc_common_cli.create_catalog_using_pest(None)?
    };

    let symbol_table = vsidlc_common_cli.create_symbol_table_from_catalog(&catalog)?;

    generate(
        symbol_table,
        &vsidlc_common_cli,
        cli,
        get_catalog_android_bp(&catalog_path, &catalog.android_bp_files),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_known_error_code() {
        // GIVEN a valid error code
        let error_code = "E102";

        // WHEN explaining it
        let result = explain(error_code);

        // THEN explanation succeeds
        assert!(result.is_ok());
    }

    #[test]
    fn test_explain_unknown_error_code() {
        // GIVEN an invalid error code
        let error_code = "UNKNOWN_ERROR_CODE";

        // WHEN explaining it
        let result = explain(error_code);

        // THEN explanation fails
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Unknown error code: UNKNOWN_ERROR_CODE. Please provide a valid error code, e.g. E102."
        );
    }
}
