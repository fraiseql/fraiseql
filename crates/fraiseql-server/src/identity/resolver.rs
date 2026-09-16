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

use serde::{Deserialize, Serialize};

use super::{
    cache::{CachedOutcome, IdentityCache},
    failure::{DenyReason, IdentityResolution, ResolveError},
    query::{BoundQuery, MissingParam, prepare_enrichment_query},
};

/// An owned, `Send` boxed future — the object-safe async return used instead of a
/// new `async_trait` macro, keeping the dyn-dispatch ratchet flat (DESIGN §2.2).
pub(super) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The configured `sub → DB → identity` query for one profile (DESIGN §7). One
/// schema, reused by the enrichment and sender profiles. `deny_unknown_fields`
/// makes a mistyped/stranded key fail loud — the failure mode that hid #242's
/// absence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrichmentQueryConfig {
    /// When `true`, every authenticated request resolves and fail-closes,
    /// whether or not the operation consumes an enriched field (DESIGN §7,
    /// amendment B). The resolver is only constructed when enabled; it does not
    /// re-check this flag per request.
    #[serde(default)]
    pub enabled:           bool,
    /// The SQL, with named `$param` tokens bound from token claims.
    pub query:             String,
    /// Column → enriched-field renaming. A declared column that is NULL/absent in
    /// the resolved row is a denial (DESIGN §5).
    #[serde(default)]
    pub map:               BTreeMap<String, String>,
    /// Positive TTL for a `Resolved` outcome. Bounded (DESIGN §6.1): a revocation
    /// propagates within this window, or immediately via `flush(sub)`.
    #[serde(default = "default_cache_ttl_secs")]
    pub cache_ttl_secs:    u64,
    /// Negative TTL for a `Denied` outcome — short, so a freshly provisioned
    /// actor goes live quickly.
    #[serde(default = "default_negative_ttl_secs")]
    pub negative_ttl_secs: u64,
    /// Optional statement run when [`query`](Self::query) matches **zero rows**
    /// (#1324), after which `query` runs again. Bound from the same claims, the
    /// same way — out-of-band, never interpolated — and executed on the same
    /// unscoped pool, below the fail-closed gate.
    ///
    /// It fires on `ZeroRows` alone: an ambiguous row set, a NULL mapped field
    /// or a missing `$param` stay denials, so provisioning can never turn the
    /// refusal of an *existing* identity into access. The statement is the
    /// policy — to refuse a subject it inserts nothing (the re-run `query` then
    /// denies, and that denial **is** cached). Raising is an outage (503), not a
    /// refusal.
    ///
    /// Belongs to the enrichment profile only; [`IdentityConfig::validate`]
    /// refuses it on the sender profile, which shares this schema.
    #[serde(default)]
    pub provision:         Option<String>,
}

/// Top-level `[identity]` configuration: one shared query schema, two profiles
/// (DESIGN §7). Lives on `ServerConfig` (the config the running server loads), so
/// it applies under any auth mode — HS256/OIDC parity by construction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    /// Consumer A — read scoping. When enabled, every authenticated request
    /// resolves and fail-closes.
    #[serde(default)]
    pub enrichment: Option<EnrichmentQueryConfig>,
    /// Consumer B — verified sender-identity, consumed by the send path (P03 +
    /// the hardening train). Resolves at send time, not per request.
    #[serde(default)]
    pub sender:     Option<EnrichmentQueryConfig>,
}

impl IdentityConfig {
    /// Refuse a configuration whose keys would be silently inert or, worse,
    /// silently act (#1324).
    ///
    /// Both profiles deserialize from one [`EnrichmentQueryConfig`], which is
    /// what keeps a key added to the shared schema from reaching one profile
    /// only. The cost is that `provision` parses on `[identity.sender]`, where
    /// it would provision a *sending mailbox* for every subject the send path
    /// cannot resolve. There is no such thing as a verified address a server
    /// invents, so this is refused rather than ignored.
    ///
    /// # Errors
    ///
    /// Returns a human-readable message naming the offending profile when the
    /// sender profile sets `provision`.
    pub fn validate(&self) -> Result<(), String> {
        if self.sender.as_ref().is_some_and(|sender| sender.provision.is_some()) {
            return Err("[identity.sender] sets `provision`, which only the read path \
                 honours: provisioning a *sending* identity would invent a verified \
                 from-address. Move the key to [identity.enrichment], or remove it."
                .to_owned());
        }
        Ok(())
    }
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

    /// Run a statement for its effect, discarding any result (#1324).
    ///
    /// Separate from [`fetch_rows`](Self::fetch_rows) because that one wraps its
    /// SQL as a `FROM` sub-query, and a data-modifying statement cannot be one:
    /// the issue's documented `INSERT … ON CONFLICT DO NOTHING` contract has no
    /// path through it. Binding is identical.
    fn execute<'a>(
        &'a self,
        sql: &'a str,
        binds: &'a [serde_json::Value],
    ) -> BoxFuture<'a, Result<(), ResolveError>>;
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

/// Bind the claim values positionally, each as the Postgres type its JSON shape
/// already is. One builder, so the `query` and the `provision` statement cannot
/// drift on what a claim binds as.
///
/// An object or an array binds as `jsonb` — that is what makes `$claims` usable
/// as the `jsonb` parameter of a provisioning function without the operator
/// spelling a cast (#1324).
fn bind_claims(binds: &[serde_json::Value]) -> Result<sqlx::postgres::PgArguments, ResolveError> {
    use sqlx::Arguments as _;

    let mut args = sqlx::postgres::PgArguments::default();
    let failed = |e: sqlx::error::BoxDynError| {
        ResolveError::new(format!("identity query could not bind a claim value: {e}"))
    };
    for value in binds {
        match value {
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    args.add(i).map_err(failed)?;
                } else if let Some(f) = n.as_f64() {
                    args.add(f).map_err(failed)?;
                } else {
                    args.add(n.to_string()).map_err(failed)?;
                }
            },
            serde_json::Value::Bool(b) => args.add(*b).map_err(failed)?,
            serde_json::Value::Null => args.add(Option::<String>::None).map_err(failed)?,
            serde_json::Value::String(s) => args.add(s.as_str()).map_err(failed)?,
            composite @ (serde_json::Value::Array(_) | serde_json::Value::Object(_)) => {
                args.add(sqlx::types::Json(composite)).map_err(failed)?;
            },
        }
    }
    Ok(args)
}

impl IdentityStore for PgIdentityStore {
    fn fetch_rows<'a>(
        &'a self,
        sql: &'a str,
        binds: &'a [serde_json::Value],
    ) -> BoxFuture<'a, Result<Vec<serde_json::Map<String, serde_json::Value>>, ResolveError>> {
        Box::pin(async move {
            // `::text` keeps the decode one shape whatever the row holds (the
            // `json` feature is on for the *encode* side, `bind_claims`); `LIMIT
            // 2` lets the resolver detect ambiguity (DESIGN §5, >1 row → Denied).
            let wrapped = format!("SELECT row_to_json(t)::text FROM ({sql}) t LIMIT 2");
            let rows = sqlx::query_as_with::<_, (String,), _>(&wrapped, bind_claims(binds)?)
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

    fn execute<'a>(
        &'a self,
        sql: &'a str,
        binds: &'a [serde_json::Value],
    ) -> BoxFuture<'a, Result<(), ResolveError>> {
        Box::pin(async move {
            sqlx::query_with(sql, bind_claims(binds)?)
                .execute(&self.pool)
                .await
                .map_err(|e| ResolveError::new(format!("identity provision failed: {e}")))?;
            Ok(())
        })
    }
}

/// The shared resolver: one per profile, server-lifetime, memoizing per
/// bound-parameter tuple.
pub struct IdentityResolver {
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

    /// Construct a Postgres-backed resolver on an unscoped pool — the server's
    /// entry point for building a profile instance from config.
    #[must_use]
    pub fn postgres(config: EnrichmentQueryConfig, pool: sqlx::PgPool) -> Self {
        Self::new(config, Arc::new(PgIdentityStore::new(pool)))
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
        // A hit is final — including a `Denied(ZeroRows)` a provision statement
        // already declined to fix. Re-provisioning a refused subject on every
        // request is one write per request; `negative_ttl_secs` bounds it.
        if let Some(cached) = self.cache.get(&key) {
            return self.finalize(sub, into_resolution(cached));
        }

        let resolution = match self.fetch_and_classify(&bound).await {
            Ok(resolution) => resolution,
            // Transient — never cached; the read path fails the request (503).
            Err(err) => return self.finalize(sub, IdentityResolution::Unavailable(err)),
        };

        // #1324: an unknown subject, and a statement that can make it known.
        //
        // The pre-provision `ZeroRows` is deliberately **not** cached: a request
        // that arrived while this one provisions would find it and fail closed
        // for the rest of `negative_ttl_secs`. Nothing else provisions —
        // `Ambiguous`, `NullField` and `MissingParam` are the denials of an
        // identity that already exists, and turning one into access is the whole
        // failure model inverted.
        let resolution = match (&resolution, self.config.provision.as_deref()) {
            (IdentityResolution::Denied(DenyReason::ZeroRows), Some(statement)) => {
                match self.provision(statement, claims, &bound).await {
                    Ok(resolution) => resolution,
                    Err(err) => return self.finalize(sub, IdentityResolution::Unavailable(err)),
                }
            },
            // Deliberately a wildcard rather than an enumeration: a denial kind
            // added later must not provision until someone decides it should.
            _ => resolution,
        };

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

    /// Fetch and classify one bound query — the step both the first resolution
    /// and the post-provision re-resolution take.
    async fn fetch_and_classify(
        &self,
        bound: &BoundQuery,
    ) -> Result<IdentityResolution, ResolveError> {
        let rows = self.store.fetch_rows(&bound.sql, &bound.binds).await?;
        Ok(classify(rows, &self.config.map))
    }

    /// Run the configured `provision` statement for an unknown subject, then
    /// re-read through the **store** (#1324).
    ///
    /// The re-read goes to the store rather than the cache on purpose: the cache
    /// holds nothing for this tuple yet, and reading it back would answer with
    /// whatever a concurrent request happened to put there.
    ///
    /// The statement is the policy. To refuse a subject it inserts nothing, and
    /// the re-read denies — a denial that *is* cached. Raising is an outage
    /// (`Unavailable` → 503, never cached), not a way to refuse.
    async fn provision(
        &self,
        statement: &str,
        claims: &HashMap<String, serde_json::Value>,
        bound: &BoundQuery,
    ) -> Result<IdentityResolution, ResolveError> {
        let provision = match prepare_enrichment_query(statement, claims) {
            Ok(provision) => provision,
            // The statement names a claim this token does not carry: a
            // fail-closed denial, exactly as it is for `query`.
            Err(MissingParam(name)) => {
                return Ok(IdentityResolution::Denied(DenyReason::MissingParam(name)));
            },
        };
        self.store.execute(&provision.sql, &provision.binds).await?;
        self.fetch_and_classify(bound).await
    }

    /// Evict every cache entry for `sub` — the admin flush surface
    /// (`admin::identity_admin_router`) and (later) the provision/deprovision
    /// mutation hook. Propagates a revoke or a fresh provision immediately.
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
