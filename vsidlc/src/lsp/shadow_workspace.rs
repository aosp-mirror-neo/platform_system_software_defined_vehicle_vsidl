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

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A `ShadowWorkspace` manages a temporary directory that mirrors a source directory.
///
/// It works by recursively creating symlinks to the original files. This allows
/// tools to "modify" files in the shadow workspace (by replacing symlinks with real files)
/// without affecting the actual source code.
///
/// We need this because there might be changes in the IDE that are not (yet) stored to disk,
/// and the protobuf crate expects us to provide paths on disk. This shadow directory mirrors
/// the original catalog's directory but replaces the files opened in the IDE with the content
/// the user sees in the IDE.
pub(crate) struct ShadowWorkspace {
    /// The underlying temporary directory on the filesystem.
    temp_dir: TempDir,
    /// The original source directory being mirrored.
    source_root: PathBuf,
}

impl ShadowWorkspace {
    /// Creates a new shadow workspace and mirrors the contents of `source_root`.
    ///
    /// This will immediately trigger a recursive traversal of the source directory
    /// to establish the initial symlink structure.
    pub(crate) fn new(source_root: PathBuf) -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;
        eprintln!("Created shadow workspace at {:?}", temp_dir.path());
        let workspace = Self { temp_dir, source_root };
        Self::mirror_directory(&workspace.source_root, workspace.temp_dir.path())?;
        Ok(workspace)
    }

    /// Recursively mirrors a directory.
    ///
    /// Directories are created physically, while files are created as symbolic
    /// links pointing back to the `src` location.
    fn mirror_directory(src: &Path, dst: &Path) -> anyhow::Result<()> {
        if !src.exists() {
            return Ok(());
        }
        if src.is_dir() {
            std::fs::create_dir_all(dst)?;
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                let path = entry.path();
                let name = entry.file_name();
                Self::mirror_directory(&path, &dst.join(name))?;
            }
        } else {
            // Use absolute paths for the link target to ensure the symlink
            // remains valid even if the process changes its working directory.
            let abs_src = std::fs::canonicalize(src).unwrap_or(src.to_path_buf());

            // If a file/link already exists at the destination, remove it before re-linking.
            if dst.exists() {
                let _ = std::fs::remove_file(dst);
            }
            symlink(&abs_src, dst)?;
        }
        Ok(())
    }

    /// Overwrites a file within the shadow workspace with new content.
    ///
    /// This breaks the symlink to the original source file and writes a
    /// real file to the temporary directory.
    ///
    /// # Arguments
    /// * `path` - The path to the file, relative to the source root or absolute.
    /// * `content` - The new string content to write.
    pub(crate) fn update_file(&self, path: &Path, content: &str) -> anyhow::Result<()> {
        let abs_path =
            if path.is_absolute() { path.to_path_buf() } else { self.source_root.join(path) };

        let relative = match abs_path.strip_prefix(&self.source_root) {
            Ok(r) => r,
            Err(_) => {
                // If the file is outside the catalog root, we ignore it.
                return Ok(());
            }
        };

        let dst = self.temp_dir.path().join(relative);

        // Ensure the parent directory exists in the shadow workspace.
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove the existing symlink or file before writing the new content.
        if dst.exists() || dst.is_symlink() {
            std::fs::remove_file(&dst)?;
        }

        std::fs::write(&dst, content)?;
        Ok(())
    }

    /// Reverts a modified file back to a symlink pointing to the original source.
    pub(crate) fn revert_file(&self, path: &Path) -> anyhow::Result<()> {
        let abs_path =
            if path.is_absolute() { path.to_path_buf() } else { self.source_root.join(path) };

        let relative = match abs_path.strip_prefix(&self.source_root) {
            Ok(r) => r,
            Err(_) => return Ok(()),
        };

        let dst = self.temp_dir.path().join(relative);

        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if dst.exists() || dst.is_symlink() {
            std::fs::remove_file(&dst)?;
        }

        let abs_src = std::fs::canonicalize(&abs_path).unwrap_or(abs_path);
        symlink(&abs_src, &dst)?;
        Ok(())
    }

    /// Returns the filesystem path to the root of the shadow workspace.
    pub(crate) fn root_path(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }

    /// Translates a path from the shadow workspace back to its original location
    /// in the source directory.
    ///
    /// If the path is not within the shadow workspace, it returns the input string.
    pub(crate) fn map_to_source(&self, path: &str) -> String {
        let path_buf = PathBuf::from(path);
        if let Ok(relative) = path_buf.strip_prefix(self.temp_dir.path()) {
            return self.source_root.join(relative).to_string_lossy().to_string();
        }
        path.to_string()
    }

    /// Translates a path from the source directory to its equivalent location
    /// in the shadow workspace.
    ///
    /// If the path is not within the source root, it returns the input path unchanged.
    ///
    /// # Arguments
    /// * `path` - The path to translate (can be relative to original workspace root or absolute).
    pub(crate) fn map_to_shadow(&self, path: &Path) -> PathBuf {
        let abs_path =
            if path.is_absolute() { path.to_path_buf() } else { self.source_root.join(path) };

        match abs_path.strip_prefix(&self.source_root) {
            Ok(relative) => self.temp_dir.path().join(relative),
            Err(_) => path.to_path_buf(),
        }
    }
}
