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
//! Provides runtime vsidl configuration for the SDV VM.

mod cache;
mod types;
pub mod vsidl_provider;
pub mod vsidl_provider_api;

/// Representation of Service Bundle metadata for VSIDL provider.
mod bundle_metadata;

/// Representation of metadata for VSIDL schemas.
mod proto_schemas_data;

/// Utilities related to reading from file.
mod file_utils;

/// Implementation that takes local VM files as a source.
mod local_provider_from_files;

/// Implementation that takes APEXes Service Bundle metadata as a source.
mod local_provider_from_apexes;

/// Implementation that takes VSIDL provider Agent as a source.
mod provider_from_agent;

/// Factory for VsidlProvider's sub-implementations to enable unit testing
mod factory;

/// Utilities related to unit testing.
#[cfg(test)]
mod test_utils;
