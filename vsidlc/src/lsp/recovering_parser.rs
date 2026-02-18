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

use pest::error::Error as PestError;
use pest::iterators::Pairs;
use pest::Parser;
use vsidl_v1_parser::pest_parser::{Rule, VsidlParser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryStrategy {
    BackwardRestricted,
    ForwardRestricted,
    BothRestricted,
    Unrestricted,
}

/// Parses VSIDL content with syntax error recovery.
///
/// Sequentially applies increasingly aggressive recovery strategies until successful
/// or all available options are exhausted:
/// 1. Backward restricted: Replaces characters backward, but never replaces or goes beyond '{'.
/// 2. Forward restricted: Replaces characters forward, but never replaces or goes beyond '}'.
/// 3. Both restricted: Can replace both forward and backward, but stays within '{' and '}'.
/// 4. Unrestricted: Replaces characters forward without brace restrictions (fallback).
///
/// "Replacing" (or patching) means overwriting invalid code with an equal number of spaces
/// instead of deleting it. This "blanks out" the error so the parser can skip it, but
/// ensures that every other character in the file stays at its original position.
/// This technique is used so that IDE error markers and line numbers for the rest of
/// the file remain accurate.
///
/// Example:
///   Before: `publisher { message: "Tire" toooopic: "front-left" capacity: 8 }`
///   After:  `publisher { message: "Tire"                        capacity: 8 }`
///
/// Returns the final parse result and all errors encountered (if any).
/// The `content` string is modified in-place to include whitespace where errors were.
pub(crate) fn parse_vsidl_with_recovery<'a>(
    content: &'a mut String,
) -> (Result<Pairs<'a, Rule>, PestError<Rule>>, Vec<PestError<Rule>>) {
    let original_content = content.clone();
    let mut errors: Vec<PestError<Rule>> = Vec::new();

    for strategy in [
        RecoveryStrategy::BackwardRestricted,
        RecoveryStrategy::ForwardRestricted,
        RecoveryStrategy::BothRestricted,
        RecoveryStrategy::Unrestricted,
    ] {
        let mut working_copy = original_content.clone();
        let mut current_errors: Vec<PestError<Rule>> = Vec::new();
        let mut next_recovery_pos_forward = 0;
        let mut next_recovery_pos_backward = working_copy.len();

        loop {
            match VsidlParser::parse(Rule::vsidl_file, &working_copy) {
                Ok(_) => {
                    // Success! Apply this working copy to original content and return.
                    *content = working_copy;
                    // Deduplicate errors by their start position.
                    let mut unique_errors = Vec::new();
                    let mut seen_positions = std::collections::HashSet::new();
                    for err in current_errors {
                        let pos = match err.location {
                            pest::error::InputLocation::Pos(pos) => pos,
                            pest::error::InputLocation::Span((start, _)) => start,
                        };
                        if seen_positions.insert(pos) {
                            unique_errors.push(err);
                        }
                    }

                    // The first parse call is performed on &working_copy.
                    // Since working_copy is a local String variable created inside the loop,
                    // the resulting Pairs would be tied to its local lifetime. If you tried
                    // to return those Pairs, the compiler would reject it because working_copy
                    // is destroyed when the function (or even the loop iteration) ends.
                    // To satisfy the 'a lifetime requirement in the return type, the parser
                    // must be run specifically against the string that content points to
                    // after it has been updated.
                    match VsidlParser::parse(Rule::vsidl_file, content) {
                        Ok(pairs) => return (Ok(pairs), unique_errors),
                        Err(e) => return (Err(e), unique_errors),
                    }
                }
                Err(e) => {
                    current_errors.push(e.clone());

                    let error_pos = match e.location {
                        pest::error::InputLocation::Pos(pos) => pos,
                        pest::error::InputLocation::Span((start, _)) => start,
                    };

                    let patched = match strategy {
                        RecoveryStrategy::ForwardRestricted => patch_forward(
                            &mut working_copy,
                            error_pos,
                            &mut next_recovery_pos_forward,
                            true,
                        ),
                        RecoveryStrategy::BackwardRestricted => patch_backward(
                            &mut working_copy,
                            error_pos,
                            &mut next_recovery_pos_backward,
                            true,
                        ),
                        RecoveryStrategy::BothRestricted => {
                            let f = patch_forward(
                                &mut working_copy,
                                error_pos,
                                &mut next_recovery_pos_forward,
                                true,
                            );
                            let b = patch_backward(
                                &mut working_copy,
                                error_pos,
                                &mut next_recovery_pos_backward,
                                true,
                            );
                            f || b
                        }
                        RecoveryStrategy::Unrestricted => patch_forward(
                            &mut working_copy,
                            error_pos,
                            &mut next_recovery_pos_forward,
                            false,
                        ),
                    };

                    if !patched {
                        // EOF effort for Forward or Unrestricted
                        if strategy == RecoveryStrategy::ForwardRestricted
                            || strategy == RecoveryStrategy::Unrestricted
                        {
                            if let Some(first) = current_errors.first() {
                                let start = match first.location {
                                    pest::error::InputLocation::Pos(pos) => pos,
                                    pest::error::InputLocation::Span((start, _)) => start,
                                };
                                if start < working_copy.len() && start >= next_recovery_pos_forward
                                {
                                    let len = working_copy.len() - start;
                                    working_copy.replace_range(start.., &" ".repeat(len));
                                    next_recovery_pos_forward = working_copy.len();
                                    continue;
                                }
                            }
                        }
                        break; // This strategy failed
                    }
                }
            }
        }

        if errors.is_empty() {
            errors = current_errors;
        }
    }

    // Deduplicate errors by their start position.
    let mut unique_errors = Vec::new();
    let mut seen_positions = std::collections::HashSet::new();
    for err in errors {
        let pos = match err.location {
            pest::error::InputLocation::Pos(pos) => pos,
            pest::error::InputLocation::Span((start, _)) => start,
        };
        if seen_positions.insert(pos) {
            unique_errors.push(err);
        }
    }

    // Final fallback: return the errors from the first failed strategy and original content
    let err =
        unique_errors.first().cloned().expect("unique_errors should not be empty if we reach here");
    (Err(err), unique_errors)
}

fn patch_forward(
    content: &mut String,
    error_pos: usize,
    next_pos: &mut usize,
    stop_at_brace: bool,
) -> bool {
    let mut patch_start = std::cmp::max(error_pos, *next_pos);
    while patch_start < content.len() {
        let c = content[patch_start..].chars().next().unwrap();
        if stop_at_brace && c == '}' {
            return false;
        }
        if c.is_whitespace() {
            patch_start += c.len_utf8();
        } else {
            break;
        }
    }

    if patch_start >= content.len() {
        return false;
    }

    let mut patch_end = patch_start;
    if let Some(c) = content[patch_start..].chars().next() {
        patch_end += c.len_utf8();
        if c.is_alphanumeric() || c == '_' {
            while patch_end < content.len() {
                let Some(nc) = content[patch_end..].chars().next() else { break };
                if stop_at_brace && nc == '}' {
                    break;
                }
                if nc == '\n' || nc == '\r' {
                    break;
                }
                if nc.is_alphanumeric() || nc == '_' {
                    patch_end += nc.len_utf8();
                } else {
                    break;
                }
            }
        }
    }

    let len = patch_end - patch_start;
    if len == 0 {
        return false;
    }
    content.replace_range(patch_start..patch_end, &" ".repeat(len));
    *next_pos = patch_end;
    true
}

fn patch_backward(
    content: &mut String,
    error_pos: usize,
    next_pos: &mut usize,
    stop_at_brace: bool,
) -> bool {
    let patch_end_limit = std::cmp::min(error_pos, *next_pos);
    if patch_end_limit == 0 {
        return false;
    }

    // Find first non-whitespace character before patch_end_limit
    let mut patch_end = patch_end_limit;
    while patch_end > 0 {
        let c = content[..patch_end].chars().next_back().unwrap();
        if c.is_whitespace() {
            patch_end -= c.len_utf8();
        } else {
            break;
        }
    }

    if patch_end == 0 {
        return false;
    }

    let c = content[..patch_end].chars().next_back().unwrap();
    if stop_at_brace && c == '{' {
        return false;
    }

    let mut patch_start = patch_end - c.len_utf8();
    if c.is_alphanumeric() || c == '_' {
        while patch_start > 0 {
            let prev = content[..patch_start].chars().next_back().unwrap();
            if prev == '\n' || prev == '\r' {
                break;
            }
            if stop_at_brace && prev == '{' {
                break;
            }
            if prev.is_alphanumeric() || prev == '_' {
                patch_start -= prev.len_utf8();
            } else {
                break;
            }
        }
    }

    let len = patch_end - patch_start;
    if len == 0 {
        return false;
    }
    content.replace_range(patch_start..patch_end, &" ".repeat(len));
    *next_pos = patch_start;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_with_recovery_simple() {
        // "publishhher" is misspelled.
        let mut content = r##"package: "com.sdv.tm"
service_bundle {
  name: "Manager"
  publishhher { message: "TirePressure" }
  subscriber { message: "TirePressure" }
}"##
        .to_string();
        let (result, errors) = parse_vsidl_with_recovery(&mut content);
        assert!(result.is_ok());
        assert!(!errors.is_empty());
        assert!(errors[0].to_string().contains("publishhher"));
    }

    #[test]
    fn test_parse_with_recovery_multiple() {
        // Multiple errors: misspelled keyword and an extra block.
        let mut content = r##"package: "com.sdv.tm"
service_bundle {
  name: "Manager"
  publishhher { message: "TirePressure" }
  { extra_stuff }
  subscriber { message: "TirePressure" }
}"##
        .to_string();
        let (result, errors) = parse_vsidl_with_recovery(&mut content);
        assert!(result.is_ok());
        assert!(errors.len() >= 2);
        // Verify we got the first error.
        assert!(errors[0].to_string().contains("publishhher"));
        // Verify we got the second error.
        assert!(errors[1].to_string().contains("{"));
    }

    #[test]
    fn test_parse_with_recovery_missing_field_name() {
        // Missing name field in service_bundle.
        let mut content = r##"package: "com.sdv.tm"
service_bundle {
  "Manager"
  subscriber { message: "TirePressure" }
}"##
        .to_string();
        let (result, errors) = parse_vsidl_with_recovery(&mut content);
        assert!(result.is_ok());
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_parse_with_recovery_backward_needed() {
        // Error reported at 'subscriber' because of missing ':' after name.
        let mut content = r##"package: "com.sdv.tm"
service_bundle {
  name "Manager"
  subscriber { message: "TirePressure" }
}"##
        .to_string();
        let (result, errors) = parse_vsidl_with_recovery(&mut content);
        assert!(result.is_ok());
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_parse_with_recovery_missing_value_cross_line() {
        let mut content = r#"package: "pkg"
service_bundle {
  name: "Writer"
  subscriber {
    message:
  }
}
service_bundle {
  name: "Reader"
}"#
        .to_string();
        let (result, errors) = parse_vsidl_with_recovery(&mut content);
        assert!(result.is_ok(), "Should recover. Errors: {:?}", errors);
        // Ensure "Reader" is still there and not swallowed
        assert!(content.contains("Reader"));
        // Ensure "message:" was patched (replaced by spaces)
        assert!(!content.contains("message:"));
    }
}
