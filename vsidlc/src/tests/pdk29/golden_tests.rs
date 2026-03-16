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

use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

use vsidlc_mod::{execute, Cli};

const GOLDEN_TEST_RUST_OUTPUT_DIR: &str = "golden_test_rust_output";
const GOLDEN_TEST_GENRULE_OUTPUT_DIR: &str = "golden_test_genrule_output";

/// Runs a golden test for the VSIDL compiler.
///
/// This function takes paths to a catalog, an optional dependency catalog, and
/// a directory containing golden files. It then executes the VSIDL compiler,
/// generating code for the given catalog and comparing the output with the
/// golden files to ensure correctness.
///
/// # Arguments
///
/// * `catalog` - The path to the VSIDL catalog.
/// * `dependency_catalog` - The optional path to a dependency catalog, or an
///   empty string if no dependencies exist.
/// * `golden_dir` - The path to the directory containing the golden files for
///   comparison.
/// * `output_dir` - The path to the directory where the generated code will be
///   written.
/// * `extra_args` - A slice of extra arguments to pass to the vsidlc command.
///
/// # Returns
///
/// * `Ok(())` - If the golden test passes, meaning the generated code matches
///   the golden files.
/// * `Err(anyhow::Error)` - If the test fails due to code generation errors or
///   mismatches with the golden files.
fn run_golden_test(
    catalog: &str,
    dependency_catalog: &str,
    golden_dir: &str,
    output_dir: &str,
    extra_args: &[&str],
) -> Result<()> {
    let catalog_path = Path::new(".").join(catalog);
    let golden_dir = Path::new(".").join(golden_dir);
    let output_dir = Path::new(".").join(output_dir);

    // Run vsidlc to generate code
    let mut cmd = vec![
        "fakeCommand",
        "--catalog-path",
        catalog_path.to_str().unwrap(),
        "--output-path",
        output_dir.to_str().unwrap(),
        "--purge",
        "--rust-formatter",
        "pretty-please",
        "--android-bp-formatter",
        "none",
        "--textproto-formatter",
        "none",
        "--target-api",
        "pdk29",
    ];

    if !dependency_catalog.is_empty() {
        cmd.push("--dependency-catalog-path");
        cmd.push(dependency_catalog);
    }

    cmd.extend_from_slice(extra_args);

    // THEN code is generated without errors
    let result = execute(Cli::parse_from(cmd));
    if result.is_err() {
        panic!("Error: {:?}", result.err().unwrap());
    }
    rename_generated_files(&output_dir.to_path_buf())?;

    // Compare generated files with golden files
    compare_directories(&golden_dir, &output_dir.join("generated_rs"))?;

    Ok(())
}

/// Renames generated files by appending extensions based on their original
/// type.
///
/// This function walks through a directory and renames files with specific
/// extensions. Files ending in ".bp" are renamed to have a "_bp" extension,
/// and files ending in ".rs" are renamed to have a "_rs" extension. This is
/// required to simplify the comparison, since golden tests file uses this
/// '._bp' schema to not be detected as android build files.
///
/// # Arguments
///
/// * `generated_dir` - A [PathBuf] representing the path to the directory
///   containing the generated files.
///
/// # Returns
///
/// * `Ok(())` - If the renaming process completes successfully.
/// * `Err(anyhow::Error)` - If an error occurs during file renaming or directory
///   traversal.
fn rename_generated_files(generated_dir: &PathBuf) -> anyhow::Result<()> {
    for entry in WalkDir::new(generated_dir) {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "bp" {
                    // golden targets shall not be captured by the build system
                    rename_file(path, "_bp")?;
                } else if ext == "rs" {
                    // golden files do not use rustfmt therefore, using `._rs` instead
                    // of simply .rs avoid those to be automatically formatted
                    // by git hook.
                    rename_file(path, "_rs")?;
                }
            }
        }
    }
    Ok(())
}

fn rename_file(path: &Path, new_extension: &str) -> anyhow::Result<()> {
    let new_path = path.with_extension(new_extension);
    fs::rename(path, new_path)?;
    Ok(())
}

fn compare_directories(golden_dir: &Path, generated_dir: &Path) -> Result<()> {
    let golden_files = list_files(golden_dir)?;
    let generated_files = list_files(generated_dir)?;

    assert_eq!(
        golden_files, generated_files,
        "File list mismatch between golden and generated directories"
    );

    for rel_path in &golden_files {
        let golden_path = golden_dir.join(rel_path);
        let generated_path = generated_dir.join(rel_path);
        compare_files(&golden_path, &generated_path)?;
    }

    Ok(())
}

fn compare_files(golden_path: &Path, generated_path: &Path) -> Result<()> {
    let output = Command::new("diff")
        // The following two lines allow to skip the Copyright line
        // in the generated files, this avoid having a failing test from one year to another.
        .arg("-I")
        .arg(r"^// Copyright (C) [[:digit:]]\{4\} The Android Open Source Project$")
        .arg("-y")
        .arg("-N")
        .arg("--suppress-common-lines")
        .arg("--color=always")
        .arg(golden_path)
        .arg(generated_path)
        .output()?;

    if !output.status.success() {
        println!("changes detected between golden and generated files");
        println!("golden file path: {golden_path:?}");
        println!("generated file path: {generated_path:?}\n");
        println!("{}", String::from_utf8_lossy(&output.stdout));
        let generated_content = fs::read_to_string(generated_path)?;
        println!("Content of {generated_path:?}:\n{generated_content}");

        let golden_content = fs::read_to_string(golden_path)?;
        println!("Content of {golden_path:?}:\n{golden_content}");

        anyhow::bail!("Files differ");
    }

    // Since we ignore copyright header in the diff command, this is required
    // to make sure generated files contains the correct header only once.
    starts_with_copyright_header(golden_path)?;
    starts_with_copyright_header(generated_path)?;

    Ok(())
}

fn starts_with_copyright_header(filepath: &Path) -> anyhow::Result<()> {
    let file =
        File::open(filepath).with_context(|| format!("Failed to open file: {:?}", filepath))?;

    // Use BufReader to avoid reading the entire file into memory
    let reader = BufReader::new(file);

    // The new header usually starts with an empty "//" line, then the text.
    // We scan the first 5 lines to find the Copyright declaration.
    // Regex handles the specific text and variable year (\d{4}).
    let copyright_regex =
        Regex::new(r"^//\s+Copyright \(C\) \d{4} The Android Open Source Project")?;

    // Check the top 5 lines of the file
    for line_result in reader.lines().take(5) {
        let line = line_result?;
        if copyright_regex.is_match(&line) {
            return Ok(());
        }
    }

    anyhow::bail!(
        "AOSP Copyright header is missing or incorrect in the first 5 lines of file: {:?}",
        filepath
    );
}

fn list_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().strip_prefix(dir).map(|p| p.to_path_buf()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort();
    Ok(entries)
}

#[cfg(test)]
mod test {
    use super::*;

    const CATALOG_LIST: &[(&str, &str, &str)] = &[
        ("./src/tests/catalogs/catalog", "", "./src/tests/pdk29/golden_catalog_rs"),
        (
            "./src/tests/catalogs/catalog_single_filegroup",
            "",
            "./src/tests/pdk29/golden_catalog_single_filegroup_rs",
        ),
        (
            "./src/tests/catalogs/catalog_single_filegroup_upperfolder",
            "",
            "./src/tests/pdk29/golden_catalog_single_filegroup_upperfolder_rs",
        ),
        (
            "./src/tests/catalogs/catalog_two_folders",
            "",
            "./src/tests/pdk29/golden_catalog_two_folders_rs",
        ),
        (
            "./src/tests/catalogs/catalog_two_folders_one_bp",
            "",
            "./src/tests/pdk29/golden_catalog_two_folders_one_bp_rs",
        ),
        (
            "./src/tests/catalogs/catalog_without_androidbp",
            "./src/tests/catalogs/catalog",
            "./src/tests/pdk29/golden_catalog_without_androidbp_rs",
        ),
        (
            "./src/tests/catalogs/catalog_with_subfolder",
            "",
            "./src/tests/pdk29/golden_catalog_with_subfolder_rs",
        ),
    ];

    #[test]
    fn test_run_golden_tests() {
        // GIVEN all golden tests
        for (catalog, dep, golden_dir) in CATALOG_LIST {
            let result =
                run_golden_test(catalog, dep, golden_dir, GOLDEN_TEST_RUST_OUTPUT_DIR, &[]);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_run_golden_tests_genrule() {
        // GIVEN all golden tests for -genrule flag
        let catalog_list =
            [("./src/tests/catalogs/catalog", "", "./src/tests/pdk29/genrule_golden/catalog")];

        for (catalog, dep, golden_dir) in catalog_list {
            let result = run_golden_test(
                catalog,
                dep,
                golden_dir,
                GOLDEN_TEST_GENRULE_OUTPUT_DIR,
                &["--genrule"],
            );
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_double_execution_only_write_files_once() {
        // GIVEN a list of vsidlc command line argument
        let args = vec![
            "fakeCommand",
            "--catalog-path",
            "./src/tests/catalogs/catalog",
            "--output-path",
            "./duplicate_write_test",
            "--rust-formatter",
            "none",
            "--target-api",
            "pdk29",
        ];

        // WHEN executing vsidlc
        let result = execute(Cli::parse_from(args.clone()));
        assert!(result.is_ok());

        // THEN files are written with current timestamp
        let generated_android_bp = Path::new(
            "./duplicate_write_test/generated_rs/service_bundles/sdv/golden/golden_tire_alert/Android.bp",
        );
        let generated_rs = Path::new(
            "./duplicate_write_test/generated_rs/service_bundles/sdv/golden/golden_tire_alert/service_bundle.rs",
        );
        let initial_timestamp_android_bp = fs::metadata(generated_android_bp)
            .expect("Failed to get metadata")
            .modified()
            .expect("Failed to get modified time");
        let initial_timestamp_rs = fs::metadata(generated_rs)
            .expect("Failed to get metadata")
            .modified()
            .expect("Failed to get modified time");

        // AND WHEN executing vsidlc AGAIN
        let result = execute(Cli::parse_from(args));

        // THEN result is valid
        assert!(result.is_ok());

        // AND THEN the modification time does not change
        let final_timestamp_android_bp = fs::metadata(generated_android_bp)
            .expect("Failed to get metadata")
            .modified()
            .expect("Failed to get modified time");
        let final_timestamp_rs = fs::metadata(generated_rs)
            .expect("Failed to get metadata")
            .modified()
            .expect("Failed to get modified time");
        assert_eq!(initial_timestamp_android_bp, final_timestamp_android_bp);
        assert_eq!(initial_timestamp_rs, final_timestamp_rs);
    }
}
