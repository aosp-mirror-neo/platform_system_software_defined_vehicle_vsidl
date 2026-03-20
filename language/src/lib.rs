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
//! This crate provides common data structures for VSIDL

/// Extract information from Android.bp files
pub mod android_bp;
/// Building the catalog from .vsidl / .proto files
pub mod catalog;
/// Utilities for command line output
pub mod cli_utils;
/// Shared constants
pub mod constants;
/// Diagnostics-related data types
pub mod diagnostics_types;
/// Utilities for accessing the file system
pub mod fs_utils;
/// Utilities for constructing names
pub mod name_utils;
/// Extracts all unit type form a catalog and dependencies
pub mod types;
/// Visitors for traversing the abstract syntax tree
pub mod visitors;

/// Input for test cases
#[cfg(test)]
pub mod test_data;

pub use android_bp::*;
pub use catalog::*;
pub use cli_utils::*;
pub use constants::*;
pub use diagnostics_types::*;
pub use fs_utils::*;
pub use name_utils::*;
pub use types::*;
pub use visitors::convert_pdk29_to_soa::ConvertPdk29ToSOA;
pub use visitors::detect_input_version::DetectInputVersion;
pub use visitors::hide_channel_syntax::HideChannelSyntax;
pub use visitors::hide_pdk29_syntax::HidePdk29Syntax;
pub use visitors::remove_non_google_options;
pub use visitors::visitor::{AstNode, VsidlVisitor};
pub use visitors::visitor_mut::{AstNodeMut, VsidlVisitorMut};

pub mod file_provider;
pub use file_provider::*;
