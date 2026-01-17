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
//! Converts the AST created using old syntax into new syntax

use crate::impl_traverse_catalog_mut;
use crate::visitors::visitor_mut::{AstNodeMut, VsidlVisitorMut};
use vsidl_v1::syntax::service_bundle::{Client, Publisher, Server, Subscriber};
use vsidl_v1::syntax::VsidlEntry;

#[derive(Default)]
pub struct KeywordConverter {}
impl_traverse_catalog_mut!(KeywordConverter);

impl VsidlVisitorMut for KeywordConverter {
    fn start_visit_vsidl_entry(&mut self, node: &mut VsidlEntry) {
        if !node.sdv_service_bundle.is_empty() && node.service_bundle.is_empty() {
            node.service_bundle = node.sdv_service_bundle.clone();
        }
    }

    fn start_visit_publisher(&mut self, node: &mut Publisher) {
        if !node.type_.is_empty() && node.message.is_empty() {
            node.message = node.type_.clone();
        }

        if !node.instance.is_empty() && node.channel.is_empty() {
            node.channel = node.instance.clone();
        }
    }

    fn start_visit_subscriber(&mut self, node: &mut Subscriber) {
        if !node.type_.is_empty() && node.message.is_empty() {
            node.message = node.type_.clone();
        }

        if !node.instance.is_empty() && node.channel.is_empty() {
            node.channel = node.instance.clone();
        }
    }

    fn start_visit_server(&mut self, node: &mut Server) {
        if !node.interface.is_empty() && node.service.is_empty() {
            node.service = node.interface.clone();
        }
    }

    fn start_visit_client(&mut self, node: &mut Client) {
        if !node.interface.is_empty() && node.service.is_empty() {
            node.service = node.interface.clone();
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_data;
    use crate::Catalog;

    #[test]
    fn test_should_convert_deprecated_keywords() {
        // GIVEN A catalog using the old keywords
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog.catalog_vsidl_files.push(test_data::VSIDL_DEPRECATED_KEYWORDS.clone());

        // WHEN executing the visitor
        KeywordConverter::traverse(&mut catalog);

        // THEN the data is available using the new keywords
        let bundle1 = &catalog.catalog_vsidl_files[0].1.service_bundle[0];
        let bundle2 = &catalog.catalog_vsidl_files[0].1.service_bundle[1];
        let service_bundle_one =
            if bundle1.name == "ServiceBundleOne" { bundle1.clone() } else { bundle2.clone() };
        let service_bundle_two =
            if bundle1.name == "ServiceBundleTwo" { bundle1.clone() } else { bundle2.clone() };
        assert_eq!(service_bundle_one.publisher[0].message, "Pressure");
        assert_eq!(service_bundle_one.publisher[1].message, "Pressure");
        assert_eq!(service_bundle_one.publisher[0].channel[0], "ONE");
        assert_eq!(service_bundle_one.publisher[1].channel[0], "TWO");
        assert_eq!(service_bundle_one.client[0].service, "DummyRpc");
        assert_eq!(service_bundle_one.client[1].service, "AnotherDummyRpc");
        assert_eq!(service_bundle_two.subscriber[0].message, "Pressure");
        assert_eq!(service_bundle_two.subscriber[0].channel[0], "TWO");
        assert_eq!(service_bundle_two.server[0].service, "DummyRpc");
        assert_eq!(service_bundle_two.server[1].service, "AnotherDummyRpc");
    }
}
