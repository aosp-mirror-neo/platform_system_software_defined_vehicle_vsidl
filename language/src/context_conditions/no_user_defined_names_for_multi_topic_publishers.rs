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
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker;
use vsidl_v1_symbols::{publisher_symbol::PublisherSymbol, SymbolTable};

#[derive(Debug, Default)]
pub struct NoUserDefinedNamesForMultiChannelPublishers {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(NoUserDefinedNamesForMultiChannelPublishers);
impl VsidlVisitor for NoUserDefinedNamesForMultiChannelPublishers {}

/// Issues an error if a publisher uses user-defined name without specifying a topic if the type has multiple topics
impl SymbolTableChecker for NoUserDefinedNamesForMultiChannelPublishers {
    fn check_publisher(&mut self, symbol_table: &SymbolTable, publisher: &PublisherSymbol) {
        // Check if unit type as topics
        let topics: Vec<String> = publisher.topics(symbol_table);
        let type_has_multiple_topics = topics.len() > 1;

        // Check if publisher a has user-defined name
        let has_user_defined_name = !publisher.service_unit_name.is_empty();

        // Check if the publisher specifies at least one topic
        let topic_specified = !publisher.topic.is_empty();

        if type_has_multiple_topics && !topic_specified && has_user_defined_name {
            self.finding_collector.error(FindingId::NoUserDefinedNamesForMultiChannelPublishers, format!(
                    "Service Bundle '{}': Service unit '{}' may not have a user-defined name without specifying the used topic. Possible topics: {:?}",
                    publisher.owning_service_bundle(symbol_table).full_name(symbol_table),
                    publisher.service_unit_name,
                    topics,
                ),
                vec![publisher.service_unit_name_span.clone().into_option()]
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::no_user_defined_names_for_multi_topic_publishers::NoUserDefinedNamesForMultiChannelPublishers;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_common::Catalog;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;
    use vsidl_v1_symbols::TargetApi;

    #[test]
    fn test_should_accept_valid_catalog() {
        // Given: A valid catalog
        let mut catalog = Catalog::new();
        catalog.proto_descriptors.extend([vsidl_v1_test_data::DESCRIPTOR_MESSAGE_PROTO.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_USER_DEFINED_NAME_MULTI_CHANNEL.clone());
        let mut creator = SymbolTableCreator::new();
        creator.set_target_api(TargetApi::Latest);
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // When: Executing CoCo
        let mut check = NoUserDefinedNamesForMultiChannelPublishers::default();
        check.check_symbol_table(linker.symbol_table());

        // Then: Should not find any errors
        assert!(check.is_ok());
    }
}
