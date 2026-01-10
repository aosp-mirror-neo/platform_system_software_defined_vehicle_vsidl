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

use crate::file_utils::read_config_file;
use crate::proto_schemas_data::ProtoSchemasData;
use log::error;
use protobuf::Message;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;
use vsidl_v1::runtime_config::SomeipRuntimeConfig;
use vsidl_v1::someip::SomeIpMapping;

/// Factory for creating BundleMetadata instances with a shared cache for proto schemas.
/// Designed for single-threaded use.
#[derive(Debug, Default)]
pub(crate) struct BundleMetadataFactory {
    /// A cache mapping a VSIDL file path to its parsed ProtoSchemasData.
    proto_schemas_cache: HashMap<PathBuf, Arc<ProtoSchemasData>>,
}

impl BundleMetadataFactory {
    /// Creates a new, empty factory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new `BundleMetadata` instance.
    /// Takes `&mut self` because it may need to mutate the cache on a cache miss.
    pub fn create_bundle_metadata(
        &mut self,
        diag_file: Option<&String>,
        someip_file: Option<&String>,
        vsidl_file: Option<&String>,
    ) -> BundleMetadata {
        let proto_schemas = self.get_or_parse_proto_schemas(vsidl_file);

        BundleMetadata {
            diagnostics_declaration: Self::parse_config_file::<DiagnosticsDeclaration>(diag_file),
            someip_mappings: Self::parse_config_file::<SomeipRuntimeConfig>(someip_file)
                .map(|config| config.some_ip_mapping),
            proto_schemas,
        }
    }

    /// Retrieves `ProtoSchemasData` from the cache or parses it if not present.
    fn get_or_parse_proto_schemas(&mut self, file: Option<&String>) -> Arc<ProtoSchemasData> {
        let Some(file_path_str) = file else {
            // No file provided, return a default/empty Arc.
            return Arc::new(ProtoSchemasData::new(std::iter::empty()));
        };

        let file_path = PathBuf::from(file_path_str);

        let schema_arc = self
            .proto_schemas_cache
            .entry(file_path.clone())
            .or_insert_with(|| Arc::new(ProtoSchemasData::new(std::iter::once(file_path))));

        Arc::clone(schema_arc)
    }

    fn parse_config_file<T: Message>(file: Option<&String>) -> Option<T> {
        let file_path = file?;
        match read_config_file::<T, _>(file_path) {
            Ok(config) => Some(config),
            Err(e) => {
                error!("Failed to parse file {file:#?}: {e}");
                None
            }
        }
    }
}

/// Implementation of BundleMetadata
#[derive(Debug, Default)]
pub(crate) struct BundleMetadata {
    pub diagnostics_declaration: Option<DiagnosticsDeclaration>,
    pub someip_mappings: Option<Vec<SomeIpMapping>>,
    pub proto_schemas: Arc<ProtoSchemasData>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that requesting the same file twice returns the exact same Arc instance
    /// (proving the cache was hit).
    #[test]
    fn test_cache_reuses_instance_for_same_file() {
        let mut factory = BundleMetadataFactory::new();
        let vsidl_file = "common_types.vsidl".to_string();

        // 1. First access: Should create a new entry and cache it
        let metadata_first = factory.create_bundle_metadata(None, None, Some(&vsidl_file));

        // 2. Second access: Should retrieve the existing entry
        let metadata_second = factory.create_bundle_metadata(None, None, Some(&vsidl_file));

        // 3. Verify internal cache state (optional, white-box testing)
        assert_eq!(factory.proto_schemas_cache.len(), 1, "Cache should contain exactly one entry");

        // 4. Verify logic: Arc::ptr_eq returns true only if both Arcs point to the same allocation
        assert!(
            Arc::ptr_eq(&metadata_first.proto_schemas, &metadata_second.proto_schemas),
            "The factory should return the same Arc instance for the same file path"
        );
    }

    /// Tests that requesting different files results in different Arc instances
    /// and increases the cache size.
    #[test]
    fn test_cache_creates_distinct_instances_for_different_files() {
        let mut factory = BundleMetadataFactory::new();
        let file_a = "service_a.vsidl".to_string();
        let file_b = "service_b.vsidl".to_string();

        let metadata_a = factory.create_bundle_metadata(None, None, Some(&file_a));
        let metadata_b = factory.create_bundle_metadata(None, None, Some(&file_b));

        // Verify cache grew
        assert_eq!(factory.proto_schemas_cache.len(), 2);

        // Verify they are different instances
        assert!(
            !Arc::ptr_eq(&metadata_a.proto_schemas, &metadata_b.proto_schemas),
            "Different files should result in different ProtoSchemasData instances"
        );
    }

    /// Tests that passing None for the vsidl file returns a default instance
    /// and does not affect the cache.
    #[test]
    fn test_none_vsidl_returns_default_and_skips_cache() {
        let mut factory = BundleMetadataFactory::new();

        // 1. Create metadata without a VSIDL file
        let metadata = factory.create_bundle_metadata(None, None, None);

        // 2. Ensure nothing was added to the factory cache
        assert!(factory.proto_schemas_cache.is_empty());

        // 3. Verify that the returned schemas object is logically empty
        // This asserts that because we provided no input file, the resulting
        // data structure contains no mappings.
        assert!(
            metadata.proto_schemas.is_empty(),
            "Expected an empty ProtoSchemasData when no VSIDL file is provided"
        );
    }

    #[test]
    fn test_cache_ignores_file_changes_on_disk_after_initial_load() {
        use std::fs::File;
        use std::io::Write;

        let mut factory = BundleMetadataFactory::new();

        // 1. Setup: Create a temporary file path
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("mutable_test.vsidl");
        let file_path_str = file_path.to_string_lossy().to_string();

        // 2. Write "Version A" to the file
        // Note: Ensure this string is valid syntax for ProtoSchemasData if your parser is strict.
        {
            let mut file = File::create(&file_path).expect("Failed to create temp file");
            writeln!(file, "// Version A Content").expect("Failed to write to temp file");
        }

        // 3. Create metadata_a (populates the cache)
        let metadata_a = factory.create_bundle_metadata(None, None, Some(&file_path_str));

        // 4. Modify the file on disk ("Version B")
        {
            let mut file = File::create(&file_path).expect("Failed to open temp file for writing");
            writeln!(file, "// Version B Content").expect("Failed to overwrite temp file");
        }

        // 5. Create metadata_b (should hit the cache)
        let metadata_b = factory.create_bundle_metadata(None, None, Some(&file_path_str));

        // 6. Clean up the file
        let _ = std::fs::remove_file(file_path);

        // 7. Assertion
        // If the cache works, metadata_b should point to the exact same Arc as metadata_a,
        // meaning it still holds "Version A" in memory and ignored the disk change.
        assert!(
            Arc::ptr_eq(&metadata_a.proto_schemas, &metadata_b.proto_schemas),
            "The factory should return the existing cached instance, ignoring file system changes"
        );
    }
}
