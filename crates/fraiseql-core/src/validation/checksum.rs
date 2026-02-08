//! Checksum validation algorithms for credit cards, IBANs, and other structured identifiers.
//!
//! This module provides validators for common checksum algorithms used in banking and
//! payment systems.

/// Luhn algorithm validator for credit cards and similar identifiers.
///
/// The Luhn algorithm (also called mod-10) is used to validate credit card numbers
/// and other identification numbers.
///
/// # Algorithm Steps
/// 1. From right to left, double every second digit
/// 2. If doubling results in a two-digit number, subtract 9
/// 3. Sum all digits
/// 4. The sum modulo 10 should equal 0
pub struct LuhnValidator;

impl LuhnValidator {
    /// Validate a string using the Luhn algorithm.
    ///
    /// # Arguments
    ///
    /// * `value` - The string to validate (must contain only digits)
    ///
    /// # Returns
    ///
    /// `true` if the value passes Luhn validation, `false` otherwise
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::validation::checksum::LuhnValidator;
    ///
    /// assert!(LuhnValidator::validate("4532015112830366")); // Valid Visa
    /// assert!(!LuhnValidator::validate("4532015112830367")); // Invalid
    /// ```
    pub fn validate(value: &str) -> bool {
        // Must contain only digits
        if !value.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }

        // Must have at least 1 digit
        if value.is_empty() {
            return false;
        }

        let mut sum = 0;
        let mut is_second = false;

        // Process digits from right to left
        for ch in value.chars().rev() {
            let digit = ch.to_digit(10).unwrap() as usize;

            let processed = if is_second {
                let doubled = digit * 2;
                if doubled > 9 {
                    doubled - 9
                } else {
                    doubled
                }
            } else {
                digit
            };

            sum += processed;
            is_second = !is_second;
        }

        sum % 10 == 0
    }

    /// Get a human-readable description of why validation failed.
    pub fn error_message() -> &'static str {
        "Invalid checksum (Luhn algorithm)"
    }
}

/// MOD-97 algorithm validator for IBANs and similar identifiers.
///
/// The MOD-97 algorithm is used to validate International Bank Account Numbers (IBANs)
/// and other financial identifiers.
///
/// # Algorithm Steps
/// 1. Move the first 4 characters to the end
/// 2. Replace letters with numbers (A=10, B=11, ..., Z=35)
/// 3. Calculate the remainder of the number modulo 97
/// 4. The remainder should be 1 for valid IBANs
pub struct Mod97Validator;

impl Mod97Validator {
    /// Validate a string using the MOD-97 algorithm.
    ///
    /// # Arguments
    ///
    /// * `value` - The string to validate (typically an IBAN or similar identifier)
    ///
    /// # Returns
    ///
    /// `true` if the value passes MOD-97 validation, `false` otherwise
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::validation::checksum::Mod97Validator;
    ///
    /// assert!(Mod97Validator::validate("GB82WEST12345698765432")); // Valid IBAN
    /// assert!(!Mod97Validator::validate("GB82WEST12345698765433")); // Invalid
    /// ```
    pub fn validate(value: &str) -> bool {
        let value_upper = value.to_uppercase();

        // Must have at least 4 characters
        if value_upper.len() < 4 {
            return false;
        }

        // Must contain only alphanumeric characters
        if !value_upper.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }

        // Rearrange: move first 4 characters to the end
        let rearranged = format!("{}{}", &value_upper[4..], &value_upper[..4]);

        // Convert to numeric string: A=10, B=11, ..., Z=35
        let mut numeric = String::new();
        for ch in rearranged.chars() {
            if ch.is_ascii_digit() {
                numeric.push(ch);
            } else if ch.is_ascii_uppercase() {
                // A=10, B=11, ..., Z=35
                numeric.push_str(&(10 + (ch as usize - 'A' as usize)).to_string());
            } else {
                return false;
            }
        }

        // Calculate mod 97
        let remainder = Self::mod97(&numeric);
        remainder == 1
    }

    /// Calculate mod 97 of a numeric string.
    ///
    /// Uses the standard modulo operator by processing the number in chunks
    /// to avoid overflow on very large numbers.
    fn mod97(numeric: &str) -> u32 {
        let mut remainder = 0u32;

        for digit_char in numeric.chars() {
            if let Some(digit) = digit_char.to_digit(10) {
                remainder = (remainder * 10 + digit) % 97;
            }
        }

        remainder
    }

    /// Get a human-readable description of why validation failed.
    pub fn error_message() -> &'static str {
        "Invalid checksum (MOD-97 algorithm)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Luhn tests
    #[test]
    fn test_luhn_valid_visa() {
        assert!(LuhnValidator::validate("4532015112830366"));
    }

    #[test]
    fn test_luhn_valid_another_visa() {
        assert!(LuhnValidator::validate("4111111111111111"));
    }

    #[test]
    fn test_luhn_invalid_checksum() {
        assert!(!LuhnValidator::validate("4532015112830367"));
    }

    #[test]
    fn test_luhn_invalid_non_digits() {
        assert!(!LuhnValidator::validate("4532-0151-1283-0366"));
    }

    #[test]
    fn test_luhn_empty_string() {
        assert!(!LuhnValidator::validate(""));
    }

    #[test]
    fn test_luhn_single_digit() {
        assert!(LuhnValidator::validate("0"));
    }

    #[test]
    fn test_luhn_all_zeros() {
        assert!(LuhnValidator::validate("0000000000000000"));
    }

    // MOD-97 tests
    #[test]
    fn test_mod97_valid_iban_gb() {
        assert!(Mod97Validator::validate("GB82WEST12345698765432"));
    }

    #[test]
    fn test_mod97_valid_iban_de() {
        assert!(Mod97Validator::validate("DE89370400440532013000"));
    }

    #[test]
    fn test_mod97_invalid_checksum() {
        assert!(!Mod97Validator::validate("GB82WEST12345698765433"));
    }

    #[test]
    fn test_mod97_invalid_too_short() {
        assert!(!Mod97Validator::validate("GB8"));
    }

    #[test]
    fn test_mod97_invalid_special_chars() {
        assert!(!Mod97Validator::validate("GB82-WEST-1234"));
    }

    #[test]
    fn test_mod97_lowercase_conversion() {
        // Should handle lowercase by converting to uppercase
        assert!(Mod97Validator::validate("gb82west12345698765432"));
    }

    #[test]
    fn test_mod97_error_message() {
        assert_eq!(Mod97Validator::error_message(), "Invalid checksum (MOD-97 algorithm)");
    }

    #[test]
    fn test_luhn_error_message() {
        assert_eq!(LuhnValidator::error_message(), "Invalid checksum (Luhn algorithm)");
    }
}
