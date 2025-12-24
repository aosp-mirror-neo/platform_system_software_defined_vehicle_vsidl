// Copyright 2023 Google LLC
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
