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

use crate::rust::android_bp;
use crate::rust::generator::{GENERATED_RS_SOURCES_DIR, SERVICE_BUNDLES_DIR};
use crate::rust::generator_step::GeneratorStepExecution;
use crate::rust::generator_tool_state::GeneratorToolState;
use crate::rust::service_bundle;
use crate::util::{write_bytes_to_file, write_rust_file};
use anyhow::Result;
use heck::ToSnakeCase;
use protobuf::MessageField;
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;
use vsidl_v1_parser::service_bundle_symbol::{ServiceBundleSource, ServiceBundleSymbol};
use vsidl_v1_parser::ProgressPrinter;

pub struct GenerateServiceBundle;

impl GeneratorStepExecution for GenerateServiceBundle {
    fn action(&self, state: &mut GeneratorToolState) -> Result<()> {
        let symbol_table: &vsidl_v1_parser::SymbolTable = &state.symbol_table;
        let cli = &state.cli.clone();
        let root_dir = &state
            .cli
            .output_path
            .clone()
            .unwrap_or_default()
            .join(GENERATED_RS_SOURCES_DIR)
            .join(SERVICE_BUNDLES_DIR);
        let main_genrule_name = state.main_genrule_name.clone().unwrap_or_default();
        let handlebars = state.handlebars.as_ref().unwrap();

        let service_bundle_symbols = symbol_table
            .get_all_service_bundle_symbols()
            .into_iter()
            .filter(|&bundle: &&ServiceBundleSymbol| bundle.source == ServiceBundleSource::CATALOG)
            .collect::<Vec<_>>();
        let printer = ProgressPrinter::new(service_bundle_symbols.len());

        service_bundle_symbols.par_iter().try_for_each(
            |&service_bundle: &&ServiceBundleSymbol| -> anyhow::Result<()> {
                let mut rust_formatter =
                    formatter::get_rust_formatter(state.cli.rust_formatter.clone());

                // The following scope makes sure the mutex is unlocked as soon as printing is finished
                {
                    let full_name = service_bundle.clone().full_name(symbol_table);
                    printer.print_next_progress_step(
                        format!("Generating service bundle code for '{full_name}'").as_ref(),
                    );
                }

                let mut skip_codegen = false;
                if let MessageField(Some(build_cfg)) = &service_bundle.build_cfg {
                    skip_codegen = build_cfg.skip_codegen.unwrap_or_default();
                }

                let service_name_snake_case = service_bundle.name.to_snake_case();
                let mut output_dir = PathBuf::from(root_dir);
                output_dir.extend(service_bundle.package(symbol_table).split('.'));
                output_dir.push(&service_name_snake_case);
                fs::create_dir_all(&output_dir)?;

                if !skip_codegen && !cli.genrule {
                    write_rust_file(
                        &service_bundle::render(service_bundle, symbol_table, cli, handlebars),
                        &output_dir.join("service_bundle.rs"),
                        rust_formatter.as_mut(),
                    )?;
                }

                if !skip_codegen {
                    write_bytes_to_file(
                        &android_bp::render(
                            (*service_bundle).clone(),
                            symbol_table,
                            handlebars,
                            cli,
                            main_genrule_name.clone(),
                        ),
                        &output_dir.join("Android.bp"),
                    )?;
                }
                Ok(())
            },
        )
    }
}
