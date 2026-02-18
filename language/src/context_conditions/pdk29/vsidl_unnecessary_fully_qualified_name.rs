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

use crate::impl_context_condition_symtab;
use crate::{FindingCollector, FindingId};
use vsidl_v1::syntax::Span;
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;
use vsidl_v1_symbols::{
    publisher_symbol::PublisherSymbol, rpc_client_symbol::RpcClientSymbol,
    rpc_server_symbol::RpcServerSymbol, subscriber_symbol::SubscriberSymbol, SymbolTable,
};

#[derive(Debug, Default)]
pub struct VsidlUnnecessaryFullyQualifiedName {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(VsidlUnnecessaryFullyQualifiedName);
impl VsidlVisitor for VsidlUnnecessaryFullyQualifiedName {}

impl VsidlUnnecessaryFullyQualifiedName {
    /// Checks if a message/service name is unnecessarily fully qualified and issues a warning
    fn check_and_warn(
        &mut self,
        package: &str,
        ast_name: &str,
        resolved_fqn: &str,
        node_description: &str,
        span: Option<Span>,
    ) {
        if ast_name != resolved_fqn {
            return;
        }

        if package.is_empty() {
            return;
        }

        let prefix_with_dot = format!("{package}.");
        if let Some(stripped_name) = ast_name.strip_prefix(&prefix_with_dot) {
            self.finding_collector.warn(
                FindingId::VsidlUnnecessaryFullyQualifiedName,
                format!(
                    "{node_description}: '{ast_name}', fully qualified name could be replaced by '{stripped_name}'",
                ),
                vec![span]
            );
        }
    }
}

/// Display a warning if unnecessary fully qualified name are using in vsidl file
impl SymbolTableChecker for VsidlUnnecessaryFullyQualifiedName {
    fn check_publisher(&mut self, symbol_table: &SymbolTable, publisher: &PublisherSymbol) {
        self.check_and_warn(
            &publisher.package(symbol_table),
            &publisher.ast_node.message,
            &publisher.publication(symbol_table).full_name(symbol_table),
            "Publisher message",
            publisher.message_span.clone().into_option(),
        );
    }

    fn check_subscriber(&mut self, symbol_table: &SymbolTable, subscriber: &SubscriberSymbol) {
        self.check_and_warn(
            &subscriber.package(symbol_table),
            &subscriber.ast_node.message,
            &subscriber.publication(symbol_table).full_name(symbol_table),
            "Subscriber message",
            subscriber.message_span.clone().into_option(),
        );
    }

    fn check_rpc_server(&mut self, symbol_table: &SymbolTable, rpc_server: &RpcServerSymbol) {
        self.check_and_warn(
            &rpc_server.package(symbol_table),
            &rpc_server.ast_node.service,
            &rpc_server.service(symbol_table).full_name(symbol_table),
            "Server service",
            rpc_server.service_span.clone().into_option(),
        );
    }

    fn check_rpc_client(&mut self, symbol_table: &SymbolTable, rpc_client: &RpcClientSymbol) {
        self.check_and_warn(
            &rpc_client.package(symbol_table),
            &rpc_client.ast_node.service,
            &rpc_client.service(symbol_table).full_name(symbol_table),
            "Client service",
            rpc_client.service_span.clone().into_option(),
        );
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::pdk29::VsidlUnnecessaryFullyQualifiedName;
    use crate::FindingSeverity;
    use vsidl_v1_common::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;

    #[test]
    fn test_fully_qualified_name_warning() {
        // GIVEN a catalog with unnecessary fully qualified name in vsidl file
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_UNNECESSARY_FULLY_QUALIFIED_NAME.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_UNNECESSARY_FULLY_QUALIFIED_NAME.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = VsidlUnnecessaryFullyQualifiedName::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN warnings should be found
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Publisher message: 'sdv.vsidl.test.TirePressure', fully qualified name could be replaced by 'TirePressure'",
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Subscriber message: 'sdv.vsidl.test.TirePressure', fully qualified name could be replaced by 'TirePressure'",
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Server service: 'sdv.vsidl.test.TireStatus', fully qualified name could be replaced by 'TireStatus'",
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Client service: 'sdv.vsidl.test.TireStatus', fully qualified name could be replaced by 'TireStatus'",
        ));
    }

    #[test]
    fn test_fully_qualified_name_subpackage_warning() {
        // GIVEN a catalog with unnecessary fully qualified name in vsidl file
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_UNNECESSARY_FULLY_QUALIFIED_NAME.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_UNNECESSARY_FULLY_QUALIFIED_NAME_SUBPACKAGE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = VsidlUnnecessaryFullyQualifiedName::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN warnings should be found
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Publisher message: 'sdv.vsidl.test.TirePressure', fully qualified name could be replaced by 'test.TirePressure'",
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Subscriber message: 'sdv.vsidl.test.TirePressure', fully qualified name could be replaced by 'test.TirePressure'",
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Server service: 'sdv.vsidl.test.TireStatus', fully qualified name could be replaced by 'test.TireStatus'",
        ));
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Client service: 'sdv.vsidl.test.TireStatus', fully qualified name could be replaced by 'test.TireStatus'",
        ));
    }

    #[test]
    fn test_fully_qualified_name_ok() {
        // GIVEN a catalog with unnecessary fully qualified name in vsidl file
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_UNNECESSARY_FULLY_QUALIFIED_NAME.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_NECESSARY_FULLY_QUALIFIED_NAME.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = VsidlUnnecessaryFullyQualifiedName::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN no warning should be found
        assert!(check.is_ok());
        assert!(!check.has_findings());
    }
}
