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

use super::{impl_context_condition, FindingCollector, FindingId};
use vsidl_v1::span::Span;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;

#[derive(Debug, Default)]
pub struct ProtoMetadata {
    finding_collector: FindingCollector,
}
impl_context_condition!(ProtoMetadata);

impl VsidlVisitor for ProtoMetadata {
    fn start_visit_proto_file(&mut self, node: &vsidl_v1_common::visitors::visitor::ProtoFile) {
        if node.0.target_name.is_none() || node.0.crate_name.is_none() {
            self.finding_collector.error(
                FindingId::ProtoMetadataProtoLibraryDoesNotExist,
                format!("Library name for proto file '{}' is undefined", node.1.name()),
                None,
                vec![Some(Span {
                    file_path: node.0.folder.join(node.1.name()).display().to_string(),
                    ..Default::default()
                })],
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::{FindingSeverity, ProtoMetadata};
    use vsidl_v1_common::Catalog;

    #[test]
    fn test_check_proto_publication_valid_metadata() {
        // GIVEN a Catalog with valid proto_descriptors
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);

        // WHEN checking the proto metadata
        let mut check = ProtoMetadata::default();
        check.check_catalog(&catalog);

        // THEN no findings should be recorded
        assert!(check.is_ok());
    }

    #[test]
    fn test_check_proto_publication_invalid_target_name() {
        // GIVEN a Catalog with proto_descriptors with None target_name
        let mut catalog = Catalog::new();
        let (mut metadata, descriptor) = vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone();
        metadata.target_name = None;
        catalog.proto_descriptors.extend([(metadata, descriptor)]);

        // WHEN checking the proto metadata
        let mut check = ProtoMetadata::default();
        check.check_catalog(&catalog);

        // THEN an error is recorded
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Library name for proto file 'messages.proto' is undefined"
        ));
    }

    #[test]
    fn test_check_proto_publication_invalid_crate_name() {
        // GIVEN a Catalog with proto_descriptors with None crate_name
        let mut catalog = Catalog::new();
        let (mut metadata, descriptor) = vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone();
        metadata.crate_name = None;
        catalog.proto_descriptors.extend([(metadata, descriptor)]);

        // WHEN checking the proto metadata
        let mut check = ProtoMetadata::default();
        check.check_catalog(&catalog);

        // THEN an error is recorded
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Library name for proto file 'messages.proto' is undefined"
        ));
    }

    #[test]
    fn test_check_proto_publication_invalid_crate_name_and_target_name() {
        // GIVEN a Catalog with proto_descriptors with None target_name and crate_name
        let mut catalog = Catalog::new();
        let (mut metadata, descriptor) = vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone();
        metadata.target_name = None;
        metadata.crate_name = None;
        catalog.proto_descriptors.extend([(metadata, descriptor)]);
        catalog.android_bp_files.push(vsidl_v1_test_data::EMPTY_ANDROID_BP.clone());

        // WHEN checking the proto metadata
        let mut check = ProtoMetadata::default();
        check.check_catalog(&catalog);

        // THEN an error is recorded
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "Library name for proto file 'messages.proto' is undefined"
        ));
    }
}
