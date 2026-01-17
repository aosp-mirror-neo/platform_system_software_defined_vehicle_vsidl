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

use crate::rust::generator::extract_filename;
use crate::rust::service_bundle::UnitTypeContext;
use handlebars::Handlebars;
use protobuf::descriptor::MethodDescriptorProto;
use serde_derive::Serialize;
use vsidl_v1_parser::proto_message_symbol::ProtoMessageSymbol;
use vsidl_v1_parser::proto_rpc_service_symbol::ProtoRpcServiceSymbol;
use vsidl_v1_parser::SymbolTable;

#[derive(Serialize, Hash, Eq, PartialEq, Clone)]
pub(crate) struct MessageContext {
    pub(crate) crate_name: String,
    pub(crate) file_name: String,
    pub(crate) message_name: String,
}

#[derive(Serialize, Hash, Eq, PartialEq, Clone)]
pub(crate) struct RpcMethodContext {
    pub(crate) name: String,
    pub(crate) request: MessageContext,
    pub(crate) response: MessageContext,
    uses_client_streaming: bool,
    uses_server_streaming: bool,
}

#[derive(Serialize)]
struct Context {
    year: i32,
    service_name: String,
    unit_name: String,
    /// the package of the service, e.g., "sdv.vsidl.rpc.test"
    service_package: String,
    /// the type name of the service, e.g., "TireStatus"
    service_type: String,
    methods: Vec<RpcMethodContext>,
    is_using_streaming: bool,
}

impl MessageContext {
    pub(crate) fn from(proto_message: ProtoMessageSymbol, _symbol_table: &SymbolTable) -> Self {
        Self {
            message_name: proto_message.name.clone().unwrap(),
            crate_name: proto_message.proto_metadata.rust_crate_name.unwrap(),
            file_name: extract_filename(&proto_message.proto_metadata.filename.to_string())
                .to_string(),
        }
    }
}

impl RpcMethodContext {
    pub(crate) fn from(
        method: MethodDescriptorProto,
        proto_rpc_service: ProtoRpcServiceSymbol,
        symbol_table: &SymbolTable,
    ) -> Self {
        let input_message_symbols =
            proto_rpc_service.enclosing_scope(symbol_table).resolve_proto_message(
                symbol_table,
                method.input_type.unwrap().trim_start_matches('.'),
            );
        let input_message_symbol = input_message_symbols.first().unwrap();
        let output_message_symbols =
            proto_rpc_service.enclosing_scope(symbol_table).resolve_proto_message(
                symbol_table,
                method.output_type.unwrap().trim_start_matches('.'),
            );
        let output_message_symbol = output_message_symbols.first().unwrap();

        Self {
            name: method.name.clone().unwrap(),
            request: MessageContext::from(input_message_symbol.clone(), symbol_table),
            response: MessageContext::from(output_message_symbol.clone(), symbol_table),
            uses_client_streaming: proto_rpc_service
                .method_uses_client_streaming(method.name.clone().unwrap()),
            uses_server_streaming: proto_rpc_service
                .method_uses_server_streaming(method.name.unwrap()),
        }
    }
}

impl Context {
    pub(self) fn from(
        proto_rpc_service: &ProtoRpcServiceSymbol,
        symbol_table: &SymbolTable,
    ) -> Self {
        let unit_type_context = UnitTypeContext::from_rpc_service(proto_rpc_service, symbol_table);

        Context {
            year: crate::rust::generator::get_current_year(),
            service_name: proto_rpc_service.name().clone(),
            unit_name: vsidl_v1_parser::get_rpc_unit_name(&format!(
                "{}.{}",
                unit_type_context.package, unit_type_context.type_name,
            ))
            .expect("invalid RPC unit name"),
            service_package: unit_type_context.package,
            service_type: unit_type_context.type_name,
            methods: unit_type_context.methods,
            is_using_streaming: proto_rpc_service.method.iter().any(|method| {
                method.client_streaming.unwrap_or(false) || method.server_streaming.unwrap_or(false)
            }),
        }
    }
}

pub(super) fn render(
    proto_rpc_service: &ProtoRpcServiceSymbol,
    symbol_table: &SymbolTable,
    handlebars: &Handlebars,
) -> String {
    handlebars.render("rpc_service", &Context::from(proto_rpc_service, symbol_table)).unwrap()
}
