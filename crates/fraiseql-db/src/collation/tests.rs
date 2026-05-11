#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::collation_config::{
    DatabaseCollationOverrides, InvalidLocaleStrategy, MySqlCollationConfig,
    PostgresCollationConfig, SqliteCollationConfig,
};

fn test_config() -> CollationConfig {
    CollationConfig {
        enabled:            true,
        fallback_locale:    "en-US".to_string(),
        allowed_locales:    vec!["en-US".into(), "fr-FR".into(), "ja-JP".into()],
        on_invalid_locale:  InvalidLocaleStrategy::Fallback,
        database_overrides: None,
    }
}

#[test]
fn test_postgres_icu_collation() {
    let config = test_config();
    let mapper = CollationMapper::new(config, DatabaseType::PostgreSQL);

    assert_eq!(mapper.map_locale("fr-FR").unwrap(), Some("fr-FR-x-icu".to_string()));
    assert_eq!(mapper.map_locale("ja-JP").unwrap(), Some("ja-JP-x-icu".to_string()));
}

#[test]
fn test_postgres_libc_collation() {
    let mut config = test_config();
    config.database_overrides = Some(DatabaseCollationOverrides {
        postgres:  Some(PostgresCollationConfig {
            use_icu:  false,
            provider: "libc".to_string(),
        }),
        mysql:     None,
        sqlite:    None,
        sqlserver: None,
    });

    let mapper = CollationMapper::new(config, DatabaseType::PostgreSQL);

    assert_eq!(mapper.map_locale("fr-FR").unwrap(), Some("fr_FR.UTF-8".to_string()));
    assert_eq!(mapper.map_locale("en-US").unwrap(), Some("en_US.UTF-8".to_string()));
}

#[test]
fn test_mysql_collation() {
    let config = test_config();
    let mapper = CollationMapper::new(config, DatabaseType::MySQL);

    // All locales map to same charset-based collation
    assert_eq!(mapper.map_locale("fr-FR").unwrap(), Some("utf8mb4_unicode_ci".to_string()));
    assert_eq!(mapper.map_locale("ja-JP").unwrap(), Some("utf8mb4_unicode_ci".to_string()));
}

#[test]
fn test_mysql_custom_collation() {
    let mut config = test_config();
    config.database_overrides = Some(DatabaseCollationOverrides {
        postgres:  None,
        mysql:     Some(MySqlCollationConfig {
            charset: "utf8mb4".to_string(),
            suffix:  "_0900_ai_ci".to_string(),
        }),
        sqlite:    None,
        sqlserver: None,
    });

    let mapper = CollationMapper::new(config, DatabaseType::MySQL);

    assert_eq!(mapper.map_locale("fr-FR").unwrap(), Some("utf8mb4_0900_ai_ci".to_string()));
}

#[test]
fn test_sqlite_collation() {
    let config = test_config();
    let mapper = CollationMapper::new(config, DatabaseType::SQLite);

    assert_eq!(mapper.map_locale("fr-FR").unwrap(), Some("NOCASE".to_string()));
}

#[test]
fn test_sqlite_disabled_nocase() {
    let mut config = test_config();
    config.database_overrides = Some(DatabaseCollationOverrides {
        postgres:  None,
        mysql:     None,
        sqlite:    Some(SqliteCollationConfig { use_nocase: false }),
        sqlserver: None,
    });

    let mapper = CollationMapper::new(config, DatabaseType::SQLite);

    assert_eq!(mapper.map_locale("fr-FR").unwrap(), None);
}

#[test]
fn test_sqlserver_collation() {
    let config = test_config();
    let mapper = CollationMapper::new(config, DatabaseType::SQLServer);

    assert_eq!(mapper.map_locale("fr-FR").unwrap(), Some("French_100_CI_AI".to_string()));
    assert_eq!(
        mapper.map_locale("ja-JP").unwrap(),
        Some("Japanese_XJIS_100_CI_AI".to_string())
    );
}

#[test]
fn test_invalid_locale_fallback() {
    let config = test_config();
    let mapper = CollationMapper::new(config, DatabaseType::PostgreSQL);

    // Invalid locale should use fallback
    let result = mapper.map_locale("invalid-locale").unwrap();
    assert_eq!(result, Some("en-US-x-icu".to_string()));
}

#[test]
fn test_invalid_locale_database_default() {
    let mut config = test_config();
    config.on_invalid_locale = InvalidLocaleStrategy::DatabaseDefault;
    let mapper = CollationMapper::new(config, DatabaseType::PostgreSQL);

    // Invalid locale should return None (use database default)
    let result = mapper.map_locale("invalid-locale").unwrap();
    assert_eq!(result, None);
}

#[test]
fn test_invalid_locale_error() {
    let mut config = test_config();
    config.on_invalid_locale = InvalidLocaleStrategy::Error;
    let mapper = CollationMapper::new(config, DatabaseType::PostgreSQL);

    // Invalid locale should return error
    let result = mapper.map_locale("invalid-locale");
    assert!(
        result.is_err(),
        "expected Err for invalid locale with Error strategy, got: {result:?}"
    );
}

#[test]
fn test_disabled_collation() {
    let mut config = test_config();
    config.enabled = false;
    let mapper = CollationMapper::new(config, DatabaseType::PostgreSQL);

    // Should always return None when disabled
    assert_eq!(mapper.map_locale("fr-FR").unwrap(), None);
    assert_eq!(mapper.map_locale("en-US").unwrap(), None);
}

#[test]
fn test_capabilities_locale_support() {
    assert!(CollationCapabilities::supports_locale_collation(DatabaseType::PostgreSQL));
    assert!(CollationCapabilities::supports_locale_collation(DatabaseType::SQLServer));
    assert!(!CollationCapabilities::supports_locale_collation(DatabaseType::MySQL));
    assert!(!CollationCapabilities::supports_locale_collation(DatabaseType::SQLite));
}

#[test]
fn test_capabilities_custom_collation() {
    assert!(CollationCapabilities::requires_custom_collation(DatabaseType::SQLite));
    assert!(!CollationCapabilities::requires_custom_collation(DatabaseType::PostgreSQL));
    assert!(!CollationCapabilities::requires_custom_collation(DatabaseType::MySQL));
    assert!(!CollationCapabilities::requires_custom_collation(DatabaseType::SQLServer));
}

#[test]
fn test_capabilities_strategy() {
    assert_eq!(
        CollationCapabilities::strategy(DatabaseType::PostgreSQL),
        "ICU collations (locale-specific)"
    );
    assert_eq!(
        CollationCapabilities::strategy(DatabaseType::MySQL),
        "UTF8MB4 collations (general)"
    );
    assert_eq!(CollationCapabilities::strategy(DatabaseType::SQLite), "NOCASE (limited)");
    assert_eq!(
        CollationCapabilities::strategy(DatabaseType::SQLServer),
        "Language-specific collations"
    );
}
