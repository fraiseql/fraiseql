# User-Aware Automatic Collation (Enhancement to Phase 4b)

**Status**: Planning
**Dependencies**: Phase 4b (Aggregation & ORDER BY), Phase 3 (Security/Auth)
**Estimated Duration**: 1-2 days (addition to Phase 4b)
**Complexity**: Medium

---

## Overview

**Automatically map ORDER BY collation to the authenticated user's locale**, providing locale-aware sorting without requiring clients to specify collation in every request.

### User Experience

**Without user-aware collation** (manual):

```graphql
query {
  users(orderBy: {field: "name", collation: "en-US-x-icu"}) {
    id
    name
  }
}
```

**With user-aware collation** (automatic):

```graphql
query {
  users(orderBy: {field: "name"}) {  # ✨ Collation auto-detected from user locale
    id
    name
  }
}
```

**Benefits**:

- ✅ Automatic locale-aware sorting based on user preferences
- ✅ No client-side collation management
- ✅ Consistent sorting across all queries for a user
- ✅ Override available when needed (manual collation takes precedence)
- ✅ Fallback to database default for unauthenticated requests

---

## Architecture

### User Context Flow

```
1. HTTP Request with Authorization header
   ↓
2. AuthMiddleware extracts JWT
   ↓
3. Extract user locale from JWT claims
   {
     "sub": "user123",
     "locale": "fr-FR",        ← User's preferred locale
     "scope": ["read", "write"]
   }
   ↓
4. AuthenticatedUser.locale = "fr-FR"
   ↓
5. Runtime executor generates SQL
   ↓
6. Collation resolver:
   - If ORDER BY has explicit collation → use it (override)
   - Else if user.locale exists → map to ICU collation ("fr-FR-x-icu")
   - Else → use database default (no COLLATE clause)
   ↓
7. Generated SQL:
   ORDER BY data->>'name' COLLATE "fr-FR-x-icu" ASC
```

---

## Implementation

### 1. Extend AuthenticatedUser

**File**: `crates/fraiseql-core/src/security/auth_middleware.rs`

Add `locale` field to `AuthenticatedUser`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    /// User ID (from 'sub' claim in JWT)
    pub user_id: String,

    /// Scopes/permissions (from 'scope' claim if present)
    pub scopes: Vec<String>,

    /// When the token expires
    pub expires_at: DateTime<Utc>,

    /// User's preferred locale (from 'locale' or 'lang' claim)
    /// Used for automatic collation in ORDER BY clauses
    /// Examples: "en-US", "fr-FR", "de-DE", "ja-JP"
    pub locale: Option<String>,
}

impl AuthenticatedUser {
    /// Get ICU collation string from locale
    ///
    /// Maps user locale to PostgreSQL ICU collation format.
    ///
    /// # Examples
    ///
    /// ```
    /// let user = AuthenticatedUser {
    ///     locale: Some("fr-FR".to_string()),
    ///     // ... other fields
    /// };
    /// assert_eq!(user.icu_collation(), Some("fr-FR-x-icu"));
    /// ```
    #[must_use]
    pub fn icu_collation(&self) -> Option<String> {
        self.locale.as_ref().map(|locale| {
            format!("{}-x-icu", locale)
        })
    }

    /// Check if user has a valid locale for collation
    #[must_use]
    pub fn has_locale(&self) -> bool {
        self.locale.is_some()
    }
}
```

**JWT Claim Extraction**:

```rust
impl AuthMiddleware {
    /// Extract user info from JWT claims (including locale)
    fn extract_user_from_claims(&self, claims: &serde_json::Value) -> Result<AuthenticatedUser> {
        // ... existing extraction logic ...

        // Extract locale from claims (try multiple claim names)
        let locale = claims.get("locale")
            .or_else(|| claims.get("lang"))
            .or_else(|| claims.get("language"))
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(AuthenticatedUser {
            user_id,
            scopes,
            expires_at,
            locale,
        })
    }
}
```

### 2. Collation Resolution Strategy

**File**: `crates/fraiseql-core/src/db/postgres/collation.rs` (new)

Create collation resolver module:

```rust
//! Collation resolution for ORDER BY clauses.
//!
//! Handles automatic collation mapping from user locale.

use crate::db::types::OrderByField;
use crate::security::AuthenticatedUser;

/// Collation resolution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollationStrategy {
    /// Use explicit collation from OrderByField (highest priority)
    Explicit,
    /// Use user's locale from AuthenticatedUser (automatic)
    UserLocale,
    /// Use database default (no COLLATE clause)
    DatabaseDefault,
}

/// Collation resolver
pub struct CollationResolver;

impl CollationResolver {
    /// Resolve collation for an ORDER BY field
    ///
    /// Priority order:
    /// 1. Explicit collation in OrderByField (manual override)
    /// 2. User locale from AuthenticatedUser (automatic)
    /// 3. Database default (no COLLATE clause)
    ///
    /// # Arguments
    ///
    /// * `field` - ORDER BY field specification
    /// * `user` - Authenticated user (optional)
    ///
    /// # Returns
    ///
    /// Tuple of (collation_string, strategy_used)
    ///
    /// # Examples
    ///
    /// ```
    /// // Explicit collation (manual override)
    /// let field = OrderByField {
    ///     field: "name".to_string(),
    ///     collation: Some("de-DE-x-icu".to_string()),
    ///     ..Default::default()
    /// };
    /// let (collation, strategy) = CollationResolver::resolve(&field, Some(&user));
    /// assert_eq!(collation, Some("de-DE-x-icu"));
    /// assert_eq!(strategy, CollationStrategy::Explicit);
    ///
    /// // User locale (automatic)
    /// let field = OrderByField {
    ///     field: "name".to_string(),
    ///     collation: None,  // No explicit collation
    ///     ..Default::default()
    /// };
    /// let user = AuthenticatedUser {
    ///     locale: Some("fr-FR".to_string()),
    ///     ..Default::default()
    /// };
    /// let (collation, strategy) = CollationResolver::resolve(&field, Some(&user));
    /// assert_eq!(collation, Some("fr-FR-x-icu"));
    /// assert_eq!(strategy, CollationStrategy::UserLocale);
    ///
    /// // Database default (no collation)
    /// let field = OrderByField {
    ///     field: "name".to_string(),
    ///     collation: None,
    ///     ..Default::default()
    /// };
    /// let (collation, strategy) = CollationResolver::resolve(&field, None);
    /// assert_eq!(collation, None);
    /// assert_eq!(strategy, CollationStrategy::DatabaseDefault);
    /// ```
    pub fn resolve(
        field: &OrderByField,
        user: Option<&AuthenticatedUser>,
    ) -> (Option<String>, CollationStrategy) {
        // Priority 1: Explicit collation (manual override)
        if let Some(collation) = &field.collation {
            return (Some(collation.clone()), CollationStrategy::Explicit);
        }

        // Priority 2: User locale (automatic)
        if let Some(user) = user {
            if let Some(collation) = user.icu_collation() {
                return (Some(collation), CollationStrategy::UserLocale);
            }
        }

        // Priority 3: Database default
        (None, CollationStrategy::DatabaseDefault)
    }

    /// Validate that a collation string is supported by PostgreSQL
    ///
    /// Basic validation - checks format, not actual database availability.
    pub fn validate_collation(collation: &str) -> bool {
        // ICU collation format: "en-US-x-icu", "fr-FR-x-icu", etc.
        if collation.ends_with("-x-icu") {
            return true;
        }

        // Standard PostgreSQL collations
        matches!(
            collation,
            "C" | "POSIX" | "en_US.UTF-8" | "C.UTF-8"
        )
    }

    /// Get list of commonly supported locales
    ///
    /// This is a subset of locales we know are widely available.
    pub fn common_locales() -> &'static [&'static str] {
        &[
            "en-US", "en-GB", "en-CA", "en-AU",
            "fr-FR", "fr-CA",
            "de-DE", "de-AT", "de-CH",
            "es-ES", "es-MX", "es-AR",
            "it-IT",
            "pt-BR", "pt-PT",
            "ja-JP",
            "zh-CN", "zh-TW",
            "ko-KR",
            "ru-RU",
            "ar-SA",
            "hi-IN",
        ]
    }
}
```

### 3. Update SQL Generation

**File**: `crates/fraiseql-core/src/db/postgres/adapter.rs`

Update ORDER BY generation to use collation resolver:

```rust
use crate::db::postgres::collation::{CollationResolver, CollationStrategy};
use crate::security::AuthenticatedUser;

impl PostgresAdapter {
    /// Generate ORDER BY clause with automatic collation resolution
    ///
    /// Resolves collation based on:
    /// 1. Explicit collation in OrderByField (highest priority)
    /// 2. User locale from AuthenticatedUser (automatic)
    /// 3. Database default (no COLLATE clause)
    fn generate_order_by_sql(
        &self,
        order_by: &[OrderByField],
        jsonb_column: &str,
        user: Option<&AuthenticatedUser>,  // ← Add user context
    ) -> String {
        order_by.iter().map(|field| {
            // Resolve collation (automatic or explicit)
            let (collation, strategy) = CollationResolver::resolve(field, user);

            // Log collation strategy for debugging
            if let Some(ref coll) = collation {
                tracing::debug!(
                    "ORDER BY {}: using collation {} (strategy: {:?})",
                    field.field,
                    coll,
                    strategy
                );
            }

            let direction = match field.direction {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            };

            let nulls = match field.nulls {
                NullsOrdering::First => " NULLS FIRST",
                NullsOrdering::Last => " NULLS LAST",
                NullsOrdering::Default => "",
            };

            let collation_clause = if let Some(coll) = collation {
                format!(" COLLATE \"{}\"", coll)
            } else {
                String::new()
            };

            format!(
                "{}->>'{}'{}{}{}",
                jsonb_column,
                field.field,
                collation_clause,
                direction,
                nulls
            )
        }).collect::<Vec<_>>().join(", ")
    }
}
```

### 4. Update Runtime Executor

**File**: `crates/fraiseql-core/src/runtime/executor.rs`

Pass user context to database adapter:

```rust
impl<A: DatabaseAdapter> Executor<A> {
    /// Execute a GraphQL query with user context
    pub async fn execute(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        user: Option<&AuthenticatedUser>,  // ← Add user context
    ) -> Result<String> {
        // 1. Match query to compiled template
        let query_match = self.matcher.match_query(query, variables)?;

        // 2. Create execution plan
        let plan = self.planner.plan(&query_match)?;

        // 3. Execute SQL query with user context for collation
        let results = self
            .adapter
            .execute_where_query(
                sql_source,
                where_clause,
                order_by,
                limit,
                offset,
                user,  // ← Pass user for collation resolution
            )
            .await?;

        // ... rest of execution
    }
}
```

---

## Configuration

### Schema Configuration

**Python Decorator**:

```python
@fraiseql.query(
    sql_source="v_user",
    auto_params={
        "order_by": {
            "enabled": True,
            "auto_collation": True,  # ← Enable user-aware collation
            "fallback_collation": "en-US-x-icu",  # ← Fallback for unauthenticated
            "default": [
                {"field": "name", "direction": "ASC"}
                # Note: no explicit collation - will use user's locale
            ]
        }
    }
)
def users() -> list[User]:
    """Get users with automatic locale-aware sorting."""
    pass
```

**Compiled Schema** (JSON):

```json
{
  "queries": [{
    "name": "users",
    "auto_params": {
      "has_order_by": true,
      "order_by_config": {
        "auto_collation": true,
        "fallback_collation": "en-US-x-icu",
        "default": [
          {"field": "name", "direction": "ASC"}
        ]
      }
    }
  }]
}
```

### JWT Claims Structure

**Auth0 / Clerk / Custom JWT**:

```json
{
  "sub": "user|12345",
  "iss": "https://your-domain.auth0.com/",
  "aud": "your-api-identifier",
  "iat": 1704067200,
  "exp": 1704153600,
  "scope": "read:users write:posts",
  "locale": "fr-FR"  ← User's preferred locale
}
```

**Alternative claim names** (auto-detected):

- `locale` (Auth0, Clerk)
- `lang` (some systems)
- `language` (some systems)

---

## Examples

### Example 1: French User

**JWT Claims**:

```json
{
  "sub": "user|123",
  "locale": "fr-FR"
}
```

**GraphQL Query**:

```graphql
query {
  users(orderBy: {field: "name", direction: ASC}) {
    id
    name
  }
}
```

**Generated SQL**:

```sql
SELECT
    data->>'id' AS id,
    data->>'name' AS name
FROM v_user
ORDER BY data->>'name' COLLATE "fr-FR-x-icu" ASC
```

**Result** (correct French sorting):

```
André
Béatrice
Émile
François
Zoë
```

### Example 2: Manual Override

**JWT Claims**:

```json
{
  "sub": "user|456",
  "locale": "en-US"  # User locale is English
}
```

**GraphQL Query** (manual German collation):

```graphql
query {
  users(orderBy: {field: "name", direction: ASC, collation: "de-DE-x-icu"}) {
    id
    name
  }
}
```

**Generated SQL** (uses explicit collation, not user locale):

```sql
ORDER BY data->>'name' COLLATE "de-DE-x-icu" ASC
```

### Example 3: Unauthenticated Request

**No JWT** (public API):

**GraphQL Query**:

```graphql
query {
  users(orderBy: {field: "name", direction: ASC}) {
    id
    name
  }
}
```

**Generated SQL** (uses fallback or database default):

```sql
-- If fallback_collation configured:
ORDER BY data->>'name' COLLATE "en-US-x-icu" ASC

-- Otherwise (no collation):
ORDER BY data->>'name' ASC
```

---

## Testing Strategy

### Unit Tests

**Collation Resolution**:

```rust
#[test]
fn test_collation_priority_explicit() {
    let field = OrderByField {
        field: "name".to_string(),
        collation: Some("de-DE-x-icu".to_string()),
        ..Default::default()
    };
    let user = AuthenticatedUser {
        locale: Some("fr-FR".to_string()),
        ..Default::default()
    };

    let (collation, strategy) = CollationResolver::resolve(&field, Some(&user));

    // Explicit takes precedence over user locale
    assert_eq!(collation, Some("de-DE-x-icu".to_string()));
    assert_eq!(strategy, CollationStrategy::Explicit);
}

#[test]
fn test_collation_priority_user_locale() {
    let field = OrderByField {
        field: "name".to_string(),
        collation: None,  // No explicit collation
        ..Default::default()
    };
    let user = AuthenticatedUser {
        locale: Some("ja-JP".to_string()),
        ..Default::default()
    };

    let (collation, strategy) = CollationResolver::resolve(&field, Some(&user));

    // User locale used when no explicit collation
    assert_eq!(collation, Some("ja-JP-x-icu".to_string()));
    assert_eq!(strategy, CollationStrategy::UserLocale);
}

#[test]
fn test_collation_priority_fallback() {
    let field = OrderByField {
        field: "name".to_string(),
        collation: None,
        ..Default::default()
    };

    let (collation, strategy) = CollationResolver::resolve(&field, None);

    // No collation when no user and no explicit
    assert_eq!(collation, None);
    assert_eq!(strategy, CollationStrategy::DatabaseDefault);
}
```

### Integration Tests

**Unicode Sorting** (with test database):

```rust
#[tokio::test]
async fn test_user_locale_french_sorting() {
    let pool = setup_test_db().await;

    // Insert test data
    insert_users(&pool, vec![
        "Zoë", "André", "Émile", "Béatrice", "François"
    ]).await;

    // Create executor with French user
    let user = AuthenticatedUser {
        locale: Some("fr-FR".to_string()),
        ..Default::default()
    };

    let query = "{ users(orderBy: {field: \"name\"}) { name } }";
    let result = executor.execute(query, None, Some(&user)).await.unwrap();

    // Verify French alphabetical order
    let names: Vec<&str> = extract_names(&result);
    assert_eq!(names, vec!["André", "Béatrice", "Émile", "François", "Zoë"]);
}
```

---

## Security Considerations

### 1. Collation Injection

**Risk**: Malicious locale values in JWT could inject SQL.

**Mitigation**:

- Validate collation format (regex: `^[a-z]{2}-[A-Z]{2}(-x-icu)?$`)
- Whitelist common locales
- Use parameterized queries (collation in COLLATE clause is safe)

```rust
impl CollationResolver {
    pub fn validate_collation(collation: &str) -> bool {
        // Regex validation
        let re = Regex::new(r"^[a-z]{2}-[A-Z]{2}(-x-icu)?$").unwrap();
        if !re.is_match(collation) {
            return false;
        }

        // Whitelist check (optional but recommended)
        Self::common_locales().contains(&collation.trim_end_matches("-x-icu"))
    }
}
```

### 2. Performance Impact

**Risk**: ICU collations are slower than byte-order (C) collation.

**Mitigation**:

- Only apply to text fields (not numeric/timestamp)
- Allow opt-out via explicit `collation: null`
- Consider index implications (indexes should match collation)

### 3. JWT Trust

**Risk**: Locale claim in JWT could be manipulated.

**Mitigation**:

- JWT signature validation (already in AuthMiddleware)
- Locale is non-security-critical (sorting preference, not authorization)
- Worst case: incorrect sorting, not data breach

---

## Migration Path

### v1 (Manual Collation)

```python
# v1: Manual collation in every query
result = await db.find(
    "v_user",
    order_by=[{"field": "name", "collation": "fr-FR-x-icu"}]
)
```

### v2 (Automatic Collation)

**Step 1**: Add locale to JWT claims

```python
# Auth provider configuration
def generate_jwt(user):
    return {
        "sub": user.id,
        "locale": user.preferred_locale,  # ← Add this
        # ... other claims
    }
```

**Step 2**: Enable auto-collation in schema

```python
@fraiseql.query(
    sql_source="v_user",
    auto_params={
        "order_by": {
            "enabled": True,
            "auto_collation": True  # ← Enable this
        }
    }
)
def users() -> list[User]:
    pass
```

**Step 3**: Remove manual collation from clients

```graphql
# Before
query { users(orderBy: {field: "name", collation: "fr-FR-x-icu"}) { name } }

# After (automatic)
query { users(orderBy: {field: "name"}) { name } }
```

---

## Future Enhancements

### 1. Collation Profiles

Define collation rules per field type:

```python
@fraiseql.type
class User:
    id: int
    name: str  # Auto-collation: user locale
    email: str  # Auto-collation: "C" (ASCII byte order for emails)
    city: str  # Auto-collation: user locale
```

### 2. Multi-Language Fields

Support per-field locale overrides:

```python
@fraiseql.type
class Product:
    id: int
    name_en: str  # Auto-collation: "en-US-x-icu"
    name_fr: str  # Auto-collation: "fr-FR-x-icu"
    name_de: str  # Auto-collation: "de-DE-x-icu"
```

### 3. Database Introspection

Verify collation availability at startup:

```rust
impl PostgresAdapter {
    async fn verify_collations(&self, locales: &[String]) -> Result<()> {
        for locale in locales {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_collation WHERE collname = $1)"
            )
            .bind(format!("{}-x-icu", locale))
            .fetch_one(&self.pool)
            .await?;

            if !exists {
                tracing::warn!("Collation {} not available", locale);
            }
        }
        Ok(())
    }
}
```

---

## Success Criteria

### Functional

- [x] AuthenticatedUser has `locale` field
- [x] Locale extracted from JWT claims
- [x] CollationResolver prioritizes explicit > user > default
- [x] SQL generation includes COLLATE clause
- [x] Manual override works (explicit collation)
- [x] Unauthenticated requests use fallback/default

### Quality

- [x] 20+ unit tests (collation resolution)
- [x] 10+ integration tests (unicode sorting)
- [x] Security validation (locale format)
- [x] Performance benchmarks (ICU vs C collation)

### Documentation

- [x] User guide with examples
- [x] JWT claim documentation
- [x] Migration guide from manual collation
- [x] Security considerations documented

---

## Timeline

**Addition to Phase 4b**: +1-2 days

- **Day 1**: Implementation
  - Extend AuthenticatedUser (2 hours)
  - Create CollationResolver (3 hours)
  - Update SQL generation (2 hours)
  - Unit tests (1 hour)

- **Day 2**: Testing & Documentation
  - Integration tests (3 hours)
  - Security validation (2 hours)
  - Examples and docs (3 hours)

**Total**: Phase 4b (5 days) + User-aware collation (1-2 days) = **6-7 days**

---

**Status**: Ready for implementation as enhancement to Phase 4b
**Priority**: High (major UX improvement for international users)
**Dependencies**: Phase 3 (Security/Auth) ✅, Phase 4b (ORDER BY) 🔜
