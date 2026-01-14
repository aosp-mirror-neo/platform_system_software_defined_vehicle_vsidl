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

use std::fmt::{self, Display};

/// Enum representing the severity of a finding encountered during VSIDL processing.
///
/// Findings indicate potential issues or errors in the VSIDL definitions.
/// The severity determines how the finding is treated by the compiler or checker.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum FindingSeverity {
    /// Represents a warning.
    ///
    /// Warnings indicate potential issues or deviations from best practices
    /// but do not prevent successful compilation or code generation.
    /// They should be reviewed but do not cause failure.
    Warn,
    /// Represents a standard error.
    ///
    /// Errors indicate definite problems in the VSIDL definitions that violate
    /// context conditions. Errors are collected during the checking process
    /// and will cause the overall process to fail after all checks are complete.
    Error,
    /// Represents a fatal error.
    ///
    /// Fatal errors indicate severe problems that prevent further processing.
    /// Encountering a fatal error should cause the compiler or checker to
    /// stop immediately.
    Fatal,
}

/// Implements the [Display] trait for [FindingSeverity].
///
/// This allows [FindingSeverity] channels to be easily converted to their
/// lowercase string representation (e.g., "warn", "error", "fatal").
impl Display for FindingSeverity {
    /// Formats the [FindingSeverity] for display purposes.
    ///
    /// Converts the enum channel name to lowercase.
    ///
    /// # Arguments
    ///
    /// * `f` - A mutable reference to a [std::fmt::Formatter].
    ///
    /// # Returns
    ///
    /// * [std::fmt::Result] - Ok if formatting was successful, Err otherwise.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format!("{self:?}").to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_warn() {
        assert_eq!(FindingSeverity::Warn.to_string(), "warn");
    }

    #[test]
    fn test_display_error() {
        assert_eq!(FindingSeverity::Error.to_string(), "error");
    }

    #[test]
    fn test_display_fatal() {
        assert_eq!(FindingSeverity::Fatal.to_string(), "fatal");
    }
}
