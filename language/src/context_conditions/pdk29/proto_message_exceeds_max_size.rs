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
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::{
    proto_publication_symbol::ProtoPublicationSymbol, symbol_table_checker::SymbolTableChecker,
    SymbolTable,
};

use crate::{FindingCollector, FindingId};

// see proto limits: https://protobuf.dev/programming-guides/proto-limits/
const PROTO_MAX_SIZE_BYTES: usize = 2 * 1024 * 1024 * 1024; // 2 GiB

#[derive(Debug, Default)]
pub struct ProtoMessageExceedsMaxSize {
    finding_collector: FindingCollector,
}

impl_context_condition_symtab!(ProtoMessageExceedsMaxSize);
impl VsidlVisitor for ProtoMessageExceedsMaxSize {}

impl SymbolTableChecker for ProtoMessageExceedsMaxSize {
    fn check_proto_publication(
        &mut self,
        symbol_table: &SymbolTable,
        publication: &ProtoPublicationSymbol,
    ) {
        if publication.message_size.size > PROTO_MAX_SIZE_BYTES {
            self.finding_collector.error(
                FindingId::ProtoMessageExceedsMaxSize,
                format!(
                    "message '{}' exceeds proto limit size of 2GiB",
                    publication.full_name(symbol_table),
                ),
                vec![publication.span()],
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::pdk29::ProtoMessageExceedsMaxSize;
    use crate::FindingSeverity;
    use vsidl_v1_common::{AstNode, Catalog};
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;

    #[test]
    fn test_proto_message_exceeds_max_size_error() {
        // GIVEN a catalog with proto message exceed max size limit
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_PROTO_MESSAGE_EXCEEDS_MAX_SIZE.clone()]);
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN executing context condition
        let mut check = ProtoMessageExceedsMaxSize::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN should have found error
        assert!(check.is_err());
        assert!(check.any_finding_contains(
            FindingSeverity::Error,
            "message 'sdv.test.TirePressure' exceeds proto limit size of 2GiB"
        ));
    }

    #[test]
    fn test_proto_message_exceeds_max_size_ok() {
        // GIVEN a catalog with proto message within max size limit
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_PROTO_MESSAGE_WITHIN_MAX_SIZE.clone()]);
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // WHEN executing context condition
        let mut check = ProtoMessageExceedsMaxSize::default();
        check.check_symbol_table(creator.symbol_table());

        // THEN check should be ok
        assert!(check.is_ok());
    }
}
