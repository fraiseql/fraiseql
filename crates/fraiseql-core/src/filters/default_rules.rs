//! Default validation rules for extended operators.
//!
//! This module provides sensible default validation rules for all 44+ rich type operators.
//! Rules can be overridden via fraiseql.toml configuration at compile time.
//!
//! Each rule is defined as a TOML-compatible structure that gets embedded in the
//! compiled schema.

use std::collections::HashMap;

use serde_json::{Value, json};

/// Get all default validation rules.
///
/// These rules are embedded in the compiled schema and applied at runtime
/// before SQL generation. They can be overridden per-application in fraiseql.toml.
pub fn get_default_rules() -> HashMap<String, Value> {
    let mut rules = HashMap::new();

    rules.extend(add_contact_rules());
    rules.extend(add_location_rules());
    rules.extend(add_financial_rules());
    rules.extend(add_identifier_rules());
    rules.extend(add_transportation_rules());
    rules.extend(add_network_rules());
    rules.extend(add_content_rules());
    rules.extend(add_measurement_rules());

    rules
}

fn add_contact_rules() -> HashMap<String, Value> {
    let mut rules = HashMap::new();

    // Email operators
    rules.insert(
        "email_domain_eq".to_string(),
        json!({
            "pattern": "^[a-z0-9]([a-z0-9-]*\\.)*[a-z0-9]([a-z0-9-]*[a-z0-9])?$",
            "description": "Valid domain name (RFC 1035)"
        }),
    );

    rules.insert(
        "email_domain_in".to_string(),
        json!({
            "pattern": "^[a-z0-9]([a-z0-9-]*\\.)*[a-z0-9]([a-z0-9-]*[a-z0-9])?$",
            "description": "Valid domain name in list"
        }),
    );

    rules.insert(
        "email_domain_endswith".to_string(),
        json!({
            "pattern": "^\\.[a-z0-9]([a-z0-9-]*\\.)*[a-z0-9]([a-z0-9-]*[a-z0-9])?$",
            "description": "Domain suffix (starts with dot)"
        }),
    );

    rules.insert(
        "email_local_part_startswith".to_string(),
        json!({
            "min_length": 1,
            "max_length": 64,
            "description": "Local part prefix (before @)"
        }),
    );

    // Phone operators
    rules.insert(
        "phone_country_code_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{2}$",
            "description": "ISO 3166-1 alpha-2 country code"
        }),
    );

    rules.insert(
        "phone_country_code_in".to_string(),
        json!({
            "pattern": "^[A-Z]{2}$",
            "description": "ISO 3166-1 alpha-2 country code"
        }),
    );

    rules.insert(
        "phone_type_eq".to_string(),
        json!({
            "enum": ["mobile", "fixed", "tollfree", "premium", "shared", "voip"],
            "description": "Phone number type"
        }),
    );

    // URL operators
    rules.insert(
        "url_protocol_eq".to_string(),
        json!({
            "enum": ["http", "https", "ftp", "ftps", "ws", "wss"],
            "description": "Protocol scheme"
        }),
    );

    // Domain operators
    rules.insert(
        "domain_name_tld_eq".to_string(),
        json!({
            "pattern": "^[a-z]{2,}$",
            "description": "Top-level domain (lowercase)"
        }),
    );

    rules.insert(
        "domain_name_tld_in".to_string(),
        json!({
            "pattern": "^[a-z]{2,}$",
            "description": "Top-level domain"
        }),
    );

    rules.insert(
        "hostname_depth_eq".to_string(),
        json!({
            "min": 1.0,
            "max": 127.0,
            "description": "Hostname depth (number of labels)"
        }),
    );

    rules
}

fn add_location_rules() -> HashMap<String, Value> {
    let mut rules = HashMap::new();

    // PostalCode operators
    rules.insert(
        "postal_code_country_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{2}$",
            "description": "ISO 3166-1 alpha-2 country code"
        }),
    );

    // Latitude/Longitude operators
    rules.insert(
        "latitude_within_range".to_string(),
        json!({
            "min": -90.0,
            "max": 90.0,
            "description": "Latitude in degrees"
        }),
    );

    rules.insert(
        "latitude_hemisphere_eq".to_string(),
        json!({
            "enum": ["North", "South"],
            "description": "Latitude hemisphere"
        }),
    );

    rules.insert(
        "longitude_within_range".to_string(),
        json!({
            "min": -180.0,
            "max": 180.0,
            "description": "Longitude in degrees"
        }),
    );

    rules.insert(
        "longitude_hemisphere_eq".to_string(),
        json!({
            "enum": ["East", "West"],
            "description": "Longitude hemisphere"
        }),
    );

    // Timezone operators
    rules.insert(
        "timezone_eq".to_string(),
        json!({
            "pattern": "^[A-Za-z_/]+$",
            "description": "IANA timezone identifier"
        }),
    );

    // LocaleCode operators
    rules.insert(
        "locale_code_eq".to_string(),
        json!({
            "pattern": "^[a-z]{2}(?:-[A-Z]{2})?$",
            "description": "BCP 47 locale code"
        }),
    );

    // LanguageCode operators
    rules.insert(
        "language_code_eq".to_string(),
        json!({
            "pattern": "^[a-z]{2}$",
            "description": "ISO 639-1 language code"
        }),
    );

    // CountryCode operators
    rules.insert(
        "country_code_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{2}$",
            "description": "ISO 3166-1 alpha-2 country code"
        }),
    );

    rules
}

fn add_financial_rules() -> HashMap<String, Value> {
    let mut rules = HashMap::new();

    // IBAN operators
    rules.insert(
        "iban_country_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{2}$",
            "description": "ISO 3166-1 country code"
        }),
    );

    rules.insert(
        "iban_check_digit_eq".to_string(),
        json!({
            "pattern": "^[0-9]{2}$",
            "length": 2,
            "description": "IBAN check digits"
        }),
    );

    // CUSIP operators
    rules.insert(
        "cusip_issuer_eq".to_string(),
        json!({
            "length": 6,
            "pattern": "^[A-Z0-9]{6}$",
            "description": "CUSIP issuer code"
        }),
    );

    // ISIN operators
    rules.insert(
        "isin_country_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{2}$",
            "description": "ISO 3166-1 country code"
        }),
    );

    // SEDOL operators (UK securities)
    rules.insert(
        "sedol_check_digit_eq".to_string(),
        json!({
            "pattern": "^[0-9]$",
            "description": "SEDOL check digit"
        }),
    );

    // LEI operators
    rules.insert(
        "lei_country_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{2}$",
            "description": "ISO 3166-1 country code"
        }),
    );

    // MIC operators (Market Identifier Code)
    rules.insert(
        "mic_country_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{2}$",
            "description": "ISO 3166-1 country code"
        }),
    );

    // CurrencyCode operators
    rules.insert(
        "currency_code_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{3}$",
            "description": "ISO 4217 currency code"
        }),
    );

    // Money operators
    rules.insert(
        "money_currency_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{3}$",
            "description": "ISO 4217 currency code"
        }),
    );

    rules.insert(
        "money_amount_within_range".to_string(),
        json!({
            "description": "Numeric money amount range"
        }),
    );

    // ExchangeCode operators
    rules.insert(
        "exchange_code_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{1,4}$",
            "description": "ISO 10383 exchange code"
        }),
    );

    // ExchangeRate operators
    rules.insert(
        "exchange_rate_within_range".to_string(),
        json!({
            "min": 0.0,
            "description": "Exchange rate range"
        }),
    );

    // StockSymbol operators
    rules.insert(
        "stock_symbol_eq".to_string(),
        json!({
            "pattern": "^[A-Z0-9]{1,5}$",
            "description": "Stock ticker symbol"
        }),
    );

    rules
}

fn add_identifier_rules() -> HashMap<String, Value> {
    let mut rules = HashMap::new();

    // Slug operators
    rules.insert(
        "slug_eq".to_string(),
        json!({
            "pattern": "^[a-z0-9]+(?:-[a-z0-9]+)*$",
            "description": "URL-safe slug"
        }),
    );

    // SemanticVersion operators
    rules.insert(
        "semantic_version_eq".to_string(),
        json!({
            "pattern": "^[0-9]+(\\.[0-9]+){0,2}(?:-[a-zA-Z0-9]+)?$",
            "description": "Semantic versioning (X.Y.Z)"
        }),
    );

    // HashSHA256 operators
    rules.insert(
        "hash_sha256_eq".to_string(),
        json!({
            "pattern": "^[a-f0-9]{64}$",
            "length": 64,
            "description": "SHA-256 hash (hexadecimal)"
        }),
    );

    // APIKey operators
    rules.insert(
        "api_key_eq".to_string(),
        json!({
            "min_length": 16,
            "max_length": 256,
            "description": "API key (alphanumeric, dashes, underscores)"
        }),
    );

    rules
}

fn add_transportation_rules() -> HashMap<String, Value> {
    let mut rules = HashMap::new();

    // LicensePlate operators
    rules.insert(
        "license_plate_country_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{2}$",
            "description": "ISO 3166-1 country code"
        }),
    );

    // VIN operators
    rules.insert(
        "vin_wmi_eq".to_string(),
        json!({
            "length": 3,
            "pattern": "^[A-Z0-9]{3}$",
            "description": "VIN World Manufacturer Identifier"
        }),
    );

    rules.insert(
        "vin_manufacturer_eq".to_string(),
        json!({
            "length": 3,
            "pattern": "^[A-Z0-9]{3}$",
            "description": "VIN manufacturer code"
        }),
    );

    // TrackingNumber operators
    rules.insert(
        "tracking_number_carrier_eq".to_string(),
        json!({
            "enum": ["UPS", "FedEx", "DHL", "USPS", "Other"],
            "description": "Shipping carrier"
        }),
    );

    // ContainerNumber operators
    rules.insert(
        "container_number_owner_code_eq".to_string(),
        json!({
            "length": 3,
            "pattern": "^[A-Z]{3}$",
            "description": "ISO 6346 owner code"
        }),
    );

    rules
}

fn add_network_rules() -> HashMap<String, Value> {
    let mut rules = HashMap::new();

    // Port operators
    rules.insert(
        "port_eq".to_string(),
        json!({
            "min": 1.0,
            "max": 65535.0,
            "description": "Network port number"
        }),
    );

    // AirportCode operators
    rules.insert(
        "airport_code_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{3}$",
            "description": "IATA airport code"
        }),
    );

    // PortCode operators
    rules.insert(
        "port_code_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{3}$",
            "description": "UN/LOCODE port code"
        }),
    );

    // FlightNumber operators
    rules.insert(
        "flight_number_airline_code_eq".to_string(),
        json!({
            "pattern": "^[A-Z]{2}$",
            "description": "IATA airline code"
        }),
    );

    rules
}

fn add_content_rules() -> HashMap<String, Value> {
    let mut rules = HashMap::new();

    // MimeType operators
    rules.insert(
        "mime_type_eq".to_string(),
        json!({
            "pattern": "^[a-z]+/[a-z0-9.+\\-]+$",
            "description": "MIME type (e.g., image/png)"
        }),
    );

    // Color operators
    rules.insert(
        "color_format_eq".to_string(),
        json!({
            "enum": ["hex", "rgb", "hsl", "named"],
            "description": "Color format"
        }),
    );

    rules
}

fn add_measurement_rules() -> HashMap<String, Value> {
    let mut rules = HashMap::new();

    // Duration operators
    rules.insert(
        "duration_within_range".to_string(),
        json!({
            "min": 0.0,
            "description": "Duration in seconds"
        }),
    );

    // Percentage operators
    rules.insert(
        "percentage_within_range".to_string(),
        json!({
            "min": 0.0,
            "max": 100.0,
            "description": "Percentage value (0-100)"
        }),
    );

    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_rules_exist() {
        let rules = get_default_rules();
        assert!(!rules.is_empty());
        println!("Total default rules: {}", rules.len());
    }

    #[test]
    fn test_email_domain_rule() {
        let rules = get_default_rules();
        assert!(rules.contains_key("email_domain_eq"));
        assert!(rules.contains_key("email_domain_in"));
    }

    #[test]
    fn test_country_code_rule() {
        let rules = get_default_rules();
        assert!(rules.contains_key("country_code_eq"));
    }
}
