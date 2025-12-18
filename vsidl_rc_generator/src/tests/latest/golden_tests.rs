// Copyright 2025 Google LLC

#[cfg(test)]
mod test {
    use anyhow::Context;
    use clap::Parser;
    use protobuf::Message;
    use std::fs;
    use std::fs::File;
    use std::io::{self, BufRead, BufReader};
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;
    use vsidl_rc_generator::{execute, Cli};
    use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;
    use vsidl_v1::runtime_config::SomeipRuntimeConfig;
    use vsidl_v1::runtime_config::VsidlRuntimeConfig;

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
            "system/software_defined_vehicle/vsidl/vsidl_rc_generator/src/tests/catalog";
        const SHORT_PATH: &str = "src/tests/catalog";

        // Define dependency paths to normalize, effectively ignoring differences between them
        const DEP_LONG_PATH: &str =
            "system/software_defined_vehicle/automotive_services/diagnostics/vsidl/v1";
        const DEP_NORMALIZED_PATH: &str = "diagnostics_v1_catalog_files";

        let filtered_lines = reader
            .lines()
            .collect::<Result<Vec<String>, _>>()? // Collect all lines, propagating I/O errors
            .into_iter()
            .map(|line| {
                line.trim() // Trim whitespace
                    .replace(LONG_PATH, SHORT_PATH)
                    .replace(DEP_LONG_PATH, DEP_NORMALIZED_PATH) // Normalize dependency path
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
        let golden_file_path = Path::new("./src/tests/latest/Android._bp");

        let args = vec![
            "vsidl_rc_generator",
            "--catalog-path",
            "./src/tests/catalog",
            "--dependency-catalog-path",
            "./diagnostics_v1_catalog_files",
            "--output-path",
            output_file_path_string.as_str(),
            "--variant",
            "runtime-config-prebuilts",
            "--filegroup",
            "system_software_defined_vehicle_core_services_stable_test_tire_vsidl",
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

    fn compare_someip_runtime_config_proto_files(golden_path: &Path, generated_path: &Path) {
        let golden_proto: SomeipRuntimeConfig =
            read_proto_from_file(golden_path).unwrap_or_else(|e| {
                // Panic with a clear error message if the file can't be read.
                panic!("Failed to build read golden file {:?}: {}", golden_path, e)
            });

        let generated_proto: SomeipRuntimeConfig = read_proto_from_file(generated_path)
            .unwrap_or_else(|e| {
                panic!("Failed to read generated file {:?}: {}", generated_path, e)
            });

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
        let golden_file_path_vsidl = Path::new("./src/tests/latest/vsidl-config.binpb");
        let golden_file_path_someip = Path::new("./src/tests/latest/someip-config.binpb");

        let args = vec![
            "vsidl_rc_generator",
            "--catalog-path",
            "./src/tests/catalog",
            "--dependency-catalog-path",
            "./diagnostics_v1_catalog_files",
            "--output-path",
            temp_dir_string.as_str(),
        ];

        let result = execute(Cli::parse_from(args));
        if result.is_err() {
            panic!("Error: {:?}", result.err().unwrap());
        }

        compare_vsidl_runtime_config_proto_files(&output_file_path_vsidl, golden_file_path_vsidl);
        compare_someip_runtime_config_proto_files(
            &output_file_path_someip,
            golden_file_path_someip,
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
            ["./src/tests/latest", package_name, "vsidl-config.binpb"].iter().collect();

        // 3. Dynamically build args using the package_name
        let args = vec![
            "vsidl_rc_generator",
            "--catalog-path",
            "./src/tests/catalog",
            "--dependency-catalog-path",
            "./diagnostics_v1_catalog_files",
            "--output-path",
            temp_dir_string.as_str(),
            "--package-name",
            package_name,
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
    fn test_run_golden_test_for_package_diagnostics() {
        run_golden_test_for_package("diagnostics");
    }

    #[test]
    fn test_run_golden_test_for_package_transmission() {
        run_golden_test_for_package("sdv.transmission");
    }

    #[test]
    fn test_run_golden_test_for_package_transmission2() {
        run_golden_test_for_package("sdv.transmission2");
    }

    #[test]
    fn test_run_golden_test_for_package_someip_rpc() {
        run_golden_test_for_package("someip.rpc");
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
            ["./src/tests/latest", package_name, bundle_name, "vsidl-config.binpb"]
                .iter()
                .collect();
        let golden_file_path_diagnostics: PathBuf =
            ["./src/tests/latest", package_name, bundle_name, "diagnostics-config.binpb"]
                .iter()
                .collect();

        // 3. Dynamically build args using the package_name
        let args = vec![
            "vsidl_rc_generator",
            "--catalog-path",
            "./src/tests/catalog",
            "--dependency-catalog-path",
            "./diagnostics_v1_catalog_files",
            "--output-path",
            temp_dir_string.as_str(),
            "--package-name",
            package_name,
            "--service-bundle-name",
            bundle_name,
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
    fn test_run_golden_test_for_package_diagnostics_diagnostics_service() {
        run_golden_test_for_bundle("diagnostics", "DiagnosticsService");
    }

    #[test]
    fn test_run_golden_test_for_package_sdv_transmission_manager() {
        run_golden_test_for_bundle("sdv.transmission", "Manager");
    }

    #[test]
    fn test_run_golden_test_for_package_sdv_transmission_monitor() {
        run_golden_test_for_bundle("sdv.transmission", "Monitor");
    }

    #[test]
    fn test_run_golden_test_for_package_sdv_transmission2_other_manager() {
        run_golden_test_for_bundle("sdv.transmission2", "OtherManager");
    }

    #[test]
    fn test_run_golden_test_for_package_sdv_transmission2_other_monitor() {
        run_golden_test_for_bundle("sdv.transmission2", "OtherMonitor");
    }

    #[test]
    fn test_run_golden_test_for_package_someip_rpc_hello_service() {
        run_golden_test_for_bundle("someip.rpc", "HelloService");
    }
}
