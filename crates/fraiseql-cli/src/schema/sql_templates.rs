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
fn extract_template_for_operator(
    db_name: &str,
    operator_name: &str,
) -> Option<String> {
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
            templates.insert(db.to_string(), template);
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
            operators.insert(
                op_name.to_string(),
                json!(templates),
            );
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
}
