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

use std::cmp::min;

/// Create a string array starting from a slice of element of type T
/// Input:
/// - elements: the slice of elements of type T
/// - limit: the number of element the caller wants to display in the String
/// - format_element: a lambda function used to transform element of type T
///   into a String that the caller wants to retrieve
///
/// Output:
/// - String: string with brackets, containing list of elements up to limits.
///   If elements has more elements, "..." is appended as last element of the array
pub fn print_list_of_element<T, F>(elements: &[T], limit: usize, format_element: F) -> String
where
    F: Fn(&T) -> String,
{
    let used_elements = min(limit, elements.len());
    let slice = &elements[0..used_elements];
    let ret = slice.iter().map(format_element).collect::<Vec<_>>().join(",");
    if elements.len() > limit {
        format!("[{ret},...]")
    } else {
        format!("[{ret}]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_print_list_of_element_empty() {
        // GIVEN an empty vector
        let elements: Vec<PathBuf> = Vec::new();

        // WHEN print_list_of_element is called
        let result = print_list_of_element(&elements, 2, |p| p.display().to_string());

        // THEN should return "[]"
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_print_list_of_element_one() {
        // GIVEN a vector with one element
        let elements: Vec<PathBuf> = vec![PathBuf::from("/path/to/file1.bp")];

        // WHEN print_list_of_element is called
        let result = print_list_of_element(&elements, 2, |p| p.display().to_string());

        // THEN should return "[]"
        assert_eq!(result, "[/path/to/file1.bp]");
    }

    #[test]
    fn test_print_list_of_element_two() {
        // GIVEN a vector with two elements
        let elements: Vec<PathBuf> =
            vec![PathBuf::from("/path/to/file1.bp"), PathBuf::from("/path/to/file2.bp")];

        // WHEN print_list_of_element is called
        let result = print_list_of_element(&elements, 2, |p| p.display().to_string());

        // THEN should return "[]"
        assert_eq!(result, "[/path/to/file1.bp,/path/to/file2.bp]");
    }

    #[test]
    fn test_print_list_of_element_more_than_two() {
        // GIVEN a vector with three elements
        let elements: Vec<PathBuf> = vec![
            PathBuf::from("/path/to/file1.bp"),
            PathBuf::from("/path/to/file2.bp"),
            PathBuf::from("/path/to/file3.bp"),
        ];

        // WHEN print_list_of_element is called
        let result = print_list_of_element(&elements, 2, |p| p.display().to_string());

        // THEN should return "[]"
        assert_eq!(result, "[/path/to/file1.bp,/path/to/file2.bp,...]");
    }

    #[test]
    fn test_print_list_of_element_custom_format() {
        // GIVEN a vector of paths
        let elements: Vec<PathBuf> = vec![
            PathBuf::from("/path/to/file1.bp"),
            PathBuf::from("/path/to/file2.bp"),
            PathBuf::from("/path/to/file3.bp"),
        ];

        // WHEN print_list_of_element is called with a custom formatter
        let result = print_list_of_element(&elements, 2, |p| format!("FILE: {}", p.display()));

        // THEN the output should be formatted according to the custom formatter
        assert_eq!(result, "[FILE: /path/to/file1.bp,FILE: /path/to/file2.bp,...]");
    }

    #[test]
    fn test_print_list_of_element_with_strings() {
        // GIVEN a vector of strings
        let elements: Vec<String> =
            vec!["element1".to_string(), "element2".to_string(), "element3".to_string()];

        // WHEN print_list_of_element is called with a custom formatter
        let result = print_list_of_element(&elements, 2, |s| format!("STR: {s}"));

        // THEN the output should be formatted according to the custom formatter
        assert_eq!(result, "[STR: element1,STR: element2,...]");
    }

    #[test]
    fn test_print_list_of_element_with_integers() {
        // GIVEN a vector of integers
        let elements: Vec<i32> = vec![1, 2, 3];

        // WHEN print_list_of_element is called
        let result = print_list_of_element(&elements, 2, |i| format!("INT: {i}"));

        // THEN the output should be formatted according to the custom formatter
        assert_eq!(result, "[INT: 1,INT: 2,...]");
    }

    #[test]
    fn test_print_list_of_element_with_empty_formatter() {
        // GIVEN a vector of strings
        let elements: Vec<String> =
            vec!["element1".to_string(), "element2".to_string(), "element3".to_string()];

        // WHEN print_list_of_element is called with an empty formatter
        let result = print_list_of_element(&elements, 2, |s| s.clone());

        // THEN the output should be the same as the input elements, joined by commas
        assert_eq!(result, "[element1,element2,...]");
    }
}
