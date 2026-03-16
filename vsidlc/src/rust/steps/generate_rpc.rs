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
use crate::rust::generator::{GENERATED_RS_SOURCES_DIR, RPC_DIR};
use crate::rust::generator_step::GeneratorStepExecution;
use crate::rust::generator_tool_state::GeneratorToolState;
use crate::rust::rpc_service;
use crate::util::{write_bytes_to_file, write_rust_file};
use anyhow::Result;
use rayon::prelude::*;
use std::fs;
use vsidl_v1_parser::proto_rpc_service_symbol::{ProtoRpcServiceSource, ProtoRpcServiceSymbol};
use vsidl_v1_parser::ProgressPrinter;

pub struct GenerateRpc;

impl GeneratorStepExecution for GenerateRpc {
    fn action(&self, state: &mut GeneratorToolState) -> Result<()> {
        let symbol_table = &state.symbol_table;
        let cli = &state.cli.clone();
        let root_dir = &state
            .cli
            .output_path
            .clone()
            .unwrap_or_default()
            .join(GENERATED_RS_SOURCES_DIR)
            .join(RPC_DIR);
        let handlebars = state.handlebars.as_ref().unwrap();
        let main_genrule_name = state.main_genrule_name.clone().unwrap_or_default();

        let rpc_servers = symbol_table
            .get_all_proto_rpc_service_symbols()
            .into_iter()
            .filter(|&service: &&ProtoRpcServiceSymbol| {
                service.source == ProtoRpcServiceSource::CATALOG
            })
            .collect::<Vec<_>>();
        let printer = ProgressPrinter::new(rpc_servers.len());

        rpc_servers.par_iter().try_for_each(
            |&proto_rpc_service: &&ProtoRpcServiceSymbol| -> anyhow::Result<()> {
                // The following scope makes sure the mutex is unlocked as soon as printing is finished
                {
                    let full_name = proto_rpc_service.clone().full_name(symbol_table);
                    printer.print_next_progress_step(
                        format!("Generating RPC code for '{full_name}'").as_ref(),
                    );
                }

                let mut rust_formatter = formatter::get_rust_formatter(state.cli.rust_formatter);

                let mut rpc_output_dir = root_dir.clone();
                rpc_output_dir.extend(proto_rpc_service.package(symbol_table).split('.'));
                rpc_output_dir.push(android_bp::get_rpc_lib_folder_name(proto_rpc_service.clone()));
                fs::create_dir_all(&rpc_output_dir)?;

                if !cli.genrule {
                    write_rust_file(
                        &rpc_service::render(proto_rpc_service, symbol_table, handlebars),
                        &rpc_output_dir.join("lib.rs"),
                        rust_formatter.as_mut(),
                        state.cli.android_bp_formatter,
                        state.cli.textproto_formatter,
                    )?;
                }

                write_bytes_to_file(
                    &android_bp::render_rpc(
                        proto_rpc_service,
                        symbol_table,
                        handlebars,
                        cli,
                        main_genrule_name.clone(),
                    ),
                    &rpc_output_dir.join("Android.bp"),
                    state.cli.android_bp_formatter,
                    state.cli.textproto_formatter,
                )?;

                Ok(())
            },
        )
    }
}
