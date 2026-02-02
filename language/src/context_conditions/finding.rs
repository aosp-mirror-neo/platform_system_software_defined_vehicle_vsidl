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

use super::FindingId;
use super::FindingSeverity;
use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use std::cmp::max;
use std::fmt;
use std::io::Write;
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};
use vsidl_v1::syntax::Span;
use vsidl_v1_common::fs_utils::{offset_to_line_col, print_spans};

/// Represents a single finding (warning, error, or fatal error) encountered during
/// context condition checks.
///
/// It encapsulates the severity of the finding, a unique identifier ([FindingId]),
/// and a descriptive message.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub severity: FindingSeverity,
    pub id: FindingId,
    pub message: String,
    pub spans: Vec<Option<Span>>,
}

impl Finding {
    /// Formats and writes the finding to a color-enabled stream.
    pub fn write_to_stream<F: vsidl_v1_common::FileProvider>(
        &self,
        stream: &mut StandardStream,
        file_provider: &F,
    ) -> std::io::Result<()> {
        let color = match self.severity {
            FindingSeverity::Error | FindingSeverity::Fatal => Color::Red,
            FindingSeverity::Warn => Color::Yellow,
        };
        stream.set_color(ColorSpec::new().set_fg(Some(color)).set_bold(true))?;
        write!(stream, "{}[{}]", self.severity, self.id)?;
        stream.reset()?;
        write!(stream, ": ")?;
        stream.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(stream, "{}", self.message)?;
        let _ = print_spans(file_provider, stream, &self.spans);
        stream.reset()?;
        stream.set_color(ColorSpec::new().set_bold(true))?;
        let severity = match self.severity {
            FindingSeverity::Error | FindingSeverity::Fatal => "error",
            FindingSeverity::Warn => "warning",
        };
        writeln!(
            stream,
            "For more information about this {}, try 'vsidlc --explain {}'",
            severity, self.id
        )?;
        stream.reset()
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // This Display impl is now primarily for non-colored contexts,
        // like logging to a file or for tests that check the message content.
        write!(f, "{}[{}]: {}", self.severity, self.id, self.message)
    }
}

impl Finding {
    pub fn to_diagnostic<F: vsidl_v1_common::FileProvider>(&self, file_provider: &F) -> Diagnostic {
        let severity = match self.severity {
            FindingSeverity::Error | FindingSeverity::Fatal => DiagnosticSeverity::ERROR,
            FindingSeverity::Warn => DiagnosticSeverity::WARNING,
        };

        let range = if let Some(Some(span)) = self.spans.first() {
            let (start_line, start_col) =
                offset_to_line_col(file_provider, &span.file_path, span.start.try_into().unwrap())
                    .unwrap();
            let (end_line, end_col) =
                offset_to_line_col(file_provider, &span.file_path, span.end.try_into().unwrap())
                    .unwrap();

            Range {
                // The "-1" is a fix for the fact that ASfP starts counting lines from zero instead of one
                start: Position {
                    line: (max(start_line, 1) - 1) as u32,
                    character: (max(start_col, 1) - 1) as u32,
                },
                end: Position {
                    line: (max(end_line, 1) - 1) as u32,
                    character: (max(end_col, 1) - 1) as u32,
                },
            }
        } else {
            // Default to a zero-length range at the start of the file if no span is available
            Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 0 },
            }
        };

        Diagnostic {
            range,
            severity: Some(severity),
            code: Some(lsp_types::NumberOrString::String(self.id.to_string())),
            source: Some("VSIDLC".to_string()),
            message: self.message.clone(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finding_display() {
        // GIVEN a Finding
        let finding = Finding {
            severity: FindingSeverity::Error,
            id: FindingId::UserDefinedNameNamingConventionFirstChar,
            message: "Test message".to_string(),
            spans: Vec::new(),
        };

        // WHEN it is displayed
        let display_str = finding.to_string();

        // THEN the output is formatted as expected
        assert_eq!(display_str, "error[E201]: Test message");
    }
}
