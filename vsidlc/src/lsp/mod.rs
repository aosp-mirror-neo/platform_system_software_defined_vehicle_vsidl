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

pub(crate) mod catalog_context;
pub(crate) mod diagnostics;
pub(crate) mod file_diagnostics;
pub(crate) mod handlers;
pub(crate) mod lsp_file_provider;
pub(crate) mod recovering_parser;
pub(crate) mod server;
pub(crate) use server::lsp_server_start;
pub(crate) mod shadow_workspace;
pub(crate) mod span_source_remapper;
#[cfg(test)]
pub(crate) mod test_utils;
pub(crate) mod token_builder;
pub(crate) mod utils;
