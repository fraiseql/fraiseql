//! The shared `sub → DB → identity` resolver (DESIGN §2, §5, §6).
//!
//! One [`IdentityResolver`] type, constructed once per profile at server startup,
//! owning a store handle and a [`IdentityCache`]. Each `resolve` call:
//!
//! 1. binds the configured query against the request's claims ([`prepare_enrichment_query`]) — a
//!    missing `$param` is a fail-closed denial;
//! 2. looks the bound-parameter tuple up in the cache;
//! 3. on a miss, fetches up to **two** rows on the unscoped store (two, so ambiguity is
//!    detectable);
//! 4. [`classify`]s the rows into the [`IdentityResolution`] failure model; and
//! 5. caches `Resolved`/`Denied` (positive/negative TTL), never `Unavailable`.
//!
//! Every `Denied` and `Unavailable` is logged server-side (DESIGN §5.4) here, so
//! neither call site can forget it; the outward-facing generic response is the
//! consumer's responsibility.

use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use serde::Deserialize;

use super::{
    cache::{CachedOutcome, IdentityCache},
    failure::{DenyReason, IdentityResolution, ResolveError},
    query::{MissingParam, prepare_enrichment_query},
};

/// An owned, `Send` boxed future — the object-safe async return used instead of a
/// new `async_trait` macro, keeping the dyn-dispatch ratchet flat (DESIGN §2.2).
pub(super) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The configured `sub → DB → identity` query for one profile (DESIGN §7). One
/// schema, reused by the enrichment and sender profiles. `deny_unknown_fields`
/// makes a mistyped/stranded key fail loud — the failure mode that hid #242's
/// absence.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnrichmentQueryConfig {
    /// When `true`, every authenticated request resolves and fail-closes,
    /// whether or not the operation consumes an enriched field (DESIGN §7,
    /// amendment B). The resolver is only constructed when enabled; it does not
    /// re-check this flag per request.
    #[serde(default)]
    pub(super) enabled:           bool,
    /// The SQL, with named `$param` tokens bound from token claims.
    pub(super) query:             String,
    /// Column → enriched-field renaming. A declared column that is NULL/absent in
    /// the resolved row is a denial (DESIGN §5).
    #[serde(default)]
    pub(super) map:               BTreeMap<String, String>,
    /// Positive TTL for a `Resolved` outcome. Bounded (DESIGN §6.1): a revocation
    /// propagates within this window, or immediately via `flush(sub)`.
    #[serde(default = "default_cache_ttl_secs")]
    pub(super) cache_ttl_secs:    u64,
    /// Negative TTL for a `Denied` outcome — short, so a freshly provisioned
    /// actor goes live quickly.
    #[serde(default = "default_negative_ttl_secs")]
    pub(super) negative_ttl_secs: u64,
}

/// DESIGN §6.1: 60s, not #242's token-remaining-lifetime — a tighter revocation
/// window at a little more DB load.
const fn default_cache_ttl_secs() -> u64 {
    60
}

/// DESIGN §6: short by default so provisioning is seen quickly.
const fn default_negative_ttl_secs() -> u64 {
    5
}

/// Executes a prepared identity query on an **unscoped** connection (no
/// per-request GUCs — DESIGN §3.3), returning up to two rows as JSON objects.
/// Abstracting the DB behind this trait makes the failure model unit-testable
/// against a mock, with the Postgres implementation exercised behind the live-DB
/// skip-clean pattern.
pub(super) trait IdentityStore: Send + Sync {
    /// Fetch up to two rows for the bound query. Values are bound positionally,
    /// never interpolated. Returns [`ResolveError`] on any transient/DB failure.
    fn fetch_rows<'a>(
        &'a self,
        sql: &'a str,
        binds: &'a [serde_json::Value],
    ) -> BoxFuture<'a, Result<Vec<serde_json::Map<String, serde_json::Value>>, ResolveError>>;
}

/// The Postgres [`IdentityStore`], running on the unscoped enrichment pool.
pub(super) struct PgIdentityStore {
    pool: sqlx::PgPool,
}

impl PgIdentityStore {
    /// Wrap the unscoped pool.
    pub(super) const fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl IdentityStore for PgIdentityStore {
    fn fetch_rows<'a>(
        &'a self,
        sql: &'a str,
        binds: &'a [serde_json::Value],
    ) -> BoxFuture<'a, Result<Vec<serde_json::Map<String, serde_json::Value>>, ResolveError>> {
        Box::pin(async move {
            // `::text` sidesteps needing sqlx's json Decode feature; `LIMIT 2`
            // lets the resolver detect ambiguity (DESIGN §5, >1 row → Denied).
            let wrapped = format!("SELECT row_to_json(t)::text FROM ({sql}) t LIMIT 2");
            let mut query = sqlx::query_as::<_, (String,)>(&wrapped);

            // Stringify upfront so the &str binds outlive the query builder.
            let string_binds: Vec<String> = binds
                .iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect();

            for (bind_value, string_val) in binds.iter().zip(&string_binds) {
                query = match bind_value {
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            query.bind(i)
                        } else if let Some(f) = n.as_f64() {
                            query.bind(f)
                        } else {
                            query.bind(string_val.as_str())
                        }
                    },
                    serde_json::Value::Bool(b) => query.bind(*b),
                    serde_json::Value::Null => query.bind(Option::<String>::None),
                    // String, Array, Object bind as their string representation.
                    _ => query.bind(string_val.as_str()),
                };
            }

            let rows = query
                .fetch_all(&self.pool)
                .await
                .map_err(|e| ResolveError::new(format!("identity query failed: {e}")))?;

            let mut out = Vec::with_capacity(rows.len());
            for (json_text,) in rows {
                let value: serde_json::Value = serde_json::from_str(&json_text).map_err(|e| {
                    ResolveError::new(format!("identity query returned invalid JSON: {e}"))
                })?;
                match value {
                    serde_json::Value::Object(map) => out.push(map),
                    _ => {
                        return Err(ResolveError::new(
                            "identity query did not return a JSON object",
                        ));
                    },
                }
            }
            Ok(out)
        })
    }
}

/// The shared resolver: one per profile, server-lifetime, memoizing per
/// bound-parameter tuple.
pub(super) struct IdentityResolver {
    config: EnrichmentQueryConfig,
    store:  Arc<dyn IdentityStore>,
    cache:  IdentityCache,
}

impl IdentityResolver {
    /// Construct a resolver from its profile config and a store.
    pub(super) fn new(config: EnrichmentQueryConfig, store: Arc<dyn IdentityStore>) -> Self {
        Self {
            config,
            store,
            cache: IdentityCache::new(),
        }
    }

    /// Resolve `sub`'s identity, using the cache. `claims` supplies the `$param`
    /// bindings; `sub` labels the cache entry (for `flush`) and the server-side
    /// denial logs. Never returns a partial result — the mapped set is
    /// all-or-nothing (DESIGN §5.2).
    pub(super) async fn resolve(
        &self,
        sub: &str,
        claims: &HashMap<String, serde_json::Value>,
    ) -> IdentityResolution {
        let bound = match prepare_enrichment_query(&self.config.query, claims) {
            Ok(bound) => bound,
            Err(MissingParam(name)) => {
                return self
                    .finalize(sub, IdentityResolution::Denied(DenyReason::MissingParam(name)));
            },
        };

        let key = cache_key(&bound.binds);
        if let Some(cached) = self.cache.get(&key) {
            return self.finalize(sub, into_resolution(cached));
        }

        let rows = match self.store.fetch_rows(&bound.sql, &bound.binds).await {
            Ok(rows) => rows,
            // Transient — never cached; the read path fails the request (503).
            Err(err) => return self.finalize(sub, IdentityResolution::Unavailable(err)),
        };

        let resolution = classify(rows, &self.config.map);
        match &resolution {
            IdentityResolution::Resolved(map) => self.cache.insert(
                key,
                sub.to_owned(),
                CachedOutcome::Resolved(map.clone()),
                Duration::from_secs(self.config.cache_ttl_secs),
            ),
            IdentityResolution::Denied(reason) => self.cache.insert(
                key,
                sub.to_owned(),
                CachedOutcome::Denied(reason.clone()),
                Duration::from_secs(self.config.negative_ttl_secs),
            ),
            // Unreachable: `classify` never yields `Unavailable`.
            IdentityResolution::Unavailable(_) => {},
        }
        self.finalize(sub, resolution)
    }

    /// Evict every cache entry for `sub` — used by the admin flush surface and
    /// (later) the provision/deprovision mutation hook.
    pub(super) fn flush(&self, sub: &str) {
        self.cache.flush(sub);
    }

    /// Evict the entire cache.
    pub(super) fn flush_all(&self) {
        self.cache.flush_all();
    }

    /// Log denials/unavailables server-side (DESIGN §5.4) and pass the resolution
    /// through unchanged. Centralized so every call site — cache hit or miss —
    /// logs uniformly and the outward body can stay generic.
    fn finalize(&self, sub: &str, resolution: IdentityResolution) -> IdentityResolution {
        match &resolution {
            IdentityResolution::Denied(reason) => tracing::warn!(
                subject = %sub,
                reason = %reason.log_label(),
                "enriched-identity resolution denied",
            ),
            IdentityResolution::Unavailable(err) => tracing::warn!(
                subject = %sub,
                error = %err,
                "enriched-identity resolution unavailable",
            ),
            IdentityResolution::Resolved(_) => {},
        }
        resolution
    }
}

/// Serialize the ordered bound-`$param` tuple to a deterministic cache key
/// (DESIGN §6, amendment A). The key is exactly as discriminating as the query's
/// `WHERE` clause — no more, no less.
fn cache_key(binds: &[serde_json::Value]) -> String {
    serde_json::Value::Array(binds.to_vec()).to_string()
}

/// Lift a cached outcome back into the full resolution type.
fn into_resolution(cached: CachedOutcome) -> IdentityResolution {
    match cached {
        CachedOutcome::Resolved(map) => IdentityResolution::Resolved(map),
        CachedOutcome::Denied(reason) => IdentityResolution::Denied(reason),
    }
}

/// Classify fetched rows against the declared field map (DESIGN §5.1). Pure — the
/// whole failure model is exercised here without a database.
///
/// - 0 rows → `Denied(ZeroRows)` (unknown/unprovisioned subject);
/// - >1 row → `Denied(Ambiguous)` (we refuse to pick one);
/// - 1 row  → `Resolved` iff **every** mapped column is present and non-null, else
///   `Denied(NullField(col))` — never a partial merge, never an empty-string GUC.
fn classify(
    rows: Vec<serde_json::Map<String, serde_json::Value>>,
    map: &BTreeMap<String, String>,
) -> IdentityResolution {
    let mut rows = rows.into_iter();
    let Some(row) = rows.next() else {
        return IdentityResolution::Denied(DenyReason::ZeroRows);
    };
    if rows.next().is_some() {
        return IdentityResolution::Denied(DenyReason::Ambiguous);
    }

    let mut resolved = serde_json::Map::with_capacity(map.len());
    for (column, field) in map {
        match row.get(column) {
            Some(value) if !value.is_null() => {
                resolved.insert(field.clone(), value.clone());
            },
            // NULL or absent — deny the whole set (DESIGN §5.2).
            _ => return IdentityResolution::Denied(DenyReason::NullField(column.clone())),
        }
    }
    IdentityResolution::Resolved(resolved)
}
