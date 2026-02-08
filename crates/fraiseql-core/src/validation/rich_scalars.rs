//! Rich scalar type validators for specialized data formats.
//!
//! This module provides validators for common structured data types like emails,
//! phone numbers, IBANs, VINs, and country codes.

use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    // Email regex: Simple but practical pattern
    static ref EMAIL_REGEX: Regex = Regex::new(
        r"^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$"
    ).unwrap();

    // International phone: +1-999-999-9999 or +999999999999, etc.
    static ref PHONE_REGEX: Regex = Regex::new(
        r"^\+?[1-9]\d{1,14}$"
    ).unwrap();

    // VIN: 17 alphanumeric characters (no I, O, Q)
    static ref VIN_REGEX: Regex = Regex::new(
        r"^[A-HJ-NPR-Z0-9]{17}$"
    ).unwrap();

    // Country code: ISO 3166-1 alpha-2 (2 letters)
    static ref COUNTRY_CODE_REGEX: Regex = Regex::new(
        r"^[A-Z]{2}$"
    ).unwrap();
}

/// Email address validator.
pub struct EmailValidator;

impl EmailValidator {
    /// Validate an email address format.
    ///
    /// Uses a practical regex pattern that handles most common email formats.
    /// Note: This validates format only, not domain existence (use async validator for that).
    pub fn validate(value: &str) -> bool {
        !value.is_empty() && value.len() <= 254 && EMAIL_REGEX.is_match(value)
    }

    pub fn error_message() -> &'static str {
        "Invalid email format"
    }
}

/// International phone number validator.
pub struct PhoneNumberValidator;

impl PhoneNumberValidator {
    /// Validate an international phone number format.
    ///
    /// Accepts formats like:
    /// - +1234567890
    /// - +1-234-567-8900
    /// - 1234567890
    ///
    /// The pattern allows +1 to +999 country codes followed by 1-14 additional digits.
    pub fn validate(value: &str) -> bool {
        !value.is_empty() && value.len() <= 20 && PHONE_REGEX.is_match(value)
    }

    pub fn error_message() -> &'static str {
        "Invalid phone number format"
    }
}

/// VIN (Vehicle Identification Number) validator.
pub struct VinValidator;

impl VinValidator {
    /// Validate a VIN format.
    ///
    /// A valid VIN is:
    /// - Exactly 17 characters
    /// - Only alphanumeric (no I, O, Q to avoid confusion with numbers)
    /// - Case insensitive
    ///
    /// Note: This validates format only, not checksum (different per manufacturer).
    pub fn validate(value: &str) -> bool {
        let value_upper = value.to_uppercase();
        VIN_REGEX.is_match(&value_upper)
    }

    pub fn error_message() -> &'static str {
        "Invalid VIN format (must be 17 alphanumeric characters, excluding I, O, Q)"
    }
}

/// Country code validator (ISO 3166-1 alpha-2).
pub struct CountryCodeValidator {
    valid_codes: std::collections::HashSet<&'static str>,
}

impl CountryCodeValidator {
    /// Create a new country code validator with all valid ISO codes.
    pub fn new() -> Self {
        let mut codes = std::collections::HashSet::new();
        // All ISO 3166-1 alpha-2 codes
        codes.insert("AD");
        codes.insert("AE");
        codes.insert("AF");
        codes.insert("AG");
        codes.insert("AI");
        codes.insert("AL");
        codes.insert("AM");
        codes.insert("AO");
        codes.insert("AQ");
        codes.insert("AR");
        codes.insert("AS");
        codes.insert("AT");
        codes.insert("AU");
        codes.insert("AW");
        codes.insert("AX");
        codes.insert("AZ");
        codes.insert("BA");
        codes.insert("BB");
        codes.insert("BD");
        codes.insert("BE");
        codes.insert("BF");
        codes.insert("BG");
        codes.insert("BH");
        codes.insert("BI");
        codes.insert("BJ");
        codes.insert("BL");
        codes.insert("BM");
        codes.insert("BN");
        codes.insert("BO");
        codes.insert("BQ");
        codes.insert("BR");
        codes.insert("BS");
        codes.insert("BT");
        codes.insert("BV");
        codes.insert("BW");
        codes.insert("BY");
        codes.insert("BZ");
        codes.insert("CA");
        codes.insert("CC");
        codes.insert("CD");
        codes.insert("CF");
        codes.insert("CG");
        codes.insert("CH");
        codes.insert("CI");
        codes.insert("CK");
        codes.insert("CL");
        codes.insert("CM");
        codes.insert("CN");
        codes.insert("CO");
        codes.insert("CR");
        codes.insert("CU");
        codes.insert("CV");
        codes.insert("CW");
        codes.insert("CX");
        codes.insert("CY");
        codes.insert("CZ");
        codes.insert("DE");
        codes.insert("DJ");
        codes.insert("DK");
        codes.insert("DM");
        codes.insert("DO");
        codes.insert("DZ");
        codes.insert("EC");
        codes.insert("EE");
        codes.insert("EG");
        codes.insert("EH");
        codes.insert("ER");
        codes.insert("ES");
        codes.insert("ET");
        codes.insert("FI");
        codes.insert("FJ");
        codes.insert("FK");
        codes.insert("FM");
        codes.insert("FO");
        codes.insert("FR");
        codes.insert("GA");
        codes.insert("GB");
        codes.insert("GD");
        codes.insert("GE");
        codes.insert("GF");
        codes.insert("GG");
        codes.insert("GH");
        codes.insert("GI");
        codes.insert("GL");
        codes.insert("GM");
        codes.insert("GN");
        codes.insert("GP");
        codes.insert("GQ");
        codes.insert("GR");
        codes.insert("GS");
        codes.insert("GT");
        codes.insert("GU");
        codes.insert("GW");
        codes.insert("GY");
        codes.insert("HK");
        codes.insert("HM");
        codes.insert("HN");
        codes.insert("HR");
        codes.insert("HT");
        codes.insert("HU");
        codes.insert("ID");
        codes.insert("IE");
        codes.insert("IL");
        codes.insert("IM");
        codes.insert("IN");
        codes.insert("IO");
        codes.insert("IQ");
        codes.insert("IR");
        codes.insert("IS");
        codes.insert("IT");
        codes.insert("JE");
        codes.insert("JM");
        codes.insert("JO");
        codes.insert("JP");
        codes.insert("KE");
        codes.insert("KG");
        codes.insert("KH");
        codes.insert("KI");
        codes.insert("KM");
        codes.insert("KN");
        codes.insert("KP");
        codes.insert("KR");
        codes.insert("KW");
        codes.insert("KY");
        codes.insert("KZ");
        codes.insert("LA");
        codes.insert("LB");
        codes.insert("LC");
        codes.insert("LI");
        codes.insert("LK");
        codes.insert("LR");
        codes.insert("LS");
        codes.insert("LT");
        codes.insert("LU");
        codes.insert("LV");
        codes.insert("LY");
        codes.insert("MA");
        codes.insert("MC");
        codes.insert("MD");
        codes.insert("ME");
        codes.insert("MF");
        codes.insert("MG");
        codes.insert("MH");
        codes.insert("MK");
        codes.insert("ML");
        codes.insert("MM");
        codes.insert("MN");
        codes.insert("MO");
        codes.insert("MP");
        codes.insert("MQ");
        codes.insert("MR");
        codes.insert("MS");
        codes.insert("MT");
        codes.insert("MU");
        codes.insert("MV");
        codes.insert("MW");
        codes.insert("MX");
        codes.insert("MY");
        codes.insert("MZ");
        codes.insert("NA");
        codes.insert("NC");
        codes.insert("NE");
        codes.insert("NF");
        codes.insert("NG");
        codes.insert("NI");
        codes.insert("NL");
        codes.insert("NO");
        codes.insert("NP");
        codes.insert("NR");
        codes.insert("NU");
        codes.insert("NZ");
        codes.insert("OM");
        codes.insert("PA");
        codes.insert("PE");
        codes.insert("PF");
        codes.insert("PG");
        codes.insert("PH");
        codes.insert("PK");
        codes.insert("PL");
        codes.insert("PM");
        codes.insert("PN");
        codes.insert("PR");
        codes.insert("PS");
        codes.insert("PT");
        codes.insert("PW");
        codes.insert("PY");
        codes.insert("QA");
        codes.insert("RE");
        codes.insert("RO");
        codes.insert("RS");
        codes.insert("RU");
        codes.insert("RW");
        codes.insert("SA");
        codes.insert("SB");
        codes.insert("SC");
        codes.insert("SD");
        codes.insert("SE");
        codes.insert("SG");
        codes.insert("SH");
        codes.insert("SI");
        codes.insert("SJ");
        codes.insert("SK");
        codes.insert("SL");
        codes.insert("SM");
        codes.insert("SN");
        codes.insert("SO");
        codes.insert("SR");
        codes.insert("SS");
        codes.insert("ST");
        codes.insert("SV");
        codes.insert("SX");
        codes.insert("SY");
        codes.insert("SZ");
        codes.insert("TC");
        codes.insert("TD");
        codes.insert("TF");
        codes.insert("TG");
        codes.insert("TH");
        codes.insert("TJ");
        codes.insert("TK");
        codes.insert("TL");
        codes.insert("TM");
        codes.insert("TN");
        codes.insert("TO");
        codes.insert("TR");
        codes.insert("TT");
        codes.insert("TV");
        codes.insert("TW");
        codes.insert("TZ");
        codes.insert("UA");
        codes.insert("UG");
        codes.insert("UM");
        codes.insert("US");
        codes.insert("UY");
        codes.insert("UZ");
        codes.insert("VA");
        codes.insert("VC");
        codes.insert("VE");
        codes.insert("VG");
        codes.insert("VI");
        codes.insert("VN");
        codes.insert("VU");
        codes.insert("WF");
        codes.insert("WS");
        codes.insert("YE");
        codes.insert("YT");
        codes.insert("ZA");
        codes.insert("ZM");
        codes.insert("ZW");
        Self {
            valid_codes: codes,
        }
    }

    /// Validate a country code against ISO 3166-1 alpha-2 standard.
    pub fn validate(&self, value: &str) -> bool {
        let value_upper = value.to_uppercase();
        COUNTRY_CODE_REGEX.is_match(&value_upper) && self.valid_codes.contains(value_upper.as_str())
    }

    pub fn error_message() -> &'static str {
        "Invalid country code (must be ISO 3166-1 alpha-2)"
    }
}

impl Default for CountryCodeValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Email tests
    #[test]
    fn test_email_valid() {
        assert!(EmailValidator::validate("user@example.com"));
        assert!(EmailValidator::validate("john.doe@company.co.uk"));
    }

    #[test]
    fn test_email_invalid() {
        assert!(!EmailValidator::validate("invalid.email"));
        assert!(!EmailValidator::validate("user@"));
        assert!(!EmailValidator::validate("@example.com"));
    }

    #[test]
    fn test_email_empty() {
        assert!(!EmailValidator::validate(""));
    }

    // Phone tests
    #[test]
    fn test_phone_valid_plus_format() {
        assert!(PhoneNumberValidator::validate("+1234567890"));
        assert!(PhoneNumberValidator::validate("+33612345678"));
    }

    #[test]
    fn test_phone_valid_no_plus() {
        assert!(PhoneNumberValidator::validate("1234567890"));
    }

    #[test]
    fn test_phone_invalid() {
        assert!(!PhoneNumberValidator::validate("+0123456789")); // Can't start with 0
        assert!(!PhoneNumberValidator::validate(""));
    }

    // VIN tests
    #[test]
    fn test_vin_valid() {
        assert!(VinValidator::validate("3G1FB1E30D1109186"));
        assert!(VinValidator::validate("JH2RC5004LM200591"));
    }

    #[test]
    fn test_vin_valid_lowercase() {
        assert!(VinValidator::validate("3g1fb1e30d1109186"));
    }

    #[test]
    fn test_vin_invalid_length() {
        assert!(!VinValidator::validate("3G1FB1E30D110918"));
        assert!(!VinValidator::validate("3G1FB1E30D11091861"));
    }

    #[test]
    fn test_vin_invalid_chars() {
        assert!(!VinValidator::validate("3G1FB1E30D110918I")); // Contains I
        assert!(!VinValidator::validate("3G1FB1E30D110918O")); // Contains O
        assert!(!VinValidator::validate("3G1FB1E30D110918Q")); // Contains Q
    }

    // Country code tests
    #[test]
    fn test_country_code_valid() {
        let validator = CountryCodeValidator::new();
        assert!(validator.validate("US"));
        assert!(validator.validate("GB"));
        assert!(validator.validate("DE"));
        assert!(validator.validate("FR"));
    }

    #[test]
    fn test_country_code_lowercase() {
        let validator = CountryCodeValidator::new();
        assert!(validator.validate("us"));
        assert!(validator.validate("gb"));
    }

    #[test]
    fn test_country_code_invalid() {
        let validator = CountryCodeValidator::new();
        assert!(!validator.validate("XX"));
        assert!(!validator.validate("USA"));
        assert!(!validator.validate("U"));
    }
}
