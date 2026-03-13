//! Collation configuration for user-aware sorting.
//!
//! Maps user locales to database-specific collation strings.

use serde::{Deserialize, Serialize};

/// Collation configuration for user-aware sorting.
///
/// This configuration enables automatic collation support based on user locale,
/// adapting to database capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CollationConfig {
    /// Enable automatic user-aware collation.
    pub enabled: bool,

    /// Fallback locale for unauthenticated users.
    pub fallback_locale: String,

    /// Allowed locales (whitelist for security).
    pub allowed_locales: Vec<String>,

    /// Strategy when user locale is not in allowed list.
    pub on_invalid_locale: InvalidLocaleStrategy,

    /// Database-specific overrides (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_overrides: Option<DatabaseCollationOverrides>,
}

impl Default for CollationConfig {
    fn default() -> Self {
        Self {
            enabled:            true,
            fallback_locale:    "en-US".to_string(),
            allowed_locales:    vec![
                "en-US".into(),
                "en-GB".into(),
                "fr-FR".into(),
                "de-DE".into(),
                "es-ES".into(),
                "ja-JP".into(),
                "zh-CN".into(),
                "pt-BR".into(),
                "it-IT".into(),
            ],
            on_invalid_locale:  InvalidLocaleStrategy::Fallback,
            database_overrides: None,
        }
    }
}

/// Strategy when user locale is not in allowed list.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvalidLocaleStrategy {
    /// Use fallback locale.
    #[default]
    Fallback,
    /// Use database default (no COLLATE clause).
    DatabaseDefault,
    /// Return error.
    Error,
}

/// Database-specific collation overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseCollationOverrides {
    /// PostgreSQL-specific settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres: Option<PostgresCollationConfig>,

    /// MySQL-specific settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mysql: Option<MySqlCollationConfig>,

    /// SQLite-specific settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sqlite: Option<SqliteCollationConfig>,

    /// SQL Server-specific settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sqlserver: Option<SqlServerCollationConfig>,
}

/// PostgreSQL-specific collation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresCollationConfig {
    /// Use ICU collations (recommended).
    pub use_icu: bool,

    /// Provider: "icu" or "libc".
    pub provider: String,
}

impl Default for PostgresCollationConfig {
    fn default() -> Self {
        Self {
            use_icu:  true,
            provider: "icu".to_string(),
        }
    }
}

/// MySQL-specific collation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySqlCollationConfig {
    /// Charset (e.g., "utf8mb4").
    pub charset: String,

    /// Collation suffix (e.g., "_unicode_ci" or "_0900_ai_ci").
    pub suffix: String,
}

impl Default for MySqlCollationConfig {
    fn default() -> Self {
        Self {
            charset: "utf8mb4".to_string(),
            suffix:  "_unicode_ci".to_string(),
        }
    }
}

/// SQLite-specific collation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteCollationConfig {
    /// Use COLLATE NOCASE for case-insensitive sorting.
    pub use_nocase: bool,
}

impl Default for SqliteCollationConfig {
    fn default() -> Self {
        Self { use_nocase: true }
    }
}

/// SQL Server-specific collation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlServerCollationConfig {
    /// Case-insensitive (CI) collations.
    pub case_insensitive: bool,

    /// Accent-insensitive (AI) collations.
    pub accent_insensitive: bool,
}

impl Default for SqlServerCollationConfig {
    fn default() -> Self {
        Self {
            case_insensitive:   true,
            accent_insensitive: true,
        }
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
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
}
