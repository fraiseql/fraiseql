//! Checksum validation algorithms for credit cards, IBANs, and other structured identifiers.
//!
//! This module provides validators for common checksum algorithms used in banking and
//! payment systems.

/// Maximum number of digits accepted by the Luhn validator.
///
/// Real-world Luhn-validated identifiers (credit cards, account numbers) top out
/// at 19 digits. A generous cap of 25 prevents O(n) iteration over attacker-
/// supplied megabyte strings while remaining compatible with every known use case.
const MAX_LUHN_DIGITS: usize = 25;

/// Maximum byte length accepted by the MOD-97 validator.
///
/// The longest IBAN defined by ISO 13616 is 34 characters (e.g. Malta).  Any
/// input longer than this limit cannot be a valid IBAN and is rejected early.
const MAX_MOD97_BYTES: usize = 34;

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
    ///
    /// # Panics
    ///
    /// Cannot panic in practice — the `expect` on `to_digit(10)` is guarded
    /// by a preceding `all(|c| c.is_ascii_digit())` check that returns `false` first.
    pub fn validate(value: &str) -> bool {
        // Must have at least 1 digit and no more than MAX_LUHN_DIGITS.
        if value.is_empty() || value.len() > MAX_LUHN_DIGITS {
            return false;
        }

        // Must contain only digits
        if !value.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }

        let mut sum = 0;
        let mut is_second = false;

        // Process digits from right to left
        for ch in value.chars().rev() {
            let digit = ch.to_digit(10).expect("pre-filtered to numeric chars only") as usize;

            let processed = if is_second {
                let doubled = digit * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                digit
            };

            sum += processed;
            is_second = !is_second;
        }

        sum % 10 == 0
    }

    /// Get a human-readable description of why validation failed.
    pub const fn error_message() -> &'static str {
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
        // Quick length pre-check: IBANs are 4–34 characters (ISO 13616).
        if value.len() < 4 || value.len() > MAX_MOD97_BYTES {
            return false;
        }

        let value_upper = value.to_uppercase();

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
    pub const fn error_message() -> &'static str {
        "Invalid checksum (MOD-97 algorithm)"
    }
}
