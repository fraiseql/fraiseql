//! Database-specific collation mapping.
//!
//! Maps user locales to database-specific collation strings, adapting to each
//! database's collation capabilities.

use crate::{
    config::CollationConfig,
    db::types::DatabaseType,
    error::{FraiseQLError, Result},
};

/// Maps user locales to database-specific collation strings.
///
/// The mapper takes a global `CollationConfig` and database type, then translates
/// user locales (e.g., "fr-FR") into the appropriate database-specific collation
/// format (e.g., "fr-FR-x-icu" for PostgreSQL with ICU).
///
/// # Examples
///
/// ```
/// use fraiseql_core::config::CollationConfig;
/// use fraiseql_core::db::{DatabaseType, collation::CollationMapper};
///
/// // PostgreSQL with ICU
/// let config = CollationConfig::default();
/// let mapper = CollationMapper::new(config.clone(), DatabaseType::PostgreSQL);
/// assert_eq!(mapper.map_locale("fr-FR").unwrap(), Some("fr-FR-x-icu".to_string()));
///
/// // MySQL (general collation, not locale-specific)
/// let mapper = CollationMapper::new(config, DatabaseType::MySQL);
/// assert_eq!(mapper.map_locale("fr-FR").unwrap(), Some("utf8mb4_unicode_ci".to_string()));
/// ```
pub struct CollationMapper {
    config:        CollationConfig,
    database_type: DatabaseType,
}

impl CollationMapper {
    /// Create a new collation mapper.
    ///
    /// # Arguments
    ///
    /// * `config` - Global collation configuration
    /// * `database_type` - Target database type
    #[must_use]
    pub fn new(config: CollationConfig, database_type: DatabaseType) -> Self {
        Self {
            config,
            database_type,
        }
    }

    /// Map user locale to database-specific collation string.
    ///
    /// # Arguments
    ///
    /// * `locale` - User locale (e.g., "fr-FR", "ja-JP")
    ///
    /// # Returns
    ///
    /// - `Ok(Some(collation))` - Database-specific collation string
    /// - `Ok(None)` - Use database default (no COLLATE clause)
    /// - `Err(_)` - Invalid locale when strategy is `Error`
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if locale is not in allowed list
    /// and `on_invalid_locale` is set to `Error`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fraiseql_core::config::CollationConfig;
    /// use fraiseql_core::db::{DatabaseType, collation::CollationMapper};
    ///
    /// let config = CollationConfig::default();
    /// let mapper = CollationMapper::new(config, DatabaseType::PostgreSQL);
    ///
    /// // Valid locale
    /// let collation = mapper.map_locale("fr-FR").unwrap();
    /// assert_eq!(collation, Some("fr-FR-x-icu".to_string()));
    ///
    /// // Invalid locale (not in allowed list)
    /// let result = mapper.map_locale("invalid");
    /// assert!(result.is_ok()); // Returns fallback by default
    /// ```
    pub fn map_locale(&self, locale: &str) -> Result<Option<String>> {
        if !self.config.enabled {
            return Ok(None);
        }

        // Check if locale is allowed
        if !self.config.allowed_locales.contains(&locale.to_string()) {
            return self.handle_invalid_locale();
        }

        match self.database_type {
            DatabaseType::PostgreSQL => Ok(self.map_postgres(locale)),
            DatabaseType::MySQL => Ok(self.map_mysql(locale)),
            DatabaseType::SQLite => Ok(self.map_sqlite(locale)),
            DatabaseType::SQLServer => Ok(self.map_sqlserver(locale)),
        }
    }

    /// Map locale for PostgreSQL.
    ///
    /// Supports both ICU and libc collations:
    /// - ICU: "fr-FR-x-icu" (recommended, Unicode-aware)
    /// - libc: "fr_FR.UTF-8" (system-dependent)
    fn map_postgres(&self, locale: &str) -> Option<String> {
        if let Some(overrides) = &self.config.database_overrides {
            if let Some(pg_config) = &overrides.postgres {
                if pg_config.use_icu {
                    return Some(format!("{locale}-x-icu"));
                }
                // libc format: en_US.UTF-8
                let libc_locale = locale.replace('-', "_");
                return Some(format!("{libc_locale}.UTF-8"));
            }
        }

        // Default: ICU collation
        Some(format!("{locale}-x-icu"))
    }

    /// Map locale for MySQL.
    ///
    /// MySQL collations are charset-based, not locale-specific.
    /// All locales map to the same general-purpose collation.
    fn map_mysql(&self, _locale: &str) -> Option<String> {
        if let Some(overrides) = &self.config.database_overrides {
            if let Some(mysql_config) = &overrides.mysql {
                return Some(format!("{}{}", mysql_config.charset, mysql_config.suffix));
            }
        }

        // Default: utf8mb4_unicode_ci (supports all languages)
        Some("utf8mb4_unicode_ci".to_string())
    }

    /// Map locale for SQLite.
    ///
    /// SQLite has very limited collation support. Only NOCASE is built-in
    /// for case-insensitive sorting.
    fn map_sqlite(&self, _locale: &str) -> Option<String> {
        if let Some(overrides) = &self.config.database_overrides {
            if let Some(sqlite_config) = &overrides.sqlite {
                return if sqlite_config.use_nocase {
                    Some("NOCASE".to_string())
                } else {
                    None
                };
            }
        }

        // Default: NOCASE
        Some("NOCASE".to_string())
    }

    /// Map locale for SQL Server.
    ///
    /// Maps common locales to SQL Server language-specific collations.
    fn map_sqlserver(&self, locale: &str) -> Option<String> {
        // Map common locales to SQL Server collations
        let collation = match locale {
            "en-US" | "en-GB" | "en-CA" | "en-AU" => "Latin1_General_100_CI_AI_SC_UTF8",
            "fr-FR" | "fr-CA" => "French_100_CI_AI",
            "de-DE" | "de-AT" | "de-CH" => "German_PhoneBook_100_CI_AI",
            "es-ES" | "es-MX" => "Modern_Spanish_100_CI_AI",
            "ja-JP" => "Japanese_XJIS_100_CI_AI",
            "zh-CN" => "Chinese_PRC_100_CI_AI",
            "pt-BR" => "Latin1_General_100_CI_AI_SC_UTF8",
            "it-IT" => "Latin1_General_100_CI_AI_SC_UTF8",
            _ => "Latin1_General_100_CI_AI_SC_UTF8", // Default
        };

        Some(collation.to_string())
    }

    /// Handle invalid locale based on configuration strategy.
    fn handle_invalid_locale(&self) -> Result<Option<String>> {
        use crate::config::InvalidLocaleStrategy;

        match self.config.on_invalid_locale {
            InvalidLocaleStrategy::Fallback => self.map_locale(&self.config.fallback_locale),
            InvalidLocaleStrategy::DatabaseDefault => Ok(None),
            InvalidLocaleStrategy::Error => Err(FraiseQLError::Validation {
                message: "Invalid locale: not in allowed list".to_string(),
                path:    None,
            }),
        }
    }

    /// Get the database type this mapper is configured for.
    #[must_use]
    pub const fn database_type(&self) -> DatabaseType {
        self.database_type
    }

    /// Check if collation is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// Database collation capabilities.
///
/// Provides information about what collation features each database supports.
pub struct CollationCapabilities;

impl CollationCapabilities {
    /// Check if database supports locale-specific collations.
    ///
    /// - PostgreSQL: ✅ Full support via ICU or libc
    /// - MySQL: ❌ Only charset-based collations
    /// - SQLite: ❌ Limited to NOCASE or custom functions
    /// - SQL Server: ✅ Language-specific collations
    #[must_use]
    pub const fn supports_locale_collation(db_type: DatabaseType) -> bool {
        matches!(db_type, DatabaseType::PostgreSQL | DatabaseType::SQLServer)
    }

    /// Check if database requires custom collation registration.
    ///
    /// SQLite requires custom collation functions to be registered for
    /// locale-aware sorting beyond NOCASE.
    #[must_use]
    pub const fn requires_custom_collation(db_type: DatabaseType) -> bool {
        matches!(db_type, DatabaseType::SQLite)
    }

    /// Get collation strategy description for database.
    #[must_use]
    pub const fn strategy(db_type: DatabaseType) -> &'static str {
        match db_type {
            DatabaseType::PostgreSQL => "ICU collations (locale-specific)",
            DatabaseType::MySQL => "UTF8MB4 collations (general)",
            DatabaseType::SQLite => "NOCASE (limited)",
            DatabaseType::SQLServer => "Language-specific collations",
        }
    }

    /// Get recommended collation provider for database.
    #[must_use]
    pub const fn recommended_provider(db_type: DatabaseType) -> Option<&'static str> {
        match db_type {
            DatabaseType::PostgreSQL => Some("icu"),
            DatabaseType::MySQL => Some("utf8mb4_unicode_ci"),
            DatabaseType::SQLite => Some("NOCASE"),
            DatabaseType::SQLServer => Some("Latin1_General_100_CI_AI_SC_UTF8"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
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
        assert!(result.is_err());
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
}
