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

use super::*;
use crate::ContextCondition;
use anyhow::{bail, Ok};
use rayon::prelude::*;
use vsidl_v1_common::Catalog;
use vsidl_v1_common::ProgressPrinter;
use vsidl_v1_symbols::symbol_table::SymbolTable;
use vsidl_v1_symbols::symbol_table::SymbolTableState;

/// Manages and executes a collection of [ContextCondition] checks.
///
/// This struct allows for dynamic addition of context conditions and executes them
/// at different stages of the compilation process (AST creation, symbol table creation,
/// symbol table linking). It collects findings (errors, warnings) from all executed conditions.
pub struct ContextConditionChecker {
    /// Stores context conditions that are compatible with parallel execution via Rayon.
    /// Each condition is associated with the [SymbolTableState] required for its execution.
    context_conditions: Vec<(SymbolTableState, Box<dyn ContextCondition + Send + Sync>)>,
    /// Collects findings (errors, warnings, fatals) from all executed context conditions.
    finding_collector: FindingCollector,
    /// Utility to print progress updates during checking.
    printer: ProgressPrinter,
    /// True if warnings shall be printed
    print_warnings: bool,
    /// True if findings shall not be printed
    quiet: bool,
}

impl ContextConditionChecker {
    /// Creates a new, empty [ContextConditionChecker].
    ///
    /// Use [add_context_condition], or [default]
    /// to populate the checker.
    pub(crate) fn new() -> Self {
        Self {
            context_conditions: Vec::new(),
            finding_collector: FindingCollector::default(),
            printer: ProgressPrinter::new(0),
            print_warnings: false,
            quiet: false,
        }
    }

    /// Returns the findings of the context condition checker
    pub fn findings(&self) -> Vec<Finding> {
        self.finding_collector.findings()
    }

    /// Counts the number of active findings with severity [FindingSeverity::Error] or [FindingSeverity::Fatal].
    ///
    /// Findings disabled via [FindingCollector::disable_finding_id] are not counted.
    ///
    /// # Returns
    ///
    /// The total count of error and fatal findings as a `u32`.
    pub fn error_count(&self) -> u32 {
        self.finding_collector.error_count()
    }

    /// Appends a [ContextCondition] to the checker.
    ///
    /// The condition will be executed in parallel with other context conditions
    /// when the compilation process reaches the specified [SymbolTableState].
    ///
    /// # Parameters
    ///
    /// * `required_state` - The [SymbolTableState] required before this condition can be run.
    /// * `context_condition` - The context condition to add. Must implement [Send] and [Sync].
    ///
    /// # Returns
    ///
    /// A mutable reference to `self` for chaining.
    pub fn add_context_condition(
        &mut self,
        required_state: SymbolTableState,
        context_condition: Box<dyn ContextCondition + Send + Sync>,
    ) -> &mut Self {
        self.context_conditions.push((required_state, context_condition));
        self.printer = ProgressPrinter::new(self.context_conditions.len());
        self
    }

    /// Executes all context conditions that operate solely on the Abstract Syntax Tree (AST).
    ///
    /// These conditions are run when the [SymbolTableState] is [SymbolTableState::NotCreated].
    /// Findings are collected, and an error summary is generated if any errors occurred.
    ///
    /// # Parameters
    ///
    /// * `catalog` - The parsed [Catalog] containing the AST.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if no errors were found by the executed conditions.
    /// * `Err(anyhow::Error)` if one or more errors were found.
    pub fn check_ast(&mut self, catalog: &Catalog) -> anyhow::Result<()> {
        let finding_collectors: Vec<FindingCollector> = self
            .context_conditions
            .par_iter_mut()
            .filter(|(state, _)| *state == SymbolTableState::NotCreated)
            .map(|(_state, context_condition)| {
                context_condition.check_catalog(catalog);
                self.printer.print_next_progress_step("Validating input");
                context_condition.finding_collector()
            })
            .collect();

        for collector in finding_collectors {
            self.finding_collector.merge(collector);
        }

        if !self.print_warnings {
            self.finding_collector.clear_warnings();
        }
        if !self.quiet {
            self.finding_collector.print_findings_if_error();
            self.finding_collector.error_summary()
        } else if self.error_count() > 0 {
            bail!(
                "aborting due to {} error{}",
                self.error_count(),
                if self.error_count() == 1 { "" } else { "s" }
            );
        } else {
            Ok(())
        }
    }

    /// Executes context conditions that require a created but not yet linked symbol table.
    ///
    /// These conditions are run when the [SymbolTableState] is
    /// [SymbolTableState::CreatedNotLinked]. Findings are collected, and an error summary
    /// is generated if any errors occurred.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The [SymbolTable] after creation but before linking.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if no errors were found by the executed conditions.
    /// * `Err(anyhow::Error)` if one or more errors were found.
    ///
    /// # Panics
    ///
    /// * If the provided `symbol_table`'s state is not [SymbolTableState::CreatedNotLinked].
    pub fn check_symbol_table(&mut self, symbol_table: &SymbolTable) -> anyhow::Result<()> {
        if symbol_table.get_state() == SymbolTableState::NotCreated {
            panic!("Called check_symbol_table on a symbol table that has not been created yet");
        }

        let finding_collectors: Vec<FindingCollector> = self
            .context_conditions
            .par_iter_mut()
            .filter(|(state, _)| *state == SymbolTableState::CreatedNotLinked)
            .map(|(_, context_condition)| {
                context_condition.check_symbol_table(symbol_table);
                self.printer.print_next_progress_step("Validating input");
                context_condition.finding_collector()
            })
            .collect();

        for collector in finding_collectors {
            self.finding_collector.merge(collector);
        }

        if !self.print_warnings {
            self.finding_collector.clear_warnings();
        }
        if !self.quiet {
            self.finding_collector.print_findings_if_error();
            self.finding_collector.error_summary()
        } else if self.error_count() > 0 {
            bail!(
                "aborting due to {} error{}",
                self.error_count(),
                if self.error_count() == 1 { "" } else { "s" }
            );
        } else {
            Ok(())
        }
    }

    /// Executes context conditions that require a fully linked symbol table.
    ///
    /// These conditions are run when the [SymbolTableState] is [SymbolTableState::Linked].
    /// Findings are collected, printed, and an error summary is generated if any errors occurred.
    ///
    /// # Parameters
    ///
    /// * `symbol_table` - The fully linked [SymbolTable].
    ///
    /// # Returns
    ///
    /// * `Ok(())` if no errors were found by the executed conditions.
    /// * `Err(anyhow::Error)` if one or more errors were found.
    ///
    /// # Panics
    ///
    /// * If the provided `symbol_table`'s state is not [SymbolTableState::Linked].
    pub fn check_linked_symbol_table(&mut self, symbol_table: &SymbolTable) -> anyhow::Result<()> {
        if symbol_table.get_state() != SymbolTableState::Linked {
            panic!(
                "Called check_linked_symbol_table on a symbol table that has not been linked yet"
            );
        }

        let finding_collectors: Vec<FindingCollector> = self
            .context_conditions
            .par_iter_mut()
            .filter(|(state, _)| *state == SymbolTableState::Linked)
            .map(|(_, context_condition)| {
                context_condition.check_linked_symbol_table(symbol_table);
                self.printer.print_next_progress_step("Validating input");
                context_condition.finding_collector()
            })
            .collect();

        for collector in finding_collectors {
            self.finding_collector.merge(collector);
        }

        // Print all findings (including warnings) after the final check stage.
        if !self.print_warnings {
            self.finding_collector.clear_warnings();
        }
        if !self.quiet {
            self.finding_collector.print_findings();
            self.finding_collector.error_summary()
        } else if self.error_count() > 0 {
            bail!(
                "aborting due to {} error{}",
                self.error_count(),
                if self.error_count() == 1 { "" } else { "s" }
            );
        } else {
            Ok(())
        }
    }

    /// Disables reporting of findings for a specific [FindingId].
    ///
    /// Any findings with the specified ID generated by context conditions will be ignored
    /// and not contribute to the error count.
    ///
    /// # Parameters
    ///
    /// * `id` - The [FindingId] to disable.
    pub fn disable_finding_id(&mut self, id: FindingId) {
        self.finding_collector.disable_finding_id(id);
    }

    /// Clears all findings collected so far.
    pub fn clear_findings(&mut self) {
        self.finding_collector = FindingCollector::default();
    }

    /// Sets the quiet mode. If true, findings will not be printed to stderr.
    pub fn set_quiet(&mut self, quiet: bool) -> &mut Self {
        self.quiet = quiet;
        self
    }

    /// Enables CoCos that only lead to warnings but never to errors.
    ///
    /// Not enabling them can make the CoCo checks faster without risking
    /// to crash or generate uncompilable code.
    pub fn enable_warnings(&mut self) {
        self.print_warnings = true;
        self.add_context_condition(
            SymbolTableState::CreatedNotLinked,
            Box::new(ProtoFqnDuplicate::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(MultipleRpcServersForSameInterface::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(RpcClientRequiresRpcServer::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(VsidlUnnecessaryFullyQualifiedName::default()),
        );
    }

    /// Enables CoCos for the latest version.
    pub fn enable_latest(&mut self) {
        self.add_context_condition(
            SymbolTableState::NotCreated,
            Box::new(TopicNameKebabCase::default()),
        )
        .add_context_condition(SymbolTableState::NotCreated, Box::new(TopicNameTooLong::default()))
        .add_context_condition(SymbolTableState::NotCreated, Box::new(PublisherCapacity::default()))
        .add_context_condition(
            SymbolTableState::NotCreated,
            Box::new(PublisherCapacityMandatory::default()),
        )
        .add_context_condition(SymbolTableState::NotCreated, Box::new(TopicMandatory::default()))
        .add_context_condition(
            SymbolTableState::NotCreated,
            Box::new(RpcChannelMandatory::default()),
        )
        .add_context_condition(
            SymbolTableState::NotCreated,
            Box::new(RpcChannelNamingConvention::default()),
        )
        .add_context_condition(
            SymbolTableState::NotCreated,
            Box::new(ServiceUnitNameTooLong::default()),
        )
        .add_context_condition(
            SymbolTableState::NotCreated,
            Box::new(UserDefinedNamesOnlyOneTopic::default()), // Checks user_defined_name usage
        )
        .add_context_condition(
            SymbolTableState::CreatedNotLinked,
            Box::new(RpcChannelUniquePerService::default()),
        )
        .add_context_condition(
            SymbolTableState::CreatedNotLinked,
            Box::new(TopicNameUnique::default()),
        )
        .add_context_condition(
            SymbolTableState::CreatedNotLinked,
            Box::new(PublicationType::default()),
        )
        .add_context_condition(
            SymbolTableState::CreatedNotLinked,
            Box::new(SubscriberType::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(NoUserDefinedNamesForMultiChannelPublishers::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(ProtoMessageExceedsMaxSize::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(ProtoMessageRequireAnnotationType::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(RpcClientChannelRequiresServerChannel::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(ServiceUnitNameTooLong::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(ServiceUnitNameDuplicate::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(SubscriberChannelsRedeclaration::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(SubscriberRequiresPublisher::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(UniqueUserDefinedServiceUnitNamesPerType::default()),
        );
    }

    /// Enables CoCos for PDK29.
    pub fn enable_pdk29(&mut self) {
        // NotCreated
        self.add_context_condition(
            SymbolTableState::NotCreated,
            Box::new(pdk29::ServiceUnitNameTooLong::default()),
        )
        .add_context_condition(
            SymbolTableState::NotCreated,
            Box::new(pdk29::SdvPublication::default()), // Checks proto options
        )
        .add_context_condition(
            SymbolTableState::NotCreated,
            Box::new(pdk29::UserDefinedNamesOnlyOneChannel::default()), // Checks user_defined_name usage
        );

        // CreatedNotLinked
        self.add_context_condition(
            SymbolTableState::CreatedNotLinked,
            Box::new(pdk29::ProtoMessageExceedsMaxSize::default()),
        )
        .add_context_condition(
            SymbolTableState::CreatedNotLinked,
            Box::new(pdk29::PublicationType::default()),
        )
        .add_context_condition(
            SymbolTableState::CreatedNotLinked,
            Box::new(pdk29::PublisherChannel::default()),
        )
        .add_context_condition(
            SymbolTableState::CreatedNotLinked,
            Box::new(pdk29::SdvPublication::default()),
        )
        .add_context_condition(
            SymbolTableState::CreatedNotLinked,
            Box::new(pdk29::SubscriberType::default()),
        );

        // Linked
        self.add_context_condition(
            SymbolTableState::Linked,
            Box::new(pdk29::MultiplePublishersSameMultipubTopicPerBundle::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(pdk29::NoUserDefinedNamesForMultiChannelPublishers::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(pdk29::ProtoMessageRequireAnnotationType::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(pdk29::ServiceUnitNameDuplicate::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(pdk29::ServiceUnitNameTooLong::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(pdk29::SubscriberRequiresPublisher::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(pdk29::SubscriberChannelsRedeclaration::default()),
        )
        .add_context_condition(
            SymbolTableState::Linked,
            Box::new(pdk29::UniqueUserDefinedServiceUnitNamesPerType::default()),
        );

        // Warnings - only add if print_warnings is true
        if self.print_warnings {
            self.add_context_condition(
                SymbolTableState::Linked,
                Box::new(pdk29::PublicationMultipubMisuse::default()),
            )
            .add_context_condition(
                SymbolTableState::Linked,
                Box::new(pdk29::PublicationUnused::default()),
            )
            .add_context_condition(
                SymbolTableState::Linked,
                Box::new(pdk29::VsidlRedundantExplicitChannelListing::default()),
            )
            .add_context_condition(
                SymbolTableState::Linked,
                Box::new(pdk29::VsidlUnnecessaryFullyQualifiedName::default()),
            );
        }
    }
}

impl Default for ContextConditionChecker {
    /// Creates a [ContextConditionChecker] with a default set of context conditions.
    ///
    /// The default set includes conditions that primarily operate on the AST or basic
    /// proto metadata, running when the [SymbolTableState] is [NotCreated].
    fn default() -> Self {
        let mut checker = ContextConditionChecker::new();
        checker
            // Android.bp related checks
            .add_context_condition(
                SymbolTableState::NotCreated,
                Box::new(AndroidBpFields::default()),
            )
            .add_context_condition(SymbolTableState::NotCreated, Box::new(EmptyCatalog::default()))
            .add_context_condition(
                SymbolTableState::NotCreated,
                Box::new(AndroidBpFilegroups::default()),
            )
            .add_context_condition(
                SymbolTableState::NotCreated,
                Box::new(AndroidBpFilegroupsComposition::default()),
            )
            .add_context_condition(
                SymbolTableState::NotCreated,
                Box::new(AndroidBpMappingProtos::default()),
            )
            // Proto metadata and structure checks
            .add_context_condition(SymbolTableState::NotCreated, Box::new(ProtoMetadata::default()))
            // Naming and uniqueness checks (AST level)
            .add_context_condition(
                SymbolTableState::NotCreated,
                Box::new(ServiceBundleNamingConvention::default()),
            )
            .add_context_condition(
                SymbolTableState::NotCreated,
                Box::new(UniqueUserDefinedServiceUnitNamesPerBundle::default()), // Checks within a single bundle AST node
            )
            .add_context_condition(
                SymbolTableState::NotCreated,
                Box::new(UserDefinedNameNamingConvention::default()),
            )
            .add_context_condition(
                SymbolTableState::NotCreated,
                Box::new(UniqueTargetName::default()), // Checks rust_library target names
            )
            .add_context_condition(
                SymbolTableState::NotCreated,
                Box::new(UserDefinedTargetNameConvention::default()), // Checks rust_library naming convention
            )
            .add_context_condition(SymbolTableState::NotCreated, Box::new(RpcStreaming::default()))
            .add_context_condition(
                SymbolTableState::NotCreated,
                Box::new(RpcDuplicateMethodName::default()),
            )
            // Conditions requiring a created but not linked symbol table
            .add_context_condition(
                SymbolTableState::CreatedNotLinked,
                Box::new(ServiceBundleFqnDuplicate::default()),
            )
            .add_context_condition(
                SymbolTableState::CreatedNotLinked,
                Box::new(RpcClientType::default()),
            )
            .add_context_condition(
                SymbolTableState::CreatedNotLinked,
                Box::new(RpcServerType::default()),
            );
        checker
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_condition::MockContextCondition;
    use vsidl_v1_common::visitors::visitor::AstNode;
    use vsidl_v1_symbols::symbol_table_creator::SymbolTableCreator;
    use vsidl_v1_symbols::symbol_table_linker::SymbolTableLinker;

    #[test]
    fn test_default() {
        // GIVEN / WHEN a default context condition checker
        let checker = ContextConditionChecker::default();

        // THEN it should already include the basic context conditions
        assert!(!checker.context_conditions.is_empty());

        // THEN the context conditions that require a symbol table are added to the checker's list.
        assert!(checker
            .context_conditions
            .iter()
            .any(|(state, _)| *state == SymbolTableState::CreatedNotLinked));
    }

    #[test]
    fn test_add_context_condition() {
        // GIVEN a context condition and context condition checker
        let mut checker = ContextConditionChecker::new();
        let mock_condition = MockContextCondition::new();

        // WHEN adding a context condition
        checker.add_context_condition(SymbolTableState::NotCreated, Box::new(mock_condition));

        // THEN the context condition shall be in the list of context conditions
        assert_eq!(checker.context_conditions.len(), 1);
    }

    #[test]
    fn test_check_ast() {
        // GIVEN a context condition checker and a mock context condition
        let mut checker = ContextConditionChecker::new();
        let mut mock_condition = MockContextCondition::new();
        let catalog = Catalog::new();

        // Expect the check_catalog function to be called once
        mock_condition.expect_check_catalog().times(1).return_const(());

        // Expect the finding_collector to be called once.
        mock_condition.expect_finding_collector().times(1).returning(FindingCollector::default);

        checker.add_context_condition(SymbolTableState::NotCreated, Box::new(mock_condition));

        // WHEN checking the AST
        let result = checker.check_ast(&catalog);

        // THEN the context condition should be checked without errors
        assert!(result.is_ok());
        // THEN check_catalog was called on the CoCo
        // AutoMock will panic if this is not the case
    }

    #[test]
    fn test_check_symbol_table() {
        // GIVEN a context condition checker and a mock context condition
        let mut checker = ContextConditionChecker::new();
        let mut mock_condition = MockContextCondition::new();
        let catalog = Catalog::new();
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);

        // Expect the check_symbol_table function to be called once
        mock_condition.expect_check_symbol_table().times(1).return_const(());

        // Expect the finding_collector to be called once.
        mock_condition.expect_finding_collector().times(1).returning(FindingCollector::default);

        checker.add_context_condition(SymbolTableState::CreatedNotLinked, Box::new(mock_condition));

        // WHEN checking the symbol table
        let result = checker.check_symbol_table(creator.symbol_table());

        // THEN the context condition should be checked without errors
        assert!(result.is_ok());
        // THEN check_symbol_table was called on the CoCo
        // AutoMock will panic if this is not the case
    }

    #[test]
    #[should_panic]
    fn test_check_symbol_table_panic() {
        // GIVEN a context condition checker and a mock context condition
        let mut checker = ContextConditionChecker::new();
        let mock_condition = MockContextCondition::new();
        let symbol_table = SymbolTable::default();

        checker.add_context_condition(SymbolTableState::CreatedNotLinked, Box::new(mock_condition));

        // WHEN checking the symbol table that is not created
        let _result = checker.check_symbol_table(&symbol_table);
    }

    #[test]
    fn test_check_linked_symbol_table() {
        // GIVEN a context condition checker and a mock context condition
        let mut checker = ContextConditionChecker::new();
        let mut mock_condition = MockContextCondition::new();
        let catalog = Catalog::new();
        let mut creator = SymbolTableCreator::new();
        catalog.accept(&mut creator);
        let mut linker = SymbolTableLinker::from(creator);
        linker.execute();

        // Expect the check_linked_symbol_table function to be called once
        mock_condition.expect_check_linked_symbol_table().times(1).return_const(());

        // Expect the finding_collector to be called once.
        mock_condition.expect_finding_collector().times(1).returning(FindingCollector::default);

        checker.add_context_condition(SymbolTableState::Linked, Box::new(mock_condition));

        // WHEN checking the linked symbol table
        let result = checker.check_linked_symbol_table(linker.symbol_table());

        // THEN the context condition should be checked without errors
        assert!(result.is_ok());
        // THEN check_linked_symbol_table was called on the CoCo
        // AutoMock will panic if this is not the case
    }

    #[test]
    #[should_panic]
    fn test_check_linked_symbol_table_panic() {
        // GIVEN a context condition checker and a mock context condition
        let mut checker = ContextConditionChecker::new();
        let mock_condition = MockContextCondition::new();
        let symbol_table = SymbolTable::default();

        checker.add_context_condition(SymbolTableState::Linked, Box::new(mock_condition));

        // WHEN checking the symbol table that is not linked
        let _result = checker.check_linked_symbol_table(&symbol_table);

        // THEN should have panicked
        // this is checked using #[should_panic]
    }

    #[test]
    fn test_disable_finding_id() {
        // GIVEN a context condition checker and a finding id
        let mut checker = ContextConditionChecker::new();
        let finding_id = FindingId::OwnedUnitTypeIsAmbiguous;

        // WHEN disabling the finding id
        checker.disable_finding_id(finding_id);

        // WHEN adding a finding with the disabled id
        checker.finding_collector.error(finding_id, "test", Vec::new());

        // THEN there's no finding (because it's disabled)
        assert!(checker.finding_collector.findings().is_empty());
    }
}
