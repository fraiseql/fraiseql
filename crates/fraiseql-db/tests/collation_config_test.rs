//! Unit tests for `CollationConfig` and related database-specific collation structs.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_db::{
    CollationConfig, DatabaseCollationOverrides, InvalidLocaleStrategy, MySqlCollationConfig,
    PostgresCollationConfig, SqlServerCollationConfig, SqliteCollationConfig,
};

// ---------------------------------------------------------------------------
// CollationConfig — defaults
// ---------------------------------------------------------------------------

#[test]
fn collation_config_default_is_enabled() {
    let cfg = CollationConfig::default();
    assert!(cfg.enabled, "CollationConfig should default to enabled");
}

#[test]
fn collation_config_default_fallback_locale() {
    let cfg = CollationConfig::default();
    assert_eq!(cfg.fallback_locale, "en-US");
}

#[test]
fn collation_config_default_allowed_locales_non_empty() {
    let cfg = CollationConfig::default();
    assert!(!cfg.allowed_locales.is_empty(), "Default allowed_locales should not be empty");
}

#[test]
fn collation_config_default_includes_common_locales() {
    let cfg = CollationConfig::default();
    let locales = &cfg.allowed_locales;
    assert!(locales.iter().any(|l| l == "en-US"), "Should allow en-US");
    assert!(locales.iter().any(|l| l == "fr-FR"), "Should allow fr-FR");
    assert!(locales.iter().any(|l| l == "de-DE"), "Should allow de-DE");
}

#[test]
fn collation_config_default_invalid_locale_strategy_is_fallback() {
    let cfg = CollationConfig::default();
    assert_eq!(cfg.on_invalid_locale, InvalidLocaleStrategy::Fallback);
}

#[test]
fn collation_config_default_no_database_overrides() {
    let cfg = CollationConfig::default();
    assert!(cfg.database_overrides.is_none(), "Default should have no database overrides");
}

// ---------------------------------------------------------------------------
// CollationConfig — serde round-trip
// ---------------------------------------------------------------------------

#[test]
fn collation_config_serde_round_trip() {
    let original = CollationConfig::default();
    let json = serde_json::to_string(&original).unwrap();
    let restored: CollationConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(original.enabled, restored.enabled);
    assert_eq!(original.fallback_locale, restored.fallback_locale);
    assert_eq!(original.allowed_locales, restored.allowed_locales);
}

#[test]
fn collation_config_serde_with_overrides() {
    let cfg = CollationConfig {
        enabled:            true,
        fallback_locale:    "de-DE".to_string(),
        allowed_locales:    vec!["de-DE".to_string(), "en-US".to_string()],
        on_invalid_locale:  InvalidLocaleStrategy::DatabaseDefault,
        database_overrides: Some(DatabaseCollationOverrides {
            postgres:  Some(PostgresCollationConfig::default()),
            mysql:     None,
            sqlite:    None,
            sqlserver: None,
        }),
    };

    let json = serde_json::to_string(&cfg).unwrap();
    let restored: CollationConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.fallback_locale, "de-DE");
    assert_eq!(restored.allowed_locales, ["de-DE", "en-US"]);
    assert!(restored.database_overrides.is_some());
    let ov = restored.database_overrides.unwrap();
    assert!(ov.postgres.is_some());
    assert!(ov.mysql.is_none());
}

// ---------------------------------------------------------------------------
// Invalid locale strategy
// ---------------------------------------------------------------------------

#[test]
fn invalid_locale_strategy_fallback_is_default() {
    let strategy = InvalidLocaleStrategy::default();
    assert_eq!(strategy, InvalidLocaleStrategy::Fallback);
}

#[test]
fn invalid_locale_strategy_serde_snake_case() {
    // Strategies must serialize with snake_case names
    let fallback = serde_json::to_string(&InvalidLocaleStrategy::Fallback).unwrap();
    let db_default = serde_json::to_string(&InvalidLocaleStrategy::DatabaseDefault).unwrap();
    let error = serde_json::to_string(&InvalidLocaleStrategy::Error).unwrap();

    assert_eq!(fallback, r#""fallback""#);
    assert_eq!(db_default, r#""database_default""#);
    assert_eq!(error, r#""error""#);
}

// ---------------------------------------------------------------------------
// PostgresCollationConfig
// ---------------------------------------------------------------------------

#[test]
fn postgres_collation_default_uses_icu() {
    let cfg = PostgresCollationConfig::default();
    assert!(cfg.use_icu, "Should default to ICU collations");
    assert_eq!(cfg.provider, "icu");
}

#[test]
fn postgres_collation_serde_round_trip() {
    let original = PostgresCollationConfig {
        use_icu:  false,
        provider: "libc".to_string(),
    };
    let json = serde_json::to_string(&original).unwrap();
    let restored: PostgresCollationConfig = serde_json::from_str(&json).unwrap();
    assert!(!restored.use_icu);
    assert_eq!(restored.provider, "libc");
}

// ---------------------------------------------------------------------------
// MySqlCollationConfig
// ---------------------------------------------------------------------------

#[test]
fn mysql_collation_default_charset() {
    let cfg = MySqlCollationConfig::default();
    assert_eq!(cfg.charset, "utf8mb4");
}

#[test]
fn mysql_collation_default_suffix_unicode() {
    let cfg = MySqlCollationConfig::default();
    assert!(
        cfg.suffix.contains("unicode"),
        "Default MySQL suffix should be unicode-based: {}",
        cfg.suffix
    );
}

#[test]
fn mysql_collation_serde_round_trip() {
    let original = MySqlCollationConfig {
        charset: "utf8mb4".to_string(),
        suffix:  "_0900_ai_ci".to_string(),
    };
    let json = serde_json::to_string(&original).unwrap();
    let restored: MySqlCollationConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.charset, "utf8mb4");
    assert_eq!(restored.suffix, "_0900_ai_ci");
}

// ---------------------------------------------------------------------------
// SqliteCollationConfig
// ---------------------------------------------------------------------------

#[test]
fn sqlite_collation_default_uses_nocase() {
    let cfg = SqliteCollationConfig::default();
    assert!(cfg.use_nocase, "SQLite should default to NOCASE collation");
}

#[test]
fn sqlite_collation_serde_round_trip() {
    let original = SqliteCollationConfig { use_nocase: false };
    let json = serde_json::to_string(&original).unwrap();
    let restored: SqliteCollationConfig = serde_json::from_str(&json).unwrap();
    assert!(!restored.use_nocase);
}

// ---------------------------------------------------------------------------
// SqlServerCollationConfig
// ---------------------------------------------------------------------------

#[test]
fn sqlserver_collation_defaults_ci_ai() {
    let cfg = SqlServerCollationConfig::default();
    assert!(cfg.case_insensitive, "SQL Server should default to case-insensitive");
    assert!(cfg.accent_insensitive, "SQL Server should default to accent-insensitive");
}

#[test]
fn sqlserver_collation_serde_round_trip() {
    let original = SqlServerCollationConfig {
        case_insensitive:   false,
        accent_insensitive: true,
    };
    let json = serde_json::to_string(&original).unwrap();
    let restored: SqlServerCollationConfig = serde_json::from_str(&json).unwrap();
    assert!(!restored.case_insensitive);
    assert!(restored.accent_insensitive);
}

// ---------------------------------------------------------------------------
// DatabaseCollationOverrides
// ---------------------------------------------------------------------------

#[test]
fn database_overrides_all_none_by_default_construction() {
    let ov = DatabaseCollationOverrides {
        postgres:  None,
        mysql:     None,
        sqlite:    None,
        sqlserver: None,
    };
    assert!(ov.postgres.is_none());
    assert!(ov.mysql.is_none());
    assert!(ov.sqlite.is_none());
    assert!(ov.sqlserver.is_none());
}

#[test]
fn database_overrides_partial_serde() {
    // Only SQLite override set — others must serialize as absent (skip_serializing_if)
    let ov = DatabaseCollationOverrides {
        postgres:  None,
        mysql:     None,
        sqlite:    Some(SqliteCollationConfig::default()),
        sqlserver: None,
    };
    let json = serde_json::to_string(&ov).unwrap();

    // "postgres", "mysql", "sqlserver" should be absent due to skip_serializing_if
    assert!(!json.contains("postgres"), "Absent postgres should not appear in JSON");
    assert!(!json.contains("mysql"), "Absent mysql should not appear in JSON");
    assert!(json.contains("sqlite"), "Present sqlite should appear in JSON");
}
