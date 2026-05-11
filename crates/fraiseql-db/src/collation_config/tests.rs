#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_collation_config_default_fallback_locale_is_en_us() {
    let config = CollationConfig::default();
    assert_eq!(config.fallback_locale, "en-US");
}

#[test]
fn test_collation_config_default_is_enabled() {
    let config = CollationConfig::default();
    assert!(config.enabled, "CollationConfig should be enabled by default");
}

#[test]
fn test_collation_config_default_allowed_locales_contains_common_locales() {
    let config = CollationConfig::default();
    assert!(config.allowed_locales.contains(&"en-US".to_string()));
    assert!(config.allowed_locales.contains(&"fr-FR".to_string()));
    assert!(config.allowed_locales.contains(&"de-DE".to_string()));
}

#[test]
fn test_collation_config_round_trip_serde() {
    let config = CollationConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let restored: CollationConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.fallback_locale, config.fallback_locale);
    assert_eq!(restored.enabled, config.enabled);
    assert_eq!(restored.allowed_locales, config.allowed_locales);
}

#[test]
fn test_collation_config_custom_locale() {
    let config = CollationConfig {
        fallback_locale: "ja-JP".to_string(),
        ..CollationConfig::default()
    };
    assert_eq!(config.fallback_locale, "ja-JP");
}

#[test]
fn test_invalid_locale_strategy_default_is_fallback() {
    let strategy = InvalidLocaleStrategy::default();
    assert_eq!(strategy, InvalidLocaleStrategy::Fallback);
}

#[test]
fn test_postgres_collation_config_default_uses_icu() {
    let config = PostgresCollationConfig::default();
    assert!(config.use_icu, "PostgreSQL default collation should use ICU");
    assert_eq!(config.provider, "icu");
}

#[test]
fn test_mysql_collation_config_default_charset() {
    let config = MySqlCollationConfig::default();
    assert_eq!(config.charset, "utf8mb4");
    assert_eq!(config.suffix, "_unicode_ci");
}

#[test]
fn test_sqlite_collation_config_default_nocase() {
    let config = SqliteCollationConfig::default();
    assert!(config.use_nocase, "SQLite default collation should use NOCASE");
}

#[test]
fn test_sqlserver_collation_config_default_case_and_accent_insensitive() {
    let config = SqlServerCollationConfig::default();
    assert!(config.case_insensitive);
    assert!(config.accent_insensitive);
}
