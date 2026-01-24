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

use dashmap::DashMap;
use protobuf::reflect::FileDescriptor;
use std::marker::{Send, Sync};

/// Trait for file descriptor caches used by the VsidlProvider
pub(crate) trait VsidlFdCache: Sync + Send {
    /// Insert a new entry into the cache
    fn insert_into_cache(&self, file_name: String, file_descriptor: FileDescriptor);
    /// Get a file descriptor from the cache
    fn get_file_descriptor(&self, key: &str) -> Option<FileDescriptor>;
    /// Return a boolean representing the presence of the file descriptor
    fn contains_file_descriptor_with_key(&self, key: &str) -> bool;
}

/// Implementation of the VsidlFdCache trait for the DashMap struct, which is meant to be used in multi-threaded
/// cases. Since DashMap implements Sync, VsidlProviders using it as their cache implement Sync as well,
/// allowing users of VsidlProvider to safely share references to it among multiple threads without needing
/// to wrap it in a Mutex or other synchronization mechanism
/// NOTE: DashMap's documentation state that these methods may deadlock, but if the map is accessed only using these APIs
/// that will not occur, since we do not hold on to any references into the map
impl VsidlFdCache for DashMap<String, FileDescriptor> {
    fn insert_into_cache(&self, file_name: String, file_descriptor: FileDescriptor) {
        self.insert(file_name, file_descriptor);
    }

    fn get_file_descriptor(&self, key: &str) -> Option<FileDescriptor> {
        let map_entry = self.get(key).map(|map_ref| map_ref.value().clone());
        if map_entry.is_none() {
            log::error!("Unable to fetch file descriptor for file {key:?}");
        }
        map_entry
    }

    fn contains_file_descriptor_with_key(&self, key: &str) -> bool {
        self.contains_key(key)
    }
}

#[cfg(test)]
mod test {
    use super::VsidlFdCache;

    use dashmap::DashMap;
    use protobuf::reflect::FileDescriptor;
    use std::fs;
    use std::sync::Arc;
    use std::thread;

    const TEST_PROTO_CONTENT: &str = "syntax = 'proto3'; message TestProto { int32 a = 1; }";

    fn mock_file_descriptor(file_name: &str, file_content: &str) -> FileDescriptor {
        let temp_dir = tempfile::tempdir().unwrap();
        let tempfile = temp_dir.path().join(file_name);
        fs::write(&tempfile, file_content).unwrap();

        let file_descriptor_protos = protobuf_parse::Parser::new()
            .pure()
            .include(&temp_dir)
            .input(&tempfile)
            .parse_and_typecheck()
            .unwrap()
            .file_descriptors;

        FileDescriptor::new_dynamic(file_descriptor_protos[0].clone(), &[]).unwrap()
    }

    fn test_cache_insert_contains_get_succeed<T: VsidlFdCache>(cache: T) {
        let key = "test.proto";
        let fd = mock_file_descriptor(key, TEST_PROTO_CONTENT);

        cache.insert_into_cache(key.to_string(), fd.clone());
        assert!(cache.contains_file_descriptor_with_key(key));

        let retrieved_fd = cache.get_file_descriptor(key).unwrap();
        assert_eq!(retrieved_fd, fd);
    }

    fn test_cache_missing_key_returns_none<T: VsidlFdCache>(cache: T) {
        let key = "missing.proto";

        let result = cache.get_file_descriptor(key);
        assert!(result.is_none());
    }

    #[test]
    fn test_dashmap_cache_insert_contains_get_succeed() {
        test_cache_insert_contains_get_succeed(DashMap::new());
    }

    #[test]
    fn test_dashmap_cache_get_for_missing_key_returns_none() {
        test_cache_missing_key_returns_none(DashMap::new());
    }

    #[test]
    fn test_dashmap_cache_concurrent_insert_get_builds_and_succeeds() {
        let cache = Arc::new(DashMap::new()); // Share the cache between threads

        let cache_clone = Arc::clone(&cache);
        let key1 = "test.proto";
        let fd1 = mock_file_descriptor(key1, TEST_PROTO_CONTENT);

        let fd1_clone = fd1.clone();
        let mut handle = thread::spawn(move || {
            cache_clone.insert_into_cache(key1.to_string(), fd1_clone);
        });
        let key2 = "other_test.proto";
        let fd2 = mock_file_descriptor(key2, TEST_PROTO_CONTENT);
        cache.insert_into_cache(key2.to_string(), fd2.clone());

        handle.join().unwrap(); // Wait for thread to finish

        let cache_clone2 = Arc::clone(&cache);
        handle = thread::spawn(move || {
            let retrieved_fd = cache_clone2.get_file_descriptor(key1).unwrap();
            assert_eq!(retrieved_fd, fd1);
        });
        let retrieved_fd = cache.get_file_descriptor(key2).unwrap();
        assert_eq!(retrieved_fd, fd2);

        handle.join().unwrap();
    }
}
