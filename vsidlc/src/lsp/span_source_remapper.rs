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

use crate::lsp::shadow_workspace::ShadowWorkspace;
use protobuf::MessageField;
use vsidl_v1::syntax::service_bundle::{Client, Publisher, Server, Subscriber};
use vsidl_v1::syntax::{BuildConfiguration, ServiceBundle, Span, VsidlEntry};
use vsidl_v1_parser::visitors::visitor_mut::VsidlVisitorMut;
use vsidl_v1_parser::Catalog;

/// A visitor that post-processes AST nodes to translate file paths within `Span` objects.
///
/// When the parser runs against a `ShadowWorkspace`, the resulting `Span` locations
/// point to temporary files in the shadow directory. This struct traverses the AST
/// and maps those temporary paths back to their original source locations.
///
/// This ensures that error messages, "go-to-definition" features, and other IDE
/// integrations report paths that the user recognizes and can edit.
#[derive(Default)]
pub(crate) struct SpanSourceRemapper<'a> {
    /// The workspace used to perform the path translation. If `None`, no
    /// transformations are applied.
    pub(crate) shadow_workspace: Option<&'a ShadowWorkspace>,
}

impl<'a> SpanSourceRemapper<'a> {
    fn update_message_field_span(&self, span: &mut MessageField<Span>) {
        if let Some(span) = span.as_mut() {
            if let Some(shadow) = self.shadow_workspace {
                span.file_path = shadow.map_to_source(&span.file_path);
            }
        }
    }

    fn update_spans(&self, spans: &mut Vec<Span>) {
        if let Some(shadow) = self.shadow_workspace {
            for span in spans {
                span.file_path = shadow.map_to_source(&span.file_path);
            }
        }
    }
}

impl<'a> VsidlVisitorMut for SpanSourceRemapper<'a> {
    fn start_visit_catalog(&mut self, catalog: &mut Catalog) {
        if let Some(shadow) = self.shadow_workspace {
            for desc in &mut catalog.proto_descriptors {
                let folder_str = desc.0.folder.to_str().expect("Path contains invalid UTF-8");
                desc.0.folder = shadow.map_to_source(folder_str).into();
            }
        }
    }

    fn start_visit_vsidl_entry(&mut self, node: &mut VsidlEntry) {
        self.update_message_field_span(&mut node.span);
        self.update_message_field_span(&mut node.package_span);
    }

    fn start_visit_service_bundle(&mut self, node: &mut ServiceBundle) {
        self.update_message_field_span(&mut node.span);
        self.update_message_field_span(&mut node.name_span);
        self.update_message_field_span(&mut node.register_reflection_metadata_span);
    }

    fn start_visit_publisher(&mut self, node: &mut Publisher) {
        self.update_message_field_span(&mut node.span);
        self.update_message_field_span(&mut node.message_span);
        self.update_spans(&mut node.channel_span);
        self.update_spans(&mut node.topic_span);
        self.update_message_field_span(&mut node.capacity_span);
        self.update_message_field_span(&mut node.service_unit_name_span);
    }

    fn start_visit_subscriber(&mut self, node: &mut Subscriber) {
        self.update_message_field_span(&mut node.span);
        self.update_message_field_span(&mut node.message_span);
        self.update_spans(&mut node.channel_span);
        self.update_spans(&mut node.topic_span);
    }

    fn start_visit_server(&mut self, node: &mut Server) {
        self.update_message_field_span(&mut node.span);
        self.update_message_field_span(&mut node.service_span);
        self.update_message_field_span(&mut node.channel_span);
        self.update_message_field_span(&mut node.service_unit_name_span);
    }

    fn start_visit_client(&mut self, node: &mut Client) {
        self.update_message_field_span(&mut node.span);
        self.update_message_field_span(&mut node.service_span);
        self.update_message_field_span(&mut node.channel_span);
    }

    fn start_visit_build_cfg(&mut self, node: &mut BuildConfiguration) {
        self.update_message_field_span(&mut node.span);
        self.update_message_field_span(&mut node.target_name_span);
        self.update_message_field_span(&mut node.skip_codegen_span);
    }
}
