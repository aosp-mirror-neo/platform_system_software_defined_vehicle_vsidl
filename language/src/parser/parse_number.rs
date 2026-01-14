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

/// A trait for parsing a string into a numeric type, with custom logic for different types.
pub(crate) trait ParseNumber: Sized {
    /// Parses a string slice into a number.
    ///
    /// # Arguments
    ///
    /// * `s`: The string slice to parse.
    ///
    /// # Returns
    ///
    /// A `Result` containing the parsed number or an error string.
    fn parse(s: &str) -> Result<Self, String>;
}

/// Determines the radix from a string prefix and returns the number string without the prefix.
fn get_radix(s: &str) -> (&str, u32) {
    if s.starts_with("0x") || s.starts_with("0X") {
        (s.trim_start_matches("0x").trim_start_matches("0X"), 16)
    } else if s.starts_with("0b") || s.starts_with("0B") {
        (s.trim_start_matches("0b").trim_start_matches("0B"), 2)
    } else if s.starts_with("0o") || s.starts_with("0O") {
        (s.trim_start_matches("0o").trim_start_matches("0O"), 8)
    } else {
        (s, 10)
    }
}

/// Macro to implement `ParseNumber` for integer types that support radix parsing.
macro_rules! impl_parse_number_for_int {
    // This branch handles types with no special pre-checks (e.g., i32, i64).
    ($int_type:ty) => {
        impl_parse_number_for_int!($int_type, |_s| Ok::<(), String>(()));
    };
    // This branch handles types with a pre-check closure (e.g., u32 to check for negative signs).
    ($int_type:ty, $pre_check:expr) => {
        impl ParseNumber for $int_type {
            fn parse(s: &str) -> Result<Self, String> {
                // Execute the pre-check before parsing
                $pre_check(s)?;

                let (s_to_parse, radix) = get_radix(s);

                let result = if radix == 10 {
                    s_to_parse.parse::<$int_type>()
                } else {
                    <$int_type>::from_str_radix(s_to_parse, radix)
                };

                result.map_err(|e| format!("'{}' is not a valid number string: {}", s, e))
            }
        }
    };
}

// Implement for `u32` with a pre-check to disallow negative numbers.
impl_parse_number_for_int!(u32, |s: &str| {
    if s.starts_with('-') {
        Err(format!("'{}' cannot be parsed to u32 because it is a negative number.", s))
    } else {
        Ok(())
    }
});

// Implement for `u32` with a pre-check to disallow negative numbers.
impl_parse_number_for_int!(u16, |s: &str| {
    if s.starts_with('-') {
        Err(format!("'{}' cannot be parsed to u16 because it is a negative number.", s))
    } else {
        Ok(())
    }
});

// Implement for `u8` with a pre-check to disallow negative numbers.
impl_parse_number_for_int!(u8, |s: &str| {
    if s.starts_with('-') {
        Err(format!("'{}' cannot be parsed to u8 because it is a negative number.", s))
    } else {
        Ok(())
    }
});

// Implement for `i32` and `i64` with no special pre-checks.
impl_parse_number_for_int!(i32);
impl_parse_number_for_int!(i64);

// `f32` implementation is simpler as it doesn't need radix support.
impl ParseNumber for f32 {
    fn parse(s: &str) -> Result<Self, String> {
        s.parse::<f32>().map_err(|e| format!("'{}' is not a valid number string: {}", s, e))
    }
}

/// Parses a string into a number, handling common edge cases.
///
/// This function can parse integers (u32, i32, i64) and floating-point numbers (f32).
/// For integers, it supports decimal, hexadecimal (0x), binary (0b), and octal (0o) notations.
///
/// # Type Parameters
///
/// * `T`: The numeric type to parse the string into. Must implement the `ParseNumber` trait.
///
/// # Returns
///
/// - `Ok(T)`: The successfully parsed number.
/// - `Err(String)`: A descriptive error message if parsing fails.
pub(crate) fn parse_string<T: ParseNumber>(s: &str) -> Result<T, String> {
    T::parse(s)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_u32_valid() {
        assert_eq!(parse_string::<u32>("123").unwrap(), 123);
        assert_eq!(parse_string::<u32>("0").unwrap(), 0);
        assert_eq!(parse_string::<u32>("4294967295").unwrap(), u32::MAX);
    }

    #[test]
    fn test_parse_u32_radix() {
        // Hex
        assert_eq!(parse_string::<u32>("0x1A").unwrap(), 26);
        assert_eq!(parse_string::<u32>("0X1a").unwrap(), 26);
        assert_eq!(parse_string::<u32>("0xffffffff").unwrap(), u32::MAX);

        // Octal
        assert_eq!(parse_string::<u32>("0o77").unwrap(), 63);
        assert_eq!(parse_string::<u32>("0O77").unwrap(), 63);

        // Binary
        assert_eq!(parse_string::<u32>("0b1101").unwrap(), 13);
        assert_eq!(parse_string::<u32>("0B1101").unwrap(), 13);
    }

    #[test]
    fn test_parse_u32_invalid() {
        // Negative
        assert!(parse_string::<u32>("-123").is_err());

        // Invalid format
        assert!(parse_string::<u32>("abc").is_err());
        assert!(parse_string::<u32>("0xG").is_err());
        assert!(parse_string::<u32>("1.0").is_err());

        // Overflow
        assert!(parse_string::<u32>("4294967296").is_err());
        assert!(parse_string::<u32>("0x100000000").is_err());
    }

    #[test]
    fn test_parse_i32_valid() {
        assert_eq!(parse_string::<i32>("123").unwrap(), 123);
        assert_eq!(parse_string::<i32>("-123").unwrap(), -123);
        assert_eq!(parse_string::<i32>("0").unwrap(), 0);
        assert_eq!(parse_string::<i32>("2147483647").unwrap(), i32::MAX);
        assert_eq!(parse_string::<i32>("-2147483648").unwrap(), i32::MIN);
    }

    #[test]
    fn test_parse_i32_radix() {
        // Hex
        assert_eq!(parse_string::<i32>("0xFF").unwrap(), 255);
        assert_eq!(parse_string::<i32>("0x7fffffff").unwrap(), i32::MAX);

        // Octal
        assert_eq!(parse_string::<i32>("0o77").unwrap(), 63);

        // Binary
        assert_eq!(parse_string::<i32>("0b1101").unwrap(), 13);
    }

    #[test]
    fn test_parse_i32_invalid() {
        // Invalid format
        assert!(parse_string::<i32>("abc").is_err());
        assert!(parse_string::<i32>("0xG").is_err());
        assert!(parse_string::<i32>("1.0").is_err());

        // Overflow
        assert!(parse_string::<i32>("2147483648").is_err());
        assert!(parse_string::<i32>("-2147483649").is_err());
        assert!(parse_string::<i32>("0x80000000").is_err());
    }

    #[test]
    fn test_parse_i64_valid() {
        assert_eq!(parse_string::<i64>("123").unwrap(), 123);
        assert_eq!(parse_string::<i64>("-123").unwrap(), -123);
        assert_eq!(parse_string::<i64>("9223372036854775807").unwrap(), i64::MAX);
        assert_eq!(parse_string::<i64>("-9223372036854775808").unwrap(), i64::MIN);
    }

    #[test]
    fn test_parse_i64_radix() {
        // Hex
        assert_eq!(parse_string::<i64>("0xFF").unwrap(), 255);
        assert_eq!(parse_string::<i64>("0x7fffffffffffffff").unwrap(), i64::MAX);
    }

    #[test]
    fn test_parse_i64_invalid() {
        // Overflow
        assert!(parse_string::<i64>("9223372036854775808").is_err());
        assert!(parse_string::<i64>("-9223372036854775809").is_err());
        assert!(parse_string::<i64>("0x8000000000000000").is_err());
    }

    #[test]
    fn test_parse_f32_valid() {
        assert_eq!(parse_string::<f32>("123.0").unwrap(), 123.0);
        assert_eq!(parse_string::<f32>("-123.45").unwrap(), -123.45);
        assert_eq!(parse_string::<f32>("0.0").unwrap(), 0.0);
        assert_eq!(parse_string::<f32>("1.23e-4").unwrap(), 0.000123);
        assert_eq!(parse_string::<f32>("-1.23e+4").unwrap(), -12300.0);
        assert_eq!(parse_string::<f32>("123").unwrap(), 123.0);
    }

    #[test]
    fn test_parse_f32_invalid() {
        // Invalid format
        assert!(parse_string::<f32>("abc").is_err());
        assert!(parse_string::<f32>("1.2.3").is_err());
        assert!(parse_string::<f32>("1.23e").is_err());

        // Radix not supported for f32
        assert!(parse_string::<f32>("0x10").is_err());
    }

    #[test]
    fn test_get_radix() {
        assert_eq!(get_radix("123"), ("123", 10));
        assert_eq!(get_radix("0x1A"), ("1A", 16));
        assert_eq!(get_radix("0X1a"), ("1a", 16));
        assert_eq!(get_radix("0o77"), ("77", 8));
        assert_eq!(get_radix("0O77"), ("77", 8));
        assert_eq!(get_radix("0b1101"), ("1101", 2));
        assert_eq!(get_radix("0B1101"), ("1101", 2));
        assert_eq!(get_radix("-123"), ("-123", 10));
    }
}
