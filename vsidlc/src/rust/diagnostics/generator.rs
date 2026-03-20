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
use handlebars::Handlebars;
use serde_derive::Serialize;
use vsidl_v1_parser::{
    service_bundle_symbol::ServiceBundleSymbol, DataItemInfo, DiagnosticsInfo,
    DiagnosticsProtoMessage, EventInfo, InputOutputControlDataInfo, MethodInfo, RoutineInfo,
    SymbolTable,
};

#[derive(Serialize)]
struct EventContext<'a> {
    id: &'a String,
    full_message_name: String,
    publisher_field_name: String,
    payload_size: usize,
    service_unit_name: &'a String,
    topic_name: Option<&'a String>,
    has_fault_listener: bool,
    fault_listener_handler_name: String,
}

impl<'a> EventContext<'a> {
    pub(self) fn new(event_info: &'a EventInfo, symbol_table: &SymbolTable) -> Self {
        EventContext {
            id: &event_info.id,
            full_message_name: full_message_name(&event_info.extended_data_message),
            publisher_field_name: format!("{}_event_publisher", to_rust_safe(&event_info.id)),
            payload_size: event_info.extended_data_message.message_size.size,
            service_unit_name: &event_info.publisher_unit_name,
            topic_name: if symbol_table.uses_pdk29() { None } else { Some(&event_info.topic) },
            has_fault_listener: event_info.fault_listener_mask != 0,
            fault_listener_handler_name: to_rust_safe_handler_name(&event_info.id),
        }
    }
}

#[derive(Serialize)]
struct DataItemContext<'a> {
    id: &'a String,
    message: String,
    handler_type: &'static str,
    handler_name: String,
    is_writable: bool,
}

impl<'a> DataItemContext<'a> {
    pub(self) fn from(data_item_info: &'a DataItemInfo) -> Self {
        let is_writable = data_item_info.is_writable;

        DataItemContext {
            id: &data_item_info.id,
            message: full_message_name(&data_item_info.message),
            handler_type: if is_writable { "DataItemHandler" } else { "ReadOnlyDataItemHandler" },
            handler_name: to_rust_safe_handler_name(&data_item_info.id),
            is_writable,
        }
    }
}

#[derive(Serialize)]
struct RoutineControlContext<'a> {
    id: &'a String,
    handler_name: String,
    start: MethodContext,
    stop: MethodContext,
    result: MethodContext,
}

impl<'a> RoutineControlContext<'a> {
    pub(self) fn from(routine_info: &'a RoutineInfo) -> Self {
        Self {
            id: &routine_info.id,
            handler_name: to_rust_safe_handler_name(&routine_info.id),
            start: MethodContext::from(&routine_info.start_method_info),
            stop: MethodContext::from(&routine_info.stop_method_info),
            result: MethodContext::from(&routine_info.result_method_info),
        }
    }
}

#[derive(Serialize)]
struct MethodContext {
    request: String,
    response: String,
}

impl MethodContext {
    pub(self) fn from(method_info: &MethodInfo) -> Self {
        Self {
            request: full_message_name(&method_info.request_message),
            response: full_message_name(&method_info.response_message),
        }
    }
}

#[derive(Serialize)]
struct IoControlContext<'a> {
    id: &'a String,
    message: String,
    handler_name: String,
}

impl<'a> IoControlContext<'a> {
    pub(self) fn from(io_control_info: &'a InputOutputControlDataInfo) -> Self {
        Self {
            id: &io_control_info.id,
            message: full_message_name(&io_control_info.message),
            handler_name: to_rust_safe_handler_name(&io_control_info.id),
        }
    }
}

#[derive(Serialize)]
struct DiagnosticsContext<'a> {
    year: i32,
    service_bundle_name: &'a str,
    events: Vec<EventContext<'a>>,
    data_items: Vec<DataItemContext<'a>>,
    routine_controls: Vec<RoutineControlContext<'a>>,
    io_controls: Vec<IoControlContext<'a>>,
    has_fault_listeners: bool,
    register_reflection_metadata: bool,
}

impl<'a> DiagnosticsContext<'a> {
    pub(self) fn from_symbol(
        service_bundle_symbol: &'a ServiceBundleSymbol,
        symbol_table: &SymbolTable,
    ) -> Self {
        Self::from_internal(
            &service_bundle_symbol.name,
            service_bundle_symbol.diagnostics_info.as_ref(),
            service_bundle_symbol.register_reflection_metadata,
            symbol_table,
        )
    }

    pub(self) fn from_internal(
        service_bundle_name: &'a str,
        diagnostics_info: Option<&'a DiagnosticsInfo>,
        register_reflection_metadata: bool,
        symbol_table: &SymbolTable,
    ) -> Self {
        let events: Vec<_> = diagnostics_info
            .as_ref()
            .map(|info| info.events.iter().map(|e| EventContext::new(e, symbol_table)).collect())
            .unwrap_or_default();

        let has_fault_listeners = events.iter().any(|event| event.has_fault_listener);

        Self {
            year: crate::rust::generator::get_current_year(),
            service_bundle_name,
            events,
            data_items: diagnostics_info
                .as_ref()
                .map(|info| info.data_items.iter().map(DataItemContext::from).collect())
                .unwrap_or_default(),
            routine_controls: diagnostics_info
                .as_ref()
                .map(|info| info.routines.iter().map(RoutineControlContext::from).collect())
                .unwrap_or_default(),
            io_controls: diagnostics_info
                .as_ref()
                .map(|info| info.io_controls.iter().map(IoControlContext::from).collect())
                .unwrap_or_default(),
            has_fault_listeners,
            register_reflection_metadata,
        }
    }
}

pub fn render_symbol(
    service_bundle: ServiceBundleSymbol,
    symbol_table: &SymbolTable,
    handlebars: &Handlebars,
) -> String {
    render_diagnostics_context(
        &DiagnosticsContext::from_symbol(&service_bundle, symbol_table),
        handlebars,
    )
}

fn render_diagnostics_context(
    diagnostics_context: &DiagnosticsContext,
    handlebars: &Handlebars,
) -> String {
    handlebars.render("diagnostics", diagnostics_context).unwrap()
}

fn full_message_name(message: &DiagnosticsProtoMessage) -> String {
    let proto_metadata = &message.proto_message.proto_metadata;

    format!(
        "{}::{}::{}",
        proto_metadata.rust_crate_name.clone().unwrap(),
        extract_filename(&proto_metadata.filename),
        &message.proto_message.name,
    )
}

fn to_rust_safe(id: &str) -> String {
    id.to_lowercase().replace(".", "_").replace("-", "_")
}

fn to_rust_safe_handler_name(id: &str) -> String {
    format!("{}_handler", to_rust_safe(id))
}
