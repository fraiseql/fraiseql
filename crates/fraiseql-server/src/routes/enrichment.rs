//! Claims enrichment for the `/auth/me` endpoint.
//!
//! Executes a configured SQL query after JWT verification to augment the
//! response with application-specific fields (roles, permissions, plans).
//!
//! Named parameters (`$sub`, `$email`) in the query are rewritten to
//! positional placeholders (`$1`, `$2`) and bound via `sqlx` — values are
//! **never** interpolated into the SQL string.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use fraiseql_core::security::oidc::MeEnrichmentConfig;
use sqlx::PgPool;

// ── Param extraction ────────────────────────────────────────────────────────

/// A query with named `$name` parameters rewritten to positional `$N` and
/// the ordered list of claim values to bind.
#[derive(Debug)]
struct BoundQuery {
    /// SQL with `$1`, `$2`, … placeholders.
    sql: String,
    /// Ordered bind values matching the positional placeholders.
    binds: Vec<serde_json::Value>,
}

/// Rewrite `$name` tokens in `query` to positional `$1`, `$2`, … and
/// look up the corresponding values in `claims`.
///
/// # Errors
///
/// Returns an error string if a referenced parameter is missing from claims.
fn prepare_enrichment_query(
    query: &str,
    claims: &HashMap<String, serde_json::Value>,
) -> Result<BoundQuery, String> {
    let mut sql = String::with_capacity(query.len());
    let mut binds: Vec<serde_json::Value> = Vec::new();
    let mut param_index: HashMap<String, usize> = HashMap::new();

    let bytes = query.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && is_name_start(bytes[i + 1]) {
            // Extract parameter name
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && is_name_char(bytes[end]) {
                end += 1;
            }
            let name = &query[start..end];

            // Reuse existing position or assign a new one
            let pos = if let Some(&existing) = param_index.get(name) {
                existing
            } else {
                let value = claims.get(name).ok_or_else(|| {
                    format!("Enrichment query references ${name} but it is not in the JWT claims")
                })?;
                binds.push(value.clone());
                let pos = binds.len();
                param_index.insert(name.to_owned(), pos);
                pos
            };

            sql.push('$');
            sql.push_str(&pos.to_string());
            i = end;
        } else {
            // SAFETY: we index byte-by-byte within ASCII SQL text
            sql.push(char::from(bytes[i]));
            i += 1;
        }
    }

    Ok(BoundQuery { sql, binds })
}

const fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

const fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// ── Cache ───────────────────────────────────────────────────────────────────

/// Cached enrichment result with expiry.
struct CacheEntry {
    value: serde_json::Map<String, serde_json::Value>,
    expires_at: Instant,
}

/// Per-`sub` cache for enrichment query results.
pub struct EnrichmentCache {
    entries: DashMap<String, CacheEntry>,
}

impl EnrichmentCache {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    /// Get a cached result if it exists and hasn't expired.
    fn get(&self, sub: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
        let entry = self.entries.get(sub)?;
        if Instant::now() < entry.expires_at {
            Some(entry.value.clone())
        } else {
            drop(entry);
            self.entries.remove(sub);
            None
        }
    }

    /// Insert a result with the given TTL.
    fn insert(
        &self,
        sub: String,
        value: serde_json::Map<String, serde_json::Value>,
        ttl: Duration,
    ) {
        self.entries.insert(sub, CacheEntry {
            value,
            expires_at: Instant::now() + ttl,
        });
    }
}

// ── Query execution ─────────────────────────────────────────────────────────

/// Execute the enrichment query and return the result as a JSON map.
///
/// - If the query returns no rows, returns `Ok(None)` and logs a warning.
/// - If the query fails, returns `Err` with the error string.
///
/// # Errors
///
/// Returns an error string if the query fails or parameter binding fails.
pub async fn run_enrichment(
    pool: &PgPool,
    config: &MeEnrichmentConfig,
    claims: &HashMap<String, serde_json::Value>,
    cache: Option<&Arc<EnrichmentCache>>,
    sub: &str,
    token_remaining_secs: u64,
) -> Result<Option<serde_json::Map<String, serde_json::Value>>, String> {
    // Check cache first
    let cache_disabled = config.cache_ttl_secs == Some(0);
    if !cache_disabled {
        if let Some(cache) = cache {
            if let Some(cached) = cache.get(sub) {
                return Ok(Some(cached));
            }
        }
    }

    // Prepare the bound query
    let bound = prepare_enrichment_query(&config.query, claims)?;

    // Execute via sqlx with positional binds
    let wrapped_sql = format!("SELECT row_to_json(t) FROM ({}) t LIMIT 1", bound.sql);
    let mut query = sqlx::query_as::<_, (serde_json::Value,)>(&wrapped_sql);

    // Stringify all values upfront so they live long enough for sqlx bind references.
    let string_binds: Vec<String> = bound
        .binds
        .iter()
        .map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .collect();

    for (bind_value, string_val) in bound.binds.iter().zip(&string_binds) {
        query = match bind_value {
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    query.bind(i)
                } else if let Some(f) = n.as_f64() {
                    query.bind(f)
                } else {
                    query.bind(string_val.as_str())
                }
            }
            serde_json::Value::Bool(b) => query.bind(b),
            serde_json::Value::Null => query.bind(Option::<String>::None),
            // String, Array, Object — all bind as their string representation
            _ => query.bind(string_val.as_str()),
        };
    }

    let row = query
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Enrichment query failed: {e}"))?;

    let Some((json_value,)) = row else {
        tracing::warn!(
            sub = sub,
            "Enrichment query returned no rows for sub={sub}",
        );
        return Ok(None);
    };

    // Parse the JSON row
    let serde_json::Value::Object(row_map) = json_value else {
        return Err("Enrichment query did not return a JSON object".to_string());
    };

    // Apply column renaming if configured
    let result = if let Some(ref map) = config.map {
        let mut renamed = serde_json::Map::new();
        for (col_name, response_name) in map {
            if let Some(value) = row_map.get(col_name) {
                renamed.insert(response_name.clone(), value.clone());
            }
        }
        renamed
    } else {
        row_map
    };

    // Cache the result
    if !cache_disabled {
        if let Some(cache) = cache {
            let ttl_secs = config.cache_ttl_secs.unwrap_or(token_remaining_secs);
            if ttl_secs > 0 {
                cache.insert(sub.to_owned(), result.clone(), Duration::from_secs(ttl_secs));
            }
        }
    }

    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test code

    use super::*;

    // ── prepare_enrichment_query ─────────────────────────────────────────

    #[test]
    fn rewrites_single_param() {
        let mut claims = HashMap::new();
        claims.insert("sub".to_owned(), serde_json::json!("user-123"));

        let bound =
            prepare_enrichment_query("SELECT role FROM users WHERE sub = $sub", &claims).unwrap();

        assert_eq!(bound.sql, "SELECT role FROM users WHERE sub = $1");
        assert_eq!(bound.binds.len(), 1);
        assert_eq!(bound.binds[0], serde_json::json!("user-123"));
    }

    #[test]
    fn rewrites_multiple_params() {
        let mut claims = HashMap::new();
        claims.insert("sub".to_owned(), serde_json::json!("u1"));
        claims.insert("email".to_owned(), serde_json::json!("a@b.com"));

        let bound = prepare_enrichment_query(
            "SELECT role FROM users WHERE sub = $sub AND email = $email",
            &claims,
        )
        .unwrap();

        assert_eq!(
            bound.sql,
            "SELECT role FROM users WHERE sub = $1 AND email = $2"
        );
        assert_eq!(bound.binds.len(), 2);
    }

    #[test]
    fn reuses_position_for_repeated_param() {
        let mut claims = HashMap::new();
        claims.insert("sub".to_owned(), serde_json::json!("u1"));

        let bound = prepare_enrichment_query(
            "SELECT * FROM users WHERE sub = $sub OR alt_sub = $sub",
            &claims,
        )
        .unwrap();

        assert_eq!(
            bound.sql,
            "SELECT * FROM users WHERE sub = $1 OR alt_sub = $1"
        );
        assert_eq!(bound.binds.len(), 1);
    }

    #[test]
    fn missing_param_returns_error() {
        let claims = HashMap::new();

        let err = prepare_enrichment_query("SELECT 1 WHERE sub = $sub", &claims).unwrap_err();

        assert!(err.contains("$sub"));
        assert!(err.contains("not in the JWT claims"));
    }

    #[test]
    fn no_params_passes_through() {
        let claims = HashMap::new();

        let bound = prepare_enrichment_query("SELECT 1 AS one", &claims).unwrap();

        assert_eq!(bound.sql, "SELECT 1 AS one");
        assert!(bound.binds.is_empty());
    }

    #[test]
    fn preserves_dollar_followed_by_digit() {
        let claims = HashMap::new();

        let bound = prepare_enrichment_query("SELECT $1", &claims).unwrap();

        // $1 is NOT a named param (digit after $) — passed through as-is
        assert_eq!(bound.sql, "SELECT $1");
    }

    // ── EnrichmentCache ─────────────────────────────────────────────────

    #[test]
    fn cache_hit_returns_value() {
        let cache = EnrichmentCache::new();
        let mut map = serde_json::Map::new();
        map.insert("role".to_owned(), serde_json::json!("admin"));

        cache.insert("user-1".to_owned(), map, Duration::from_secs(60));

        let result = cache.get("user-1");
        assert!(result.is_some());
        assert_eq!(result.unwrap()["role"], "admin");
    }

    #[test]
    fn cache_miss_returns_none() {
        let cache = EnrichmentCache::new();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn expired_entry_returns_none() {
        let cache = EnrichmentCache::new();
        let map = serde_json::Map::new();

        // Insert with already-expired timestamp.
        // Use checked_sub to avoid overflow; fall back to UNIX_EPOCH-equivalent via now().
        let expires_at = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        cache.entries.insert("user-1".to_owned(), CacheEntry {
            value: map,
            expires_at,
        });

        assert!(cache.get("user-1").is_none());
    }

    // ── Security: adversarial inputs ────────────────────────────────────

    #[test]
    fn sql_injection_in_claim_value_is_bound_not_interpolated() {
        let mut claims = HashMap::new();
        claims.insert(
            "email".to_owned(),
            serde_json::json!("'; DROP TABLE users; --"),
        );

        let bound = prepare_enrichment_query(
            "SELECT role FROM users WHERE email = $email",
            &claims,
        )
        .unwrap();

        // The malicious value must appear as a bind parameter, not in the SQL
        assert_eq!(bound.sql, "SELECT role FROM users WHERE email = $1");
        assert_eq!(bound.binds[0], serde_json::json!("'; DROP TABLE users; --"));
        assert!(!bound.sql.contains("DROP"));
    }

    #[test]
    fn sql_comment_in_claim_value_is_bound_not_interpolated() {
        let mut claims = HashMap::new();
        claims.insert("sub".to_owned(), serde_json::json!("user /* */ OR 1=1"));

        let bound =
            prepare_enrichment_query("SELECT role FROM users WHERE sub = $sub", &claims).unwrap();

        assert_eq!(bound.sql, "SELECT role FROM users WHERE sub = $1");
        assert_eq!(bound.binds[0], serde_json::json!("user /* */ OR 1=1"));
        assert!(!bound.sql.contains("/*"));
    }

    #[test]
    fn overlapping_param_names_are_distinguished() {
        // $email vs $email_verified — ensure greedy match doesn't
        // treat $email_verified as "$email" + "verified"
        let mut claims = HashMap::new();
        claims.insert("email".to_owned(), serde_json::json!("a@b.com"));
        claims.insert("email_verified".to_owned(), serde_json::json!(true));

        let bound = prepare_enrichment_query(
            "SELECT * FROM users WHERE email = $email AND verified = $email_verified",
            &claims,
        )
        .unwrap();

        assert_eq!(
            bound.sql,
            "SELECT * FROM users WHERE email = $1 AND verified = $2"
        );
        assert_eq!(bound.binds.len(), 2);
        assert_eq!(bound.binds[0], serde_json::json!("a@b.com"));
        assert_eq!(bound.binds[1], serde_json::json!(true));
    }

    #[test]
    fn param_at_end_of_query() {
        let mut claims = HashMap::new();
        claims.insert("sub".to_owned(), serde_json::json!("u1"));

        let bound =
            prepare_enrichment_query("SELECT * FROM users WHERE sub = $sub", &claims).unwrap();

        assert_eq!(bound.sql, "SELECT * FROM users WHERE sub = $1");
    }

    #[test]
    fn unicode_claim_value_is_bound() {
        let mut claims = HashMap::new();
        claims.insert("sub".to_owned(), serde_json::json!("用户-émoji-🍓"));

        let bound =
            prepare_enrichment_query("SELECT role FROM users WHERE sub = $sub", &claims).unwrap();

        assert_eq!(bound.binds[0], serde_json::json!("用户-émoji-🍓"));
    }
}
