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
//! Removes PDK29 syntax from the AST

use crate::impl_traverse_catalog_mut;
use crate::visitors::visitor_mut::{AstNodeMut, VsidlVisitorMut};
use protobuf::descriptor::MessageOptions;

#[derive(Default)]
pub struct HidePdk29Syntax {}
impl_traverse_catalog_mut!(HidePdk29Syntax);

impl VsidlVisitorMut for HidePdk29Syntax {
    fn start_visit_proto_message_options(&mut self, node: &mut MessageOptions) {
        // Field 50001 is the SdvPublication extension on MessageOptions
        node.special_fields.mut_unknown_fields().remove(50001);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_data;
    use crate::visitors::detect_input_version::DetectInputVersion;
    use crate::Catalog;

    #[test]
    fn test_should_hide_pdk29_syntax() {
        // GIVEN A catalog using PDK29 syntax
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);

        // PRE-CONDITION: It uses PDK29 syntax
        let version = DetectInputVersion::traverse(&catalog);
        assert!(version.uses_pdk29);

        // WHEN executing the visitor
        HidePdk29Syntax::traverse(&mut catalog);

        // THEN PDK29 syntax is no longer detected
        let version = DetectInputVersion::traverse(&catalog);
        assert!(!version.uses_pdk29);
    }
}
