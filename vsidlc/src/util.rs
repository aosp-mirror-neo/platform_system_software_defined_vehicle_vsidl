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

use crate::cli::{AndroidBpFormat, TextprotoFormat};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::Builder;

/// Writes the content to the file with path, if content is different.
/// Automatically formats .textproto and Android.bp files if $ANDROID_BUILD_TOP is set and formatting is enabled for that type.
pub(crate) fn write_bytes_to_file(
    content: &String,
    path: &Path,
    android_bp_format: AndroidBpFormat,
    textproto_format: TextprotoFormat,
) -> anyhow::Result<()> {
    let mut final_content = content.clone();

    if let Ok(top) = std::env::var("ANDROID_BUILD_TOP") {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let should_format = (extension == "textproto"
            && textproto_format == TextprotoFormat::ClangFormat)
            || (file_name == "Android.bp" && android_bp_format == AndroidBpFormat::Bpfmt);

        if should_format {
            // Use a temporary file to format the content before comparison.
            // Create it in the same directory as the target file so tools can find local config (e.g. .clang-format).
            let dir = path.parent().unwrap_or_else(|| Path::new("."));
            let temp_file = Builder::new().suffix(&format!(".{}", extension)).tempfile_in(dir)?;

            let temp_path = temp_file.path();
            fs::write(temp_path, content)?;

            let status = if extension == "textproto" {
                Command::new(format!("{}/tools/repohooks/tools/clang-format.py", top))
                    .arg("--fix")
                    .arg("--clang-format")
                    .arg(format!(
                        "{}/prebuilts/clang/host/linux-x86/clang-stable/bin/clang-format",
                        top
                    ))
                    .arg("--git-clang-format")
                    .arg(format!(
                        "{}/prebuilts/clang/host/linux-x86/clang-stable/bin/git-clang-format",
                        top
                    ))
                    .arg("--style")
                    .arg("file")
                    .arg("--working-tree")
                    .arg(temp_path)
                    .status()
            } else {
                Command::new(format!("{}/out/host/linux-x86/bin/bpfmt", top))
                    .arg("-w")
                    .arg(temp_path)
                    .status()
            };

            match status {
                Ok(s) if s.success() => {
                    if let Ok(formatted) = fs::read_to_string(temp_path) {
                        final_content = formatted;
                    }
                }
                Ok(s) => eprintln!("Formatting failed for {:?}: command exited with {}", path, s),
                Err(e) => eprintln!("Failed to run formatter for {:?}: {}", path, e),
            }
            // tempfile will automatically delete the file when dropped.
        }
    }

    // Skips if the same content already exists in the path.
    if let Ok(existing_content) = fs::read(path) {
        if existing_content.as_slice() == final_content.as_bytes() {
            return Ok(());
        }
    }
    Ok(fs::write(path, final_content)?)
}

// Rust files need to be additionally formatted to avoid unwanted re-compilation
pub(crate) fn write_rust_file(
    content: &str,
    path: &Path,
    formatter: &mut dyn formatter::Formatter,
    android_bp_format: AndroidBpFormat,
    textproto_format: TextprotoFormat,
) -> anyhow::Result<()> {
    let formatted_string = match formatter.format(content) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Formatting failed for {:?}: {}", path, e);
            return Err(e);
        }
    };
    write_bytes_to_file(&formatted_string, path, android_bp_format, textproto_format)?;
    Ok(())
}
