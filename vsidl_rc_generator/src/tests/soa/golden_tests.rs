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

#[cfg(test)]
mod test {
    use anyhow::Context;
    use clap::Parser;
    use protobuf::Message;
    use std::collections::BTreeMap;
    use std::fs;
    use std::fs::File;
    use std::io::{self, BufRead, BufReader};
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;
    use vsidl_rc_generator::{execute, Cli};
    use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;
    use vsidl_v1::runtime_config::SomeipRuntimeConfig;
    use vsidl_v1::runtime_config::VsidlRuntimeConfig;
    use vsidl_v1::someip::some_ip_mapping::Mapping_type;
    #[allow(unused_imports)]
    use vsidl_v1::someip::SomeIpMapping;

    /// Compares two files, ignoring lines that start with "//" (after trimming whitespace)
    /// and ignoring empty lines.
    ///
    /// Panics with a detailed diff if the filtered contents are not identical.
    fn compare_files(golden_path: &Path, generated_path: &Path) {
        // Read and filter both files
        let golden_lines = read_and_filter(golden_path)
            .unwrap_or_else(|_| panic!("Failed to read golden file: {:?}", golden_path));

        let generated_lines = read_and_filter(generated_path)
            .unwrap_or_else(|_| panic!("Failed to read generated file: {:?}", generated_path));

        // Use assert_eq! which provides a nice diff output on failure.
        // This will panic if the vecs are not equal.
        assert_eq!(
            golden_lines,
            generated_lines,
            "Generated file content does not match golden file (ignoring // comments and empty lines)."
        );
    }

    /// Helper function to read a file and return a Vec<String> of its significant lines.
    /// "Significant" means non-empty and not a comment (starting with "//").
    /// This function also normalizes the catalog and dependency paths.
    fn read_and_filter(path: &Path) -> io::Result<Vec<String>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        // Define the paths for replacement
        const LONG_PATH: &str =
            "system/software_defined_vehicle/vsidl/vsidl_rc_generator/src/tests/soa/catalog";
        const SHORT_PATH: &str = "src/tests/soa/catalog";

        let filtered_lines = reader
            .lines()
            .collect::<Result<Vec<String>, _>>()? // Collect all lines, propagating I/O errors
            .into_iter()
            .map(|line| {
                line.trim() // Trim whitespace
                    .replace(LONG_PATH, SHORT_PATH)
            })
            .filter(|line| !line.starts_with("//") && !line.is_empty()) // Filter out comments and empty lines
            .collect();

        Ok(filtered_lines)
    }

    #[test]
    fn test_run_golden_test_for_android_bp_file() {
        let dir = tempdir().expect("Failed to create temp dir");
        let output_file_path = dir.path().join("Android._bp");
        let output_file_path_string = output_file_path.display().to_string();
        let golden_file_path = Path::new("./src/tests/soa/golden/Android._bp");

        let args = vec![
            "vsidl_rc_generator",
            "--catalog-path",
            "./src/tests/soa/catalog",
            "--output-path",
            output_file_path_string.as_str(),
            "--variant",
            "runtime-config-prebuilts",
            "--filegroup",
            "vsidl_rc_gen_tests_soa_tire_catalog_protos",
            "--target-api",
            "latest",
        ];

        let result = execute(Cli::parse_from(args));
        if result.is_err() {
            panic!("Error: {:?}", result.err().unwrap());
        }

        compare_files(&output_file_path, golden_file_path);
    }

    /// Reads bytes from a file and parses them into a proto message
    fn read_proto_from_file<T: Message>(path: &Path) -> anyhow::Result<T> {
        let content = fs::read(path).with_context(|| format!("Failed to read file: {path:#?}"))?;
        T::parse_from_bytes(&content)
            .with_context(|| format!("Failed to parse proto from file: {path:#?}"))
    }

    /// Sorts repeated fields within the config to ensure deterministic comparison.
    fn normalize_vsidl_runtime_config(config: &mut VsidlRuntimeConfig) {
        // 1. Sort UnitTypeDescriptorMapping
        // We sort by package, then bundle, then type name to create a unique order.
        config.unit_type_mapping.sort_by(|a, b| {
            a.package_name
                .cmp(&b.package_name)
                .then_with(|| a.service_bundle_name.cmp(&b.service_bundle_name))
                .then_with(|| a.type_name.cmp(&b.type_name))
        });

        // 2. Sort MessageDescriptorMapping
        // Sort simply by the message name.
        config.descriptor_mapping.sort_by(|a, b| a.message_name.cmp(&b.message_name));

        // 3. Sort FileDescriptorProto
        // These are standard google protos, usually sorted by the file name.
        config.file_descriptor_proto.sort_by(|a, b| a.name().cmp(b.name()));
    }

    fn compare_vsidl_runtime_config_proto_files(golden_path: &Path, generated_path: &Path) {
        let mut golden_proto: VsidlRuntimeConfig = read_proto_from_file(golden_path)
            .unwrap_or_else(|e| {
                // Panic with a clear error message if the file can't be read.
                panic!("Failed to build read golden file {:?}: {}", golden_path, e)
            });

        let mut generated_proto: VsidlRuntimeConfig = read_proto_from_file(generated_path)
            .unwrap_or_else(|e| {
                panic!("Failed to read generated file {:?}: {}", generated_path, e)
            });

        // Normalize both protos before comparison
        normalize_vsidl_runtime_config(&mut golden_proto);
        normalize_vsidl_runtime_config(&mut generated_proto);

        assert_eq!(golden_proto, generated_proto);
    }

    /// Sorts repeated fields and maps within the config to ensure deterministic comparison.
    fn normalize_someip_runtime_config(config: &mut SomeipRuntimeConfig) {
        // Sort the main some_ip_mapping array.
        config.some_ip_mapping.sort_by(|a, b| match (&a.mapping_type, &b.mapping_type) {
            (
                Some(Mapping_type::SdvTypeMapping(a_map)),
                Some(Mapping_type::SdvTypeMapping(b_map)),
            ) => a_map
                .someip_service_id
                .cmp(&b_map.someip_service_id)
                .then_with(|| a_map.unit_type.package_name.cmp(&b_map.unit_type.package_name))
                .then_with(|| a_map.unit_type.type_name.cmp(&b_map.unit_type.type_name)),
            (
                Some(Mapping_type::SomeipServiceMapping(a_map)),
                Some(Mapping_type::SomeipServiceMapping(b_map)),
            ) => a_map.someip_service_id.cmp(&b_map.someip_service_id),
            (
                Some(Mapping_type::SdvTypeMapping(_)),
                Some(Mapping_type::SomeipServiceMapping(_)),
            ) => std::cmp::Ordering::Less,
            (
                Some(Mapping_type::SomeipServiceMapping(_)),
                Some(Mapping_type::SdvTypeMapping(_)),
            ) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (&Some(_), &Some(_)) => todo!(),
        });

        // Normalize within each mapping
        for mapping in &mut config.some_ip_mapping {
            if let Some(Mapping_type::SomeipServiceMapping(ref mut service_mapping)) =
                mapping.mapping_type
            {
                for instance in &mut service_mapping.instance {
                    // Sort the sdv_topic_parameterization map by key
                    let sorted_map: BTreeMap<u32, _> =
                        instance.sdv_topic_parameterization.clone().into_iter().collect();
                    instance.sdv_topic_parameterization = sorted_map.into_iter().collect();
                }
            }
            // SdvTypeMapping doesn't seem to have maps that need sorting based on the description.
        }
    }

    fn compare_someip_runtime_config_proto_files(golden_path: &Path, generated_path: &Path) {
        let mut golden_proto: SomeipRuntimeConfig = read_proto_from_file(golden_path)
            .unwrap_or_else(|e| {
                // Panic with a clear error message if the file can't be read.
                panic!("Failed to build read golden file {:?}: {}", golden_path, e)
            });

        let mut generated_proto: SomeipRuntimeConfig = read_proto_from_file(generated_path)
            .unwrap_or_else(|e| {
                panic!("Failed to read generated file {:?}: {}", generated_path, e)
            });

        // Normalize both protos before comparison
        normalize_someip_runtime_config(&mut golden_proto);
        normalize_someip_runtime_config(&mut generated_proto);

        assert_eq!(golden_proto, generated_proto);
    }

    fn compare_diagnostics_declaration_proto_files(golden_path: &Path, generated_path: &Path) {
        let golden_proto: DiagnosticsDeclaration = read_proto_from_file(golden_path)
            .unwrap_or_else(|e| {
                // Panic with a clear error message if the file can't be read.
                panic!("Failed to build read golden file {:?}: {}", golden_path, e)
            });

        let generated_proto: DiagnosticsDeclaration = read_proto_from_file(generated_path)
            .unwrap_or_else(|e| {
                panic!("Failed to read generated file {:?}: {}", generated_path, e)
            });

        assert_eq!(golden_proto, generated_proto);
    }

    #[test]
    fn test_run_golden_test_for_catalog() {
        let dir = tempdir().expect("Failed to create temp dir");
        let temp_dir_string = dir.path().display().to_string();
        let output_file_path_vsidl = dir.path().join("vsidl-config.binpb");
        let output_file_path_someip = dir.path().join("someip-config.binpb");
        let golden_file_path_vsidl = Path::new("./src/tests/soa/golden/vsidl-config.binpb");
        let golden_file_path_someip = Path::new("./src/tests/soa/golden/someip-config.binpb");

        let args = vec![
            "vsidl_rc_generator",
            "--catalog-path",
            "./src/tests/soa/catalog",
            "--output-path",
            temp_dir_string.as_str(),
            "--target-api",
            "latest",
        ];

        let result = execute(Cli::parse_from(args));
        if result.is_err() {
            panic!("Error: {:?}", result.err().unwrap());
        }

        compare_vsidl_runtime_config_proto_files(&output_file_path_vsidl, golden_file_path_vsidl);
        compare_someip_runtime_config_proto_files(
            golden_file_path_someip,
            &output_file_path_someip,
        );
    }

    /// Test fixture to run a golden test for a specific package.
    fn run_golden_test_for_package(package_name: &str) {
        // 1. Setup temp directory
        let dir = tempdir().expect("Failed to create temp dir");
        let temp_dir_string = dir.path().display().to_string();
        let output_file_path_vsidl = dir.path().join("vsidl-config.binpb");

        // 2. Dynamically create the golden file path using the package_name
        let golden_file_path_vsidl: PathBuf =
            ["./src/tests/soa/golden/", package_name, "vsidl-config.binpb"].iter().collect();

        // 3. Dynamically build args using the package_name
        let args = vec![
            "vsidl_rc_generator",
            "--catalog-path",
            "./src/tests/soa/catalog",
            "--output-path",
            temp_dir_string.as_str(),
            "--package-name",
            package_name,
            "--target-api",
            "latest",
        ];

        // 4. Execute the command
        let result = execute(Cli::parse_from(args));
        if result.is_err() {
            panic!(
                "Error executing golden test for package '{}': {:?}",
                package_name,
                result.err().unwrap()
            );
        }

        // 5. Compare the output file to the golden file
        compare_vsidl_runtime_config_proto_files(&output_file_path_vsidl, &golden_file_path_vsidl);
    }

    #[test]
    fn test_run_golden_test_for_package_sdv_tests_soa_transmission() {
        run_golden_test_for_package("sdv.tests.soa.transmission");
    }

    /// Test fixture to run a golden test for a specific bundle.
    fn run_golden_test_for_bundle(package_name: &str, bundle_name: &str) {
        // 1. Setup temp directory
        let dir = tempdir().expect("Failed to create temp dir");
        let temp_dir_string = dir.path().display().to_string();
        let output_file_path_vsidl = dir.path().join("vsidl-config.binpb");
        let output_file_path_diagnostics = dir.path().join("diagnostics-config.binpb");

        // 2. Dynamically create the golden files paths using the package_name
        let golden_file_path_vsidl: PathBuf =
            ["./src/tests/soa/golden/", package_name, bundle_name, "vsidl-config.binpb"]
                .iter()
                .collect();
        let golden_file_path_diagnostics: PathBuf =
            ["./src/tests/soa/golden/", package_name, bundle_name, "diagnostics-config.binpb"]
                .iter()
                .collect();

        // 3. Dynamically build args using the package_name
        let args = vec![
            "vsidl_rc_generator",
            "--catalog-path",
            "./src/tests/soa/catalog",
            "--output-path",
            temp_dir_string.as_str(),
            "--package-name",
            package_name,
            "--service-bundle-name",
            bundle_name,
            "--target-api",
            "latest",
        ];

        // 4. Execute the command
        let result = execute(Cli::parse_from(args));
        if result.is_err() {
            panic!(
                "Error executing golden test for bundle '{}' '{}': {:?}",
                package_name,
                bundle_name,
                result.err().unwrap()
            );
        }

        // 5. Compare the output files to the golden files
        compare_vsidl_runtime_config_proto_files(&output_file_path_vsidl, &golden_file_path_vsidl);
        compare_diagnostics_declaration_proto_files(
            &output_file_path_diagnostics,
            &golden_file_path_diagnostics,
        );
    }

    #[test]
    fn test_run_golden_test_for_package_sdv_tests_soa_transmission_tire_manager() {
        run_golden_test_for_bundle("sdv.tests.soa.transmission", "TireManager");
    }

    #[test]
    fn test_run_golden_test_for_package_sdv_tests_soa_transmission_pressure_manager() {
        run_golden_test_for_bundle("sdv.tests.soa.transmission", "PressureManager");
    }

    #[test]
    fn test_run_golden_test_for_package_sdv_tests_soa_transmission_tire_monitor() {
        run_golden_test_for_bundle("sdv.tests.soa.transmission", "TireMonitor");
    }
}
