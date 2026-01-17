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

use super::{Finding, FindingId, FindingSeverity};
use anyhow::bail;
use std::collections::HashSet;
use std::io::Write;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use vsidl_v1::syntax::Span;
use vsidl_v1_common::DiskFileProvider;

/// Collects findings (warnings, errors, fatal errors) encountered during context condition checks.
///
/// It stores findings along with their severity, a unique identifier ([FindingId]), and a descriptive message.
/// It also allows disabling specific finding IDs and merging findings from other collectors.
#[derive(Debug, Default, Clone)]
pub struct FindingCollector {
    /// Stores the collected findings.
    //TODO(b/381038068): Add source location
    findings: Vec<Finding>,
    /// Keeps track of [FindingId]s that should be ignored and not reported.
    disabled_finding_ids: HashSet<FindingId>,
}

impl FindingCollector {
    /// Records a warning finding.
    ///
    /// Warnings indicate potential issues but do not stop the compilation process.
    ///
    /// # Parameters
    ///
    /// * `id` - The unique identifier ([FindingId]) for this type of warning.
    /// * `message` - A descriptive message explaining the warning.
    pub fn warn(&mut self, id: FindingId, message: impl Into<String>, spans: Vec<Option<Span>>) {
        self.findings.push(Finding {
            severity: FindingSeverity::Warn,
            id,
            message: message.into(),
            spans,
        });
    }

    /// Records an error finding.
    ///
    /// Errors indicate problems that prevent successful compilation. The process will typically
    /// stop after all context condition checks are complete if errors are found.
    ///
    /// # Parameters
    ///
    /// * `id` - The unique identifier ([FindingId]) for this type of error.
    /// * `message` - A descriptive message explaining the error.
    pub fn error(&mut self, id: FindingId, message: impl Into<String>, spans: Vec<Option<Span>>) {
        self.findings.push(Finding {
            severity: FindingSeverity::Error,
            id,
            message: message.into(),
            spans,
        });
    }

    /// Records a fatal error finding and immediately stops the execution.
    ///
    /// Fatal errors indicate severe problems that prevent further processing. The program
    /// will print all findings collected so far and then panic.
    ///
    /// # Parameters
    ///
    /// * `id` - The unique identifier ([FindingId]) for this type of fatal error.
    /// * `message` - A descriptive message explaining the fatal error.
    ///
    /// # Panics
    ///
    /// This function will always panic after recording the finding and printing the summary,
    /// except when running in a test environment (`#[cfg(not(test))]`).
    pub fn fatal(&mut self, id: FindingId, message: impl Into<String>, spans: Vec<Option<Span>>) {
        self.findings.push(Finding {
            severity: FindingSeverity::Fatal,
            id,
            message: message.into(),
            spans,
        });
        self.print_findings();
        #[cfg(not(test))] // In tests, fatal errors should not quit the test
        panic!("Fatal error encountered.");
    }

    /// Returns a vector containing all currently recorded findings, excluding disabled ones.
    ///
    /// Findings whose IDs are in the `disabled_finding_ids` set are filtered out.
    ///
    /// # Returns
    ///
    /// A `Vec<Finding>` containing the findings.
    pub fn findings(&self) -> Vec<Finding> {
        self.findings
            .iter()
            .filter(|finding| !self.disabled_finding_ids.contains(&finding.id))
            .cloned()
            .collect()
    }

    /// Disables reporting for a specific finding ID.
    ///
    /// Findings with the specified [FindingId] will not be included in the output of
    /// [FindingCollector::findings], [FindingCollector::print_findings], or contribute to the error count.
    ///
    /// # Parameters
    ///
    /// * `id` - The [FindingId] to disable.
    pub fn disable_finding_id(&mut self, id: FindingId) {
        self.disabled_finding_ids.insert(id);
    }

    /// Removes all warnings from the findings.
    pub fn clear_warnings(&mut self) {
        self.findings.retain(|finding| finding.severity != FindingSeverity::Warn);
    }

    /// Merges findings from another [FindingCollector] into this one.
    ///
    /// This is useful for combining results from multiple stages or parallel checks.
    /// Findings from `other` that are disabled in `other` will not be added.
    /// Findings added to `self` will still be subject to `self`'s `disabled_finding_ids`.
    ///
    /// # Parameters
    ///
    /// * `other` - The [FindingCollector] whose findings will be added to `self`.
    pub fn merge(&mut self, other: FindingCollector) {
        self.findings.extend(other.findings());
    }

    /// Prints all active findings to standard error (stderr).
    ///
    /// Each finding is formatted with ANSI color codes for emphasis, showing its
    /// severity, ID, and message. Findings disabled via [FindingCollector::disable_finding_id] are not printed.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` containing the formatted messages that were printed.
    pub fn print_findings(&self) -> Vec<String> {
        let mut stderr = StandardStream::stderr(ColorChoice::Auto);
        let mut messages = Vec::new();
        let file_provider = DiskFileProvider {};
        for finding in &self.findings() {
            finding.write_to_stream(&mut stderr, &file_provider).unwrap();
            messages.push(finding.to_string());
        }
        messages
    }

    /// Prints all active findings to stderr, but only if at least one [FindingSeverity::Error]
    /// or [FindingSeverity::Fatal] finding exists.
    ///
    /// This is useful for displaying errors only when the compilation process needs to be halted.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` containing the formatted messages that were printed, or an empty vector
    /// if no errors or fatal errors were found.
    pub fn print_findings_if_error(&self) -> Vec<String> {
        if self.error_count() > 0 {
            eprintln!();
            return self.print_findings();
        }
        Vec::new()
    }

    /// Counts the number of active findings with severity [FindingSeverity::Error] or [FindingSeverity::Fatal].
    ///
    /// Findings disabled via [FindingCollector::disable_finding_id] are not counted.
    ///
    /// # Returns
    ///
    /// The total count of error and fatal findings as a `u32`.
    pub fn error_count(&self) -> u32 {
        self.findings()
            .iter()
            .filter(|f| {
                f.severity == FindingSeverity::Error || f.severity == FindingSeverity::Fatal
            })
            .count() as u32
    }

    /// Checks if any error or fatal error findings have been recorded and returns an error if so.
    ///
    /// If the [FindingCollector::error_count] is greater than zero, this function prints a summary
    /// message to stderr and returns an [anyhow::Error]. Otherwise, it returns `Ok(())`.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if no active error or fatal findings exist.
    /// * `Err(anyhow::Error)` if one or more active error or fatal findings exist.
    pub fn error_summary(&self) -> anyhow::Result<()> {
        let error_count = self.error_count();

        if error_count > 0 {
            let mut stderr = StandardStream::stderr(ColorChoice::Auto);
            stderr.set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_bold(true)).unwrap();
            write!(&mut stderr, "error").unwrap();
            stderr.reset().unwrap();
            write!(&mut stderr, ": ").unwrap();
            stderr.set_color(ColorSpec::new().set_bold(true)).unwrap();
            write!(
                &mut stderr,
                "aborting due to {} error{}",
                error_count,
                if error_count == 1 { "" } else { "s" }
            )
            .unwrap();
            stderr.reset().unwrap();
            writeln!(&mut stderr).unwrap();
            bail!(
                "aborting due to {} error{}",
                error_count,
                if error_count == 1 { "" } else { "s" }
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warn() {
        // GIVEN a FindingCollector
        let mut collector = FindingCollector::default();

        // WHEN a warning is added
        collector.warn(FindingId::MultipleRpcServersForSameInterface, "test warning", Vec::new());

        // THEN the warning is present in the findings
        let findings = collector.findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::Warn);
        assert_eq!(findings[0].id, FindingId::MultipleRpcServersForSameInterface);
        assert_eq!(findings[0].message, "test warning");
    }

    #[test]
    fn test_error() {
        // GIVEN a FindingCollector
        let mut collector = FindingCollector::default();

        // WHEN an error is added
        collector.error(
            FindingId::UserDefinedNameNamingConventionFirstChar,
            "test error",
            Vec::new(),
        );

        // THEN the error is present in the findings
        let findings = collector.findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::Error);
        assert_eq!(findings[0].id, FindingId::UserDefinedNameNamingConventionFirstChar);
        assert_eq!(findings[0].message, "test error");
    }

    #[test]
    fn test_fatal() {
        // GIVEN a FindingCollector
        let mut collector = FindingCollector::default();

        // WHEN a fatal error is added
        collector.fatal(FindingId::SdvPublicationMessageCountPositive, "test fatal", Vec::new());

        // THEN the fatal error is present in the findings
        let findings = collector.findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::Fatal);
        assert_eq!(findings[0].id, FindingId::SdvPublicationMessageCountPositive);
        assert_eq!(findings[0].message, "test fatal");
    }

    #[test]
    fn test_disable_finding_id() {
        // GIVEN a FindingCollector with a warning and an error
        let mut collector = FindingCollector::default();
        collector.warn(FindingId::MultipleRpcServersForSameInterface, "test warning", Vec::new());
        collector.error(
            FindingId::UserDefinedNameNamingConventionFirstChar,
            "test error",
            Vec::new(),
        );

        // WHEN the warning's finding ID is disabled
        collector.disable_finding_id(FindingId::MultipleRpcServersForSameInterface);

        // THEN the warning is not present in the findings, but the error is
        let findings = collector.findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::Error);
        assert_eq!(findings[0].id, FindingId::UserDefinedNameNamingConventionFirstChar);
        assert_eq!(findings[0].message, "test error");
    }

    #[test]
    fn test_merge() {
        // GIVEN two FindingCollectors, each with a finding
        let mut collector1 = FindingCollector::default();
        collector1.warn(FindingId::MultipleRpcServersForSameInterface, "test warning", Vec::new());

        let mut collector2 = FindingCollector::default();
        collector2.error(
            FindingId::UserDefinedNameNamingConventionFirstChar,
            "test error",
            Vec::new(),
        );

        // WHEN collector2 is merged into collector1
        collector1.merge(collector2);

        // THEN collector1 contains both findings
        let findings = collector1.findings();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].severity, FindingSeverity::Warn);
        assert_eq!(findings[0].id, FindingId::MultipleRpcServersForSameInterface);
        assert_eq!(findings[0].message, "test warning");
        assert_eq!(findings[1].severity, FindingSeverity::Error);
        assert_eq!(findings[1].id, FindingId::UserDefinedNameNamingConventionFirstChar);
        assert_eq!(findings[1].message, "test error");
    }

    #[test]
    fn test_print_findings() {
        // GIVEN a FindingCollector with a warning and an error
        let mut collector = FindingCollector::default();
        collector.warn(FindingId::MultipleRpcServersForSameInterface, "test warning", Vec::new());
        collector.error(
            FindingId::UserDefinedNameNamingConventionFirstChar,
            "test error",
            Vec::new(),
        );

        // WHEN print_findings is called
        let printed_findings = collector.print_findings();

        // THEN the returned vector contains the formatted findings
        assert_eq!(printed_findings.len(), 2);
        assert_eq!(
            printed_findings[0],
            format!(
                "{}",
                Finding {
                    severity: FindingSeverity::Warn,
                    id: FindingId::MultipleRpcServersForSameInterface,
                    message: "test warning".to_string(),
                    spans: Vec::new(),
                }
            )
        );
        assert_eq!(
            printed_findings[1],
            format!(
                "{}",
                Finding {
                    severity: FindingSeverity::Error,
                    id: FindingId::UserDefinedNameNamingConventionFirstChar,
                    message: "test error".to_string(),
                    spans: Vec::new(),
                }
            )
        );
    }

    #[test]
    fn test_error_summary_no_errors() {
        // GIVEN a FindingCollector with no errors
        let collector = FindingCollector::default();

        // WHEN error_summary is called
        let result = collector.error_summary();

        // THEN the result is Ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_summary_with_errors() {
        // GIVEN a FindingCollector with an error
        let mut collector = FindingCollector::default();
        collector.error(
            FindingId::UserDefinedNameNamingConventionFirstChar,
            "test error",
            Vec::new(),
        );

        // WHEN error_summary is called
        let result = collector.error_summary();

        // THEN the result is an Err
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), format!("aborting due to 1 error"));
    }

    #[test]
    fn test_error_summary_with_warnings() {
        // GIVEN a FindingCollector with only warnings
        let mut collector = FindingCollector::default();
        collector.warn(FindingId::MultipleRpcServersForSameInterface, "test warning", Vec::new());

        // WHEN error_summary is called
        let result = collector.error_summary();

        // THEN the result is Ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_summary_with_disabled_errors() {
        // GIVEN a FindingCollector with an error, which is disabled
        let mut collector = FindingCollector::default();
        collector.error(
            FindingId::UserDefinedNameNamingConventionFirstChar,
            "test error",
            Vec::new(),
        );
        collector.disable_finding_id(FindingId::UserDefinedNameNamingConventionFirstChar);

        // WHEN error_summary is called
        let result = collector.error_summary();

        // THEN the result is Ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_findings_if_error_with_errors() {
        // GIVEN a FindingCollector with an error
        let mut collector = FindingCollector::default();
        collector.error(
            FindingId::UserDefinedNameNamingConventionFirstChar,
            "test error",
            Vec::new(),
        );

        // WHEN print_findings_if_error is called
        let printed_findings = collector.print_findings_if_error();

        // THEN the findings are printed
        assert_eq!(printed_findings.len(), 1);
        assert_eq!(
            printed_findings[0],
            format!(
                "{}",
                Finding {
                    severity: FindingSeverity::Error,
                    id: FindingId::UserDefinedNameNamingConventionFirstChar,
                    message: "test error".to_string(),
                    spans: Vec::new(),
                }
            )
        );
    }

    #[test]
    fn test_print_findings_if_error_no_errors() {
        // GIVEN a FindingCollector with no errors
        let collector = FindingCollector::default();

        // WHEN print_findings_if_error is called
        let printed_findings = collector.print_findings_if_error();

        // THEN no findings are printed
        assert_eq!(printed_findings.len(), 0);
    }

    #[test]
    fn test_print_findings_if_error_with_warnings() {
        // GIVEN a FindingCollector with only warnings
        let mut collector = FindingCollector::default();
        collector.warn(FindingId::MultipleRpcServersForSameInterface, "test warning", Vec::new());

        // WHEN print_findings_if_error is called
        let printed_findings = collector.print_findings_if_error();

        // THEN no findings are printed
        assert_eq!(printed_findings.len(), 0);
    }

    #[test]
    fn test_print_findings_if_error_with_fatal() {
        // GIVEN a FindingCollector with a fatal error
        let mut collector = FindingCollector::default();
        collector.fatal(FindingId::SdvPublicationMessageCountPositive, "test fatal", Vec::new());

        // WHEN print_findings_if_error is called
        let printed_findings = collector.print_findings_if_error();

        // THEN the findings are printed
        assert_eq!(printed_findings.len(), 1);
        assert_eq!(
            printed_findings[0],
            format!(
                "{}",
                Finding {
                    severity: FindingSeverity::Fatal,
                    id: FindingId::SdvPublicationMessageCountPositive,
                    message: "test fatal".to_string(),
                    spans: Vec::new(),
                }
            )
        );
    }

    #[test]
    fn test_print_findings_if_error_with_disabled_errors() {
        // GIVEN a FindingCollector with an error, which is disabled
        let mut collector = FindingCollector::default();
        collector.error(
            FindingId::UserDefinedNameNamingConventionFirstChar,
            "test error",
            Vec::new(),
        );
        collector.disable_finding_id(FindingId::UserDefinedNameNamingConventionFirstChar);

        // WHEN print_findings_if_error is called
        let printed_findings = collector.print_findings_if_error();

        // THEN no findings are printed
        assert_eq!(printed_findings.len(), 0);
    }

    #[test]
    fn test_error_count_no_errors() {
        // GIVEN a FindingCollector with no errors
        let collector = FindingCollector::default();

        // WHEN error_count is called
        let error_count = collector.error_count();

        // THEN the error count is 0
        assert_eq!(error_count, 0);
    }

    #[test]
    fn test_error_count_with_errors() {
        // GIVEN a FindingCollector with an error
        let mut collector = FindingCollector::default();
        collector.error(
            FindingId::UserDefinedNameNamingConventionFirstChar,
            "test error",
            Vec::new(),
        );

        // WHEN error_count is called
        let error_count = collector.error_count();

        // THEN the error count is 1
        assert_eq!(error_count, 1);
    }

    #[test]
    fn test_error_count_with_warnings() {
        // GIVEN a FindingCollector with a warning
        let mut collector = FindingCollector::default();
        collector.warn(FindingId::MultipleRpcServersForSameInterface, "test warning", Vec::new());

        // WHEN error_count is called
        let error_count = collector.error_count();

        // THEN the error count is 0
        assert_eq!(error_count, 0);
    }
    #[test]
    fn test_error_count_with_fatal() {
        // GIVEN a FindingCollector with a fatal error
        let mut collector = FindingCollector::default();
        collector.fatal(FindingId::SdvPublicationMessageCountPositive, "test fatal", Vec::new());

        // WHEN error_count is called
        let error_count = collector.error_count();

        // THEN the error count is 1
        assert_eq!(error_count, 1);
    }
    #[test]
    fn test_error_count_with_fatal_and_error() {
        // GIVEN a FindingCollector with a fatal error and a error
        let mut collector = FindingCollector::default();
        collector.fatal(FindingId::SdvPublicationMessageCountPositive, "test fatal", Vec::new());
        collector.error(
            FindingId::UserDefinedNameNamingConventionFirstChar,
            "test error",
            Vec::new(),
        );

        // WHEN error_count is called
        let error_count = collector.error_count();

        // THEN the error count is 2
        assert_eq!(error_count, 2);
    }
}
