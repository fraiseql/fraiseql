# Global Collation Configuration (Revision to Phase 4b)

**Status**: Planning Revision
**Date**: 2026-01-12
**Context**: Simplify collation to global config + database-specific exposure

---

## Overview

**Original Plan**: Per-query collation configuration
**Revised Plan**: **Global configuration linked to database type**

### Key Insights

1. **User locale is a user preference** - not a query-specific setting
2. **Collation support varies by database** - PostgreSQL ICU ≠ MySQL ≠ SQLite
3. **Configuration should be centralized** - one place, applies everywhere
4. **Database determines available collations** - expose what the DB supports

---

## Architecture: Global Config + Database Discovery

### 1. Global Runtime Configuration

**File**: `crates/fraiseql-core/src/config/mod.rs` (or `runtime/config.rs`)

```rust
/// Global runtime configuration for FraiseQL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Query plan caching
    pub cache_query_plans: bool,

    /// Query validation limits
    pub max_query_depth: usize,
    pub max_query_complexity: usize,

    /// Tracing
    pub enable_tracing: bool,

    /// Collation configuration (NEW)
    pub collation: CollationConfig,
}

/// Collation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollationConfig {
    /// Enable automatic user-aware collation
    pub enabled: bool,

    /// Fallback locale for unauthenticated users
    pub fallback_locale: String,  // e.g., "en-US"

    /// Allowed locales (whitelist for security)
    pub allowed_locales: Vec<String>,

    /// Strategy when user locale is not in allowed list
    pub on_invalid_locale: InvalidLocaleStrategy,

    /// Database-specific overrides (optional)
    pub database_overrides: Option<DatabaseCollationOverrides>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidLocaleStrategy {
    /// Use fallback locale
    Fallback,
    /// Use database default (no COLLATE clause)
    DatabaseDefault,
    /// Return error
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseCollationOverrides {
    /// PostgreSQL-specific settings
    pub postgres: Option<PostgresCollationConfig>,

    /// MySQL-specific settings
    pub mysql: Option<MySQLCollationConfig>,

    /// SQLite-specific settings
    pub sqlite: Option<SQLiteCollationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresCollationConfig {
    /// Use ICU collations (recommended)
    pub use_icu: bool,  // true → "en-US-x-icu", false → "en_US.UTF-8"

    /// Provider: "icu" or "libc"
    pub provider: String,  // "icu" or "libc"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySQLCollationConfig {
    /// Collation format: e.g., "utf8mb4_unicode_ci"
    pub charset: String,  // "utf8mb4"
    pub suffix: String,   // "_unicode_ci" or "_0900_ai_ci"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SQLiteCollationConfig {
    /// SQLite has limited collation support
    pub use_nocase: bool,  // Use COLLATE NOCASE for case-insensitive
}

impl Default for CollationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            fallback_locale: "en-US".to_string(),
            allowed_locales: vec![
                "en-US".into(), "en-GB".into(), "fr-FR".into(),
                "de-DE".into(), "es-ES".into(), "ja-JP".into(),
                "zh-CN".into(), "pt-BR".into(), "it-IT".into(),
            ],
            on_invalid_locale: InvalidLocaleStrategy::Fallback,
            database_overrides: None,
        }
    }
}
```

### 2. Database-Specific Collation Mapping

**File**: `crates/fraiseql-core/src/db/collation.rs`

```rust
//! Database-specific collation mapping
//!
//! Maps user locales to database-specific collation strings.

use crate::config::CollationConfig;
use crate::db::types::DatabaseType;

pub struct CollationMapper {
    config: CollationConfig,
    database_type: DatabaseType,
}

impl CollationMapper {
    pub fn new(config: CollationConfig, database_type: DatabaseType) -> Self {
        Self { config, database_type }
    }

    /// Map user locale to database-specific collation string
    ///
    /// # Arguments
    ///
    /// * `locale` - User locale (e.g., "fr-FR")
    ///
    /// # Returns
    ///
    /// Database-specific collation string or None
    ///
    /// # Examples
    ///
    /// ```
    /// let mapper = CollationMapper::new(config, DatabaseType::PostgreSQL);
    ///
    /// // PostgreSQL with ICU
    /// assert_eq!(mapper.map_locale("fr-FR"), Some("fr-FR-x-icu"));
    ///
    /// // MySQL
    /// let mapper = CollationMapper::new(config, DatabaseType::MySQL);
    /// assert_eq!(mapper.map_locale("fr-FR"), Some("utf8mb4_unicode_ci"));
    ///
    /// // SQLite (limited support)
    /// let mapper = CollationMapper::new(config, DatabaseType::SQLite);
    /// assert_eq!(mapper.map_locale("fr-FR"), Some("NOCASE"));
    /// ```
    pub fn map_locale(&self, locale: &str) -> Option<String> {
        // Check if locale is allowed
        if !self.config.allowed_locales.contains(&locale.to_string()) {
            return self.handle_invalid_locale();
        }

        match self.database_type {
            DatabaseType::PostgreSQL => self.map_postgres(locale),
            DatabaseType::MySQL => self.map_mysql(locale),
            DatabaseType::SQLite => self.map_sqlite(locale),
            DatabaseType::SQLServer => self.map_sqlserver(locale),
        }
    }

    fn map_postgres(&self, locale: &str) -> Option<String> {
        if let Some(overrides) = &self.config.database_overrides {
            if let Some(pg_config) = &overrides.postgres {
                if pg_config.use_icu {
                    return Some(format!("{}-x-icu", locale));
                } else {
                    // libc format: en_US.UTF-8
                    let libc_locale = locale.replace('-', "_");
                    return Some(format!("{}.UTF-8", libc_locale));
                }
            }
        }

        // Default: ICU collation
        Some(format!("{}-x-icu", locale))
    }

    fn map_mysql(&self, locale: &str) -> Option<String> {
        if let Some(overrides) = &self.config.database_overrides {
            if let Some(mysql_config) = &overrides.mysql {
                // MySQL collations are charset-based, not locale-based
                // All locales map to the same collation
                return Some(format!(
                    "{}{}",
                    mysql_config.charset,
                    mysql_config.suffix
                ));
            }
        }

        // Default: utf8mb4_unicode_ci (supports all languages)
        Some("utf8mb4_unicode_ci".to_string())
    }

    fn map_sqlite(&self, _locale: &str) -> Option<String> {
        if let Some(overrides) = &self.config.database_overrides {
            if let Some(sqlite_config) = &overrides.sqlite {
                if sqlite_config.use_nocase {
                    return Some("NOCASE".to_string());
                }
            }
        }

        // SQLite has very limited collation support
        // NOCASE is the only built-in case-insensitive collation
        Some("NOCASE".to_string())
    }

    fn map_sqlserver(&self, locale: &str) -> Option<String> {
        // SQL Server collation format:
        // Latin1_General_100_CI_AI_SC_UTF8 (case-insensitive, accent-insensitive)
        // French_100_CI_AI (for French)

        // Map common locales to SQL Server collations
        let collation = match locale {
            "en-US" | "en-GB" => "Latin1_General_100_CI_AI_SC_UTF8",
            "fr-FR" | "fr-CA" => "French_100_CI_AI",
            "de-DE" | "de-AT" => "German_PhoneBook_100_CI_AI",
            "es-ES" => "Modern_Spanish_100_CI_AI",
            "ja-JP" => "Japanese_XJIS_100_CI_AI",
            "zh-CN" => "Chinese_PRC_100_CI_AI",
            _ => "Latin1_General_100_CI_AI_SC_UTF8",  // Default
        };

        Some(collation.to_string())
    }

    fn handle_invalid_locale(&self) -> Option<String> {
        match self.config.on_invalid_locale {
            InvalidLocaleStrategy::Fallback => {
                self.map_locale(&self.config.fallback_locale)
            }
            InvalidLocaleStrategy::DatabaseDefault => None,
            InvalidLocaleStrategy::Error => {
                // This will be handled by caller
                None
            }
        }
    }
}

/// Collation capabilities by database
pub struct CollationCapabilities;

impl CollationCapabilities {
    /// Check if database supports locale-specific collations
    pub fn supports_locale_collation(db_type: DatabaseType) -> bool {
        matches!(
            db_type,
            DatabaseType::PostgreSQL | DatabaseType::SQLServer
        )
    }

    /// Check if database requires custom collation registration
    pub fn requires_custom_collation(db_type: DatabaseType) -> bool {
        matches!(db_type, DatabaseType::SQLite)
    }

    /// Get collation strategy for database
    pub fn strategy(db_type: DatabaseType) -> &'static str {
        match db_type {
            DatabaseType::PostgreSQL => "ICU collations (locale-specific)",
            DatabaseType::MySQL => "UTF8MB4 collations (general)",
            DatabaseType::SQLite => "NOCASE (limited)",
            DatabaseType::SQLServer => "Language-specific collations",
        }
    }
}
```

### 3. Server Initialization

**File**: `crates/fraiseql-server/src/main.rs`

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = load_config()?;

    // Create database adapter
    let adapter = PostgresAdapter::new(&config.database_url).await?;
    let db_type = adapter.database_type();

    // Create collation mapper (global, database-aware)
    let collation_mapper = CollationMapper::new(
        config.runtime.collation.clone(),
        db_type,
    );

    // Create executor with collation support
    let executor = Executor::with_collation(
        schema,
        adapter,
        config.runtime,
        collation_mapper,  // ← Global collation mapper
    );

    // Start server
    Server::new(executor).run().await
}
```

### 4. Configuration File

**File**: `fraiseql.toml` (or JSON/YAML)

```toml
[database]
url = "postgresql://localhost/mydb"
pool_size = 10

[runtime]
cache_query_plans = true
max_query_depth = 10
max_query_complexity = 1000
enable_tracing = true

[runtime.collation]
enabled = true
fallback_locale = "en-US"
on_invalid_locale = "fallback"  # or "database_default" or "error"

# Whitelist of allowed locales (security)
allowed_locales = [
    "en-US", "en-GB", "en-CA", "en-AU",
    "fr-FR", "fr-CA",
    "de-DE", "de-AT", "de-CH",
    "es-ES", "es-MX",
    "ja-JP", "zh-CN", "pt-BR"
]

# PostgreSQL-specific settings (optional)
[runtime.collation.database_overrides.postgres]
use_icu = true      # Use ICU collations (recommended)
provider = "icu"    # or "libc"

# MySQL-specific settings (optional)
[runtime.collation.database_overrides.mysql]
charset = "utf8mb4"
suffix = "_unicode_ci"  # or "_0900_ai_ci" for MySQL 8.0+

# SQLite-specific settings (optional)
[runtime.collation.database_overrides.sqlite]
use_nocase = true   # Use COLLATE NOCASE
```

### 5. Python Schema (No Per-Query Config Needed)

```python
# ✅ BEFORE (per-query config - removed)
@fraiseql.query(
    sql_source="v_user",
    auto_params={
        "order_by": {
            "enabled": True,
            "auto_collation": True,  # ← NO LONGER NEEDED
            "fallback_collation": "en-US-x-icu"
        }
    }
)
def users() -> list[User]:
    pass

# ✅ AFTER (global config applies automatically)
@fraiseql.query(sql_source="v_user")
def users() -> list[User]:
    """Users will be sorted with user-aware collation automatically."""
    pass
```

**Collation is now a runtime concern, not a schema concern.**

---

## Database-Specific Behavior

### PostgreSQL (Full Support)

**Configuration**:

```toml
[runtime.collation.database_overrides.postgres]
use_icu = true
provider = "icu"
```

**Generated SQL**:

```sql
-- User locale: fr-FR
ORDER BY data->>'name' COLLATE "fr-FR-x-icu" ASC

-- User locale: ja-JP
ORDER BY data->>'name' COLLATE "ja-JP-x-icu" ASC
```

**Collation Discovery** (optional at startup):

```rust
impl PostgresAdapter {
    pub async fn discover_available_collations(&self) -> Result<Vec<String>> {
        let collations: Vec<String> = sqlx::query_scalar(
            "SELECT collname FROM pg_collation WHERE collname LIKE '%-x-icu'"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(collations)
    }
}
```

### MySQL (Limited Support)

**Configuration**:

```toml
[runtime.collation.database_overrides.mysql]
charset = "utf8mb4"
suffix = "_unicode_ci"
```

**Generated SQL**:

```sql
-- ALL locales use the same collation (MySQL limitation)
ORDER BY JSON_UNQUOTE(JSON_EXTRACT(data, '$.name'))
    COLLATE utf8mb4_unicode_ci ASC
```

**Why**: MySQL collations are charset-based, not locale-based. `utf8mb4_unicode_ci` handles all Unicode correctly but doesn't distinguish between locales.

**Alternative** (MySQL 8.0+):

```toml
suffix = "_0900_ai_ci"  # More modern, better Unicode support
```

### SQLite (Minimal Support)

**Configuration**:

```toml
[runtime.collation.database_overrides.sqlite]
use_nocase = true
```

**Generated SQL**:

```sql
-- NOCASE is the only built-in case-insensitive collation
ORDER BY json_extract(data, '$.name') COLLATE NOCASE ASC
```

**Limitation**: SQLite doesn't have locale-aware collations built-in. Options:

1. Use `NOCASE` (case-insensitive only)
2. Register custom collation functions (complex)
3. Accept database default (byte order)

**Recommendation**: Document limitation, use database default or NOCASE.

### SQL Server (Good Support)

**Configuration**:

```toml
[runtime.collation.database_overrides.sqlserver]
# Mapped internally based on locale
```

**Generated SQL**:

```sql
-- User locale: fr-FR
ORDER BY JSON_VALUE(data, '$.name') COLLATE French_100_CI_AI ASC

-- User locale: de-DE
ORDER BY JSON_VALUE(data, '$.name') COLLATE German_PhoneBook_100_CI_AI ASC
```

---

## Environment-Based Configuration

### Development

```toml
[runtime.collation]
enabled = true
fallback_locale = "en-US"
on_invalid_locale = "fallback"

# Minimal whitelist for development
allowed_locales = ["en-US", "fr-FR"]
```

### Production

```toml
[runtime.collation]
enabled = true
fallback_locale = "en-US"
on_invalid_locale = "error"  # Strict validation

# Comprehensive whitelist
allowed_locales = [
    "en-US", "en-GB", "fr-FR", "de-DE", "es-ES",
    "ja-JP", "zh-CN", "pt-BR", "it-IT", "ru-RU"
]

[runtime.collation.database_overrides.postgres]
use_icu = true
provider = "icu"
```

### Testing (Disabled)

```toml
[runtime.collation]
enabled = false  # Disable for deterministic tests
fallback_locale = "C"  # Byte order (fast, deterministic)
```

---

## Migration Path

### Step 1: Update Configuration

```toml
# Add to fraiseql.toml or environment variables
[runtime.collation]
enabled = true
fallback_locale = "en-US"
allowed_locales = ["en-US", "fr-FR", "de-DE"]
```

### Step 2: Remove Per-Query Config

```python
# BEFORE
@fraiseql.query(
    sql_source="v_user",
    auto_params={
        "order_by": {"auto_collation": True}  # ← Remove this
    }
)

# AFTER
@fraiseql.query(sql_source="v_user")
def users() -> list[User]:
    pass
```

### Step 3: Ensure JWT Contains Locale

```javascript
// Auth0 Action
exports.onExecutePostLogin = async (event, api) => {
  const locale = event.user.user_metadata.locale || 'en-US';
  api.idToken.setCustomClaim('locale', locale);
};
```

### Step 4: Test & Deploy

```bash
# Test with different locales
curl -H "Authorization: Bearer $TOKEN_FR" \
     -d '{"query":"{ users { name } }"}' \
     http://localhost:8080/graphql

# Verify collation in logs
# → "Using collation fr-FR-x-icu for ORDER BY (strategy: UserLocale)"
```

---

## Advantages of Global Config

### 1. **Simplicity** ✅

- One place to configure collation behavior
- No per-query decisions needed
- Schema stays clean and focused on data model

### 2. **Consistency** ✅

- All queries behave the same way
- User sees consistent sorting everywhere
- No edge cases where some queries have collation and others don't

### 3. **Database-Aware** ✅

- Configuration adapts to database capabilities
- PostgreSQL gets ICU, MySQL gets utf8mb4, SQLite gets NOCASE
- No invalid collations sent to database

### 4. **Security** ✅

- Centralized whitelist validation
- Easier to audit and update
- Clear policy enforcement

### 5. **Performance** ✅

- Collation mapper initialized once at startup
- No per-query configuration parsing
- Can cache collation mappings

### 6. **Observability** ✅

- Single place to log collation behavior
- Easier to monitor and debug
- Clear metrics on collation usage

---

## Edge Cases & Overrides

### Case 1: Disable Collation for Specific Field

**Use Case**: Email addresses should use byte order (C collation) for speed.

**Solution**: Explicit collation in query (manual override still works):

```graphql
query {
  users(orderBy: {field: "email", collation: "C"}) {
    email
  }
}
```

### Case 2: Disable Collation Globally (Testing)

**Solution**: Configuration flag:

```toml
[runtime.collation]
enabled = false
```

### Case 3: Different Locale for Specific Query

**Use Case**: Admin dashboard always uses en-US, regardless of user locale.

**Solution**: Not supported by design. If needed, use explicit collation:

```graphql
query {
  adminUsers(orderBy: {field: "name", collation: "en-US-x-icu"}) {
    name
  }
}
```

**Rationale**: This should be rare. User preference should apply everywhere.

---

## Implementation Checklist

- [ ] Create `CollationConfig` in `config/mod.rs`
- [ ] Create `CollationMapper` in `db/collation.rs`
- [ ] Add database-specific mapping (PostgreSQL, MySQL, SQLite, SQL Server)
- [ ] Integrate with `RuntimeConfig`
- [ ] Update `Executor` to use global collation mapper
- [ ] Update `DatabaseAdapter` trait to accept collation mapper
- [ ] Write unit tests for collation mapping
- [ ] Write integration tests with different databases
- [ ] Update documentation (no per-query config)
- [ ] Create migration guide

---

## Summary: Why Global Config is Better

**Original Plan**:

```python
# Every query needs configuration
@fraiseql.query(
    sql_source="v_user",
    auto_params={"order_by": {"auto_collation": True}}  # ← Repetitive
)
```

**Revised Plan**:

```python
# Collation works automatically for all queries
@fraiseql.query(sql_source="v_user")
def users() -> list[User]:
    pass
```

**Configuration** (one time, applies everywhere):

```toml
[runtime.collation]
enabled = true
fallback_locale = "en-US"
allowed_locales = ["en-US", "fr-FR", "de-DE", "ja-JP"]

[runtime.collation.database_overrides.postgres]
use_icu = true
```

**Result**:

- ✅ Simpler schemas (less boilerplate)
- ✅ Consistent behavior (no query-specific surprises)
- ✅ Database-aware (adapts to capabilities)
- ✅ Easier to reason about (global policy)
- ✅ Better performance (single mapper)

---

**Status**: Ready for implementation with revised architecture
**Priority**: High (correct approach for system-wide feature)
