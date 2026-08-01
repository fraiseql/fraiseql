//! Database-specific collation mapping.
//!
//! Maps user locales to database-specific collation strings, adapting to each
//! database's collation capabilities.

use fraiseql_error::{FraiseQLError, Result};

use crate::{collation_config::CollationConfig, types::DatabaseType};

/// Maps user locales to database-specific collation strings.
///
/// The mapper takes a global `CollationConfig` and database type, then translates
/// user locales (e.g., "fr-FR") into the appropriate database-specific collation
/// format (e.g., "fr-FR-x-icu" for PostgreSQL with ICU).
///
/// # Examples
///
/// ```
/// use fraiseql_db::CollationConfig;
/// use fraiseql_db::{DatabaseType, collation::CollationMapper};
///
/// // PostgreSQL with ICU
/// let config = CollationConfig::default();
/// let mapper = CollationMapper::new(config, DatabaseType::PostgreSQL);
/// assert_eq!(mapper.map_locale("fr-FR").unwrap(), Some("fr-FR-x-icu".to_string()));
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
    pub const fn new(config: CollationConfig, database_type: DatabaseType) -> Self {
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
    /// use fraiseql_db::CollationConfig;
    /// use fraiseql_db::{DatabaseType, collation::CollationMapper};
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
    /// assert!(result.is_ok(), "utf8 is a valid collation: {result:?}");
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if `locale` is not in the allowed list
    /// and the configured `InvalidLocaleStrategy` is `Reject`.
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

    /// Handle invalid locale based on configuration strategy.
    fn handle_invalid_locale(&self) -> Result<Option<String>> {
        use crate::collation_config::InvalidLocaleStrategy;

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
    /// PostgreSQL has full support via ICU or libc.
    #[must_use]
    pub const fn supports_locale_collation(db_type: DatabaseType) -> bool {
        matches!(db_type, DatabaseType::PostgreSQL)
    }

    /// Get collation strategy description for database.
    #[must_use]
    pub const fn strategy(db_type: DatabaseType) -> &'static str {
        match db_type {
            DatabaseType::PostgreSQL => "ICU collations (locale-specific)",
        }
    }

    /// Get recommended collation provider for database.
    #[must_use]
    pub const fn recommended_provider(db_type: DatabaseType) -> Option<&'static str> {
        match db_type {
            DatabaseType::PostgreSQL => Some("icu"),
        }
    }
}

#[cfg(test)]
mod tests;
