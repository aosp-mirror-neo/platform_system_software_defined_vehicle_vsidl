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
use anyhow::Result;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub trait FileProvider {
    fn read_to_string(&self, path: &Path) -> Result<String>;
    fn list_files_with_extension(&self, path: &Path, extension: &str) -> Result<Vec<PathBuf>>;
    fn is_dir(&self, path: &Path) -> bool;
    fn canonicalize(&self, path: &Path) -> Result<PathBuf>;
}

#[derive(Clone)]
pub struct DiskFileProvider;

impl FileProvider for DiskFileProvider {
    fn read_to_string(&self, path: &Path) -> Result<String> {
        Ok(std::fs::read_to_string(path)?)
    }

    fn list_files_with_extension(
        &self,
        path: &Path,
        target_extension: &str,
    ) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        if target_extension.is_empty() {
            anyhow::bail!("Extension must not be empty");
        }

        for entry in WalkDir::new(path).into_iter() {
            let entry = entry?;
            let filename = entry.path();
            let Some(extension_os) = filename.extension() else {
                continue;
            };
            if let Some(extension) = extension_os.to_str() {
                if target_extension == extension {
                    files.push(filename.to_path_buf());
                }
            }
        }

        Ok(files)
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        Ok(path.canonicalize()?)
    }
}

impl<T: FileProvider + ?Sized> FileProvider for &T {
    fn read_to_string(&self, path: &Path) -> Result<String> {
        (**self).read_to_string(path)
    }

    fn list_files_with_extension(&self, path: &Path, extension: &str) -> Result<Vec<PathBuf>> {
        (**self).list_files_with_extension(path, extension)
    }

    fn is_dir(&self, path: &Path) -> bool {
        (**self).is_dir(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        (**self).canonicalize(path)
    }
}
