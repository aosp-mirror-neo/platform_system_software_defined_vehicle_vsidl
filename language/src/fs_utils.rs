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

use crate::file_provider::FileProvider;
use glob::glob;
use std::collections::HashMap;
use std::fs;
use std::io::{self, prelude::*, BufReader};
use std::path::Path;
use std::path::PathBuf;
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};
use vsidl_v1::syntax::Span;

/// Finds files within a specified directory that match a given glob pattern.
///
/// This function searches recursively within the `dir_path` for files matching
/// the `regex_pattern` (which is interpreted as a glob pattern).
///
/// # Parameters
///
/// * `dir_path` - The path to the directory to start the search from.
/// * `regex_pattern` - The glob pattern used to match file names (e.g., `*.txt`, `**/*.rs`).
///
/// # Returns
///
/// A vector of `PathBuf` objects representing the paths of the matched files.
///
/// # Errors
///
/// If the glob pattern is invalid or if there's an I/O error while accessing the file system
/// during the search.
pub fn find_matching_files_glob(
    dir_path: &str,
    regex_pattern: &str,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut matches = Vec::new();

    let pattern = format!("{dir_path}/{regex_pattern}"); // Construct the glob pattern
    for file in glob(&pattern).unwrap() {
        matches.push(file?);
    }

    Ok(matches)
}

/// Converts a byte offset in a file to a 1-indexed line and column number.
///
/// This function reads a file and determines the line and column
/// corresponding to a given byte offset.
///
/// # Parameters
///
/// * `file_path` - The path to the file.
/// * `offset` - The byte offset to convert.
///
/// # Returns
///
/// A tuple containing the 1-indexed line and column numbers.
///
/// # Errors
///
/// If the file cannot be opened or read, or if the offset is out of bounds.
pub fn offset_to_line_col<F: FileProvider>(
    file_provider: &F,
    file_path: &str,
    offset: usize,
) -> anyhow::Result<(usize, usize)> {
    let path = Path::new(file_path);
    let content = file_provider.read_to_string(path)?;
    let reader = BufReader::new(content.as_bytes());

    let mut line_number = 1;
    let mut current_offset = 0;

    for line in reader.lines() {
        let line_content = line?;
        // The length includes the newline character, which is consumed by lines()
        // but its byte length needs to be accounted for.
        // On Unix-like systems, this is 1 byte ('\n'). On Windows, it's 2 ('\r\n').
        // lines() handles this, so we add 1 for the newline character it strips.
        // A more robust solution might use line_content.as_bytes().len().
        let line_len = line_content.len() + 1;

        if current_offset + line_len > offset {
            let column_number = offset - current_offset + 1;
            return Ok((line_number, column_number));
        }

        current_offset += line_len;
        line_number += 1;
    }

    Err(anyhow::anyhow!("Offset out of bounds"))
}

/// Converts a 1-indexed line and column number in a file to a 0-indexed byte offset.
///
/// This function reads a file and determines the byte offset
/// corresponding to a given line and column number.
///
/// # Parameters
///
/// * `file_path` - The path to the file.
/// * `target_line` - The 1-indexed line number.
/// * `target_column` - The 1-indexed column number.
///
/// # Returns
///
/// The 0-indexed byte offset.
///
/// # Errors
///
/// If the file cannot be opened or read, or if the line/column is out of bounds.
pub fn line_col_to_offset<F: FileProvider>(
    file_provider: &F,
    file_path: &str,
    target_line: usize,
    target_column: usize,
) -> anyhow::Result<usize> {
    let path = Path::new(file_path);
    let content = file_provider.read_to_string(path)?;
    let reader = BufReader::new(content.as_bytes());

    let mut current_line_number = 1;
    let mut current_offset = 0;

    for line in reader.lines() {
        let line_content = line?;
        if current_line_number == target_line {
            let mut char_indices = line_content.char_indices();
            let byte_offset_in_line = if target_column > 1 {
                char_indices.nth(target_column - 1).map(|(idx, _)| idx).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "Column out of bounds")
                })?
            } else {
                0
            };
            return Ok(current_offset + byte_offset_in_line);
        }
        current_offset += line_content.len() + 1; // +1 for newline
        current_line_number += 1;
    }

    Err(anyhow::anyhow!("Line out of bounds"))
}

/// Prints a span from a file with bold blue line numbers and pipes.
///
/// This function takes a `StandardStream` and a slice of `Span` objects,
/// groups them by file, and prints the relevant lines from each file,
/// highlighting the line numbers and providing context for the spans.
///
/// # Parameters
///
/// * `stream` - A mutable reference to a `StandardStream` for colored output.
/// * `spans` - A slice of `Option<Span>` to be printed.
///
/// # Returns
///
/// An `io::Result` indicating success or failure.
///
/// # Errors
///
/// If there is an I/O error while writing to the stream or reading from a file.
pub fn print_spans<F: FileProvider>(
    file_provider: &F,
    stream: &mut StandardStream,
    spans: &[Option<Span>],
) -> anyhow::Result<()> {
    // 1. Group spans by file path.
    let mut file_spans: HashMap<&str, Vec<(u32, u32)>> = HashMap::new();
    for span in spans.iter().flatten() {
        file_spans.entry(&span.file_path).or_default().push((span.start, span.end));
    }

    // 2. Iterate through each file and print its spans.
    for (file_path, offsets) in file_spans {
        // Print a header for the file.
        stream.set_color(ColorSpec::new().set_fg(Some(Color::Blue)).set_bold(true))?;
        write!(stream, "  --> {}", file_path)?;
        if offsets.len() == 1 {
            let (start, _) = offsets[0];
            let (line, column) =
                offset_to_line_col(file_provider, file_path, start.try_into().unwrap())?;
            write!(stream, ":{}:{}", line, column)?;
        }
        writeln!(stream)?;
        stream.reset()?;

        // Get the line numbers for each span and sort them.
        let mut lines_to_print = Vec::new();
        for &(start, end) in &offsets {
            let (start_line, _) =
                offset_to_line_col(file_provider, file_path, start.try_into().unwrap())?;
            let (end_line, _) =
                offset_to_line_col(file_provider, file_path, end.try_into().unwrap())?;
            for line in start_line..=end_line {
                lines_to_print.push(line);
            }
        }
        lines_to_print.sort_unstable();
        lines_to_print.dedup();

        if lines_to_print.is_empty() {
            continue;
        }

        let content = file_provider.read_to_string(Path::new(file_path))?;
        let mut reader = BufReader::new(content.as_bytes());
        let mut current_line_number = 1;
        let mut line = String::new();
        let mut last_printed_line = 0;

        stream.set_color(ColorSpec::new().set_fg(Some(Color::Blue)).set_bold(true))?;
        writeln!(stream, "     |")?;

        while reader.read_line(&mut line)? > 0 {
            if lines_to_print.contains(&current_line_number) {
                // Check if there was a gap between the last printed line.
                if last_printed_line != 0 && current_line_number > last_printed_line + 1 {
                    stream.set_color(ColorSpec::new().set_fg(Some(Color::Blue)).set_bold(true))?;
                    writeln!(stream, " ... |")?;
                    stream.reset()?;
                }

                // Print line number and line content.
                stream.set_color(ColorSpec::new().set_fg(Some(Color::Blue)).set_bold(true))?;
                write!(stream, "{:4} |", current_line_number)?;
                stream.reset()?;
                writeln!(stream, " {}", line.trim_end())?;
                last_printed_line = current_line_number;
            }
            current_line_number += 1;
            line.clear();
        }

        stream.set_color(ColorSpec::new().set_fg(Some(Color::Blue)).set_bold(true))?;
        writeln!(stream, "     |")?;
        stream.reset()?;
    }

    Ok(())
}

/// Recursively copies the contents of the source directory to the destination directory.
pub fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    // 1. Ensure the destination directory exists.
    fs::create_dir_all(&dst)?;

    // 2. Iterate over the contents of the source directory.
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;

        // 3. Define the path for the current item in the destination directory.
        let dest_path = dst.as_ref().join(entry.file_name());

        if file_type.is_dir() {
            // 4. If it's a directory, recursively call this function.
            copy_dir_all(entry.path(), &dest_path)?;
        } else if file_type.is_file() {
            // 5. If it's a file, copy the file.
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // ------------------------ //
    // find_matching_files_glob //
    // ------------------------ //

    #[test]
    fn test_find_matching_files_glob() {
        // GIVEN
        //     /tmp/random_folder/test1.txt
        //     /tmp/random_folder/test2.rs
        //     /tmp/random_folder/test3.txt
        //     /tmp/random_folder/subdir/test4.txt
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().to_string_lossy().to_string();

        // Create some test files
        let file1_path = temp_dir.path().join("test1.txt");
        let file2_path = temp_dir.path().join("test2.rs");
        let file3_path = temp_dir.path().join("test3.txt");
        let file4_path = temp_dir.path().join("subdir/test4.txt");

        fs::create_dir_all(temp_dir.path().join("subdir")).unwrap();

        fs::File::create(&file1_path).unwrap();
        fs::File::create(&file2_path).unwrap();
        fs::File::create(&file3_path).unwrap();
        fs::File::create(&file4_path).unwrap();

        // WHEN matching *.txt
        let mut matches = find_matching_files_glob(&temp_path, "*.txt").unwrap();
        // THEN test1.txt and text3.txt are returned
        assert_eq!(matches.len(), 2);
        matches.sort();
        assert_eq!(
            matches,
            vec![
                PathBuf::from(format!("{temp_path}/test1.txt")),
                PathBuf::from(format!("{temp_path}/test3.txt"))
            ]
        );

        // WHEN matching *.rs
        let matches = find_matching_files_glob(&temp_path, "*.rs").unwrap();
        // THEN test2.rs is returned
        assert_eq!(matches.len(), 1);
        assert_eq!(matches, vec![PathBuf::from(format!("{temp_path}/test2.rs"))]);

        // WHEN matching *.xyz
        let matches = find_matching_files_glob(&temp_path, "*.xyz").unwrap();
        // THEN no file is returned
        assert_eq!(matches.len(), 0);

        // WHEN matching subdir/*.txt
        let matches = find_matching_files_glob(&temp_path, "subdir/*.txt").unwrap();
        // THEN only test4.txt is returned
        assert_eq!(matches.len(), 1);
        assert_eq!(matches, vec![PathBuf::from(format!("{temp_path}/subdir/test4.txt"))]);

        // WHEN matching **/*.txt
        let mut matches = find_matching_files_glob(&temp_path, "**/*.txt").unwrap();
        // THEN all .txt files are returned (test1.txt ,test3.txt, test4.txt)
        assert_eq!(matches.len(), 3);
        matches.sort();
        assert_eq!(
            matches,
            vec![
                PathBuf::from(format!("{temp_path}/subdir/test4.txt")),
                PathBuf::from(format!("{temp_path}/test1.txt")),
                PathBuf::from(format!("{temp_path}/test3.txt"))
            ]
        );
    }

    // ------------------ //
    // offset_to_line_col //
    // ------------------ //

    #[test]
    fn test_offset_to_line_col() {
        // GIVEN
        //     /tmp/random_folder/test.txt
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line 1\nline 2\nline 3").unwrap();
        let file_provider = crate::file_provider::DiskFileProvider {};

        // WHEN offset is on the first line
        // THEN line 1, column 3 is returned
        let (line, col) =
            offset_to_line_col(&file_provider, file_path.to_str().unwrap(), 2).unwrap();
        assert_eq!(line, 1);
        assert_eq!(col, 3);

        // WHEN offset is on the second line
        // THEN line 2, column 4 is returned
        let (line, col) =
            offset_to_line_col(&file_provider, file_path.to_str().unwrap(), 10).unwrap();
        assert_eq!(line, 2);
        assert_eq!(col, 4);

        // WHEN offset is at the beginning of a line
        // THEN line 3, column 1 is returned
        let (line, col) =
            offset_to_line_col(&file_provider, file_path.to_str().unwrap(), 14).unwrap();
        assert_eq!(line, 3);
        assert_eq!(col, 1);
    }

    // ------------------ //
    // line_col_to_offset //
    // ------------------ //

    #[test]
    fn test_line_col_to_offset() {
        // GIVEN
        //     /tmp/random_folder/test_loc.txt
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test_loc.txt");
        fs::write(&file_path, "line 1\nline 2\nline 3").unwrap();
        let file_provider = crate::file_provider::DiskFileProvider {};

        // WHEN converting line 1, column 3
        // THEN offset should be 2
        assert_eq!(
            line_col_to_offset(&file_provider, file_path.to_str().unwrap(), 1, 3).unwrap(),
            2
        );

        // WHEN converting line 2, column 4
        // THEN offset should be 10
        assert_eq!(
            line_col_to_offset(&file_provider, file_path.to_str().unwrap(), 2, 4).unwrap(),
            10
        );

        // WHEN converting line 3, column 1
        // THEN offset should be 14
        assert_eq!(
            line_col_to_offset(&file_provider, file_path.to_str().unwrap(), 3, 1).unwrap(),
            14
        );
    }
}
