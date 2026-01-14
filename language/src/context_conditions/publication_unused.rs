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
use vsidl_v1_symbols::{
    proto_publication_symbol::ProtoPublicationSymbol, SymbolTable, SymbolTableChecker,
};

#[derive(Debug, Default)]
pub struct PublicationUnused {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(PublicationUnused);
impl VsidlVisitor for PublicationUnused {}

/// Issues a warning if a publication is unused
impl SymbolTableChecker for PublicationUnused {
    fn check_proto_publication(
        &mut self,
        symbol_table: &SymbolTable,
        publication: &ProtoPublicationSymbol,
    ) {
        if publication.publishers(symbol_table).is_empty() {
            self.finding_collector.warn(
                FindingId::PublicationUnused,
                format!("Publication '{}' is unused.", publication.full_name(symbol_table)),
                vec![publication.span()],
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::publication_unused::PublicationUnused;
    use crate::FindingSeverity;
    use vsidl_v1_common::{AstNode, Catalog};
    use vsidl_v1_symbols::{
        symbol_table_creator::SymbolTableCreator, symbol_table_linker::SymbolTableLinker,
    };

    #[test]
    fn test_publication_unused_warning() {
        // GIVEN a catalog with unused publication
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_PUBLICATION_MULTIPUB_MISUSE.clone()]);
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = PublicationUnused::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN warnings should be found
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Publication 'sdv.test.TirePressure' is unused."
        ));
    }

    #[test]
    fn test_publication_used_ok() {
        // GIVEN a catalog without unused publication
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_PUBLICATION_MULTIPUB_MISUSE.clone()]);
        catalog.catalog_vsidl_files.push(vsidl_v1_test_data::VSIDL_PUBLICATION_MULTIPUB.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = PublicationUnused::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN result should be ok
        assert!(check.is_ok());
        assert!(!check.has_findings());
    }
}
