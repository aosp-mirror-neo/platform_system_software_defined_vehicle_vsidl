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

use super::{FindingCollector, FindingId};
use crate::impl_context_condition_symtab;
use heck::ToKebabCase;
use sdv_comms_id_validator::service_unit_name_validation::{
    validate_length, SERVICE_UNIT_NAME_MAX_LENGTH_BYTES,
};
use vsidl_v1::diagnostics_syntax::diagnostics_declaration::Event;
use vsidl_v1::span::Span;
use vsidl_v1::syntax::service_bundle::{Publisher, Server, Subscriber};
use vsidl_v1::syntax::VsidlEntry;
use vsidl_v1_common::name_to_fqn;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::specific_symbols::proto_message_symbol::ProtoMessageSymbol;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;

#[derive(Debug, Default)]
pub struct ServiceUnitNameTooLong {
    finding_collector: FindingCollector,
    package: String,
}
impl_context_condition_symtab!(ServiceUnitNameTooLong);

/// Checks that the name of a service unit is not exceeding the internal limit
impl ServiceUnitNameTooLong {
    fn check(&mut self, service_unit_name: String, node_type: &str, span: Option<Span>) {
        let result = validate_length(&service_unit_name);

        if result.is_err() {
            self.finding_collector.error(
                FindingId::ServiceUnitNameTooLong,
                format!(
                    "Service unit name for {node_type} '{service_unit_name}' exceeds {SERVICE_UNIT_NAME_MAX_LENGTH_BYTES} chars"
                ),
                vec![span],
            );
        }
    }
}

impl VsidlVisitor for ServiceUnitNameTooLong {
    fn start_visit_vsidl_entry(&mut self, node: &VsidlEntry) {
        self.package = node.package.clone();
    }

    fn start_visit_publisher(&mut self, node: &Publisher) {
        // if there's no service unit name and topics are not defined, length check is covered in check_publisher()
        if node.service_unit_name.is_empty() && !node.topic.is_empty() {
            // Validate generated name
            for (index, topic) in node.topic.iter().enumerate() {
                let generated_name = topic.to_string().to_kebab_case();
                self.check(generated_name, "publisher", node.topic_span.get(index).cloned());
            }
        } else if !node.service_unit_name.is_empty() {
            // Validate user-defined name
            self.check(
                node.service_unit_name.clone(),
                "publisher",
                node.service_unit_name_span.clone().into_option(),
            );
        }
    }

    fn start_visit_subscriber(&mut self, node: &Subscriber) {
        // if there are no explicit topics, length check is covered in check_subscriber()
        if !node.topic.is_empty() {
            // Validate generated name
            for (index, topic) in node.topic.iter().enumerate() {
                let generated_name = topic.to_string().to_kebab_case();
                self.check(generated_name, "subscriber", node.topic_span.get(index).cloned());
            }
        }
    }

    fn start_visit_server(&mut self, node: &Server) {
        if node.service_unit_name.is_empty() {
            // Validate generated name
            let generated_name =
                name_to_fqn(&node.service, &self.package).replace('.', "-").to_kebab_case();
            self.check(generated_name, "server", node.service_span.clone().into_option());
        } else {
            // Validate user-defined name
            self.check(
                node.service_unit_name.clone(),
                "server",
                node.service_unit_name_span.clone().into_option(),
            );
        }
    }

    fn start_visit_diagnostics_event(&mut self, node: &Event) {
        if node.service_unit_name.is_empty() {
            // Check generated name
            self.check(
                format!(
                    "com-android-sdv-diagnostics-event-event-{}",
                    node.id.replace('.', "-").to_kebab_case()
                ),
                "diagnostics event",
                None,
            );
        } else {
            // Check user-defined name
            self.check(node.service_unit_name.clone(), "diagnostics event", None);
        }
    }
}

impl SymbolTableChecker for ServiceUnitNameTooLong {
    fn check_publisher(
        &mut self,
        symbol_table: &vsidl_v1_symbols::symbol_table::SymbolTable,
        publisher: &vsidl_v1_symbols::specific_symbols::publisher_symbol::PublisherSymbol,
    ) {
        // User-defined names are checked in start_visit_publisher()
        if !publisher.service_unit_name.is_empty() {
            return;
        }

        // Explicit topics are checked in start_visit_publisher()
        if !publisher.topic.is_empty() {
            return;
        }

        // Get message symbol
        let proto_message_symbols = publisher
            .enclosing_scope(symbol_table)
            .resolve_proto_message(symbol_table, publisher.message.clone());
        if proto_message_symbols.is_empty() {
            // This error is handled by the publication_type context condition
            return;
        }
        let proto_message_symbol: ProtoMessageSymbol = proto_message_symbols[0].clone();
        let topics = proto_message_symbol.topics(symbol_table);

        // Validate generated name
        for topic in &topics {
            let generated_name = topic.name().to_kebab_case();
            self.check(
                generated_name,
                "publisher",
                publisher.ast_node.message_span.clone().into_option(),
            );
        }
    }

    fn check_subscriber(
        &mut self,
        symbol_table: &vsidl_v1_symbols::symbol_table::SymbolTable,
        subscriber: &vsidl_v1_symbols::specific_symbols::subscriber_symbol::SubscriberSymbol,
    ) {
        // Explicit topics are checked in start_visit_subscriber()
        if !subscriber.topic.is_empty() {
            return;
        }

        // Get message symbol
        let proto_message_symbols = subscriber
            .enclosing_scope(symbol_table)
            .resolve_proto_message(symbol_table, subscriber.message.clone());
        if proto_message_symbols.is_empty() {
            // This error is handled by the subscriber_type context condition
            return;
        }
        let proto_message_symbol: ProtoMessageSymbol = proto_message_symbols[0].clone();
        let topics = proto_message_symbol.topics(symbol_table);

        // Validate generated name
        for topic in &topics {
            let generated_name = topic.name().to_kebab_case();
            self.check(
                generated_name,
                "subscriber",
                subscriber.ast_node.message_span.clone().into_option(),
            );
        }
    }

    fn check_rpc_client(
        &mut self,
        symbol_table: &vsidl_v1_symbols::symbol_table::SymbolTable,
        rpc_client: &vsidl_v1_symbols::specific_symbols::rpc_client_symbol::RpcClientSymbol,
    ) {
        // Validate user-defined name
        let service = rpc_client.service(symbol_table);
        if !service.servers(symbol_table).is_empty() {
            let server = service.servers(symbol_table)[0].clone();
            if server.has_user_defined_name() {
                self.check(
                    server.service_unit_name(symbol_table),
                    "client",
                    rpc_client.ast_node.service_span.clone().into_option(),
                );
                return;
            }
        }

        // Validate generated name
        let fqn = name_to_fqn(&service.name(), &service.package(symbol_table));
        let generated_name = fqn.replace('.', "-").to_kebab_case();
        self.check(
            generated_name,
            "client",
            rpc_client.ast_node.service_span.clone().into_option(),
        );
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::service_unit_name_too_long::ServiceUnitNameTooLong;
    use crate::FindingSeverity;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;
    use vsidl_v1_symbols::TargetApi;

    #[test]
    fn test_should_reject_long_user_defined_name() {
        // Given: A catalog with a publisher that has two topics and a user-defined name
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_NAME_TOO_LONG_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_USER_DEFINED_NAME_TOO_LONG.clone());

        // When: Executing context condition
        let mut coco = ServiceUnitNameTooLong::default();
        coco.check_catalog(&catalog);

        // Then: context condition should fail
        assert!(coco.is_err());
        assert!(coco.any_finding_contains(FindingSeverity::Error, "ThisIsAVeryLongPublisherNameThatExceedsOurLimits012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789"));
        assert!(coco.any_finding_contains(FindingSeverity::Error, "ThisIsAVeryLongRpcNameThatExceedsOurLimits012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789"));
    }

    #[test]
    fn test_should_reject_long_generated_name() {
        // GIVEN A catalog with service units that will generate names that are too long
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_NAME_TOO_LONG_PROTO.clone()]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_GENERATED_NAME_TOO_LONG.clone());

        let mut creator: SymbolTableCreator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker: SymbolTableLinker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut coco = ServiceUnitNameTooLong::default();
        coco.check_catalog(&catalog);
        coco.check_symbol_table(linker.symbol_table());

        // THEN context condition should fail
        assert!(coco.is_err());
        eprintln!("Findings: {:?}", coco.finding_collector.findings());

        // THEN publisher with implicit topics was detected
        assert!(coco.any_finding_contains(FindingSeverity::Error, "for publisher 'sdv-test-this-is-another-very-long-publisher-name-that-exceeds-our-limits01234567890123456789012345678901234567890123456789012345678901234567890123456789-three"));
        assert!(coco.any_finding_contains(FindingSeverity::Error, "for publisher 'sdv-test-this-is-another-very-long-publisher-name-that-exceeds-our-limits01234567890123456789012345678901234567890123456789012345678901234567890123456789-four"));
        // THEN subscriber with explicit topics was detected
        assert!(coco.any_finding_contains(FindingSeverity::Error, "for subscriber 'sdv-test-this-is-a-very-long-subscriber-name-that-exceeds-our-limits01234567890123456789012345678901234567890123456789012345678901234567890123456789-five"));
        // THEN subscriber with implicit topics was detected
        assert!(coco.any_finding_contains(FindingSeverity::Error, "for subscriber 'sdv-test-this-is-another-very-long-publisher-name-that-exceeds-our-limits01234567890123456789012345678901234567890123456789012345678901234567890123456789-three"));
        assert!(coco.any_finding_contains(FindingSeverity::Error, "for subscriber 'sdv-test-this-is-another-very-long-publisher-name-that-exceeds-our-limits01234567890123456789012345678901234567890123456789012345678901234567890123456789-four"));
        // THEN RPC server was detected
        assert!(coco.any_finding_contains(FindingSeverity::Error, "sdv-test-this-is-a-very-long-rpc-server-name-that-exceeds-our-limits01234567890123456789012345678901234567890123456789012345678901234567890123456789"));
        // THEN RPC client was detected
        assert!(coco.any_finding_contains(FindingSeverity::Error, "sdv-test-this-is-a-very-long-rpc-client-name-that-exceeds-our-limits01234567890123456789012345678901234567890123456789012345678901234567890123456789"));
    }

    #[test]
    fn test_should_reject_long_user_defined_name_in_diagnostics() {
        // Given: A catalog with a publisher that has two topics and a user-defined name
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_USER_DEFINED_NAME_TOO_LONG_IN_DIAGNOSTICS.clone());
        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // When: Executing CoCo
        let mut coco = ServiceUnitNameTooLong::default();
        coco.check_catalog(&catalog);
        coco.check_symbol_table(linker.symbol_table());

        // Then: CoCo should fail
        assert!(coco.is_err());
        assert!(coco.any_finding_contains(FindingSeverity::Error, "ThisIsAVeryLongPublisherNameThatExceedsOurLimits012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789"));
    }

    #[test]
    fn test_should_ignore_publisher_with_invalid_type() {
        // Given: A catalog with a service bundle that contains a publisher using an RPC type as publication type
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_DUMMY_RPC_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_INVALID_PUBLICATION_TYPE_GENERATED_NAME.clone());
        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);

        // When: Executing CoCo
        let mut check = ServiceUnitNameTooLong::default();
        check.check_symbol_table(creator.symbol_table());

        // Then: Should not have found errors
        assert!(check.is_ok());
    }

    #[test]
    fn test_should_accept_long_generated_name_with_short_user_defined_name() {
        // GIVEN a catalog with service units that have too long generated name but user define short service unit name
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_NAME_TOO_LONG_PROTO.clone()]);
        catalog.catalog_vsidl_files.push(
            vsidl_v1_test_data::VSIDL_GENERATED_NAME_TOO_LONG_WITH_SHORT_USER_DEFINED_NAME.clone(),
        );

        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN Executing context condition
        let mut check = ServiceUnitNameTooLong::default();
        check.check_catalog(&catalog);
        check.check_symbol_table(linker.symbol_table());

        // THEN the result should be ok
        assert!(check.is_ok());
    }
}
