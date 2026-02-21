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
//! Removes channel syntax from the AST

use crate::impl_traverse_catalog_mut;
use crate::visitors::visitor_mut::{AstNodeMut, VsidlVisitorMut};
use vsidl_v1::syntax::service_bundle::Publisher;

#[derive(Default)]
pub struct HideChannelSyntax {}
impl_traverse_catalog_mut!(HideChannelSyntax);

impl VsidlVisitorMut for HideChannelSyntax {
    fn start_visit_publisher(&mut self, node: &mut Publisher) {
        node.capacity = 0;
        node.capacity_span.clear();
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_data;
    use crate::visitors::detect_input_version::DetectInputVersion;
    use crate::Catalog;

    #[test]
    fn test_should_hide_channel_syntax() {
        // GIVEN A catalog using channel syntax (capacity)
        let mut catalog = Catalog::new();
        catalog.catalog_vsidl_files.push(test_data::VSIDL_TIRE_PRESSURE_PUBLISHER.clone());

        // PRE-CONDITION: It uses channel syntax
        let version = DetectInputVersion::traverse(&catalog);
        assert!(version.uses_channels);

        // WHEN executing the visitor
        HideChannelSyntax::traverse(&mut catalog);

        // THEN channel syntax is no longer detected
        let version = DetectInputVersion::traverse(&catalog);
        assert!(!version.uses_channels);
    }
}
