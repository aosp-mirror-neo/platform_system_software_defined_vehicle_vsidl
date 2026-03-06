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

use crate::lsp::file_diagnostics::FileDiagnostics;
use crate::lsp::shadow_workspace::ShadowWorkspace;
use crate::Cli;
use std::collections::HashMap;
use std::sync::Arc;
use vsidl_v1_parser::{Catalog, Finding, SymbolTable};

/// Holds the state for a single catalog context.
#[derive(Clone)]
pub(crate) struct CatalogContext {
    /// The CLI configuration specific to this catalog (paths, dependencies).
    pub cli: Cli,
    /// The symbol table for this catalog.
    pub symbol_table: SymbolTable,
    /// The parsed catalog.
    pub catalog: Catalog,
    /// Tracks active findings per file to avoid resending duplicates and to manage clearing correctly.
    pub active_findings: HashMap<String, FileDiagnostics>,
    /// Shadow workspace for handling dirty files with external parsers.
    pub shadow_workspace: Option<Arc<ShadowWorkspace>>,
    /// PERSISTENT recovered syntax errors per file.
    pub recovered_syntax_errors: HashMap<String, Vec<Finding>>,
}
