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

use serde_derive::Serialize;
use std::collections::BTreeSet;
use std::env;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use tinytemplate::TinyTemplate;
use vsidl_v1_parser::{service_bundle_symbol::ServiceBundleSymbol, SymbolTable};

#[derive(Serialize)]
struct BundleContext<'a> {
    name: &'a str,
    package: String,
    full_name: String,
    has_diagnostics: bool,
}

#[derive(Serialize)]
struct Context<'a> {
    year: i32,
    catalog_path: String,
    filegroup: &'a str,
    bundles: Vec<BundleContext<'a>>,
    packages: Vec<String>,
    dependencies: Vec<String>,
    package_contains_some_ip: bool,
    documentation_lines: &'a Vec<String>,
    target_api: &'a str,
}

const ANDROID_BUILD_TOP: &str = "ANDROID_BUILD_TOP";

/// This function uses the `ANDROID_BUILD_TOP` environment variable. As a fallback for
/// testing environments (like sandboxes where `ANDROID_BUILD_TOP` might not be set),
/// it assumes the needed files are present in the current working directory (`.`).
/// (similar to get_vsidl_syntax_default_path() in vsidl/language/src/catalog.rs)
fn get_build_root() -> String {
    env::var(ANDROID_BUILD_TOP).unwrap_or_else(|_| ".".to_string())
}

fn normalize_path(path: &Path) -> PathBuf {
    let components = path.components();
    let mut ret = PathBuf::new();

    for component in components {
        match component {
            Component::Prefix(..) => {
                unreachable!("Path containing prefix component occurs on Windows systems only")
            }
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}

/// Resolves a path for use in generated build files.
///
/// Tries to make the path relative to the Android build root ($ANDROID_BUILD_TOP).
/// If the path is outside the build root, returns the normalized absolute path.
pub fn resolve_build_path(path: &Path) -> String {
    let normalized_path = normalize_path(path);
    let build_root = get_build_root();

    match normalized_path.strip_prefix(&build_root) {
        Ok(relative_path) => {
            // Success: Path is inside build root, return the relative path.
            relative_path.display().to_string()
        }
        Err(_) => {
            // Failure: Path is outside build root, return the (normalized) absolute path.
            normalized_path.display().to_string()
        }
    }
}

impl<'a> Context<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(self) fn from(
        catalog_path: &'a Path,
        symbol_table: &'a SymbolTable,
        bundles: &'a [ServiceBundleSymbol],
        packages: &'a BTreeSet<String>,
        filegroup: &'a str,
        dependencies: &'a [&std::path::Path],
        package_contains_some_ip: bool,
        documentation_lines: &'a Vec<String>,
        target_api: &'a str,
    ) -> Self {
        Context {
            year: crate::generator::get_current_year(),
            catalog_path: resolve_build_path(catalog_path),
            filegroup,
            bundles: bundles
                .iter()
                .map(|service_bundle| BundleContext {
                    name: &service_bundle.name,
                    package: service_bundle.package(symbol_table),
                    full_name: service_bundle.full_name(symbol_table),
                    has_diagnostics: service_bundle.diagnostics_info.is_some(),
                })
                .collect(),
            packages: packages.iter().cloned().collect(),
            dependencies: dependencies.iter().cloned().map(resolve_build_path).collect::<Vec<_>>(),
            package_contains_some_ip,
            documentation_lines,
            target_api,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    catalog_path: &Path,
    symbol_table: &SymbolTable,
    bundles: &[ServiceBundleSymbol],
    packages: &BTreeSet<String>,
    filegroup: &str,
    dependencies: &[&std::path::Path],
    package_contains_some_ip: bool,
    documentation_lines: &Vec<String>,
    target_api: &str,
    tt_engine: &TinyTemplate,
) -> String {
    tt_engine
        .render(
            "android_bp",
            &Context::from(
                catalog_path,
                symbol_table,
                bundles,
                packages,
                filegroup,
                dependencies,
                package_contains_some_ip,
                documentation_lines,
                target_api,
            ),
        )
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, File};
    use std::sync::Mutex;

    /// Some unit tests manipulate same OS env vars, thus need to be ran
    /// sequentially. This mutex enforces that.
    /// Some tests are expected to panic, this poisons the mutex.
    /// Mutex is still safe to use. If future tests use this mutex, they
    /// should ensure that system is left in a default state post potential panic
    static OS_ENV_TEST_GUARD: Mutex<()> = Mutex::new(());

    struct EnvAndroidBuildTopCleanup {
        pre_test_value: Option<String>,
    }
    impl Drop for EnvAndroidBuildTopCleanup {
        fn drop(&mut self) {
            if let Some(pre_test_value) = &self.pre_test_value {
                std::env::set_var(ANDROID_BUILD_TOP, pre_test_value);
            } else {
                std::env::remove_var(ANDROID_BUILD_TOP);
            }
        }
    }
    #[test]
    fn test_resolve_build_path_normal_operation() {
        let _guard =
            OS_ENV_TEST_GUARD.lock().unwrap_or_else(|poison_warn| poison_warn.into_inner());

        // prepare: input is valid path, inside ANDROID_BUILD_TOP
        let original_android_build_top = std::env::var(ANDROID_BUILD_TOP);
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let subdir_name = "foo";
        create_dir_all(temp_dir.path().join(subdir_name)).expect("Failed to create sub dir");
        let _file = File::create(temp_dir.path().join(subdir_name).join("bar.txt"))
            .expect("Failed to create temp file");

        std::env::set_var(ANDROID_BUILD_TOP, temp_dir.path().to_str().unwrap());
        let _cleanup =
            EnvAndroidBuildTopCleanup { pre_test_value: original_android_build_top.ok() };

        // act, assert
        let path = temp_dir.path().join(format!("{subdir_name}/../{subdir_name}")).join("bar.txt");
        let resolved_path = resolve_build_path(&path);
        assert_eq!(resolved_path, "foo/bar.txt");
    }

    #[test]
    fn test_resolve_build_path_returns_absolute_when_outside_android_build_top() {
        let _guard =
            OS_ENV_TEST_GUARD.lock().unwrap_or_else(|poison_warn| poison_warn.into_inner());

        //prepare: input is valid path, outside ANDROID_BUILD_TOP
        let original_android_build_top = std::env::var(ANDROID_BUILD_TOP);
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let subdir_name = "foo";
        create_dir_all(temp_dir.path().join(subdir_name)).expect("Failed to create sub dir");
        let _file =
            File::create(temp_dir.path().join("bar.txt")).expect("Failed to create temp file");

        // Set build top to the *inner* directory: /.../temp_dir/foo
        std::env::set_var(ANDROID_BUILD_TOP, temp_dir.path().join(subdir_name).to_str().unwrap());
        let _cleanup =
            EnvAndroidBuildTopCleanup { pre_test_value: original_android_build_top.ok() };

        // act: Path is the *sibling* file: /.../temp_dir/bar.txt
        let path = temp_dir.path().join("bar.txt");
        let result = resolve_build_path(&path);

        // assert: The result should be the (normalized) absolute path
        // We call normalize_path here to match the function's internal behavior
        let expected_str = normalize_path(&path).display().to_string();
        assert_eq!(result, expected_str);
    }
}
