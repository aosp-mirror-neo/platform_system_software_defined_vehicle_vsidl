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
//! Generates services Rust code for service bundles.

use crate::rust::android_bp;
use crate::rust::generator_step::GeneratorStepExecution;
use crate::rust::generator_tool_state::GeneratorToolState;
use crate::rust::service_bundle::{extract_proto_typename_from_unit_type, ServiceBundleContext};
use crate::util::{write_bytes_to_file, write_rust_file};
use anyhow::{Ok, Result};
use serde_derive::Serialize;
use std::collections::BTreeSet;
use std::fs;
use vsidl_v1_parser::service_bundle_symbol::ServiceBundleSource;

#[derive(Serialize)]
struct ServicesTemplateData {
    context: ServiceBundleContext,
    crate_name: String,
    package_name: String,
    dependencies: Vec<String>,
    proto_imports: Vec<String>,
}

pub struct GenerateServices;

impl GeneratorStepExecution for GenerateServices {
    fn should_run(&self, state: &GeneratorToolState) -> bool {
        state.cli.services
    }

    fn action(&self, state: &mut GeneratorToolState) -> Result<()> {
        let output_path = &state.cli.output_path.clone().unwrap_or_default();
        let symbol_table = &state.symbol_table;
        let format = &state.cli.rust_formatter;
        let cli = &state.cli;
        let handlebars = state.handlebars.as_ref().unwrap();

        // Setup rustfmt before generating anything that would stay unformatted if environment variables are incorrect
        let mut rust_formatter = formatter::get_rust_formatter(*format);

        for service_bundle in symbol_table
            .get_all_service_bundle_symbols()
            .iter()
            .filter(|s| s.source == ServiceBundleSource::CATALOG)
        {
            let context = ServiceBundleContext::from(service_bundle, symbol_table, cli);
            let mut proto_imports_set = BTreeSet::new();
            for rpc_server in &context.rpc_servers {
                for method in &rpc_server.unit_type.methods {
                    proto_imports_set.insert(format!(
                        "use {}::{}::{};",
                        method.request.crate_name,
                        method.request.file_name,
                        method.request.message_name
                    ));
                    proto_imports_set.insert(format!(
                        "use {}::{}::{};",
                        method.response.crate_name,
                        method.response.file_name,
                        method.response.message_name
                    ));
                }
            }
            for rpc_client in &context.rpc_clients {
                for method in &rpc_client.unit_type.methods {
                    proto_imports_set.insert(format!(
                        "use {}::{}::{};",
                        method.request.crate_name,
                        method.request.file_name,
                        method.request.message_name
                    ));
                    proto_imports_set.insert(format!(
                        "use {}::{}::{};",
                        method.response.crate_name,
                        method.response.file_name,
                        method.response.message_name
                    ));
                }
            }
            for publication in &context.publications {
                let unit_type = &publication.0.unit_type;
                proto_imports_set.insert(format!(
                    "use {}::{}::{};",
                    unit_type.crate_name,
                    unit_type.proto_filename,
                    extract_proto_typename_from_unit_type(&unit_type.proto_type_name)
                ));
            }
            for subscription in &context.subscriptions {
                let unit_type = &subscription.0.unit_type;
                proto_imports_set.insert(format!(
                    "use {}::{}::{};",
                    unit_type.crate_name,
                    unit_type.proto_filename,
                    extract_proto_typename_from_unit_type(&unit_type.proto_type_name)
                ));
            }

            let data = ServicesTemplateData {
                context,
                crate_name: android_bp::Context::get_crate_and_library_name_from_service_bundle(
                    (*service_bundle).clone(),
                    symbol_table,
                ),
                package_name: service_bundle
                    .symbol
                    .enclosing_scope(symbol_table)
                    .full_name(symbol_table),
                dependencies: android_bp::Context::collect_dependencies(
                    (*service_bundle).clone(),
                    symbol_table,
                ),
                proto_imports: proto_imports_set.into_iter().collect(),
            };

            let service_dir = output_path.join("services").join(&data.context.service_bundle_name);
            fs::create_dir_all(service_dir.join("src"))?;

            let bp_content = handlebars.render("services_android_bp", &data)?;
            write_bytes_to_file(
                &bp_content,
                &service_dir.join("Android.bp"),
                state.cli.android_bp_formatter,
                state.cli.textproto_formatter,
            )?;

            let main_rs_content = handlebars.render("services_main_rs", &data)?;
            write_rust_file(
                &main_rs_content,
                &service_dir.join("src/main.rs"),
                rust_formatter.as_mut(),
                state.cli.android_bp_formatter,
                state.cli.textproto_formatter,
            )?;
        }

        Ok(())
    }
}
