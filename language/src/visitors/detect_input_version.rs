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

//! Detects PDK29 syntax and new channel syntax

use crate::impl_traverse_catalog;
use crate::visitors::visitor::{AstNode, VsidlVisitor};
use protobuf::descriptor::DescriptorProto;
use vsidl_v1::annotations::SdvPublication;
use vsidl_v1::syntax::service_bundle::Publisher;

#[derive(Default)]
pub struct DetectInputVersion {
    pub uses_pdk29: bool,
    pub uses_channels: bool,
}
impl_traverse_catalog!(DetectInputVersion);

impl VsidlVisitor for DetectInputVersion {
    fn start_visit_sdv_publication(&mut self, _node: &SdvPublication) {
        self.uses_pdk29 = true;
    }

    fn start_visit_proto_single_pub_message(
        &mut self,
        _message: &DescriptorProto,
        _publication: &SdvPublication,
    ) {
        self.uses_pdk29 = true;
    }

    fn start_visit_proto_multi_pub_message(
        &mut self,
        _message: &DescriptorProto,
        _publication: &SdvPublication,
    ) {
        self.uses_pdk29 = true;
    }

    fn start_visit_publisher(&mut self, node: &Publisher) {
        self.uses_channels = self.uses_channels || node.capacity != 0;
    }
}
