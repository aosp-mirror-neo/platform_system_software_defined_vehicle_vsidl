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

use is_terminal::is_terminal;
use std::io::stderr;
use std::sync::{Arc, Mutex};

/// A utility for displaying progress updates to the standard error stream.
///
/// This printer shows the progress in the format "[current/total] message".
/// When printing to an interactive terminal (TTY), it overwrites the previous
/// progress line with the new one. When printing to a file or pipe, each
/// progress step is printed on a new line.
///
/// # Examples
///
/// ```
/// let total_steps = 3;
/// let printer = ProgressPrinter::new(total_steps);
///
/// printer.print_next_progress_step("Starting task...");
/// thread::sleep(Duration::from_millis(100)); // Simulate work
/// printer.print_next_progress_step("Processing data...");
/// thread::sleep(Duration::from_millis(100)); // Simulate work
/// printer.print_next_progress_step("Finishing up.");
///
/// // Example Output (to stderr, potentially overwriting lines on a terminal):
/// // [1/3] Starting task...
/// // [2/3] Processing data...
/// // [3/3] Finishing up.
/// ```
pub struct ProgressPrinter {
    /// The number of completed process steps.
    current: Arc<Mutex<usize>>,
    /// The total number of steps expected in the process.
    total: usize,
    /// Flag indicating if the standard error stream is connected to a terminal.
    is_terminal: bool,
}

impl ProgressPrinter {
    /// Creates a new [ProgressPrinter].
    ///
    /// Initializes the progress counter to 0 and determines if the output
    /// is being sent to an interactive terminal.
    ///
    /// # Parameters
    ///
    /// * `total` - The total number of steps expected for the task being monitored.
    ///
    /// # Returns
    ///
    /// A new instance of [ProgressPrinter].
    pub fn new(total: usize) -> Self {
        Self { current: Arc::new(Mutex::new(0)), total, is_terminal: is_terminal(stderr()) }
    }

    /// Prints the next progress step message and increments the internal counter.
    ///
    /// Displays the progress in the format "[current/total] message" to the
    /// standard error stream. If connected to a terminal, it attempts to
    /// overwrite the previous progress line. Otherwise (e.g., redirecting to a file),
    /// it prints each step on a new line. A newline is always added after the
    /// final step or when not printing to a terminal.
    ///
    /// # Parameters
    ///
    /// * `message` - The descriptive message for the current progress step.
    ///
    /// # Panics
    ///
    /// * If the internal mutex protecting the current step count is poisoned.
    pub fn print_next_progress_step(&self, message: &str) {
        let mut current = self.current.lock().unwrap();
        *current += 1;

        if self.is_terminal {
            // Only clear lines if we're in a terminal, so that printing to a file
            // doesn't get obfuscated with ANSI codes
            eprint!("{}{}", anes::ClearLine::Left, anes::MoveCursorToColumn(1));
        }
        eprint!(
            "[{}/{}] {}{}",
            *current,
            self.total,
            message,
            if *current == self.total || !self.is_terminal { "\n" } else { "" }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // Note: Without a dedicated library for capturing stderr (like `gag`)
    // or modifying the production code to inject a writer, directly testing
    // the exact string output of `eprint!` in unit tests is complex and can
    // be unreliable using only the standard library or general-purpose I/O
    // libraries for this specific macro.
    // These tests focus on the state changes (the progress counter) and
    // exercising the different code paths.

    #[test]
    fn test_print_next_progress_step_terminal_not_last_step() {
        let printer = ProgressPrinter {
            current: Arc::new(Mutex::new(0)),
            total: 3,
            is_terminal: true, // Manually set for test case
        };

        // We expect ANSI codes for clearing and moving cursor, then "[1/3] Test message 1"
        // but cannot easily capture/assert the eprint! output directly here.
        printer.print_next_progress_step("Test message 1");
        assert_eq!(*printer.current.lock().unwrap(), 1, "Counter should increment");
    }

    #[test]
    fn test_print_next_progress_step_terminal_last_step() {
        let printer = ProgressPrinter {
            current: Arc::new(Mutex::new(2)), // Start near the end
            total: 3,
            is_terminal: true, // Manually set for test case
        };

        // We expect ANSI codes, then "[3/3] Final message\n" (with newline)
        printer.print_next_progress_step("Final message");
        assert_eq!(*printer.current.lock().unwrap(), 3, "Counter should increment to total");
    }

    #[test]
    fn test_print_next_progress_step_non_terminal_not_last_step() {
        let printer = ProgressPrinter {
            current: Arc::new(Mutex::new(0)),
            total: 3,
            is_terminal: false, // Manually set for test case
        };

        // We expect "[1/3] Test message 1\n" (with newline)
        printer.print_next_progress_step("Test message 1");
        assert_eq!(*printer.current.lock().unwrap(), 1, "Counter should increment");
    }

    #[test]
    fn test_print_next_progress_step_non_terminal_last_step() {
        let printer = ProgressPrinter {
            current: Arc::new(Mutex::new(2)), // Start near the end
            total: 3,
            is_terminal: false, // Manually set for test case
        };
        // We expect "[3/3] Final message\n" (with newline)
        printer.print_next_progress_step("Final message");
        assert_eq!(*printer.current.lock().unwrap(), 3, "Counter should increment to total");
    }
}
