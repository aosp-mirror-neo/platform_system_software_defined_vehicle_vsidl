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

use crate::rust::generator_chain_builder::GeneratorChainBuilder;
use crate::rust::generator_tool_state::GeneratorToolState;
use crate::rust::steps::generate_apex::GenerateApex;
use crate::rust::steps::generate_diagnostics::GenerateDiagnostics;
use crate::rust::steps::generate_main_android_bp::GenerateMainAndroidBp;
use crate::rust::steps::generate_rpc::GenerateRpc;
use crate::rust::steps::generate_service_bundle::GenerateServiceBundle;
use crate::rust::steps::generate_services::GenerateServices;
use crate::rust::steps::initialize_handlebars::InitializeHandlebars;
use crate::rust::steps::setup_output_directory::SetupOutputDirectory;
use crate::{AndroidBpFileGroups, Cli};
use chrono::Datelike;
use heck::ToSnakeCase;
use vsidl_v1_parser::SymbolTable;

pub const GENERATED_RS_SOURCES_DIR: &str = "generated_rs/";
pub const SERVICE_BUNDLES_DIR: &str = "service_bundles";
pub const RPC_DIR: &str = "rpc";

/// Generate rust middleware for each service bundle. Generated targets are written into respective folder inside `output_path`
pub fn generate_middleware(
    symbol_table: SymbolTable,
    cli: &Cli,
    filegroups: AndroidBpFileGroups,
) -> anyhow::Result<()> {
    let mut generator_state = GeneratorToolState::new(symbol_table, cli.clone(), filegroups);

    if let Some(chain) = GeneratorChainBuilder::new()
        .add_step(Box::new(SetupOutputDirectory))
        .add_step(Box::new(InitializeHandlebars))
        .add_step(Box::new(GenerateMainAndroidBp))
        .add_step(Box::new(GenerateServiceBundle))
        .add_step(Box::new(GenerateRpc))
        .add_step(Box::new(GenerateDiagnostics))
        .add_step(Box::new(GenerateApex))
        .add_step(Box::new(GenerateServices))
        .build()
    {
        chain.execute(&mut generator_state)?;
    }

    generator_state.finding_collector.print_findings();

    Ok(())
}

pub(crate) fn get_current_year() -> i32 {
    chrono::Utc::now().year()
}

pub(crate) fn extract_filename(path: &str) -> &str {
    match path.rsplit_once('/') {
        None => path,
        Some((_, suffix)) => suffix,
    }
    .strip_suffix(".proto")
    .unwrap()
}

pub(crate) fn get_rpc_crate_name(package: String, type_: String) -> String {
    format!("{package}_{type_}_rpc").replace('.', "_").to_snake_case()
}
