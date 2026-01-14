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

//! Utilities for protobuf message size computing.

/// Compute size of the tag segment in protobuf wire format
pub fn compute_tag_size(tag: i32) -> usize {
    if tag < (1 << 4) {
        1
    } else if tag < (1 << 11) {
        2
    } else if tag < (1 << 18) {
        3
    } else if tag < (1 << 25) {
        4
    } else {
        5
    }
}

/// Compute size of the length segment in protobuf wire format
pub fn compute_length_size(content_size: usize) -> usize {
    if content_size < (1 << 7) {
        1
    } else if content_size < (1 << 14) {
        2
    } else if content_size < (1 << 21) {
        3
    } else if content_size < (1 << 28) {
        4
    } else {
        5
    }
}

/// Compute the length of DataTunnel message given a user message_size.
/// Middleware adds a varint as prefix to identify the size of the message.
/// This function returns message_size as original_message_size + header_size.
pub fn compute_message_length_including_header(original_message_size: u32) -> u32 {
    original_message_size + compute_length_size(original_message_size as usize) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tag size is encoded has varint, but has an offset of 3 bit
    // more details: https://protobuf.dev/programming-guides/encoding/#structure
    #[test]
    fn test_tag_size() {
        assert_eq!(compute_tag_size(1), 1);
        assert_eq!(compute_tag_size((1 << 4) - 1), 1);
        assert_eq!(compute_tag_size(1 << 4), 2);
        assert_eq!(compute_tag_size((1 << 11) - 1), 2);
        assert_eq!(compute_tag_size(1 << 11), 3);
        assert_eq!(compute_tag_size((1 << 18) - 1), 3);
        assert_eq!(compute_tag_size(1 << 18), 4);
        assert_eq!(compute_tag_size((1 << 25) - 1), 4);
        assert_eq!(compute_tag_size(1 << 25), 5);
        assert_eq!(compute_tag_size(536870911), 5); // biggest supported identifier number by protobuf
    }

    // Testing first two and last two numbers that requires
    // 1 byte to be represented as varint in protobuf wire format
    // All numbers in between result in the same length segment
    #[test]
    fn test_single_byte_usize() {
        assert_eq!(compute_length_size(0), 1);
        assert_eq!(compute_length_size(1), 1);
        assert_eq!(compute_length_size((1 << 7) - 2), 1);
        assert_eq!(compute_length_size((1 << 7) - 1), 1);
    }

    // Testing first two and last two numbers that requires
    // 2 bytes to be represented as varint in protobuf wire format
    // All numbers in between result in the same length segment
    #[test]
    fn test_2_bytes_usize() {
        assert_eq!(compute_length_size(1 << 7), 2);
        assert_eq!(compute_length_size((1 << 7) + 1), 2);
        assert_eq!(compute_length_size((1 << 14) - 2), 2);
        assert_eq!(compute_length_size((1 << 14) - 1), 2);
    }

    // Testing first two and last two numbers that requires
    // 3 bytes to be represented as varint in protobuf wire format
    // All numbers in between result in the same length segment
    #[test]
    fn test_3_bytes_usize() {
        assert_eq!(compute_length_size(1 << 14), 3);
        assert_eq!(compute_length_size((1 << 14) + 1), 3);
        assert_eq!(compute_length_size((1 << 21) - 2), 3);
        assert_eq!(compute_length_size((1 << 21) - 1), 3);
    }

    // Testing first two and last two numbers that requires
    // 4 bytes to be represented as varint in protobuf wire format
    // All numbers in between result in the same length segment
    #[test]
    fn test_4_bytes_usize() {
        assert_eq!(compute_length_size(1 << 21), 4);
        assert_eq!(compute_length_size((1 << 21) + 1), 4);
        assert_eq!(compute_length_size((1 << 28) - 2), 4);
        assert_eq!(compute_length_size((1 << 28) - 1), 4);
    }

    // Testing first two and last two numbers that requires
    // 5 bytes to be represented as varint in protobuf wire format
    // All numbers in between result in the same length segment
    #[test]
    fn test_5_bytes_usize() {
        assert_eq!(compute_length_size(1 << 28), 5);
        assert_eq!(compute_length_size((1 << 28) + 1), 5);
        assert_eq!(compute_length_size(usize::MAX - 1), 5);
        assert_eq!(compute_length_size(usize::MAX), 5);
    }

    // Validating the total size of protobuf wire message (header + message)
    // for messages of different lengths requiring different header
    // length to prefix the message
    #[test]
    fn test_header_message_size() {
        // 1 additional byte as header varint
        assert_eq!(compute_message_length_including_header(0), 1);
        assert_eq!(compute_message_length_including_header(1), 2);
        assert_eq!(compute_message_length_including_header(10), 11);
        assert_eq!(compute_message_length_including_header((1 << 7) - 1), 128);
        // 2 additional bytes as header varint
        assert_eq!(compute_message_length_including_header(1 << 7), 130);
        // 3 additional bytes as header varint
        assert_eq!(compute_message_length_including_header(1 << 14), 16387);
        // 4 additional bytes as header varint
        assert_eq!(compute_message_length_including_header(1 << 21), 2097156);
        // 5 additional bytes as header varint
        assert_eq!(compute_message_length_including_header(1 << 28), 268435461);
        // maximum input before overflowing
        assert_eq!(compute_message_length_including_header(u32::MAX - 5), u32::MAX);
    }
}
