#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::collation_config::{
    DatabaseCollationOverrides, InvalidLocaleStrategy, PostgresCollationConfig,
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
        postgres: Some(PostgresCollationConfig {
            use_icu:  false,
            provider: "libc".to_string(),
        }),
    });

    let mapper = CollationMapper::new(config, DatabaseType::PostgreSQL);

    assert_eq!(mapper.map_locale("fr-FR").unwrap(), Some("fr_FR.UTF-8".to_string()));
    assert_eq!(mapper.map_locale("en-US").unwrap(), Some("en_US.UTF-8".to_string()));
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
}

#[test]
fn test_capabilities_strategy() {
    assert_eq!(
        CollationCapabilities::strategy(DatabaseType::PostgreSQL),
        "ICU collations (locale-specific)"
    );
}
