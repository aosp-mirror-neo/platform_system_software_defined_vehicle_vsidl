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

use super::{FindingCollector, FindingSeverity};
use mockall::automock;
use vsidl_v1_common::{AstNode, Catalog, VsidlVisitor};
use vsidl_v1_symbols::{symbol_table::SymbolTable, SymbolTableChecker};
impl VsidlVisitor for MockContextCondition {}
impl SymbolTableChecker for MockContextCondition {}

/// Represents a single validation rule or check that can be applied to a VSIDL model.
///
/// Context conditions (CoCos) are used to enforce rules that cannot be expressed
/// solely by the grammar, ensuring the semantic correctness and well-formedness
/// of the VSIDL input. They operate at different stages of the compilation process:
/// on the raw Abstract Syntax Tree (AST) via [VsidlVisitor], and on the Symbol Table
/// before and after linking via [SymbolTableChecker].
///
/// Each context condition implementation collects findings (errors, warnings)
/// using a [FindingCollector].
#[automock]
pub trait ContextCondition: VsidlVisitor + SymbolTableChecker {
    /// Returns a clone of the internal finding collector associated with this context condition.
    ///
    /// The collector holds all the findings (errors, warnings, fatals) reported by this
    /// specific condition during its checks.
    ///
    /// # Returns
    ///
    /// A [FindingCollector] instance containing the findings for this condition.
    fn finding_collector(&mut self) -> FindingCollector;

    /// Executes the context condition check directly on the VSIDL Abstract Syntax Tree (AST).
    ///
    /// This method traverses the given [Catalog] (which represents the AST) using the
    /// visitor pattern ([VsidlVisitor]). Implementations typically override specific
    /// `(start|end)_visit_*` methods from the [VsidlVisitor] trait to perform checks
    /// on relevant AST nodes.
    ///
    /// Findings are reported using the internal [FindingCollector].
    ///
    /// # Parameters
    ///
    /// * `catalog`: A reference to the [Catalog] representing the parsed VSIDL AST.
    fn check_catalog(&mut self, catalog: &Catalog) {
        catalog.accept(self.as_mut_dyn_visitor());
    }

    /// Executes the context condition check on the symbol table *before* linking.
    ///
    /// This method traverses the given [SymbolTable] using the checker pattern ([SymbolTableChecker]).
    /// It's typically used for checks that require symbol information but not fully resolved
    /// cross-references (e.g., checking for duplicate definitions within a scope).
    /// Findings are reported using the internal [FindingCollector].
    ///
    /// # Parameters
    ///
    /// * `symbol_table`: A reference to the [SymbolTable] after creation but before linking.
    ///
    /// # Panics
    ///
    /// This method assumes the symbol table state is at least [SymbolTableState::CreatedNotLinked].
    /// Behavior is undefined if called on a table in the [SymbolTableState::NotCreated] state.
    fn check_symbol_table(&mut self, symbol_table: &SymbolTable) {
        symbol_table.accept(self.as_mut_dyn_checker());
    }

    /// Executes the context condition check on the symbol table *after* linking.
    ///
    /// This method traverses the given [SymbolTable] using the checker pattern ([SymbolTableChecker]).
    /// It's used for checks that require fully resolved symbol references and relationships
    /// across the entire model (e.g., ensuring a subscriber has a corresponding publisher).
    /// Findings are reported using the internal [FindingCollector].
    ///
    /// # Parameters
    ///
    /// * `symbol_table`: A reference to the fully linked [SymbolTable].
    ///
    /// # Panics
    ///
    /// This method assumes the symbol table state is [SymbolTableState::Linked].
    /// Behavior is undefined if called on a table that is not fully linked.
    fn check_linked_symbol_table(&mut self, symbol_table: &SymbolTable) {
        symbol_table.accept(self.as_mut_dyn_checker());
    }

    /// Checks if the context condition has reported any findings with severity [FindingSeverity::Error] or [FindingSeverity::Fatal].
    ///
    /// # Returns
    ///
    /// `true` if no Error or Fatal findings have been reported, `false` otherwise.
    fn is_ok(&mut self) -> bool {
        for finding in &self.finding_collector().findings() {
            if finding.severity == FindingSeverity::Error
                || finding.severity == FindingSeverity::Fatal
            {
                return false;
            }
        }
        true
    }

    /// Checks if the context condition has reported at least one finding with severity
    /// [FindingSeverity::Error] or [FindingSeverity::Fatal].
    ///
    /// This is the inverse of [ContextCondition::is_ok].
    ///
    /// # Returns
    ///
    /// `true` if at least one Error or Fatal finding has been reported, `false` otherwise.
    fn is_err(&mut self) -> bool {
        !self.is_ok()
    }

    /// Checks if the context condition has reported any findings (regardless of severity).
    ///
    /// # Returns
    ///
    /// `true` if the finding collector associated with this condition is not empty, `false` otherwise.
    fn has_findings(&mut self) -> bool {
        !self.finding_collector().findings().is_empty()
    }

    /// Checks if any finding of a specific severity contains a given substring in its message.
    ///
    /// # Parameters
    ///
    /// * `severity`: The [FindingSeverity] to filter findings by.
    /// * `message`: The substring to search for within the finding messages.
    ///
    /// # Returns
    ///
    /// `true` if at least one finding matches the severity and contains the message substring, `false` otherwise.
    fn any_finding_contains(&mut self, severity: FindingSeverity, message: &str) -> bool {
        self.finding_collector()
            .findings()
            .iter()
            .filter(|e| e.severity == severity)
            .any(|e| e.message.contains(message))
    }
}

/// Implements the [ContextCondition] trait for a struct.
///
/// This macro assumes the struct:
/// 1. Implements [VsidlVisitor]
/// 2. Has a field named `finding_collector` of type [FindingCollector].
///
/// It provides the default implementation for [ContextCondition::finding_collector].
///
/// # Usage
///
/// ```rust,ignore
/// use super::{FindingCollector, FindingId};
/// use crate::impl_context_condition;
///
/// #[derive(Debug, Default)]
/// struct MyAstCondition {
///     finding_collector: FindingCollector,
/// }
/// // Apply the macro
/// impl_context_condition!(MyAstCondition);
///
/// // Required by the macro
/// impl VsidlVisitor for MyAstCondition { /* ... visit methods ... */ }
///
/// ```
#[macro_export]
macro_rules! impl_context_condition {
    ($type_name:ident) => {
        impl vsidl_v1_symbols::symbol_table_checker::SymbolTableChecker for $type_name {}

        impl $crate::ContextCondition for $type_name {
            /// Returns a clone of the internal finding collector.
            fn finding_collector(&mut self) -> $crate::FindingCollector
            where
                Self: $crate::ContextCondition + Default,
            {
                self.finding_collector.clone()
            }
        }
    };
}

/// Implements the [ContextCondition] trait for a struct intended primarily for symbol table checks.
///
/// This macro assumes the struct:
/// 1. Implements [VsidlVisitor] (often trivially, if checks are only on the symbol table only).
/// 2. Implements [SymbolTableChecker].
/// 3. Has a field named `finding_collector` of type [FindingCollector].
///
/// It provides the default implementation for [ContextCondition::finding_collector].
/// This macro is functionally similar to [impl_context_condition] but serves as a semantic marker
/// for conditions that primarily operate on the [SymbolTable].
///
/// # Usage
///
/// ```rust,ignore
/// use super::{FindingCollector, FindingId};
/// use crate::impl_context_condition_symtab;
///
/// #[derive(Debug, Default)]
/// struct MySymtabCondition {
///     finding_collector: FindingCollector,
/// }
/// // Apply the macro
/// impl_context_condition_symtab!(MySymtabCondition);
///
/// // Required by the macro, can be empty if not used
/// impl VsidlVisitor for MySymtabCondition {}
/// // Required by the macro
/// impl SymbolTableChecker for MySymtabCondition { /* ... check methods ... */ }
///
/// ```
#[macro_export]
macro_rules! impl_context_condition_symtab {
    ($type_name:ident) => {
        // Note: Unlike impl_context_condition, this macro doesn't add an empty
        // `impl SymbolTableChecker`. It assumes the user provides a meaningful
        // implementation as this macro is intended for symbol table checks.

        impl $crate::ContextCondition for $type_name {
            /// Returns a clone of the internal finding collector.
            fn finding_collector(&mut self) -> $crate::FindingCollector
            where
                Self: $crate::ContextCondition + Default,
            {
                self.finding_collector.clone()
            }
        }
    };
}
