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

use anyhow::anyhow;
use lsp_types::Url;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use vsidl_v1_parser::file_provider::{DiskFileProvider, FileProvider};

type DocumentStore = Arc<Mutex<HashMap<Url, String>>>;

#[derive(Clone)]
pub(crate) struct LspFileProvider {
    pub(crate) document_store: DocumentStore,
    pub(crate) file_hashes: Arc<Mutex<HashMap<PathBuf, u64>>>,
    pub(crate) disk_fallback: DiskFileProvider,
}

/// Helper function to calculate a hash for a string.
pub(crate) fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

impl LspFileProvider {
    pub(crate) fn get_hash(&self, path: &Path) -> Option<u64> {
        self.file_hashes.lock().ok().and_then(|h| h.get(path).cloned())
    }

    pub(crate) fn update_hash(&self, path: PathBuf, hash: u64) {
        if let Ok(mut hashes) = self.file_hashes.lock() {
            hashes.insert(path, hash);
        }
    }
}

impl FileProvider for LspFileProvider {
    fn read_to_string(&self, path: &Path) -> anyhow::Result<String> {
        let url = Url::from_file_path(path)
            .map_err(|_| anyhow!("Failed to convert path to URL: {:?}", path))?;
        let content = {
            let store = self.document_store.lock().map_err(|e| anyhow!("Lock poisoned: {}", e))?;
            store.get(&url).cloned()
        };
        let content = if let Some(content) = content {
            content
        } else {
            self.disk_fallback.read_to_string(path)?
        };

        // Warm up the hash for the file if it's not yet tracked.
        // This ensures the initial hashes of all files are present
        // after the catalog gets read for the first time.
        // We do not update the hash if it already exists to ensure
        // changes are acknowledged intentionally, e.g., by re-validating
        // the catalog.
        let mut hashes = self.file_hashes.lock().unwrap();
        if !hashes.contains_key(path) {
            let hash = calculate_hash(&content);
            hashes.insert(path.to_path_buf(), hash);
        }

        Ok(content)
    }

    fn list_files_with_extension(
        &self,
        path: &Path,
        extension: &str,
    ) -> anyhow::Result<Vec<PathBuf>> {
        self.disk_fallback.list_files_with_extension(path, extension)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.disk_fallback.is_dir(path)
    }

    fn canonicalize(&self, path: &Path) -> anyhow::Result<PathBuf> {
        self.disk_fallback.canonicalize(path)
    }
}
