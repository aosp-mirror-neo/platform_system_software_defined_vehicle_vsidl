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

use super::types;
use super::types::ProtoMessage;
use crate::message_size::{self, MessageSize, MessageSizeMetadataSet};
use crate::Catalog;
use heck::ToKebabCase;
use indexmap::IndexMap;
use protobuf::MessageField;
use std::collections::HashMap;
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::routine::Method;
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::DataItem;
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::Event;
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::InputOutputControlData;
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::Routine;
use vsidl_v1::diagnostics_syntax::DiagnosticsDeclaration;
use vsidl_v1::syntax::ServiceBundle;

const EMPTY_MESSAGE_NAME: &str = "google.protobuf.Empty";

/// Diagnostics information of the service bundle.
#[derive(Default, Debug, Clone)]
pub struct DiagnosticsInfo {
    declaration: DiagnosticsDeclaration,
    /// Information about service bundle's data items.
    pub data_items: Vec<DataItemInfo>,
    /// Information about service bundle's events.
    pub events: Vec<EventInfo>,
    /// Information about service bundle's IO controls.
    pub io_controls: Vec<InputOutputControlDataInfo>,
    /// Information about service bundle's routines.
    pub routines: Vec<RoutineInfo>,
}

impl DiagnosticsInfo {
    /// Returns raw [DiagnosticsDeclaration] proto message for this [DiagnosticsInfo].
    pub fn as_raw(&self) -> &DiagnosticsDeclaration {
        &self.declaration
    }

    /// Creates [DiagnosticsInfo] from [DiagnosticsContext] and [ServiceBundle].
    ///
    /// Note: This expects an already initialized context to avoid expensive re-scanning
    /// of the catalog for every service bundle.
    pub fn from(
        context: &mut DiagnosticsContext,
        package: &str,
        service_bundle: &ServiceBundle,
    ) -> anyhow::Result<Option<Self>> {
        let Some(diagnostics_declaration) = service_bundle.diagnostics_declaration.as_ref() else {
            return Ok(None);
        };

        let declaration = Self::resolve_message_names(package, diagnostics_declaration);

        Ok(Some(Self {
            data_items: declaration
                .data_item
                .iter()
                .map(|item| DataItemInfo::from(context, item))
                .collect::<Result<_, _>>()?,
            events: declaration
                .event
                .iter()
                .map(|event| EventInfo::from(context, event))
                .collect::<Result<_, _>>()?,
            io_controls: declaration
                .io_control
                .iter()
                .map(|io_control| InputOutputControlDataInfo::from(context, io_control))
                .collect::<Result<_, _>>()?,
            routines: declaration
                .routine
                .iter()
                .map(|routine| RoutineInfo::from(context, routine))
                .collect::<Result<_, _>>()?,
            declaration,
        }))
    }

    fn resolve_message_names(
        package: &str,
        declaration: &DiagnosticsDeclaration,
    ) -> DiagnosticsDeclaration {
        let mut declaration = declaration.clone();

        for data_item in &mut declaration.data_item {
            Self::replace_with_full_message_name(package, &mut data_item.message_name);
        }

        for event in &mut declaration.event {
            Self::replace_with_full_message_name(package, &mut event.extended_data_message_name);
        }

        for io_control in &mut declaration.io_control {
            Self::replace_with_full_message_name(package, &mut io_control.message_name)
        }

        for routine in &mut declaration.routine {
            Self::resolve_message_names_for_method(package, &mut routine.start);
            Self::resolve_message_names_for_method(package, &mut routine.stop);
            Self::resolve_message_names_for_method(package, &mut routine.result);
        }

        declaration
    }

    fn resolve_message_names_for_method(package: &str, method: &mut MessageField<Method>) {
        if let Some(ref mut method) = method.0 {
            Self::replace_with_full_message_name(package, &mut method.request);
            Self::replace_with_full_message_name(package, &mut method.response);
        } else {
            *method = Self::default_routine_method();
        }
    }

    fn default_routine_method() -> MessageField<Method> {
        MessageField::some(Method {
            request: EMPTY_MESSAGE_NAME.to_string(),
            response: EMPTY_MESSAGE_NAME.to_string(),
            ..Default::default()
        })
    }

    fn replace_with_full_message_name(package: &str, message_name: &mut String) {
        if message_name.is_empty() {
            *message_name = EMPTY_MESSAGE_NAME.to_string();
        }

        match message_name.rsplit_once('.') {
            Some(_) => {
                // message_name is already a full message name
            }
            None => *message_name = format!("{package}.{message_name}"),
        };
    }
}

/// Information about data item diagnostics entity.
#[derive(Debug, Clone)]
pub struct DataItemInfo {
    /// Id of the data item.
    pub id: String,
    /// Description of the underlying protobuf message.
    pub message: DiagnosticsProtoMessage,
    /// Whether the data item is writable.
    pub is_writable: bool,
}

impl DataItemInfo {
    /// Creates [DataItemInfo] from [DiagnosticsContext] and [DataItem].
    fn from(context: &mut DiagnosticsContext, data_item_proto: &DataItem) -> anyhow::Result<Self> {
        Ok(Self {
            id: data_item_proto.id.clone(),
            message: context.get_message_by_full_name(&data_item_proto.message_name)?,
            is_writable: data_item_proto.is_writable,
        })
    }
}

/// Information about event diagnostics entity.
#[derive(Debug, Clone)]
pub struct EventInfo {
    /// Id of the event.
    pub id: String,
    /// Description of the underlying protobuf message of this event's extended data.
    pub extended_data_message: DiagnosticsProtoMessage,
    /// Mask to filter fault status updates of the fault related to this event.
    pub fault_listener_mask: u32,
    /// Service unit name of a corresponding event publisher.
    ///
    /// Either user-defined or generated based on event id.
    pub publisher_unit_name: String,
}

impl EventInfo {
    /// Creates [EventInfo] from [DiagnosticsContext] and [Event].
    fn from(context: &mut DiagnosticsContext, event_proto: &Event) -> anyhow::Result<Self> {
        let publisher_unit_name = if event_proto.service_unit_name.is_empty() {
            format!(
                "com-sdv-google-diagnostics-event-event-{}",
                event_proto.id.replace('.', "-").to_kebab_case()
            )
        } else {
            event_proto.service_unit_name.clone()
        };

        Ok(Self {
            id: event_proto.id.clone(),
            extended_data_message: context
                .get_message_by_full_name(&event_proto.extended_data_message_name)?,
            fault_listener_mask: event_proto.fault_listener_mask,
            publisher_unit_name,
        })
    }
}

/// Information about IO control data diagnostics entity.
#[derive(Debug, Clone)]
pub struct InputOutputControlDataInfo {
    /// Id of the IO control data.
    pub id: String,
    /// Description of the underlying protobuf message.
    pub message: DiagnosticsProtoMessage,
}

impl InputOutputControlDataInfo {
    /// Creates [IoControllableDataInfo] from [DiagnosticsContext] and [IoControllableData].
    fn from(
        context: &mut DiagnosticsContext,
        io_control_proto: &InputOutputControlData,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            id: io_control_proto.id.clone(),
            message: context.get_message_by_full_name(&io_control_proto.message_name)?,
        })
    }
}

/// Information about routine diagnostics entity.
#[derive(Debug, Clone)]
pub struct RoutineInfo {
    /// Id of the routine.
    pub id: String,
    /// Information about routine's start method.
    pub start_method_info: MethodInfo,
    /// Information about routine's stop method.
    pub stop_method_info: MethodInfo,
    /// Information about routine's result method.
    pub result_method_info: MethodInfo,
}

impl RoutineInfo {
    /// Creates [RoutineInfo] from [DiagnosticsContext] and [Routine].
    fn from(context: &mut DiagnosticsContext, routine_proto: &Routine) -> anyhow::Result<Self> {
        Ok(Self {
            id: routine_proto.id.clone(),
            start_method_info: MethodInfo::from(context, routine_proto.start.as_ref())?,
            stop_method_info: MethodInfo::from(context, routine_proto.stop.as_ref())?,
            result_method_info: MethodInfo::from(context, routine_proto.result.as_ref())?,
        })
    }
}

/// Information about routine's method.
#[derive(Debug, Clone)]
pub struct MethodInfo {
    /// Description of the underlying protobuf message of the request.
    pub request_message: DiagnosticsProtoMessage,
    /// Description of the underlying protobuf message of the response.
    pub response_message: DiagnosticsProtoMessage,
}

impl MethodInfo {
    /// Creates [MethodInfo] from [DiagnosticsContext] and [Method].
    fn from(
        context: &mut DiagnosticsContext,
        method_proto: Option<&Method>,
    ) -> anyhow::Result<Self> {
        Ok(match method_proto {
            Some(method_proto) => Self {
                request_message: context.get_message_by_full_name(&method_proto.request)?,
                response_message: context.get_message_by_full_name(&method_proto.response)?,
            },
            None => Self {
                request_message: context.get_empty_message()?,
                response_message: context.get_empty_message()?,
            },
        })
    }
}

/// A context object that holds catalog metadata (messages and sizes).
///
/// This should be initialized once and reused across multiple `DiagnosticsInfo::from` calls
/// to improve performance.
pub struct DiagnosticsContext {
    messages: HashMap<String, ProtoMessage>,
    message_sizes_metadata_set: MessageSizeMetadataSet,
    messages_computed_sizes: IndexMap<String, MessageSize>,
}

impl DiagnosticsContext {
    /// Creates a new context by scanning the catalog. This is an expensive operation.
    pub fn new(catalog: &Catalog) -> anyhow::Result<Self> {
        let messages = types::get_all_protobuf_messages(catalog)?;
        let message_sizes_metadata_set = message_size::get_message_size_metadata(
            &catalog.proto_descriptors.iter().map(|(_, d)| d.clone()).collect(),
        )?;

        Ok(Self { messages, message_sizes_metadata_set, messages_computed_sizes: IndexMap::new() })
    }

    fn get_empty_message(&mut self) -> anyhow::Result<DiagnosticsProtoMessage> {
        self.get_message_by_full_name("google.protobuf.Empty")
    }

    fn get_message_by_full_name(
        &mut self,
        full_message_name: &str,
    ) -> anyhow::Result<DiagnosticsProtoMessage> {
        Ok(DiagnosticsProtoMessage {
            proto_message: self
                .messages
                .get(full_message_name)
                .unwrap_or_else(|| panic!("Message {full_message_name} not found"))
                .clone(),
            message_size: message_size::compute_message_size(
                full_message_name,
                &mut self.messages_computed_sizes,
                &self.message_sizes_metadata_set,
            )?,
        })
    }
}

/// Protobuf message information with metadata useful for diagnostics.
#[derive(Debug, Clone)]
pub struct DiagnosticsProtoMessage {
    /// General protobuf message information.
    #[allow(dead_code)]
    pub proto_message: ProtoMessage,
    /// Message size information.
    #[allow(dead_code)]
    pub message_size: MessageSize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_raw() {
        let declaration = DiagnosticsDeclaration {
            data_item: vec![DataItem { id: "test_id".to_string(), ..Default::default() }],
            ..Default::default()
        };
        let diagnostics_info =
            DiagnosticsInfo { declaration: declaration.clone(), ..Default::default() };
        assert_eq!(diagnostics_info.as_raw(), &declaration);
    }

    #[test]
    fn test_method_info_from() {
        // GIVEN A valid diagnostics context
        let catalog = Catalog::new();
        let mut context = DiagnosticsContext::new(&catalog).unwrap();

        // WHEN calling method info from without a method
        let method_info = MethodInfo::from(&mut context, None).unwrap();

        // THEN the request and response messages should be google.protobuf.Empty
        assert_eq!(method_info.request_message.proto_message.package, "google.protobuf");
        assert_eq!(method_info.request_message.proto_message.name, "Empty");
        assert_eq!(method_info.response_message.proto_message.package, "google.protobuf");
        assert_eq!(method_info.response_message.proto_message.name, "Empty");
    }
}
