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

//! Rust Code Formatter.

use clap::ValueEnum;

mod no_format;
mod pretty;
mod rustfmt;

/// Enum to control the formatting
#[derive(ValueEnum, Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum Format {
    /// Format rust.
    RustFmt,
    /// Do not format.
    None,
    /// Format with pretty please.
    PrettyPlease,
}

/// Trait that formats a given string.
pub trait Formatter {
    /// Formats a given string.
    fn format(&mut self, input: &str) -> anyhow::Result<String>;
}

/// Constructor for a formatter.
pub fn get_rust_formatter(format: Format) -> Box<dyn Formatter> {
    match format {
        Format::RustFmt => Box::new(rustfmt::RustFmt::new()),
        Format::None => Box::new(no_format::NoFormat {}),
        Format::PrettyPlease => Box::new(pretty::PrettyPlease {}),
    }
}
