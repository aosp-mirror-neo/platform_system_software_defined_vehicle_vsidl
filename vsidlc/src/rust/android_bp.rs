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

use crate::Cli;
use handlebars::Handlebars;
use heck::ToSnakeCase;
use indexmap::IndexSet;
use protobuf::MessageField;
use serde_derive::Serialize;
use std::path::PathBuf;
use vsidl_v1_parser::proto_rpc_service_symbol::ProtoRpcServiceSymbol;
use vsidl_v1_parser::service_bundle_symbol::ServiceBundleSymbol;
use vsidl_v1_parser::DiagnosticsProtoMessage;
use vsidl_v1_parser::SymbolTable;

pub(crate) fn get_rpc_crate_name(
    rpc_service: &ProtoRpcServiceSymbol,
    symbol_table: &SymbolTable,
) -> String {
    crate::rust::generator::get_rpc_crate_name(
        rpc_service.package(symbol_table),
        rpc_service.name().clone(),
    )
}

pub(crate) fn get_rpc_target_name(
    rpc_service: &ProtoRpcServiceSymbol,
    symbol_table: &SymbolTable,
) -> String {
    format!("lib{}_rs", &get_rpc_crate_name(rpc_service, symbol_table))
}

pub(crate) fn get_diagnostics_message_target_name(message: &DiagnosticsProtoMessage) -> String {
    message.proto_message.proto_metadata.rust_build_target_name.clone().unwrap()
}

pub(crate) fn get_rpc_lib_folder_name(rpc_service: ProtoRpcServiceSymbol) -> String {
    rpc_service.name().replace(".", "_").to_snake_case()
}

#[derive(Serialize)]
struct RpcLib {
    target_name: String,
    crate_name: String,
    folder: String,
    dependencies: Vec<String>,
    is_streaming: bool,
    service_bundle_package: String,
    unit_type_name: String,
}

#[derive(Serialize)]
pub(crate) struct Context {
    year: i32,
    crate_name: String,
    dependencies: Vec<String>,
    has_diagnostics: bool,
    has_publisher: bool,
    rpc_libs: Vec<RpcLib>,
    generate_service_bundle_target: bool,
    genrule: bool,
    catalog_module_name: String,
    service_bundle_file_path: String,
    package_name: String,
    service_bundle_name: String,
}

#[derive(Serialize)]
pub(crate) struct RpcContext {
    year: i32,
    genrule: bool,
    catalog_module_name: String,
    rpc_libs: Vec<RpcLib>,
}

impl RpcLib {
    pub(self) fn from(rpc_service: &ProtoRpcServiceSymbol, symbol_table: &SymbolTable) -> Self {
        let mut dependencies = IndexSet::<String>::new();
        let mut streaming = false;
        for method in &rpc_service.method {
            let input_message_symbols =
                rpc_service.enclosing_scope(symbol_table).resolve_proto_message(
                    symbol_table,
                    method.input_type.clone().unwrap().trim_start_matches('.'),
                );
            let input_proto_message = input_message_symbols.first().unwrap();
            let output_message_symbols =
                rpc_service.enclosing_scope(symbol_table).resolve_proto_message(
                    symbol_table,
                    method.output_type.clone().unwrap().trim_start_matches('.'),
                );
            let output_proto_message = output_message_symbols.first().unwrap();

            dependencies
                .insert(input_proto_message.proto_metadata.rust_build_target_name.clone().unwrap());
            dependencies.insert(
                output_proto_message.proto_metadata.rust_build_target_name.clone().unwrap(),
            );

            if method.client_streaming.unwrap_or(false) || method.server_streaming.unwrap_or(false)
            {
                streaming = true;
            };
        }

        let unit_type_snake_case = get_rpc_lib_folder_name(rpc_service.clone());

        Self {
            target_name: get_rpc_target_name(rpc_service, symbol_table),
            crate_name: get_rpc_crate_name(rpc_service, symbol_table),
            folder: unit_type_snake_case,
            dependencies: dependencies.into_iter().collect(),
            is_streaming: streaming,
            service_bundle_package: rpc_service.package(symbol_table),
            unit_type_name: rpc_service.name().clone(),
        }
    }
}

impl Context {
    pub(crate) fn from(
        service_bundle: ServiceBundleSymbol,
        symbol_table: &SymbolTable,
        cli: &Cli,
        main_genrule_name: String,
    ) -> Self {
        let mut skip_codegen = false;
        if let MessageField(Some(build_cfg)) = &service_bundle.build_cfg {
            skip_codegen = build_cfg.skip_codegen.unwrap_or_default();
        }
        Context {
            year: crate::rust::generator::get_current_year(),
            crate_name: Self::get_crate_and_library_name_from_service_bundle(
                service_bundle.clone(),
                symbol_table,
            ),
            dependencies: Self::collect_dependencies(service_bundle.clone(), symbol_table),
            has_diagnostics: service_bundle.diagnostics_info.is_some(),
            has_publisher: !service_bundle.publishers(symbol_table).is_empty(),
            // RPC libs are no longer generated as part of service bundles
            // because unit type ownership is deprecated
            rpc_libs: Vec::new(),
            generate_service_bundle_target: !skip_codegen,
            genrule: cli.genrule,
            catalog_module_name: main_genrule_name,
            service_bundle_file_path: {
                let service_name_snake_case = service_bundle.name.to_snake_case();
                let mut path = PathBuf::new();
                path.extend(service_bundle.package(symbol_table).split('.'));
                path.push(format!("{}/service_bundle.rs", service_name_snake_case));
                path.to_str().unwrap().to_string()
            },
            package_name: service_bundle.package(symbol_table),
            service_bundle_name: service_bundle.name.clone(),
        }
    }

    pub(crate) fn get_crate_and_library_name_from_service_bundle(
        service_bundle: ServiceBundleSymbol,
        symbol_table: &SymbolTable,
    ) -> String {
        let mut custom_crate_name = String::new();
        if let MessageField(Some(build_cfg)) = &service_bundle.build_cfg {
            custom_crate_name = build_cfg.target_name.clone().unwrap_or_default().clone();
        }

        vsidl_v1_parser::get_crate_and_library_name(
            &service_bundle.package(symbol_table),
            &service_bundle.name.clone(),
            &custom_crate_name,
        )
    }

    pub(crate) fn collect_dependencies(
        service_bundle: ServiceBundleSymbol,
        symbol_table: &SymbolTable,
    ) -> Vec<String> {
        let mut dependencies = IndexSet::<String>::new();

        for subscriber in &service_bundle.subscribers(symbol_table) {
            let proto_message = subscriber.message(symbol_table);
            dependencies
                .insert(proto_message.proto_metadata.rust_build_target_name.clone().unwrap());

            // PDK29 compatibility
            if symbol_table.uses_pdk29() {
                let publication = subscriber.publication(symbol_table);
                for proto_dependency in &publication.proto_dependencies {
                    dependencies.insert(proto_dependency.rust_build_target_name.clone().unwrap());
                }
            }
        }

        for publisher in &service_bundle.publishers(symbol_table) {
            let proto_message = publisher.message(symbol_table);
            dependencies
                .insert(proto_message.proto_metadata.rust_build_target_name.clone().unwrap());
            // Dependencies required for registering message metadata used in reflection:
            if service_bundle.ast_node.register_reflection_metadata {
                for proto_dependency in &proto_message.proto_dependencies {
                    dependencies.insert(proto_dependency.rust_build_target_name.clone().unwrap());
                }
            }

            // PDK29 compatibility
            if symbol_table.uses_pdk29() {
                let publication = publisher.publication(symbol_table);
                for proto_dependency in &publication.proto_dependencies {
                    dependencies.insert(proto_dependency.rust_build_target_name.clone().unwrap());
                }
            }
        }

        for rpc_server_symbol in &service_bundle.servers(symbol_table) {
            let rpc_service = rpc_server_symbol.service(symbol_table);
            dependencies.insert(get_rpc_target_name(rpc_service, symbol_table));
        }

        for rpc_client_symbol in &service_bundle.clients(symbol_table) {
            let rpc_service = rpc_client_symbol.service(symbol_table);
            dependencies.insert(get_rpc_target_name(rpc_service, symbol_table));
        }

        if let Some(diagnostics_info) = &service_bundle.diagnostics_info {
            for event in &diagnostics_info.events {
                dependencies
                    .insert(get_diagnostics_message_target_name(&event.extended_data_message));
            }

            for data_item in &diagnostics_info.data_items {
                dependencies.insert(get_diagnostics_message_target_name(&data_item.message));
            }

            for io_control in &diagnostics_info.io_controls {
                dependencies.insert(get_diagnostics_message_target_name(&io_control.message));
            }

            for routine in &diagnostics_info.routines {
                dependencies.insert(get_diagnostics_message_target_name(
                    &routine.start_method_info.request_message,
                ));
                dependencies.insert(get_diagnostics_message_target_name(
                    &routine.start_method_info.response_message,
                ));
                dependencies.insert(get_diagnostics_message_target_name(
                    &routine.stop_method_info.request_message,
                ));
                dependencies.insert(get_diagnostics_message_target_name(
                    &routine.stop_method_info.response_message,
                ));
                dependencies.insert(get_diagnostics_message_target_name(
                    &routine.result_method_info.request_message,
                ));
                dependencies.insert(get_diagnostics_message_target_name(
                    &routine.result_method_info.response_message,
                ));
            }
        }

        dependencies.into_iter().collect()
    }
}

impl RpcContext {
    pub(crate) fn from(
        rpc_service: &ProtoRpcServiceSymbol,
        symbol_table: &SymbolTable,
        cli: &Cli,
        main_genrule_name: String,
    ) -> Self {
        RpcContext {
            year: crate::rust::generator::get_current_year(),
            genrule: cli.genrule,
            catalog_module_name: main_genrule_name,
            rpc_libs: vec![RpcLib::from(rpc_service, symbol_table)],
        }
    }
}

pub(super) fn render(
    service_bundle_symbol: ServiceBundleSymbol,
    symbol_table: &SymbolTable,
    handlebars: &Handlebars,
    cli: &Cli,
    main_genrule_name: String,
) -> String {
    handlebars
        .render(
            "android_bp",
            &Context::from(service_bundle_symbol, symbol_table, cli, main_genrule_name),
        )
        .unwrap()
}

pub(super) fn render_rpc(
    proto_rpc_service: &ProtoRpcServiceSymbol,
    symbol_table: &SymbolTable,
    handlebars: &Handlebars,
    cli: &Cli,
    main_genrule_name: String,
) -> String {
    handlebars
        .render(
            "rpc/android_bp",
            &RpcContext::from(proto_rpc_service, symbol_table, cli, main_genrule_name),
        )
        .unwrap()
}
