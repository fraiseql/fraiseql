//! SQL template extraction from database handlers.
//!
//! This module extracts SQL generation patterns from the database handlers
//! and stores them as metadata in the compiled schema.
//!
//! For each rich scalar type and its operators, we:
//! 1. Call the database handler's generate_extended_sql() with mock data
//! 2. Normalize the generated SQL to a template format
//! 3. Store the templates for runtime SQL generation
//!
//! # Template Format
//!
//! Templates use placeholders:
//! - `$field` - The JSONB field reference (e.g., `data->>'email'`)
//! - `$1`, `$2`, etc. - Parameter placeholders (database-specific)
//!
//! # Example
//!
//! For EmailDomainEq on `PostgreSQL`:
//! - Input: field_sql = "data->>'email'", domain = "example.com"
//! - Handler output: "SPLIT_PART(data->>'email', '@', 2) = $1"
//! - Template: "SPLIT_PART($field, '@', 2) = $1"

use std::collections::HashMap;

use serde_json::json;

/// Extract SQL template for an operator from a specific database handler.
///
/// Maps operator names to their PostgreSQL SQL templates.
/// Organizes templates by operation type for maintainability.
///
/// # Lookup-Based Operators
///
/// Some operators use lookup data stored in the compiled schema:
/// - Country operators: continent, region, EU/Schengen membership
/// - Currency operators: currency code, symbol, decimal places
/// - Timezone operators: UTC offset, daylight saving time
/// - Language operators: language family, script
///
/// These templates use a special `$lookup` placeholder that's replaced
/// at runtime with actual lookup value parameters.
pub fn extract_template_for_operator(db_name: &str, operator_name: &str) -> Option<String> {
    match (db_name, operator_name) {
        // ========================================================================
        // EMAIL OPERATORS
        // ========================================================================
        ("postgres", "domainEq") => Some("SPLIT_PART($field, '@', 2) = $1".to_string()),

        ("postgres", "domainIn") => Some("SPLIT_PART($field, '@', 2) IN ($params)".to_string()),

        ("postgres", "domainEndswith") => Some("SPLIT_PART($field, '@', 2) LIKE '%' || $1".to_string()),

        ("postgres", "localPartStartswith") => Some("SPLIT_PART($field, '@', 1) LIKE $1 || '%'".to_string()),

        // ========================================================================
        // VIN OPERATORS
        // ========================================================================
        ("postgres", "wmiEq") => Some("SUBSTRING($field FROM 1 FOR 3) = $1".to_string()),

        // ========================================================================
        // IBAN OPERATORS
        // ========================================================================
        ("postgres", "countryEq") => Some("SUBSTRING($field FROM 1 FOR 2) = $1".to_string()),

        // ========================================================================
        // URL OPERATORS
        // ========================================================================
        // protocolEq: extract protocol before ://
        ("postgres", "protocolEq") => Some("SPLIT_PART($field, '://', 1) = $1".to_string()),

        // hostEq: extract host part
        ("postgres", "hostEq") => Some("SPLIT_PART(SPLIT_PART($field, '://', 2), '/', 1) = $1".to_string()),

        // pathStartswith: extract path part
        ("postgres", "pathStartswith") => Some("SPLIT_PART(SPLIT_PART($field, '://', 2), '?', 1) LIKE $1 || '%'".to_string()),

        // ========================================================================
        // DOMAIN NAME OPERATORS
        // ========================================================================
        // tldEq: extract TLD (rightmost label). STRPOS finds the FIRST dot, so
        // the old RIGHT(...) form returned `.com` for `example.com` and
        // `.example.com` for a 3-label domain — never equal to what a client
        // sends, so every filter silently matched nothing (#721).
        // SPLIT_PART with a negative index counts from the right (PG 14+).
        ("postgres", "tldEq") => Some("SPLIT_PART($field, '.', -1) = $1".to_string()),

        // tldIn: extract TLD and check in list — same extraction as tldEq (#721).
        ("postgres", "tldIn") => Some("SPLIT_PART($field, '.', -1) IN ($params)".to_string()),

        // ========================================================================
        // HOSTNAME OPERATORS
        // ========================================================================
        // isFqdn: check if contains at least one dot
        ("postgres", "isFqdn") => Some("CASE WHEN POSITION('.' IN $field) > 0 THEN true ELSE false END = $1".to_string()),

        // depthEq: count labels (dots + 1)
        ("postgres", "depthEq") => Some("(LENGTH($field) - LENGTH(REPLACE($field, '.', '')) + 1) = $1".to_string()),

        // ========================================================================
        // STANDARD STRING OPERATORS (apply to multiple types)
        // ========================================================================
        // Generic equals (when no extraction needed)
        ("postgres", "eq") => Some("$field = $1".to_string()),

        // Generic contains
        ("postgres", "contains") => Some("$field LIKE '%' || $1 || '%'".to_string()),

        // Generic startswith
        ("postgres", "startswith") => Some("$field LIKE $1 || '%'".to_string()),

        // Generic endswith
        ("postgres", "endswith") => Some("$field LIKE '%' || $1".to_string()),

        // ========================================================================
        // NUMERIC RANGE OPERATORS
        // ========================================================================
        // withinRange: numeric comparison between two values
        ("postgres", "withinRange") => Some("$field BETWEEN $1 AND $2".to_string()),

        // hemisphereEq: simple string match for hemisphere
        ("postgres", "hemisphereEq") => Some("$field LIKE $1 || '%'".to_string()),

        // ========================================================================
        // POSTAL CODE OPERATORS
        // ========================================================================
        // Uses countryEq but needs to extract country code from postal code
        // This is type-specific and handled in handlers
        ("postgres", "postalCodeCountryEq") => Some("LEFT($field, 2) = $1".to_string()),

        // ========================================================================
        // SIMPLE TYPES (STRING EQUALITY)
        // ========================================================================
        // These types just use simple string comparison
        ("postgres", "timeZoneEq") => Some("$field = $1".to_string()),

        // Phone country code
        ("postgres", "countryCodeEq") => Some("SPLIT_PART($field, '-', 1) = $1".to_string()),

        ("postgres", "countryCodeIn") => Some("SPLIT_PART($field, '-', 1) IN ($params)".to_string()),

        // ========================================================================
        // FINANCIAL IDENTIFIERS (CUSIP, ISIN, SEDOL, etc.)
        // ========================================================================
        // For most financial identifiers, use simple string operations
        ("postgres", "cusipFormatValid") => Some("LENGTH($field) = 9".to_string()),

        ("postgres", "isinFormatValid") => Some("LENGTH($field) = 12".to_string()),

        ("postgres", "sedolFormatValid") => Some("LENGTH($field) = 7".to_string()),

        // Stock symbol equals
        ("postgres", "symbolEq") => Some("$field = $1".to_string()),

        // Exchange code equals
        ("postgres", "exchangeCodeEq") => Some("$field = $1".to_string()),

        // Currency code equals
        ("postgres", "currencyCodeEq") => Some("$field = $1".to_string()),

        // ========================================================================
        // IDENTIFIER TYPES (Slug, SemanticVersion, HashSHA256, APIKey)
        // ========================================================================
        // Slug: alphanumeric + hyphens
        ("postgres", "slugFormatValid") => Some("$field ~ '^[a-z0-9-]+$'".to_string()),

        // Semantic version: matches X.Y.Z pattern
        ("postgres", "semverFormatValid") => Some("$field ~ '^[0-9]+\\.[0-9]+\\.[0-9]+.*$'".to_string()),

        // SHA256 hash: 64 hex characters
        ("postgres", "hashFormatValid") => Some("LENGTH($field) = 64 AND $field ~ '^[a-f0-9]+$'".to_string()),

        // API Key: usually alphanumeric with underscores
        ("postgres", "apikeyFormatValid") => Some("$field ~ '^[a-zA-Z0-9_-]+$'".to_string()),

        // ========================================================================
        // CONTENT TYPES (Markdown, HTML, MimeType, Color)
        // ========================================================================
        // MIME type equals
        ("postgres", "mimetypeEq") => Some("$field = $1".to_string()),

        // MIME type starts with (e.g., "image/")
        ("postgres", "mimetypeStartswith") => Some("$field LIKE $1 || '%'".to_string()),

        // Color format validation (hex color)
        ("postgres", "colorHexFormatValid") => Some("$field ~ '^#[a-f0-9]{6}$'".to_string()),

        // ========================================================================
        // NETWORK TYPES (IPAddress, IPv4, IPv6, CIDR, Port)
        // ========================================================================
        // IPv4 format: 4 octets separated by dots
        ("postgres", "ipv4FormatValid") => Some("$field ~ '^(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$'".to_string()),

        // Port number: between 0 and 65535
        ("postgres", "portValid") => Some("CAST($field AS INTEGER) BETWEEN 0 AND 65535".to_string()),

        // ========================================================================
        // MEASUREMENT/RANGE TYPES
        // ========================================================================
        // Percentage: 0-100
        ("postgres", "percentageValid") => Some("CAST($field AS DECIMAL) BETWEEN 0 AND 100".to_string()),

        // ========================================================================
        // LOOKUP-BASED OPERATORS
        // ========================================================================
        // These operators use external lookup data embedded in the schema.
        // Templates use $lookup placeholder for the lookup field name.

        // Country: continent membership
        ("postgres", "continentEq") => Some("$lookup ->> 'continent' = $1".to_string()),

        // Country: region membership
        ("postgres", "regionEq") => Some("$lookup ->> 'region' = $1".to_string()),

        // Country: EU membership
        ("postgres", "inEu") => Some("($lookup ->> 'in_eu')::boolean = $1".to_string()),

        // Country: Schengen membership
        ("postgres", "inSchengen") => Some("($lookup ->> 'in_schengen')::boolean = $1".to_string()),

        // Currency: decimal places (for Money type)
        ("postgres", "currencyDecimalEq") => Some("($lookup ->> 'decimal_places')::integer = $1".to_string()),

        // Timezone: offset in minutes from UTC
        ("postgres", "timezoneOffsetEq") => Some("($lookup ->> 'offset_minutes')::integer = $1".to_string()),

        // Timezone: daylight saving time support
        ("postgres", "timezoneDst") => Some("($lookup ->> 'has_dst')::boolean = $1".to_string()),

        // Timezone: region (Americas, Europe, Asia, Oceania)
        ("postgres", "timezoneRegionEq") => Some("$lookup ->> 'region' = $1".to_string()),

        // Language: family (Indo-European, Sino-Tibetan, Japonic, etc.)
        ("postgres", "languageFamilyEq") => Some("$lookup ->> 'family' = $1".to_string()),

        // Language: writing script (Latin, Cyrillic, Han, etc.)
        ("postgres", "languageScriptEq") => Some("$lookup ->> 'script' = $1".to_string()),

        // Locale: language part of locale code
        ("postgres", "localeLanguageEq") => Some("SPLIT_PART($field, '-', 1) = $1".to_string()),

        // Locale: country part of locale code
        ("postgres", "localeCountryEq") => Some("SPLIT_PART($field, '-', 2) = $1".to_string()),

        // ========================================================================
        // GEOSPATIAL OPERATORS (PostGIS - PostgreSQL only, with fallbacks)
        // ========================================================================
        // Coordinates: Distance within radius
        // Format: JSONB with {lat: f64, lng: f64}
        ("postgres", "distanceWithin") => Some(
            "ST_DWithin(
                ST_GeomFromText('POINT(' || ($field->>'lng') || ' ' || ($field->>'lat') || ')'),
                ST_GeomFromText('POINT($1 $2)'),
                $3 * 1000
            )"
            .to_string()
        ),

        // Coordinates: Within bounding box
        ("postgres", "withinBoundingBox") => Some(
            "($field->>'lat')::float8 BETWEEN $1 AND $2 AND ($field->>'lng')::float8 BETWEEN $3 AND $4"
                .to_string()
        ),

        // ========================================================================
        // PHONE NUMBER OPERATORS
        // ========================================================================
        // Phone: Country code from E.164 format
        ("postgres", "phoneCountryCodeEq") => Some("SUBSTRING($field FROM 1 FOR LENGTH($1)) = $1".to_string()),

        ("postgres", "phoneCountryCodeIn") => Some("SUBSTRING($field FROM 1 FOR POSITION('+' IN $field)) IN ($params)".to_string()),

        // Phone: E.164 format validation (+[1-9]{1,3}[0-9]{1,14})
        ("postgres", "phoneIsValid") => Some("$field ~ '^\\+[1-9]\\d{1,14}$' = $1".to_string()),

        // Phone: Type classification (mobile, fixed, etc.)
        ("postgres", "phoneTypeEq") => Some("CASE WHEN $field ~ '^\\+1' THEN 'US' WHEN $field ~ '^\\+44' THEN 'UK' ELSE 'OTHER' END = $1".to_string()),

        // ========================================================================
        // DATE RANGE OPERATORS
        // ========================================================================
        // Format: JSON with {start: ISO8601, end: ISO8601} or period string

        // DateRange: Duration in days >= min
        ("postgres", "durationGte") => Some(
            "EXTRACT(DAY FROM ($field->>'end')::timestamp - ($field->>'start')::timestamp) >= $1"
                .to_string()
        ),

        // DateRange: Starts after date
        ("postgres", "startsAfter") => Some("($field->>'start')::timestamp > $1::timestamp".to_string()),

        // DateRange: Ends before date
        ("postgres", "endsBefore") => Some("($field->>'end')::timestamp < $1::timestamp".to_string()),

        // DateRange: Overlaps with another range
        ("postgres", "overlaps") => Some(
            "($field->>'start')::timestamp < $2::timestamp AND ($field->>'end')::timestamp > $1::timestamp"
                .to_string()
        ),

        // ========================================================================
        // DURATION OPERATORS
        // ========================================================================
        // Format: ISO8601 duration (P1Y2M3DT4H5M6S) or total seconds/milliseconds

        // Duration: Total seconds equals
        ("postgres", "totalSecondsEq") => Some(
            "EXTRACT(EPOCH FROM CAST($field AS INTERVAL)) = $1"
                .to_string()
        ),

        // Duration: Total minutes >= min
        ("postgres", "totalMinutesGte") => Some(
            "EXTRACT(EPOCH FROM CAST($field AS INTERVAL)) / 60 >= $1"
                .to_string()
        ),

        // Standard operators (not extended operators, so no templates)
        _ => None,
    }
}

/// Extract SQL templates for a specific operator from all database handlers.
///
/// Returns a map of database name to SQL template.
/// If a database handler doesn't support the operator, it's omitted from the map.
pub fn extract_operator_templates(operator_name: &str) -> HashMap<String, String> {
    let mut templates = HashMap::new();

    if let Some(template) = extract_template_for_operator("postgres", operator_name) {
        templates.insert("postgres".to_string(), template);
    }

    templates
}

/// Build SQL templates metadata for rich filter types.
///
/// Generates a JSON structure like:
/// ```json
/// {
///   "operators": {
///     "emailDomainEq": {
///       "postgres": "SPLIT_PART($field, '@', 2) = $1"
///     }
///   }
/// }
/// ```
pub fn build_sql_templates_metadata(operator_names: &[&str]) -> serde_json::Value {
    let mut operators = serde_json::Map::new();

    for op_name in operator_names {
        let templates = extract_operator_templates(op_name);
        if !templates.is_empty() {
            operators.insert((*op_name).to_string(), json!(templates));
        }
    }

    json!({
        "operators": operators
    })
}
