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
use vsidl_v1_common::visitors::visitor::VsidlVisitor;
use vsidl_v1_symbols::{
    proto_publication_symbol::ProtoPublicationSymbol, SymbolTable, SymbolTableChecker,
};

#[derive(Debug, Default)]
pub struct PublicationMultipubMisuse {
    finding_collector: FindingCollector,
}
impl_context_condition_symtab!(PublicationMultipubMisuse);
impl VsidlVisitor for PublicationMultipubMisuse {}

/// Issues a warning if a publication is specified as `MULTI_PUB` but is used by at most one publisher.
impl SymbolTableChecker for PublicationMultipubMisuse {
    fn check_proto_publication(
        &mut self,
        symbol_table: &SymbolTable,
        publication: &ProtoPublicationSymbol,
    ) {
        if publication.is_multi_pub() && publication.publishers(symbol_table).len() == 1 {
            self.finding_collector.warn(
                FindingId::PublicationMultipubMisuse,
                format!(
                    "Publication '{}' is used by only one publisher, `MULTI_PUB` specification could be removed.",
                    publication.full_name(symbol_table)
                ),
                Some("Publication is used by only one publisher, `MULTI_PUB` specification could be removed.".to_string()),
                vec![publication.span()],
            );
        }
    }
}

#[cfg(test)]
mod test {
    use crate::context_condition::ContextCondition;
    use crate::pdk29::PublicationMultipubMisuse;
    use crate::FindingSeverity;
    use vsidl_v1_common::{AstNode, Catalog};
    use vsidl_v1_symbols::{
        symbol_table_creator::SymbolTableCreator, symbol_table_linker::SymbolTableLinker,
    };

    #[test]
    fn test_publication_multipub_misuse_warning() {
        // GIVEN a catalog with multipub misuse
        let mut catalog = Catalog::new();
        catalog
            .proto_descriptors
            .extend([vsidl_v1_test_data::DESCRIPTOR_PUBLICATION_MULTIPUB_MISUSE.clone()]);
        catalog
            .catalog_vsidl_files
            .push(vsidl_v1_test_data::VSIDL_PUBLICATION_MULTIPUB_MISUSE.clone());
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // WHEN executing context condition
        let mut check = PublicationMultipubMisuse::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN warnings should be found
        assert!(check.has_findings());
        assert!(check.any_finding_contains(
            FindingSeverity::Warn,
            "Publication 'sdv.test.TirePressure' is used by only one publisher, `MULTI_PUB` specification could be removed."
        ));
    }

    #[test]
    fn test_publication_multipub_ok() {
        // GIVEN a catalog with multipub
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
        let mut check = PublicationMultipubMisuse::default();
        check.check_linked_symbol_table(linker.symbol_table());

        // THEN result should be ok
        assert!(check.is_ok());
        assert!(!check.has_findings());
    }
}
