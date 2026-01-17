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

use crate::rust::diagnostics;
use crate::rust::generator::{GENERATED_RS_SOURCES_DIR, SERVICE_BUNDLES_DIR};
use crate::rust::generator_step::GeneratorStepExecution;
use crate::rust::generator_tool_state::GeneratorToolState;
use crate::util::write_rust_file;
use anyhow::Result;
use heck::ToSnakeCase;
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;
use vsidl_v1_parser::service_bundle_symbol::{ServiceBundleSource, ServiceBundleSymbol};

pub struct GenerateDiagnostics;

impl GeneratorStepExecution for GenerateDiagnostics {
    fn should_run(&self, state: &GeneratorToolState) -> bool {
        !state.cli.genrule // Do not generate rust code when --genrule flag is set
    }

    fn action(&self, state: &mut GeneratorToolState) -> Result<()> {
        let symbol_table = &state.symbol_table;
        let root_dir = &state
            .cli
            .output_path
            .clone()
            .unwrap_or_default()
            .join(GENERATED_RS_SOURCES_DIR)
            .join(SERVICE_BUNDLES_DIR);
        let handlebars = state.handlebars.as_ref().unwrap();

        symbol_table.get_all_service_bundle_symbols().par_iter().try_for_each(
            |&service_bundle: &&ServiceBundleSymbol| -> anyhow::Result<()> {
                if service_bundle.source == ServiceBundleSource::DEPENDENCY {
                    return Ok(());
                }

                if service_bundle.diagnostics_info.is_some() {
                    let mut rust_formatter =
                        formatter::get_rust_formatter(state.cli.rust_formatter.clone());
                    let service_name_snake_case = service_bundle.name.to_snake_case();
                    let mut output_dir = PathBuf::from(root_dir);
                    output_dir.extend(service_bundle.package(symbol_table).split('.'));
                    output_dir.push(&service_name_snake_case);
                    fs::create_dir_all(&output_dir)?;

                    write_rust_file(
                        &diagnostics::render_symbol((*service_bundle).clone(), handlebars),
                        &output_dir.join("diagnostics.rs"),
                        rust_formatter.as_mut(),
                    )?;
                }
                Ok(())
            },
        )
    }
}
