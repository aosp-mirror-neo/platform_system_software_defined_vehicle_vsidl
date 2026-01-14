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

use super::{impl_context_condition_symtab, FindingCollector, FindingId};
use vsidl_v1_common::name_to_fqn;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::symbol::SymbolKind;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

#[derive(Debug, Default)]
pub struct SdvPublication {
    finding_collector: FindingCollector,
    package: String,
}
impl_context_condition_symtab!(SdvPublication);

impl SdvPublication {
    fn check_message_count(
        &mut self,
        message: &protobuf::descriptor::DescriptorProto,
        publication: &vsidl_v1::annotations::SdvPublication,
    ) {
        let message_fqn = name_to_fqn(&message.name.clone().unwrap(), &self.package);

        // Check message count field
        if publication.message_count < 2 || !publication.message_count.is_multiple_of(2) {
            self.finding_collector.error(
                FindingId::SdvPublicationMessageCountPositive,
                format!("Message count must be set, >= 2, and even for: {message_fqn}"),
                Vec::new(),
            );
        }
    }
}

impl VsidlVisitor for SdvPublication {
    fn start_visit_proto_file(&mut self, node: &vsidl_v1_common::visitors::visitor::ProtoFile) {
        self.package = node.1.package().to_string();
    }

    fn start_visit_proto_single_pub_message(
        &mut self,
        message: &protobuf::descriptor::DescriptorProto,
        publication: &vsidl_v1::annotations::SdvPublication,
    ) {
        self.check_message_count(message, publication);
    }

    fn start_visit_proto_multi_pub_message(
        &mut self,
        message: &protobuf::descriptor::DescriptorProto,
        publication: &vsidl_v1::annotations::SdvPublication,
    ) {
        self.check_message_count(message, publication);

        let message_fqn = name_to_fqn(&message.name.clone().unwrap(), &self.package);
        let has_channels_enum =
            !publication.instances_enum.is_empty() || !publication.channels_enum.is_empty();
        if has_channels_enum {
            self.finding_collector.error(
                FindingId::SdvPublicationMultiPubWithChannels,
                format!("Channels for multipub types are not allowed: {message_fqn}"),
                Vec::new(),
            );
        }
    }
}

impl SymbolTableChecker for SdvPublication {
    fn check_proto_publication(
        &mut self,
        symbol_table: &vsidl_v1_symbols::SymbolTable,
        publication: &vsidl_v1_symbols::proto_publication_symbol::ProtoPublicationSymbol,
    ) {
        // Get name of the channels enum
        let channels_enum_name = publication.channels_enum_name();

        // If there's no channels enum (i.e. implicitly the UNIQUE channel), there's nothing to check
        if channels_enum_name.is_none() || channels_enum_name.clone().unwrap().is_empty() {
            return;
        }

        if let Some(channels_enum_name) = channels_enum_name {
            let enclosing_scope_id = &publication.symbol.enclosing_scope;

            // Resolve the enum name
            let resolved_symbols: Vec<_> = symbol_table
                .resolve(enclosing_scope_id, &channels_enum_name, false)
                .into_iter()
                .filter(|symbol| matches!(symbol, SymbolKind::ProtoEnum(_)))
                .collect();

            // ERROR: Enum is unknown
            if resolved_symbols.is_empty() {
                self.finding_collector.error(
                    FindingId::SdvPublicationChannelsEnumUndefined,
                    format!(
                        "channels_enum '{}' of Proto message '{}' is undefined",
                        channels_enum_name,
                        publication.full_name(symbol_table)
                    ),
                    vec![publication.span()],
                );
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::sdv_publication::SdvPublication;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;

    #[test]
    fn test_should_reject_singlepub_publication_without_message_count() {
        // GIVEN A catalog with an SDV publication without a model
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_INVALID_SDV_PUBLICATION.clone()]);

        // WHEN Executing context condition
        let mut check = SdvPublication::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.is_err());
        assert!(check
            .any_finding_contains(FindingSeverity::Error, "sdv.test.MissingMessageCountSinglePub"));
    }

    #[test]
    fn test_should_reject_singlepub_publication_with_zero_message_count() {
        // GIVEN A catalog with an SDV publication without a model
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_INVALID_SDV_PUBLICATION.clone()]);

        // WHEN Executing context condition
        let mut check = SdvPublication::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.is_err());
        assert!(check
            .any_finding_contains(FindingSeverity::Error, "sdv.test.ZeroMessageCountSinglePub"));
    }

    #[test]
    fn test_should_reject_singlepub_publication_with_uneven_message_count() {
        // GIVEN A catalog with an SDV publication without a model
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_INVALID_SDV_PUBLICATION.clone()]);

        // WHEN Executing context condition
        let mut check = SdvPublication::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.is_err());
        assert!(check
            .any_finding_contains(FindingSeverity::Error, "sdv.test.UnevenMessageCountSinglePub"));
    }

    #[test]
    fn test_should_reject_multipub_publication_without_message_count() {
        // GIVEN A catalog with an SDV publication without a model
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_INVALID_SDV_PUBLICATION.clone()]);

        // WHEN Executing context condition
        let mut check = SdvPublication::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.is_err());
        assert!(check
            .any_finding_contains(FindingSeverity::Error, "sdv.test.MissingMessageCountMultiPub"));
    }

    #[test]
    fn test_should_reject_undefined_channels_enum() {
        // GIVEN A catalog with an SDV publication without a model
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_INVALID_SDV_PUBLICATION.clone()]);
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN Executing context condition
        let mut check = SdvPublication::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN Should have found error
        assert!(check.is_err());
        assert!(
            check.any_finding_contains(FindingSeverity::Error, "sdv.test.UndefinedChannelsEnum")
        );
    }

    #[test]
    fn test_should_reject_multipub_publication_with_zero_message_count() {
        // GIVEN A catalog with an SDV publication without a model
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_INVALID_SDV_PUBLICATION.clone()]);

        // WHEN Executing context condition
        let mut check = SdvPublication::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.is_err());
        assert!(
            check.any_finding_contains(FindingSeverity::Error, "sdv.test.ZeroMessageCountMultiPub")
        );
    }

    #[test]
    fn test_should_reject_multipub_publication_with_uneven_message_count() {
        // GIVEN A catalog with an SDV publication without a model
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_INVALID_SDV_PUBLICATION.clone()]);

        // WHEN Executing context condition
        let mut check = SdvPublication::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.is_err());
        assert!(check
            .any_finding_contains(FindingSeverity::Error, "sdv.test.UnevenMessageCountMultiPub"));
    }

    #[test]
    fn test_should_reject_multipub_publication_with_channels_enum() {
        // GIVEN A catalog with an SDV publication without a model
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_INVALID_SDV_PUBLICATION.clone()]);

        // WHEN Executing context condition
        let mut check = SdvPublication::default();
        check.check_catalog(&catalog);

        // THEN Should have found error
        assert!(check.is_err());
        assert!(check.any_finding_contains(FindingSeverity::Error, "sdv.test.MultiPubWithChannels"));
    }

    #[test]
    fn test_should_accept_valid_service_bundle() {
        // GIVEN A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([
            vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_ANOTHER_DUMMY_RPC_PROTO.clone(),
            vsidl_v1_test_data::DESCRIPTOR_DIAGNOSTICS_PROTO.clone(),
        ]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_TWO_BUNDLES_VALID.clone());

        // WHEN Executing context condition
        let mut check = SdvPublication::default();
        check.check_catalog(&catalog);

        // THEN Should not have found any errors
        assert!(check.is_ok());
    }
}
