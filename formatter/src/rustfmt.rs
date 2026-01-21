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

use crate::Formatter;
use anyhow::bail;
use anyhow::Context;
use std::env;
use std::env::consts::OS;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;

pub const ANDROID_BUILD_TOP_ENV_VAR: &str = "ANDROID_BUILD_TOP";
pub const RUSTFMT_PATH_ENV_VAR: &str = "RUSTFMT_PATH";
pub const RUSTFMT_TOML_PATH_ENV_VAR: &str = "RUSTFMT_TOML_PATH";

pub struct RustFmt {
    rustfmt_cmd: Command,
}

impl RustFmt {
    pub fn new() -> RustFmt {
        let Ok(cmd) = get_rust_formatter() else {
            panic!("unable to initialize rustfmt");
        };
        RustFmt { rustfmt_cmd: cmd }
    }
}

impl Formatter for RustFmt {
    fn format(&mut self, input: &str) -> anyhow::Result<String> {
        let bytes = format_rust_code(input, &mut self.rustfmt_cmd)
            .or_else(|e| bail!("{}. Use '--rust-formatter none' to avoid formatting", e))?;
        String::from_utf8(bytes)
            .or_else(|e| bail!("{}. Use '--rust-formatter none' to avoid formatting", e))
    }
}

// Returns a command object pointing to the right rustfmt to be used.
fn get_rust_formatter() -> anyhow::Result<Command> {
    let (rustfmt_path, config_path) = get_rust_paths()?;

    let mut formatter = Command::new(rustfmt_path);
    formatter
        .arg(format!("--config-path={config_path}"))
        .args(["--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    Ok(formatter)
}

fn format_rust_code(content: &str, formatter: &mut Command) -> anyhow::Result<Vec<u8>> {
    let mut formatter_child = formatter
        .spawn()
        .with_context(|| format!("FAILED to start the formatter: {formatter:?}"))?;
    let stdin = formatter_child.stdin.take();
    if stdin.is_none() {
        bail!("FAILED the get formatter stdin: {formatter_child:?}");
    }
    // It is safe to unwrap since the value is not none
    let mut stdin = stdin.unwrap();
    if let Err(e) = BufWriter::new(&mut stdin).write_all(content.as_bytes()) {
        bail!("FAILED the send a generated code to formatter: {e:#?}, {formatter_child:?}")
    }
    // close stdin of the child process to stop processing
    drop(stdin);
    match formatter_child.wait_with_output() {
        Err(e) => bail!("FAILED to get the formatted code: {e:#?}, "),
        Ok(output) => {
            if !output.status.success() {
                bail!("FAILED formatting : {output:?}")
            }
            Ok(output.stdout)
        }
    }
}

// Get the rustfmt and rustfmt.toml paths
//
// In case RUSTFMT_PATH and RUSTFMT_TOML_PATH environment variables are set,
// rustfmt and rustfmt.toml from the paths defined by these variablies will be chosen.
//
// In case RUSTFMT_PATH and RUSTFMT_TOML_PATH are unset, but ANDROID_BUILD_TOP is set,
// rustfmt from prebuilts and rustfmt.toml from soong folder will be chosen
//
// In case neither ANDROID_BUILD_TOP nor RUSTFMT_PATH and RUSTFMT_TOML_PATH are set, the
// function fails with an error
fn get_rust_paths() -> anyhow::Result<(String, String)> {
    let root = env::var(ANDROID_BUILD_TOP_ENV_VAR);
    let android_build_top_set = root.is_ok();
    let custom_rustfmt_path_set = validate_rustfmt_env_vars()?;

    // if no path to rusfmt is set, fail!
    if !android_build_top_set && !custom_rustfmt_path_set {
        bail!("Provide either ${ANDROID_BUILD_TOP_ENV_VAR} or (${RUSTFMT_PATH_ENV_VAR} and ${RUSTFMT_TOML_PATH_ENV_VAR})");
    }

    if custom_rustfmt_path_set {
        return Ok((env::var(RUSTFMT_PATH_ENV_VAR)?, env::var(RUSTFMT_TOML_PATH_ENV_VAR)?));
    }

    // safe to unwrap because android_build_top_set is true here
    let root = root.unwrap();
    Ok((
        format!("{root}/prebuilts/rust-toolchain/{OS}-x86/stable/rustfmt"),
        format!("{root}/build/soong/scripts/rustfmt.toml"),
    ))
}

// Check that the two environment vars RUSTFMT_PATH and RUSTFMT_TOML_PATH specify a valid Rust formatter
// returns:
// - Ok(false) if both variables are not set
// - Ok(true) if both variables are set
// - Err() if only one variable is set
fn validate_rustfmt_env_vars() -> anyhow::Result<bool> {
    let rustfmt_path_ok = env::var(RUSTFMT_PATH_ENV_VAR).is_ok();
    let rustfmt_toml_path_ok = env::var(RUSTFMT_TOML_PATH_ENV_VAR).is_ok();

    // If both environment vars don't exist, there's nothing to check
    if !rustfmt_path_ok && !rustfmt_toml_path_ok {
        return Ok(false);
    }

    // Check that not just one of the variables exists
    if rustfmt_path_ok ^ rustfmt_toml_path_ok {
        let (unset_var, set_var) = if rustfmt_path_ok {
            (RUSTFMT_TOML_PATH_ENV_VAR, RUSTFMT_PATH_ENV_VAR)
        } else {
            (RUSTFMT_PATH_ENV_VAR, RUSTFMT_TOML_PATH_ENV_VAR)
        };
        const EXPLANATION: &str = "Please set both to use a custom Rust formatter, or none of the two environment variables to use the default formatter from Android.";
        bail!(
            "{} environment variable is not set, but {} is set. {}",
            unset_var,
            set_var,
            EXPLANATION
        );
    }

    // Check that RUSTFMT_TOML_PATH points to a .toml file
    if let Ok(env_rustfmt_toml_path) = env::var(RUSTFMT_TOML_PATH_ENV_VAR) {
        if !is_toml_file(&env_rustfmt_toml_path) {
            bail!("{} does not contain the path to a .toml file", RUSTFMT_TOML_PATH_ENV_VAR);
        }
    }
    Ok(true)
}

// Check if a given path points to a .toml file
fn is_toml_file(path_str: &str) -> bool {
    let path = Path::new(path_str);
    path.is_file() && path.extension().is_some_and(|ext| ext == "toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    const FAKE_TOML_PATH: &str = "/tmp/test_validate_rustfmt_env_vars.toml";

    /// The following mutex is used to guarantee that every test in this file having unsafe section
    /// are run sequentially. This prevent unexpected behavior.
    /// The data protected by the mutex (i32) is not relevant.
    static SAFETY_MUTEX: Mutex<i32> = Mutex::new(0);

    // create toml configuration file
    fn create_fake_toml_file() {
        std::fs::File::create(FAKE_TOML_PATH).unwrap();
    }

    // clean up toml configuration file
    fn clean_up_fake_toml_file() {
        std::fs::remove_file(FAKE_TOML_PATH).unwrap();
    }

    #[test]
    fn test_validate_rustfmt_env_vars() {
        let _keep_lock = SAFETY_MUTEX.lock();

        // setup
        create_fake_toml_file();

        // WHEN no variable is set
        // SAFETY: SAFETY_MUTEX ensure test in this file are executed sequentially
        // https://doc.rust-lang.org/std/env/fn.remove_var.html
        unsafe {
            env::remove_var(RUSTFMT_PATH_ENV_VAR);
            env::remove_var(RUSTFMT_TOML_PATH_ENV_VAR);
        }

        // THEN validate_rustfmt_env_vars return Ok(false)
        let r = validate_rustfmt_env_vars();
        assert!(r.is_ok());
        assert!(!r.unwrap());

        // WHEN only rustfmt path variable is set
        // SAFETY: SAFETY_MUTEX ensure test in this file are executed sequentially
        // https://doc.rust-lang.org/std/env/fn.remove_var.html
        unsafe {
            env::set_var(RUSTFMT_PATH_ENV_VAR, "test_path_furstfmt");
        }

        // THEN validate_rustfmt_env_vars return Err
        let r = validate_rustfmt_env_vars();
        assert!(r.is_err());

        // WHEN only rustfmt.toml path variable is set
        // SAFETY: SAFETY_MUTEX ensure test in this file are executed sequentially
        // https://doc.rust-lang.org/std/env/fn.remove_var.html
        unsafe {
            env::remove_var(RUSTFMT_PATH_ENV_VAR);
            env::set_var(RUSTFMT_TOML_PATH_ENV_VAR, "test_path_furstfmt.toml");
        }

        // THEN validate_rustfmt_env_vars return Err
        let r = validate_rustfmt_env_vars();
        assert!(r.is_err());

        // WHEN rustfmt and rustfmt.toml are both set
        // SAFETY: SAFETY_MUTEX ensure test in this file are executed sequentially
        // https://doc.rust-lang.org/std/env/fn.remove_var.html
        unsafe {
            env::set_var(RUSTFMT_PATH_ENV_VAR, "any_path");
            env::set_var(RUSTFMT_TOML_PATH_ENV_VAR, FAKE_TOML_PATH);
        }

        // THEN validate_rustfmt_env_vars returns Ok(true)
        let r = validate_rustfmt_env_vars();
        assert!(r.is_ok());
        assert!(r.unwrap());

        // teardown
        clean_up_fake_toml_file();
    }

    #[test]
    fn test_get_rust_paths() {
        let _keep_lock = SAFETY_MUTEX.lock();

        // setup
        create_fake_toml_file();

        // WHEN no variable is set
        // SAFETY: SAFETY_MUTEX ensure test in this file are executed sequentially
        // https://doc.rust-lang.org/std/env/fn.remove_var.html
        unsafe {
            env::remove_var(ANDROID_BUILD_TOP_ENV_VAR);
            env::remove_var(RUSTFMT_PATH_ENV_VAR);
            env::remove_var(RUSTFMT_TOML_PATH_ENV_VAR);
        }

        // THEN get_rust_paths return Err
        let r = get_rust_paths();
        assert!(r.is_err());

        // WHEN rustfmt and rustfmt_toml are set
        // SAFETY: SAFETY_MUTEX ensure test in this file are executed sequentially
        // https://doc.rust-lang.org/std/env/fn.remove_var.html
        unsafe {
            env::set_var(RUSTFMT_PATH_ENV_VAR, "any_path");
            env::set_var(RUSTFMT_TOML_PATH_ENV_VAR, FAKE_TOML_PATH);
        }

        // THEN get_rust_paths return the rust variable values
        let r = get_rust_paths();
        assert!(r.is_ok());
        assert_eq!((String::from("any_path"), String::from(FAKE_TOML_PATH)), r.unwrap());

        // WHEN android_build and rustfmt and rustfmt_toml are set
        // SAFETY: SAFETY_MUTEX ensure test in this file are executed sequentially
        // https://doc.rust-lang.org/std/env/fn.remove_var.html
        unsafe {
            env::set_var(ANDROID_BUILD_TOP_ENV_VAR, "/tmp/android");
        }

        // THEN get_rust_paths return the rust variable values
        let r = get_rust_paths();
        assert!(r.is_ok());
        assert_eq!((String::from("any_path"), String::from(FAKE_TOML_PATH)), r.unwrap());

        // WHEN android_build and one of the rustfmt and rustfmt_toml are set
        // SAFETY: SAFETY_MUTEX ensure test in this file are executed sequentially
        // https://doc.rust-lang.org/std/env/fn.remove_var.html
        unsafe {
            env::remove_var(RUSTFMT_PATH_ENV_VAR);
        }

        // THEN get_rust_paths return Err
        let r = get_rust_paths();
        assert!(r.is_err());

        // WHEN only android_build is set
        // SAFETY: SAFETY_MUTEX ensure test in this file are executed sequentially
        // https://doc.rust-lang.org/std/env/fn.remove_var.html
        unsafe {
            env::remove_var(RUSTFMT_TOML_PATH_ENV_VAR);
        }

        // THEN get_rust_paths return path relative to android_build
        let r = get_rust_paths();
        assert!(r.is_ok());
        assert_eq!(
            (
                String::from("/tmp/android/prebuilts/rust-toolchain/linux-x86/stable/rustfmt"),
                String::from("/tmp/android/build/soong/scripts/rustfmt.toml")
            ),
            r.unwrap()
        );

        // teardown
        clean_up_fake_toml_file();
    }
}
