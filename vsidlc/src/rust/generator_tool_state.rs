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

use crate::{AndroidBpFileGroups, Cli};
use handlebars::Handlebars;
use vsidl_v1_parser::{FindingCollector, SymbolTable};

/// Contains the shared state that is passed between generator steps.
pub struct GeneratorToolState {
    /// The parsed VSIDL symbols and their relationships.
    pub symbol_table: SymbolTable,
    /// The command line arguments passed to vsidlc.
    pub cli: Cli,
    /// Filegroups for Android.bp generation, categorized by catalog and dependencies.
    pub filegroups: AndroidBpFileGroups,
    /// The Handlebars instance used for rendering templates.
    pub handlebars: Option<Handlebars<'static>>,
    /// The name of the main genrule, if applicable.
    pub main_genrule_name: Option<String>,
    /// Collects findings (warnings, errors) during the generation process.
    pub finding_collector: FindingCollector,
}

impl GeneratorToolState {
    pub fn new(symbol_table: SymbolTable, cli: Cli, filegroups: AndroidBpFileGroups) -> Self {
        Self {
            symbol_table,
            cli,
            filegroups,
            handlebars: None,
            main_genrule_name: None,
            finding_collector: FindingCollector::default(),
        }
    }
}
