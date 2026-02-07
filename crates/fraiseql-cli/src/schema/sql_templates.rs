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
//! For EmailDomainEq on PostgreSQL:
//! - Input: field_sql = "data->>'email'", domain = "example.com"
//! - Handler output: "SPLIT_PART(data->>'email', '@', 2) = $1"
//! - Template: "SPLIT_PART($field, '@', 2) = $1"

use std::collections::HashMap;

use serde_json::json;

/// Extract SQL template for an operator from a specific database handler.
///
/// Maps operator names to SQL templates for all 4 databases.
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
fn extract_template_for_operator(db_name: &str, operator_name: &str) -> Option<String> {
    match (db_name, operator_name) {
        // ========================================================================
        // EMAIL OPERATORS
        // ========================================================================
        ("postgres", "domainEq") => Some("SPLIT_PART($field, '@', 2) = $1".to_string()),
        ("mysql", "domainEq") => Some("SUBSTRING_INDEX($field, '@', -1) = ?".to_string()),
        ("sqlite", "domainEq") => Some("SUBSTR($field, INSTR($field, '@') + 1) = ?".to_string()),
        ("sqlserver", "domainEq") => Some("SUBSTRING($field, CHARINDEX('@', $field) + 1, LEN($field)) = ?".to_string()),

        ("postgres", "domainIn") => Some("SPLIT_PART($field, '@', 2) IN ($params)".to_string()),
        ("mysql", "domainIn") => Some("SUBSTRING_INDEX($field, '@', -1) IN ($params)".to_string()),
        ("sqlite", "domainIn") => Some("SUBSTR($field, INSTR($field, '@') + 1) IN ($params)".to_string()),
        ("sqlserver", "domainIn") => Some("SUBSTRING($field, CHARINDEX('@', $field) + 1, LEN($field)) IN ($params)".to_string()),

        ("postgres", "domainEndswith") => Some("SPLIT_PART($field, '@', 2) LIKE '%' || $1".to_string()),
        ("mysql", "domainEndswith") => Some("SUBSTRING_INDEX($field, '@', -1) LIKE CONCAT('%', ?)".to_string()),
        ("sqlite", "domainEndswith") => Some("SUBSTR($field, INSTR($field, '@') + 1) LIKE '%' || ?".to_string()),
        ("sqlserver", "domainEndswith") => Some("SUBSTRING($field, CHARINDEX('@', $field) + 1, LEN($field)) LIKE '%' + ?".to_string()),

        ("postgres", "localPartStartswith") => Some("SPLIT_PART($field, '@', 1) LIKE $1 || '%'".to_string()),
        ("mysql", "localPartStartswith") => Some("SUBSTRING_INDEX($field, '@', 1) LIKE CONCAT(?, '%')".to_string()),
        ("sqlite", "localPartStartswith") => Some("SUBSTR($field, 1, INSTR($field, '@') - 1) LIKE ? || '%'".to_string()),
        ("sqlserver", "localPartStartswith") => Some("SUBSTRING($field, 1, CHARINDEX('@', $field) - 1) LIKE ? + '%'".to_string()),

        // ========================================================================
        // VIN OPERATORS
        // ========================================================================
        ("postgres", "wmiEq") => Some("SUBSTRING($field FROM 1 FOR 3) = $1".to_string()),
        ("mysql", "wmiEq") => Some("SUBSTRING($field, 1, 3) = ?".to_string()),
        ("sqlite", "wmiEq") => Some("SUBSTR($field, 1, 3) = ?".to_string()),
        ("sqlserver", "wmiEq") => Some("SUBSTRING($field, 1, 3) = ?".to_string()),

        // ========================================================================
        // IBAN OPERATORS
        // ========================================================================
        ("postgres", "countryEq") => Some("SUBSTRING($field FROM 1 FOR 2) = $1".to_string()),
        ("mysql", "countryEq") => Some("SUBSTRING($field, 1, 2) = ?".to_string()),
        ("sqlite", "countryEq") => Some("SUBSTR($field, 1, 2) = ?".to_string()),
        ("sqlserver", "countryEq") => Some("SUBSTRING($field, 1, 2) = ?".to_string()),

        // ========================================================================
        // URL OPERATORS
        // ========================================================================
        // protocolEq: extract protocol before ://
        ("postgres", "protocolEq") => Some("SPLIT_PART($field, '://', 1) = $1".to_string()),
        ("mysql", "protocolEq") => Some("SUBSTRING($field, 1, LOCATE('://', $field) - 1) = ?".to_string()),
        ("sqlite", "protocolEq") => Some("SUBSTR($field, 1, INSTR($field, '://') - 1) = ?".to_string()),
        ("sqlserver", "protocolEq") => Some("SUBSTRING($field, 1, CHARINDEX('://', $field) - 1) = ?".to_string()),

        // hostEq: extract host part
        ("postgres", "hostEq") => Some("SPLIT_PART(SPLIT_PART($field, '://', 2), '/', 1) = $1".to_string()),
        ("mysql", "hostEq") => Some("SUBSTRING(SUBSTRING($field, LOCATE('://', $field) + 3), 1, LOCATE('/', SUBSTRING($field, LOCATE('://', $field) + 3)) - 1) = ?".to_string()),
        ("sqlite", "hostEq") => Some("SUBSTR($field, INSTR($field, '://') + 3, INSTR(SUBSTR($field, INSTR($field, '://') + 3), '/') - 1) = ?".to_string()),
        ("sqlserver", "hostEq") => Some("SUBSTRING(SUBSTRING($field, CHARINDEX('://', $field) + 3), 1, CHARINDEX('/', SUBSTRING($field, CHARINDEX('://', $field) + 3)) - 1) = ?".to_string()),

        // pathStartswith: extract path part
        ("postgres", "pathStartswith") => Some("SPLIT_PART(SPLIT_PART($field, '://', 2), '?', 1) LIKE $1 || '%'".to_string()),
        ("mysql", "pathStartswith") => Some("SUBSTRING(SUBSTRING($field, LOCATE('://', $field) + 3), LOCATE('/', SUBSTRING($field, LOCATE('://', $field) + 3)), LOCATE('?', SUBSTRING($field, LOCATE('://', $field) + 3)) - LOCATE('/', SUBSTRING($field, LOCATE('://', $field) + 3))) LIKE CONCAT(?, '%')".to_string()),
        ("sqlite", "pathStartswith") => Some("SUBSTR(SUBSTR($field, INSTR($field, '://') + 3), INSTR(SUBSTR($field, INSTR($field, '://') + 3), '/')) LIKE ? || '%'".to_string()),
        ("sqlserver", "pathStartswith") => Some("SUBSTRING($field, CHARINDEX('/', $field, CHARINDEX('://', $field) + 3), CHARINDEX('?', $field) - CHARINDEX('/', $field, CHARINDEX('://', $field) + 3)) LIKE ? + '%'".to_string()),

        // ========================================================================
        // DOMAIN NAME OPERATORS
        // ========================================================================
        // tldEq: extract TLD (rightmost part after last dot)
        ("postgres", "tldEq") => Some("RIGHT($field, LENGTH($field) - STRPOS($field, '.') + 1) = $1".to_string()),
        ("mysql", "tldEq") => Some("SUBSTRING($field, LOCATE('.', REVERSE($field)) + 1) = ?".to_string()),
        ("sqlite", "tldEq") => Some("SUBSTR($field, INSTR($field, '.') + 1) = ?".to_string()),
        ("sqlserver", "tldEq") => Some("SUBSTRING($field, CHARINDEX('.', REVERSE($field)) + 1, LEN($field)) = ?".to_string()),

        // tldIn: extract TLD and check in list
        ("postgres", "tldIn") => Some("RIGHT($field, LENGTH($field) - STRPOS($field, '.') + 1) IN ($params)".to_string()),
        ("mysql", "tldIn") => Some("SUBSTRING($field, LOCATE('.', REVERSE($field)) + 1) IN ($params)".to_string()),
        ("sqlite", "tldIn") => Some("SUBSTR($field, INSTR($field, '.') + 1) IN ($params)".to_string()),
        ("sqlserver", "tldIn") => Some("SUBSTRING($field, CHARINDEX('.', REVERSE($field)) + 1, LEN($field)) IN ($params)".to_string()),

        // ========================================================================
        // HOSTNAME OPERATORS
        // ========================================================================
        // isFqdn: check if contains at least one dot
        ("postgres", "isFqdn") => Some("CASE WHEN POSITION('.' IN $field) > 0 THEN true ELSE false END = $1".to_string()),
        ("mysql", "isFqdn") => Some("CASE WHEN LOCATE('.', $field) > 0 THEN 1 ELSE 0 END = ?".to_string()),
        ("sqlite", "isFqdn") => Some("CASE WHEN INSTR($field, '.') > 0 THEN 1 ELSE 0 END = ?".to_string()),
        ("sqlserver", "isFqdn") => Some("CASE WHEN CHARINDEX('.', $field) > 0 THEN 1 ELSE 0 END = ?".to_string()),

        // depthEq: count labels (dots + 1)
        ("postgres", "depthEq") => Some("(LENGTH($field) - LENGTH(REPLACE($field, '.', '')) + 1) = $1".to_string()),
        ("mysql", "depthEq") => Some("(LENGTH($field) - LENGTH(REPLACE($field, '.', '')) + 1) = ?".to_string()),
        ("sqlite", "depthEq") => Some("(LENGTH($field) - LENGTH(REPLACE($field, '.', '')) + 1) = ?".to_string()),
        ("sqlserver", "depthEq") => Some("(LEN($field) - LEN(REPLACE($field, '.', '')) + 1) = ?".to_string()),

        // ========================================================================
        // STANDARD STRING OPERATORS (apply to multiple types)
        // ========================================================================
        // Generic equals (when no extraction needed)
        ("postgres", "eq") => Some("$field = $1".to_string()),
        ("mysql", "eq") => Some("$field = ?".to_string()),
        ("sqlite", "eq") => Some("$field = ?".to_string()),
        ("sqlserver", "eq") => Some("$field = ?".to_string()),

        // Generic contains
        ("postgres", "contains") => Some("$field LIKE '%' || $1 || '%'".to_string()),
        ("mysql", "contains") => Some("$field LIKE CONCAT('%', ?, '%')".to_string()),
        ("sqlite", "contains") => Some("$field LIKE '%' || ? || '%'".to_string()),
        ("sqlserver", "contains") => Some("$field LIKE '%' + ? + '%'".to_string()),

        // Generic startswith
        ("postgres", "startswith") => Some("$field LIKE $1 || '%'".to_string()),
        ("mysql", "startswith") => Some("$field LIKE CONCAT(?, '%')".to_string()),
        ("sqlite", "startswith") => Some("$field LIKE ? || '%'".to_string()),
        ("sqlserver", "startswith") => Some("$field LIKE ? + '%'".to_string()),

        // Generic endswith
        ("postgres", "endswith") => Some("$field LIKE '%' || $1".to_string()),
        ("mysql", "endswith") => Some("$field LIKE CONCAT('%', ?)".to_string()),
        ("sqlite", "endswith") => Some("$field LIKE '%' || ?".to_string()),
        ("sqlserver", "endswith") => Some("$field LIKE '%' + ?".to_string()),

        // ========================================================================
        // NUMERIC RANGE OPERATORS
        // ========================================================================
        // withinRange: numeric comparison between two values
        ("postgres", "withinRange") => Some("$field BETWEEN $1 AND $2".to_string()),
        ("mysql", "withinRange") => Some("$field BETWEEN ? AND ?".to_string()),
        ("sqlite", "withinRange") => Some("$field BETWEEN ? AND ?".to_string()),
        ("sqlserver", "withinRange") => Some("$field BETWEEN ? AND ?".to_string()),

        // hemisphereEq: simple string match for hemisphere
        ("postgres", "hemisphereEq") => Some("$field LIKE $1 || '%'".to_string()),
        ("mysql", "hemisphereEq") => Some("$field LIKE CONCAT(?, '%')".to_string()),
        ("sqlite", "hemisphereEq") => Some("$field LIKE ? || '%'".to_string()),
        ("sqlserver", "hemisphereEq") => Some("$field LIKE ? + '%'".to_string()),

        // ========================================================================
        // POSTAL CODE OPERATORS
        // ========================================================================
        // Uses countryEq but needs to extract country code from postal code
        // This is type-specific and handled in handlers
        ("postgres", "postalCodeCountryEq") => Some("LEFT($field, 2) = $1".to_string()),
        ("mysql", "postalCodeCountryEq") => Some("LEFT($field, 2) = ?".to_string()),
        ("sqlite", "postalCodeCountryEq") => Some("SUBSTR($field, 1, 2) = ?".to_string()),
        ("sqlserver", "postalCodeCountryEq") => Some("LEFT($field, 2) = ?".to_string()),

        // ========================================================================
        // SIMPLE TYPES (STRING EQUALITY)
        // ========================================================================
        // These types just use simple string comparison
        ("postgres", "timeZoneEq") => Some("$field = $1".to_string()),
        ("mysql", "timeZoneEq") => Some("$field = ?".to_string()),
        ("sqlite", "timeZoneEq") => Some("$field = ?".to_string()),
        ("sqlserver", "timeZoneEq") => Some("$field = ?".to_string()),

        // Phone country code
        ("postgres", "countryCodeEq") => Some("SPLIT_PART($field, '-', 1) = $1".to_string()),
        ("mysql", "countryCodeEq") => Some("SUBSTRING_INDEX($field, '-', 1) = ?".to_string()),
        ("sqlite", "countryCodeEq") => Some("SUBSTR($field, 1, INSTR($field, '-') - 1) = ?".to_string()),
        ("sqlserver", "countryCodeEq") => Some("SUBSTRING($field, 1, CHARINDEX('-', $field) - 1) = ?".to_string()),

        ("postgres", "countryCodeIn") => Some("SPLIT_PART($field, '-', 1) IN ($params)".to_string()),
        ("mysql", "countryCodeIn") => Some("SUBSTRING_INDEX($field, '-', 1) IN ($params)".to_string()),
        ("sqlite", "countryCodeIn") => Some("SUBSTR($field, 1, INSTR($field, '-') - 1) IN ($params)".to_string()),
        ("sqlserver", "countryCodeIn") => Some("SUBSTRING($field, 1, CHARINDEX('-', $field) - 1) IN ($params)".to_string()),

        // ========================================================================
        // FINANCIAL IDENTIFIERS (CUSIP, ISIN, SEDOL, etc.)
        // ========================================================================
        // For most financial identifiers, use simple string operations
        ("postgres", "cusipFormatValid") => Some("LENGTH($field) = 9".to_string()),
        ("mysql", "cusipFormatValid") => Some("LENGTH($field) = 9".to_string()),
        ("sqlite", "cusipFormatValid") => Some("LENGTH($field) = 9".to_string()),
        ("sqlserver", "cusipFormatValid") => Some("LEN($field) = 9".to_string()),

        ("postgres", "isinFormatValid") => Some("LENGTH($field) = 12".to_string()),
        ("mysql", "isinFormatValid") => Some("LENGTH($field) = 12".to_string()),
        ("sqlite", "isinFormatValid") => Some("LENGTH($field) = 12".to_string()),
        ("sqlserver", "isinFormatValid") => Some("LEN($field) = 12".to_string()),

        ("postgres", "sedolFormatValid") => Some("LENGTH($field) = 7".to_string()),
        ("mysql", "sedolFormatValid") => Some("LENGTH($field) = 7".to_string()),
        ("sqlite", "sedolFormatValid") => Some("LENGTH($field) = 7".to_string()),
        ("sqlserver", "sedolFormatValid") => Some("LEN($field) = 7".to_string()),

        // Stock symbol equals
        ("postgres", "symbolEq") => Some("$field = $1".to_string()),
        ("mysql", "symbolEq") => Some("$field = ?".to_string()),
        ("sqlite", "symbolEq") => Some("$field = ?".to_string()),
        ("sqlserver", "symbolEq") => Some("$field = ?".to_string()),

        // Exchange code equals
        ("postgres", "exchangeCodeEq") => Some("$field = $1".to_string()),
        ("mysql", "exchangeCodeEq") => Some("$field = ?".to_string()),
        ("sqlite", "exchangeCodeEq") => Some("$field = ?".to_string()),
        ("sqlserver", "exchangeCodeEq") => Some("$field = ?".to_string()),

        // Currency code equals
        ("postgres", "currencyCodeEq") => Some("$field = $1".to_string()),
        ("mysql", "currencyCodeEq") => Some("$field = ?".to_string()),
        ("sqlite", "currencyCodeEq") => Some("$field = ?".to_string()),
        ("sqlserver", "currencyCodeEq") => Some("$field = ?".to_string()),

        // ========================================================================
        // IDENTIFIER TYPES (Slug, SemanticVersion, HashSHA256, APIKey)
        // ========================================================================
        // Slug: alphanumeric + hyphens
        ("postgres", "slugFormatValid") => Some("$field ~ '^[a-z0-9-]+$'".to_string()),
        ("mysql", "slugFormatValid") => Some("$field REGEXP '^[a-z0-9-]+$'".to_string()),
        ("sqlite", "slugFormatValid") => Some("$field GLOB '[a-z0-9-]*'".to_string()),
        ("sqlserver", "slugFormatValid") => Some("$field LIKE '[a-z0-9-]*'".to_string()),

        // Semantic version: matches X.Y.Z pattern
        ("postgres", "semverFormatValid") => Some("$field ~ '^[0-9]+\\.[0-9]+\\.[0-9]+.*$'".to_string()),
        ("mysql", "semverFormatValid") => Some("$field REGEXP '^[0-9]+\\.[0-9]+\\.[0-9]+.*$'".to_string()),
        ("sqlite", "semverFormatValid") => Some("$field GLOB '[0-9]*.[0-9]*.[0-9]*'".to_string()),
        ("sqlserver", "semverFormatValid") => Some("$field LIKE '[0-9]%.[0-9]%.[0-9]%'".to_string()),

        // SHA256 hash: 64 hex characters
        ("postgres", "hashFormatValid") => Some("LENGTH($field) = 64 AND $field ~ '^[a-f0-9]+$'".to_string()),
        ("mysql", "hashFormatValid") => Some("LENGTH($field) = 64 AND $field REGEXP '^[a-f0-9]+$'".to_string()),
        ("sqlite", "hashFormatValid") => Some("LENGTH($field) = 64 AND $field GLOB '[a-f0-9]*'".to_string()),
        ("sqlserver", "hashFormatValid") => Some("LEN($field) = 64 AND $field LIKE '[a-f0-9]%'".to_string()),

        // API Key: usually alphanumeric with underscores
        ("postgres", "apikeyFormatValid") => Some("$field ~ '^[a-zA-Z0-9_-]+$'".to_string()),
        ("mysql", "apikeyFormatValid") => Some("$field REGEXP '^[a-zA-Z0-9_-]+$'".to_string()),
        ("sqlite", "apikeyFormatValid") => Some("$field GLOB '[a-zA-Z0-9_-]*'".to_string()),
        ("sqlserver", "apikeyFormatValid") => Some("$field LIKE '[a-zA-Z0-9_-]*'".to_string()),

        // ========================================================================
        // CONTENT TYPES (Markdown, HTML, MimeType, Color)
        // ========================================================================
        // MIME type equals
        ("postgres", "mimetypeEq") => Some("$field = $1".to_string()),
        ("mysql", "mimetypeEq") => Some("$field = ?".to_string()),
        ("sqlite", "mimetypeEq") => Some("$field = ?".to_string()),
        ("sqlserver", "mimetypeEq") => Some("$field = ?".to_string()),

        // MIME type starts with (e.g., "image/")
        ("postgres", "mimetypeStartswith") => Some("$field LIKE $1 || '%'".to_string()),
        ("mysql", "mimetypeStartswith") => Some("$field LIKE CONCAT(?, '%')".to_string()),
        ("sqlite", "mimetypeStartswith") => Some("$field LIKE ? || '%'".to_string()),
        ("sqlserver", "mimetypeStartswith") => Some("$field LIKE ? + '%'".to_string()),

        // Color format validation (hex color)
        ("postgres", "colorHexFormatValid") => Some("$field ~ '^#[a-f0-9]{6}$'".to_string()),
        ("mysql", "colorHexFormatValid") => Some("$field REGEXP '^#[a-f0-9]{6}$'".to_string()),
        ("sqlite", "colorHexFormatValid") => Some("$field GLOB '#[a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9]'".to_string()),
        ("sqlserver", "colorHexFormatValid") => Some("$field LIKE '#[a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9]'".to_string()),

        // ========================================================================
        // NETWORK TYPES (IPAddress, IPv4, IPv6, CIDR, Port)
        // ========================================================================
        // IPv4 format: 4 octets separated by dots
        ("postgres", "ipv4FormatValid") => Some("$field ~ '^(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$'".to_string()),
        ("mysql", "ipv4FormatValid") => Some("$field REGEXP '^(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\\\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\\\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\\\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$'".to_string()),
        ("sqlite", "ipv4FormatValid") => Some("CAST(CAST(CAST(CAST(SUBSTR($field,1,INSTR($field,'.')-1) AS INT) AS INT) > 0 AS INT) || CAST(CAST(SUBSTR($field,1,INSTR($field,'.')-1) AS INT) <= 255 AS INT) AS INT) = 1".to_string()),
        ("sqlserver", "ipv4FormatValid") => Some("CONVERT(BIT, CASE WHEN $field LIKE '[0-9].*.[0-9].*.[0-9].*.[0-9]' THEN 1 ELSE 0 END) = 1".to_string()),

        // Port number: between 0 and 65535
        ("postgres", "portValid") => Some("CAST($field AS INTEGER) BETWEEN 0 AND 65535".to_string()),
        ("mysql", "portValid") => Some("CAST($field AS UNSIGNED) BETWEEN 0 AND 65535".to_string()),
        ("sqlite", "portValid") => Some("CAST($field AS INTEGER) BETWEEN 0 AND 65535".to_string()),
        ("sqlserver", "portValid") => Some("CAST($field AS INT) BETWEEN 0 AND 65535".to_string()),

        // ========================================================================
        // MEASUREMENT/RANGE TYPES
        // ========================================================================
        // Percentage: 0-100
        ("postgres", "percentageValid") => Some("CAST($field AS DECIMAL) BETWEEN 0 AND 100".to_string()),
        ("mysql", "percentageValid") => Some("CAST($field AS DECIMAL) BETWEEN 0 AND 100".to_string()),
        ("sqlite", "percentageValid") => Some("CAST($field AS REAL) BETWEEN 0 AND 100".to_string()),
        ("sqlserver", "percentageValid") => Some("CAST($field AS DECIMAL) BETWEEN 0 AND 100".to_string()),

        // ========================================================================
        // LOOKUP-BASED OPERATORS
        // ========================================================================
        // These operators use external lookup data embedded in the schema.
        // Templates use $lookup placeholder for the lookup field name.

        // Country: continent membership
        ("postgres", "continentEq") => Some("$lookup ->> 'continent' = $1".to_string()),
        ("mysql", "continentEq") => Some("JSON_EXTRACT($lookup, '$.continent') = ?".to_string()),
        ("sqlite", "continentEq") => Some("json_extract($lookup, '$.continent') = ?".to_string()),
        ("sqlserver", "continentEq") => Some("JSON_VALUE($lookup, '$.continent') = ?".to_string()),

        // Country: region membership
        ("postgres", "regionEq") => Some("$lookup ->> 'region' = $1".to_string()),
        ("mysql", "regionEq") => Some("JSON_EXTRACT($lookup, '$.region') = ?".to_string()),
        ("sqlite", "regionEq") => Some("json_extract($lookup, '$.region') = ?".to_string()),
        ("sqlserver", "regionEq") => Some("JSON_VALUE($lookup, '$.region') = ?".to_string()),

        // Country: EU membership
        ("postgres", "inEu") => Some("($lookup ->> 'in_eu')::boolean = $1".to_string()),
        ("mysql", "inEu") => Some("JSON_EXTRACT($lookup, '$.in_eu') = ?".to_string()),
        ("sqlite", "inEu") => Some("json_extract($lookup, '$.in_eu') = ?".to_string()),
        ("sqlserver", "inEu") => Some("JSON_VALUE($lookup, '$.in_eu') = ?".to_string()),

        // Country: Schengen membership
        ("postgres", "inSchengen") => Some("($lookup ->> 'in_schengen')::boolean = $1".to_string()),
        ("mysql", "inSchengen") => Some("JSON_EXTRACT($lookup, '$.in_schengen') = ?".to_string()),
        ("sqlite", "inSchengen") => Some("json_extract($lookup, '$.in_schengen') = ?".to_string()),
        ("sqlserver", "inSchengen") => Some("JSON_VALUE($lookup, '$.in_schengen') = ?".to_string()),

        // Currency: decimal places (for Money type)
        ("postgres", "currencyDecimalEq") => Some("($lookup ->> 'decimal_places')::integer = $1".to_string()),
        ("mysql", "currencyDecimalEq") => Some("JSON_EXTRACT($lookup, '$.decimal_places') = ?".to_string()),
        ("sqlite", "currencyDecimalEq") => Some("json_extract($lookup, '$.decimal_places') = ?".to_string()),
        ("sqlserver", "currencyDecimalEq") => Some("JSON_VALUE($lookup, '$.decimal_places') = ?".to_string()),

        // Timezone: offset in minutes from UTC
        ("postgres", "timezoneOffsetEq") => Some("($lookup ->> 'offset_minutes')::integer = $1".to_string()),
        ("mysql", "timezoneOffsetEq") => Some("JSON_EXTRACT($lookup, '$.offset_minutes') = ?".to_string()),
        ("sqlite", "timezoneOffsetEq") => Some("json_extract($lookup, '$.offset_minutes') = ?".to_string()),
        ("sqlserver", "timezoneOffsetEq") => Some("JSON_VALUE($lookup, '$.offset_minutes') = ?".to_string()),

        // Timezone: daylight saving time support
        ("postgres", "timezoneDst") => Some("($lookup ->> 'has_dst')::boolean = $1".to_string()),
        ("mysql", "timezoneDst") => Some("JSON_EXTRACT($lookup, '$.has_dst') = ?".to_string()),
        ("sqlite", "timezoneDst") => Some("json_extract($lookup, '$.has_dst') = ?".to_string()),
        ("sqlserver", "timezoneDst") => Some("JSON_VALUE($lookup, '$.has_dst') = ?".to_string()),

        // Timezone: region (Americas, Europe, Asia, Oceania)
        ("postgres", "timezoneRegionEq") => Some("$lookup ->> 'region' = $1".to_string()),
        ("mysql", "timezoneRegionEq") => Some("JSON_EXTRACT($lookup, '$.region') = ?".to_string()),
        ("sqlite", "timezoneRegionEq") => Some("json_extract($lookup, '$.region') = ?".to_string()),
        ("sqlserver", "timezoneRegionEq") => Some("JSON_VALUE($lookup, '$.region') = ?".to_string()),

        // Language: family (Indo-European, Sino-Tibetan, Japonic, etc.)
        ("postgres", "languageFamilyEq") => Some("$lookup ->> 'family' = $1".to_string()),
        ("mysql", "languageFamilyEq") => Some("JSON_EXTRACT($lookup, '$.family') = ?".to_string()),
        ("sqlite", "languageFamilyEq") => Some("json_extract($lookup, '$.family') = ?".to_string()),
        ("sqlserver", "languageFamilyEq") => Some("JSON_VALUE($lookup, '$.family') = ?".to_string()),

        // Language: writing script (Latin, Cyrillic, Han, etc.)
        ("postgres", "languageScriptEq") => Some("$lookup ->> 'script' = $1".to_string()),
        ("mysql", "languageScriptEq") => Some("JSON_EXTRACT($lookup, '$.script') = ?".to_string()),
        ("sqlite", "languageScriptEq") => Some("json_extract($lookup, '$.script') = ?".to_string()),
        ("sqlserver", "languageScriptEq") => Some("JSON_VALUE($lookup, '$.script') = ?".to_string()),

        // Locale: language part of locale code
        ("postgres", "localeLanguageEq") => Some("SPLIT_PART($field, '-', 1) = $1".to_string()),
        ("mysql", "localeLanguageEq") => Some("SUBSTRING_INDEX($field, '-', 1) = ?".to_string()),
        ("sqlite", "localeLanguageEq") => Some("SUBSTR($field, 1, INSTR($field, '-') - 1) = ?".to_string()),
        ("sqlserver", "localeLanguageEq") => Some("SUBSTRING($field, 1, CHARINDEX('-', $field) - 1) = ?".to_string()),

        // Locale: country part of locale code
        ("postgres", "localeCountryEq") => Some("SPLIT_PART($field, '-', 2) = $1".to_string()),
        ("mysql", "localeCountryEq") => Some("SUBSTRING_INDEX(SUBSTRING_INDEX($field, '-', 2), '-', -1) = ?".to_string()),
        ("sqlite", "localeCountryEq") => Some("SUBSTR($field, INSTR($field, '-') + 1) = ?".to_string()),
        ("sqlserver", "localeCountryEq") => Some("SUBSTRING($field, CHARINDEX('-', $field) + 1, LEN($field)) = ?".to_string()),

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
        // MySQL: Uses ST_Distance_Sphere for great-circle distance
        ("mysql", "distanceWithin") => Some(
            "ST_Distance_Sphere(
                ST_GeomFromText(CONCAT('POINT(', JSON_EXTRACT($field, '$.lng'), ' ', JSON_EXTRACT($field, '$.lat'), ')')),
                ST_GeomFromText(CONCAT('POINT(', ?, ' ', ?, ')'))
            ) <= ? * 1000"
            .to_string()
        ),
        // SQLite: Haversine formula approximation
        ("sqlite", "distanceWithin") => Some(
            "111.111 * DEGREES(ACOS(LEAST(1, GREATEST(-1,
                COS(RADIANS(90 - json_extract($field, '$.lat'))) *
                COS(RADIANS(90 - ?)) *
                COS(RADIANS(json_extract($field, '$.lng') - ?)) +
                SIN(RADIANS(90 - json_extract($field, '$.lat'))) *
                SIN(RADIANS(90 - ?))
            )))) <= ?"
            .to_string()
        ),
        // SQL Server: Uses geography type
        ("sqlserver", "distanceWithin") => Some(
            "geography::Point(JSON_VALUE($field, '$.lat'), JSON_VALUE($field, '$.lng'), 4326)
                .STDistance(geography::Point(?, ?, 4326)) <= ? * 1000"
            .to_string()
        ),

        // Coordinates: Within bounding box
        ("postgres", "withinBoundingBox") => Some(
            "($field->>'lat')::float8 BETWEEN $1 AND $2 AND ($field->>'lng')::float8 BETWEEN $3 AND $4"
                .to_string()
        ),
        ("mysql", "withinBoundingBox") => Some(
            "JSON_EXTRACT($field, '$.lat') BETWEEN ? AND ? AND JSON_EXTRACT($field, '$.lng') BETWEEN ? AND ?"
                .to_string()
        ),
        ("sqlite", "withinBoundingBox") => Some(
            "json_extract($field, '$.lat') BETWEEN ? AND ? AND json_extract($field, '$.lng') BETWEEN ? AND ?"
                .to_string()
        ),
        ("sqlserver", "withinBoundingBox") => Some(
            "JSON_VALUE($field, '$.lat') BETWEEN ? AND ? AND JSON_VALUE($field, '$.lng') BETWEEN ? AND ?"
                .to_string()
        ),

        // ========================================================================
        // PHONE NUMBER OPERATORS
        // ========================================================================
        // Phone: Country code from E.164 format
        ("postgres", "phoneCountryCodeEq") => Some("SUBSTRING($field FROM 1 FOR LENGTH($1)) = $1".to_string()),
        ("mysql", "phoneCountryCodeEq") => Some("SUBSTRING($field, 1, LENGTH(?)) = ?".to_string()),
        ("sqlite", "phoneCountryCodeEq") => Some("SUBSTR($field, 1, LENGTH(?)) = ?".to_string()),
        ("sqlserver", "phoneCountryCodeEq") => Some("SUBSTRING($field, 1, LEN(?)) = ?".to_string()),

        ("postgres", "phoneCountryCodeIn") => Some("SUBSTRING($field FROM 1 FOR POSITION('+' IN $field)) IN ($params)".to_string()),
        ("mysql", "phoneCountryCodeIn") => Some("SUBSTRING($field, 1, LOCATE('+', $field)) IN ($params)".to_string()),
        ("sqlite", "phoneCountryCodeIn") => Some("SUBSTR($field, 1, INSTR($field, '+')) IN ($params)".to_string()),
        ("sqlserver", "phoneCountryCodeIn") => Some("SUBSTRING($field, 1, CHARINDEX('+', $field)) IN ($params)".to_string()),

        // Phone: E.164 format validation (+[1-9]{1,3}[0-9]{1,14})
        ("postgres", "phoneIsValid") => Some("$field ~ '^\\+[1-9]\\d{1,14}$' = $1".to_string()),
        ("mysql", "phoneIsValid") => Some("$field REGEXP '^\\\\+[1-9]\\\\d{1,14}$' = ?".to_string()),
        ("sqlite", "phoneIsValid") => Some("$field GLOB '+[1-9]*' AND LENGTH($field) BETWEEN 5 AND 15".to_string()),
        ("sqlserver", "phoneIsValid") => Some("$field LIKE '+[1-9]%'".to_string()),

        // Phone: Type classification (mobile, fixed, etc.)
        ("postgres", "phoneTypeEq") => Some("CASE WHEN $field ~ '^\\+1' THEN 'US' WHEN $field ~ '^\\+44' THEN 'UK' ELSE 'OTHER' END = $1".to_string()),
        ("mysql", "phoneTypeEq") => Some("CASE WHEN $field REGEXP '^\\\\+1' THEN 'US' WHEN $field REGEXP '^\\\\+44' THEN 'UK' ELSE 'OTHER' END = ?".to_string()),
        ("sqlite", "phoneTypeEq") => Some("CASE WHEN $field GLOB '+1*' THEN 'US' WHEN $field GLOB '+44*' THEN 'UK' ELSE 'OTHER' END = ?".to_string()),
        ("sqlserver", "phoneTypeEq") => Some("CASE WHEN $field LIKE '+1%' THEN 'US' WHEN $field LIKE '+44%' THEN 'UK' ELSE 'OTHER' END = ?".to_string()),

        // ========================================================================
        // DATE RANGE OPERATORS
        // ========================================================================
        // Format: JSON with {start: ISO8601, end: ISO8601} or period string

        // DateRange: Duration in days >= min
        ("postgres", "durationGte") => Some(
            "EXTRACT(DAY FROM ($field->>'end')::timestamp - ($field->>'start')::timestamp) >= $1"
                .to_string()
        ),
        ("mysql", "durationGte") => Some(
            "DATEDIFF(JSON_EXTRACT($field, '$.end'), JSON_EXTRACT($field, '$.start')) >= ?"
                .to_string()
        ),
        ("sqlite", "durationGte") => Some(
            "CAST((julianday(json_extract($field, '$.end')) - julianday(json_extract($field, '$.start'))) AS INTEGER) >= ?"
                .to_string()
        ),
        ("sqlserver", "durationGte") => Some(
            "DATEDIFF(DAY, JSON_VALUE($field, '$.start'), JSON_VALUE($field, '$.end')) >= ?"
                .to_string()
        ),

        // DateRange: Starts after date
        ("postgres", "startsAfter") => Some("($field->>'start')::timestamp > $1::timestamp".to_string()),
        ("mysql", "startsAfter") => Some("JSON_EXTRACT($field, '$.start') > ?".to_string()),
        ("sqlite", "startsAfter") => Some("json_extract($field, '$.start') > ?".to_string()),
        ("sqlserver", "startsAfter") => Some("JSON_VALUE($field, '$.start') > ?".to_string()),

        // DateRange: Ends before date
        ("postgres", "endsBefore") => Some("($field->>'end')::timestamp < $1::timestamp".to_string()),
        ("mysql", "endsBefore") => Some("JSON_EXTRACT($field, '$.end') < ?".to_string()),
        ("sqlite", "endsBefore") => Some("json_extract($field, '$.end') < ?".to_string()),
        ("sqlserver", "endsBefore") => Some("JSON_VALUE($field, '$.end') < ?".to_string()),

        // DateRange: Overlaps with another range
        ("postgres", "overlaps") => Some(
            "($field->>'start')::timestamp < $2::timestamp AND ($field->>'end')::timestamp > $1::timestamp"
                .to_string()
        ),
        ("mysql", "overlaps") => Some(
            "JSON_EXTRACT($field, '$.start') < ? AND JSON_EXTRACT($field, '$.end') > ?"
                .to_string()
        ),
        ("sqlite", "overlaps") => Some(
            "json_extract($field, '$.start') < ? AND json_extract($field, '$.end') > ?"
                .to_string()
        ),
        ("sqlserver", "overlaps") => Some(
            "JSON_VALUE($field, '$.start') < ? AND JSON_VALUE($field, '$.end') > ?"
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
        ("mysql", "totalSecondsEq") => Some(
            "CAST(REPLACE($field, 'PT', '') AS UNSIGNED) = ?"
                .to_string()
        ),
        ("sqlite", "totalSecondsEq") => Some(
            "CAST(REPLACE($field, 'PT', '') AS INTEGER) = ?"
                .to_string()
        ),
        ("sqlserver", "totalSecondsEq") => Some(
            "CAST(SUBSTRING($field, 3, LEN($field)) AS BIGINT) = ?"
                .to_string()
        ),

        // Duration: Total minutes >= min
        ("postgres", "totalMinutesGte") => Some(
            "EXTRACT(EPOCH FROM CAST($field AS INTERVAL)) / 60 >= $1"
                .to_string()
        ),
        ("mysql", "totalMinutesGte") => Some(
            "CAST(REPLACE($field, 'PT', '') AS UNSIGNED) / 60 >= ?"
                .to_string()
        ),
        ("sqlite", "totalMinutesGte") => Some(
            "CAST(REPLACE($field, 'PT', '') AS INTEGER) / 60 >= ?"
                .to_string()
        ),
        ("sqlserver", "totalMinutesGte") => Some(
            "CAST(SUBSTRING($field, 3, LEN($field)) AS BIGINT) / 60 >= ?"
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

    for db in &["postgres", "mysql", "sqlite", "sqlserver"] {
        if let Some(template) = extract_template_for_operator(db, operator_name) {
            templates.insert((*db).to_string(), template);
        }
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
///       "postgres": "SPLIT_PART($field, '@', 2) = $1",
///       "mysql": "SUBSTRING_INDEX($field, '@', -1) = ?",
///       "sqlite": "...",
///       "sqlserver": "..."
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_operator_templates() {
        let templates = extract_operator_templates("domainEq");

        // Should have templates for all 4 databases
        assert_eq!(templates.len(), 4);
        assert!(templates.contains_key("postgres"));
        assert!(templates.contains_key("mysql"));
        assert!(templates.contains_key("sqlite"));
        assert!(templates.contains_key("sqlserver"));

        // Verify templates are correct
        assert!(templates["postgres"].contains("SPLIT_PART"));
        assert!(templates["mysql"].contains("SUBSTRING_INDEX"));
    }

    #[test]
    fn test_build_sql_templates_metadata() {
        let operators = vec!["domainEq", "wmiEq"];
        let metadata = build_sql_templates_metadata(&operators);

        assert!(metadata.get("operators").is_some());
        let ops = metadata["operators"].as_object().unwrap();
        assert_eq!(ops.len(), 2);
        assert!(ops.contains_key("domainEq"));
        assert!(ops.contains_key("wmiEq"));
    }

    #[test]
    fn test_extract_vin_templates() {
        let templates = extract_operator_templates("wmiEq");

        assert!(templates.contains_key("postgres"));
        assert!(templates["postgres"].contains("SUBSTRING"));
        assert!(templates["mysql"].contains("SUBSTRING"));
    }

    #[test]
    fn test_geospatial_templates() {
        let templates = extract_operator_templates("distanceWithin");

        assert!(templates.contains_key("postgres"));
        assert!(templates["postgres"].contains("ST_DWithin"));

        assert!(templates.contains_key("mysql"));
        assert!(templates["mysql"].contains("ST_Distance_Sphere"));

        assert!(templates.contains_key("sqlite"));
        assert!(templates["sqlite"].contains("Haversine") || templates["sqlite"].contains("ACOS"));

        assert!(templates.contains_key("sqlserver"));
        assert!(templates["sqlserver"].contains("geography"));
    }

    #[test]
    fn test_phone_templates() {
        let templates = extract_operator_templates("phoneCountryCodeEq");

        assert!(templates.contains_key("postgres"));
        assert!(templates.contains_key("mysql"));
        assert!(templates.contains_key("sqlite"));
        assert!(templates.contains_key("sqlserver"));
    }

    #[test]
    fn test_date_range_templates() {
        let templates = extract_operator_templates("durationGte");

        assert!(templates.contains_key("postgres"));
        assert!(templates["postgres"].contains("EXTRACT"));

        assert!(templates.contains_key("mysql"));
        assert!(templates["mysql"].contains("DATEDIFF"));
    }

    #[test]
    fn test_duration_templates() {
        let templates = extract_operator_templates("totalSecondsEq");

        assert!(templates.contains_key("postgres"));
        assert!(templates["postgres"].contains("EPOCH"));

        assert!(templates.contains_key("mysql"));
        assert!(templates["mysql"].contains("REPLACE"));
    }
}
