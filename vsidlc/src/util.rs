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

use std::fs;
use std::path::Path;

/// Writes the content to the file with path, if content is different
pub(crate) fn write_bytes_to_file(content: &String, path: &Path) -> anyhow::Result<()> {
    // Skips if the same content already exists in the path.
    if let Ok(existing_content) = fs::read(path) {
        if existing_content.as_slice() == (std::convert::AsRef::<[u8]>::as_ref(content)) {
            return Ok(());
        }
    }
    Ok(fs::write(path, content)?)
}

// Rust files need to be additionally formatted to avoid unwanted re-compilation
pub(crate) fn write_rust_file(
    content: &str,
    path: &Path,
    formatter: &mut dyn formatter::Formatter,
) -> anyhow::Result<()> {
    let formatted_string = formatter.format(content)?;
    write_bytes_to_file(&formatted_string, path)?;
    Ok(())
}
