//! Lookup data for geographic, currency, and locale-based operators.
//!
//! This module provides static lookup tables used by rich filter operators
//! that require knowledge of external data (countries, currencies, timezones, etc.).
//!
//! # Data Structure
//!
//! Lookup tables are embedded as JSON in the compiled schema metadata, enabling
//! the runtime to perform lookups without external dependencies:
//!
//! ```json
//! {
//!   "countries": {
//!     "US": { "continent": "North America", "regions": ["Americas"], "in_eu": false, "in_schengen": false },
//!     "FR": { "continent": "Europe", "regions": ["Europe"], "in_eu": true, "in_schengen": true },
//!     ...
//!   },
//!   "currencies": {
//!     "USD": { "name": "US Dollar", "symbol": "$" },
//!     "EUR": { "name": "Euro", "symbol": "€" },
//!     ...
//!   },
//!   "timezones": {
//!     "UTC": { "offset": 0, "dst": false },
//!     "EST": { "offset": -300, "dst": true },
//!     ...
//!   }
//! }
//! ```
//!
//! # Lookup Operators
//!
//! These operators use the lookup data at runtime:
//! - Country: continent, region, EU/Schengen membership
//! - Currency: code, symbol, decimal places
//! - Timezone: UTC offset, daylight saving time
//! - Language: language family
//! - Locale: language, country, script

use std::collections::HashMap;

use serde_json::{Value, json};

/// Build lookup data for rich filter operators.
///
/// Returns a JSON structure containing all lookup tables needed by operators.
pub fn build_lookup_data() -> Value {
    json!({
        "countries": build_countries_lookup(),
        "currencies": build_currencies_lookup(),
        "timezones": build_timezones_lookup(),
        "languages": build_languages_lookup(),
    })
}

/// Build country code lookup table with continent, region, EU, Schengen data.
fn build_countries_lookup() -> HashMap<String, Value> {
    let mut countries = HashMap::new();

    // Europe
    countries.insert(
        "FR".to_string(),
        json!({
            "name": "France",
            "continent": "Europe",
            "region": "EU",
            "in_eu": true,
            "in_schengen": true,
        }),
    );
    countries.insert(
        "DE".to_string(),
        json!({
            "name": "Germany",
            "continent": "Europe",
            "region": "EU",
            "in_eu": true,
            "in_schengen": true,
        }),
    );
    countries.insert(
        "GB".to_string(),
        json!({
            "name": "United Kingdom",
            "continent": "Europe",
            "region": "EU",
            "in_eu": false,
            "in_schengen": false,
        }),
    );
    countries.insert(
        "IT".to_string(),
        json!({
            "name": "Italy",
            "continent": "Europe",
            "region": "EU",
            "in_eu": true,
            "in_schengen": true,
        }),
    );
    countries.insert(
        "ES".to_string(),
        json!({
            "name": "Spain",
            "continent": "Europe",
            "region": "EU",
            "in_eu": true,
            "in_schengen": true,
        }),
    );
    countries.insert(
        "PL".to_string(),
        json!({
            "name": "Poland",
            "continent": "Europe",
            "region": "EU",
            "in_eu": true,
            "in_schengen": true,
        }),
    );
    countries.insert(
        "NL".to_string(),
        json!({
            "name": "Netherlands",
            "continent": "Europe",
            "region": "EU",
            "in_eu": true,
            "in_schengen": true,
        }),
    );
    countries.insert(
        "BE".to_string(),
        json!({
            "name": "Belgium",
            "continent": "Europe",
            "region": "EU",
            "in_eu": true,
            "in_schengen": true,
        }),
    );
    countries.insert(
        "CH".to_string(),
        json!({
            "name": "Switzerland",
            "continent": "Europe",
            "region": "EU",
            "in_eu": false,
            "in_schengen": true,
        }),
    );

    // North America
    countries.insert(
        "US".to_string(),
        json!({
            "name": "United States",
            "continent": "North America",
            "region": "Americas",
            "in_eu": false,
            "in_schengen": false,
        }),
    );
    countries.insert(
        "CA".to_string(),
        json!({
            "name": "Canada",
            "continent": "North America",
            "region": "Americas",
            "in_eu": false,
            "in_schengen": false,
        }),
    );
    countries.insert(
        "MX".to_string(),
        json!({
            "name": "Mexico",
            "continent": "North America",
            "region": "Americas",
            "in_eu": false,
            "in_schengen": false,
        }),
    );

    // South America
    countries.insert(
        "BR".to_string(),
        json!({
            "name": "Brazil",
            "continent": "South America",
            "region": "Americas",
            "in_eu": false,
            "in_schengen": false,
        }),
    );
    countries.insert(
        "AR".to_string(),
        json!({
            "name": "Argentina",
            "continent": "South America",
            "region": "Americas",
            "in_eu": false,
            "in_schengen": false,
        }),
    );

    // Asia
    countries.insert(
        "CN".to_string(),
        json!({
            "name": "China",
            "continent": "Asia",
            "region": "Asia",
            "in_eu": false,
            "in_schengen": false,
        }),
    );
    countries.insert(
        "JP".to_string(),
        json!({
            "name": "Japan",
            "continent": "Asia",
            "region": "Asia",
            "in_eu": false,
            "in_schengen": false,
        }),
    );
    countries.insert(
        "IN".to_string(),
        json!({
            "name": "India",
            "continent": "Asia",
            "region": "Asia",
            "in_eu": false,
            "in_schengen": false,
        }),
    );
    countries.insert(
        "SG".to_string(),
        json!({
            "name": "Singapore",
            "continent": "Asia",
            "region": "Asia",
            "in_eu": false,
            "in_schengen": false,
        }),
    );

    // Africa
    countries.insert(
        "ZA".to_string(),
        json!({
            "name": "South Africa",
            "continent": "Africa",
            "region": "Africa",
            "in_eu": false,
            "in_schengen": false,
        }),
    );
    countries.insert(
        "EG".to_string(),
        json!({
            "name": "Egypt",
            "continent": "Africa",
            "region": "Africa",
            "in_eu": false,
            "in_schengen": false,
        }),
    );

    // Oceania
    countries.insert(
        "AU".to_string(),
        json!({
            "name": "Australia",
            "continent": "Oceania",
            "region": "Oceania",
            "in_eu": false,
            "in_schengen": false,
        }),
    );
    countries.insert(
        "NZ".to_string(),
        json!({
            "name": "New Zealand",
            "continent": "Oceania",
            "region": "Oceania",
            "in_eu": false,
            "in_schengen": false,
        }),
    );

    countries
}

/// Build currency code lookup table with symbols and decimal places.
fn build_currencies_lookup() -> HashMap<String, Value> {
    let mut currencies = HashMap::new();

    currencies.insert(
        "USD".to_string(),
        json!({
            "name": "US Dollar",
            "symbol": "$",
            "decimal_places": 2,
        }),
    );
    currencies.insert(
        "EUR".to_string(),
        json!({
            "name": "Euro",
            "symbol": "€",
            "decimal_places": 2,
        }),
    );
    currencies.insert(
        "GBP".to_string(),
        json!({
            "name": "British Pound",
            "symbol": "£",
            "decimal_places": 2,
        }),
    );
    currencies.insert(
        "JPY".to_string(),
        json!({
            "name": "Japanese Yen",
            "symbol": "¥",
            "decimal_places": 0,
        }),
    );
    currencies.insert(
        "CHF".to_string(),
        json!({
            "name": "Swiss Franc",
            "symbol": "₣",
            "decimal_places": 2,
        }),
    );
    currencies.insert(
        "CAD".to_string(),
        json!({
            "name": "Canadian Dollar",
            "symbol": "C$",
            "decimal_places": 2,
        }),
    );
    currencies.insert(
        "AUD".to_string(),
        json!({
            "name": "Australian Dollar",
            "symbol": "A$",
            "decimal_places": 2,
        }),
    );
    currencies.insert(
        "CNY".to_string(),
        json!({
            "name": "Chinese Yuan",
            "symbol": "¥",
            "decimal_places": 2,
        }),
    );
    currencies.insert(
        "INR".to_string(),
        json!({
            "name": "Indian Rupee",
            "symbol": "₹",
            "decimal_places": 2,
        }),
    );
    currencies.insert(
        "MXN".to_string(),
        json!({
            "name": "Mexican Peso",
            "symbol": "$",
            "decimal_places": 2,
        }),
    );

    currencies
}

/// Build timezone lookup table with UTC offsets and DST info.
fn build_timezones_lookup() -> HashMap<String, Value> {
    let mut timezones = HashMap::new();

    // UTC
    timezones.insert(
        "UTC".to_string(),
        json!({
            "offset_minutes": 0,
            "has_dst": false,
            "region": "UTC",
        }),
    );

    // Europe
    timezones.insert(
        "GMT".to_string(),
        json!({
            "offset_minutes": 0,
            "has_dst": false,
            "region": "Europe",
        }),
    );
    timezones.insert(
        "CET".to_string(),
        json!({
            "offset_minutes": 60,
            "has_dst": true,
            "region": "Europe",
        }),
    );
    timezones.insert(
        "CEST".to_string(),
        json!({
            "offset_minutes": 120,
            "has_dst": false,
            "region": "Europe",
        }),
    );

    // North America
    timezones.insert(
        "EST".to_string(),
        json!({
            "offset_minutes": -300,
            "has_dst": true,
            "region": "Americas",
        }),
    );
    timezones.insert(
        "EDT".to_string(),
        json!({
            "offset_minutes": -240,
            "has_dst": false,
            "region": "Americas",
        }),
    );
    timezones.insert(
        "CST".to_string(),
        json!({
            "offset_minutes": -360,
            "has_dst": true,
            "region": "Americas",
        }),
    );
    timezones.insert(
        "CDT".to_string(),
        json!({
            "offset_minutes": -300,
            "has_dst": false,
            "region": "Americas",
        }),
    );
    timezones.insert(
        "MST".to_string(),
        json!({
            "offset_minutes": -420,
            "has_dst": true,
            "region": "Americas",
        }),
    );
    timezones.insert(
        "MDT".to_string(),
        json!({
            "offset_minutes": -360,
            "has_dst": false,
            "region": "Americas",
        }),
    );
    timezones.insert(
        "PST".to_string(),
        json!({
            "offset_minutes": -480,
            "has_dst": true,
            "region": "Americas",
        }),
    );
    timezones.insert(
        "PDT".to_string(),
        json!({
            "offset_minutes": -420,
            "has_dst": false,
            "region": "Americas",
        }),
    );

    // Asia
    timezones.insert(
        "JST".to_string(),
        json!({
            "offset_minutes": 540,
            "has_dst": false,
            "region": "Asia",
        }),
    );
    timezones.insert(
        "IST".to_string(),
        json!({
            "offset_minutes": 330,
            "has_dst": false,
            "region": "Asia",
        }),
    );
    timezones.insert(
        "SGT".to_string(),
        json!({
            "offset_minutes": 480,
            "has_dst": false,
            "region": "Asia",
        }),
    );
    timezones.insert(
        "AEST".to_string(),
        json!({
            "offset_minutes": 600,
            "has_dst": false,
            "region": "Oceania",
        }),
    );

    timezones
}

/// Build language code lookup table with language families.
fn build_languages_lookup() -> HashMap<String, Value> {
    let mut languages = HashMap::new();

    // Indo-European
    languages.insert(
        "EN".to_string(),
        json!({
            "name": "English",
            "family": "Indo-European",
            "script": "Latin",
        }),
    );
    languages.insert(
        "FR".to_string(),
        json!({
            "name": "French",
            "family": "Indo-European",
            "script": "Latin",
        }),
    );
    languages.insert(
        "DE".to_string(),
        json!({
            "name": "German",
            "family": "Indo-European",
            "script": "Latin",
        }),
    );
    languages.insert(
        "ES".to_string(),
        json!({
            "name": "Spanish",
            "family": "Indo-European",
            "script": "Latin",
        }),
    );
    languages.insert(
        "IT".to_string(),
        json!({
            "name": "Italian",
            "family": "Indo-European",
            "script": "Latin",
        }),
    );
    languages.insert(
        "PT".to_string(),
        json!({
            "name": "Portuguese",
            "family": "Indo-European",
            "script": "Latin",
        }),
    );
    languages.insert(
        "RU".to_string(),
        json!({
            "name": "Russian",
            "family": "Indo-European",
            "script": "Cyrillic",
        }),
    );

    // Sino-Tibetan
    languages.insert(
        "ZH".to_string(),
        json!({
            "name": "Chinese",
            "family": "Sino-Tibetan",
            "script": "Han",
        }),
    );

    // Japonic
    languages.insert(
        "JA".to_string(),
        json!({
            "name": "Japanese",
            "family": "Japonic",
            "script": "Japanese",
        }),
    );

    // Koreanic
    languages.insert(
        "KO".to_string(),
        json!({
            "name": "Korean",
            "family": "Koreanic",
            "script": "Hangul",
        }),
    );

    // Austroasiatic
    languages.insert(
        "VI".to_string(),
        json!({
            "name": "Vietnamese",
            "family": "Austroasiatic",
            "script": "Latin",
        }),
    );

    languages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_lookup_data() {
        let data = build_lookup_data();

        assert!(data.get("countries").is_some());
        assert!(data.get("currencies").is_some());
        assert!(data.get("timezones").is_some());
        assert!(data.get("languages").is_some());
    }

    #[test]
    fn test_countries_have_required_fields() {
        let countries = build_countries_lookup();

        for (code, data) in countries {
            assert!(data.get("name").is_some(), "Country {code} missing name");
            assert!(data.get("continent").is_some(), "Country {code} missing continent");
            assert!(data.get("in_eu").is_some(), "Country {code} missing in_eu");
            assert!(data.get("in_schengen").is_some(), "Country {code} missing in_schengen");
        }
    }

    #[test]
    fn test_currencies_have_required_fields() {
        let currencies = build_currencies_lookup();

        for (code, data) in currencies {
            assert!(data.get("name").is_some(), "Currency {code} missing name");
            assert!(data.get("symbol").is_some(), "Currency {code} missing symbol");
            assert!(data.get("decimal_places").is_some(), "Currency {code} missing decimal_places");
        }
    }

    #[test]
    fn test_timezones_have_required_fields() {
        let timezones = build_timezones_lookup();

        for (code, data) in timezones {
            assert!(data.get("offset_minutes").is_some(), "Timezone {code} missing offset_minutes");
            assert!(data.get("has_dst").is_some(), "Timezone {code} missing has_dst");
        }
    }

    #[test]
    fn test_eu_member_states() {
        let countries = build_countries_lookup();

        // Check some known EU members
        assert!(countries["FR"]["in_eu"].as_bool().unwrap());
        assert!(countries["DE"]["in_eu"].as_bool().unwrap());
        assert!(countries["IT"]["in_eu"].as_bool().unwrap());

        // Check non-EU
        assert!(!countries["US"]["in_eu"].as_bool().unwrap());
        assert!(!countries["GB"]["in_eu"].as_bool().unwrap());
    }

    #[test]
    fn test_schengen_members() {
        let countries = build_countries_lookup();

        // Check some known Schengen members
        assert!(countries["FR"]["in_schengen"].as_bool().unwrap());
        assert!(countries["DE"]["in_schengen"].as_bool().unwrap());
        assert!(countries["CH"]["in_schengen"].as_bool().unwrap());

        // Check non-Schengen
        assert!(!countries["US"]["in_schengen"].as_bool().unwrap());
        assert!(!countries["GB"]["in_schengen"].as_bool().unwrap());
    }
}
