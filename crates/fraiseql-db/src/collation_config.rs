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
