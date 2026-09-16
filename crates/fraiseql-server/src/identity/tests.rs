//! Test suite for the enriched-identity resolver.
//!
//! The `prepare_enrichment_query` adversarial cases are ported verbatim from #242
//! (`routes/enrichment.rs`, `v2.2.1`) — the hard-to-get-right, already-correct
//! core, pinned as a fixed point (DESIGN §8, P00). The cache, the failure model
//! (against a mock store), and the Postgres store (behind the live-DB skip-clean
//! pattern) are exercised on top (P01).

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::Utc;
use fraiseql_core::{
    security::{ENRICHED_NAMESPACE_PREFIX, SecurityContext},
    types::UserId,
};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{
    admin::identity_admin_router,
    apply::{EnrichmentOutcome, enrich_security_context},
    cache::{CachedOutcome, IdentityCache},
    failure::{DenyReason, IdentityResolution, ResolveError},
    query::{MissingParam, prepare_enrichment_query},
    resolver::{
        BoxFuture, EnrichmentQueryConfig, IdentityConfig, IdentityResolver, IdentityStore,
        PgIdentityStore,
    },
};

// ── prepare_enrichment_query (ported verbatim from #242) ──────────────────

#[test]
fn rewrites_single_param() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), json!("user-123"));

    let bound =
        prepare_enrichment_query("SELECT role FROM users WHERE sub = $sub", &claims).unwrap();

    assert_eq!(bound.sql, "SELECT role FROM users WHERE sub = $1");
    assert_eq!(bound.binds.len(), 1);
    assert_eq!(bound.binds[0], json!("user-123"));
}

#[test]
fn rewrites_multiple_params() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), json!("u1"));
    claims.insert("email".to_owned(), json!("a@b.com"));

    let bound = prepare_enrichment_query(
        "SELECT role FROM users WHERE sub = $sub AND email = $email",
        &claims,
    )
    .unwrap();

    assert_eq!(bound.sql, "SELECT role FROM users WHERE sub = $1 AND email = $2");
    assert_eq!(bound.binds.len(), 2);
}

#[test]
fn reuses_position_for_repeated_param() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), json!("u1"));

    let bound =
        prepare_enrichment_query("SELECT * FROM users WHERE sub = $sub OR alt_sub = $sub", &claims)
            .unwrap();

    assert_eq!(bound.sql, "SELECT * FROM users WHERE sub = $1 OR alt_sub = $1");
    assert_eq!(bound.binds.len(), 1);
}

#[test]
fn missing_param_returns_structured_error() {
    let claims = HashMap::new();

    // Refined from #242's message string to a structured `MissingParam` so the
    // resolver maps it directly to a fail-closed denial (DESIGN §5).
    let err = prepare_enrichment_query("SELECT 1 WHERE sub = $sub", &claims).unwrap_err();

    assert_eq!(err, MissingParam("sub".to_owned()));
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

    // $1 is NOT a named param (digit after $) — passed through as-is.
    assert_eq!(bound.sql, "SELECT $1");
}

#[test]
fn sql_injection_in_claim_value_is_bound_not_interpolated() {
    let mut claims = HashMap::new();
    claims.insert("email".to_owned(), json!("'; DROP TABLE users; --"));

    let bound =
        prepare_enrichment_query("SELECT role FROM users WHERE email = $email", &claims).unwrap();

    // The malicious value must appear as a bind parameter, not in the SQL.
    assert_eq!(bound.sql, "SELECT role FROM users WHERE email = $1");
    assert_eq!(bound.binds[0], json!("'; DROP TABLE users; --"));
    assert!(!bound.sql.contains("DROP"));
}

#[test]
fn sql_comment_in_claim_value_is_bound_not_interpolated() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), json!("user /* */ OR 1=1"));

    let bound =
        prepare_enrichment_query("SELECT role FROM users WHERE sub = $sub", &claims).unwrap();

    assert_eq!(bound.sql, "SELECT role FROM users WHERE sub = $1");
    assert_eq!(bound.binds[0], json!("user /* */ OR 1=1"));
    assert!(!bound.sql.contains("/*"));
}

#[test]
fn overlapping_param_names_are_distinguished() {
    // $email vs $email_verified — ensure the greedy match doesn't treat
    // $email_verified as "$email" + "verified".
    let mut claims = HashMap::new();
    claims.insert("email".to_owned(), json!("a@b.com"));
    claims.insert("email_verified".to_owned(), json!(true));

    let bound = prepare_enrichment_query(
        "SELECT * FROM users WHERE email = $email AND verified = $email_verified",
        &claims,
    )
    .unwrap();

    assert_eq!(bound.sql, "SELECT * FROM users WHERE email = $1 AND verified = $2");
    assert_eq!(bound.binds.len(), 2);
    assert_eq!(bound.binds[0], json!("a@b.com"));
    assert_eq!(bound.binds[1], json!(true));
}

#[test]
fn param_at_end_of_query() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), json!("u1"));

    let bound = prepare_enrichment_query("SELECT * FROM users WHERE sub = $sub", &claims).unwrap();

    assert_eq!(bound.sql, "SELECT * FROM users WHERE sub = $1");
}

#[test]
fn unicode_claim_value_is_bound() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), json!("用户-émoji-🍓"));

    let bound =
        prepare_enrichment_query("SELECT role FROM users WHERE sub = $sub", &claims).unwrap();

    assert_eq!(bound.binds[0], json!("用户-émoji-🍓"));
}

// ── IdentityCache (DESIGN §6) ─────────────────────────────────────────────

fn resolved(field: &str, value: Value) -> CachedOutcome {
    let mut map = serde_json::Map::new();
    map.insert(field.to_owned(), value);
    CachedOutcome::Resolved(map)
}

#[test]
fn cache_returns_inserted_outcome() {
    let cache = IdentityCache::new();
    cache.insert(
        "[\"u1\"]".to_owned(),
        "u1".to_owned(),
        resolved("actor_role", json!("admin")),
        Duration::from_mins(1),
    );

    match cache.get("[\"u1\"]") {
        Some(CachedOutcome::Resolved(map)) => assert_eq!(map["actor_role"], "admin"),
        other => panic!("expected Resolved, got {:?}", other.is_some()),
    }
}

#[test]
fn cache_miss_returns_none() {
    let cache = IdentityCache::new();
    assert!(cache.get("nonexistent").is_none());
}

#[test]
fn cache_expired_entry_returns_none() {
    let cache = IdentityCache::new();
    // A zero TTL is already elapsed by the next monotonic `get` (strict `<`).
    cache.insert(
        "[\"u1\"]".to_owned(),
        "u1".to_owned(),
        resolved("actor_role", json!("admin")),
        Duration::from_secs(0),
    );

    assert!(cache.get("[\"u1\"]").is_none());
}

#[test]
fn cache_flush_evicts_only_matching_subject() {
    let cache = IdentityCache::new();
    // Two entries for u1 (different bound tuples), one for u2.
    cache.insert(
        "[\"u1\"]".to_owned(),
        "u1".to_owned(),
        resolved("r", json!("a")),
        Duration::from_mins(1),
    );
    cache.insert(
        "[\"u1\",\"x\"]".to_owned(),
        "u1".to_owned(),
        resolved("r", json!("a")),
        Duration::from_mins(1),
    );
    cache.insert(
        "[\"u2\"]".to_owned(),
        "u2".to_owned(),
        resolved("r", json!("b")),
        Duration::from_mins(1),
    );

    cache.flush("u1");

    assert_eq!(cache.len(), 1);
    assert!(cache.get("[\"u1\"]").is_none());
    assert!(cache.get("[\"u1\",\"x\"]").is_none());
    assert!(cache.get("[\"u2\"]").is_some());
}

#[test]
fn cache_flush_all_clears() {
    let cache = IdentityCache::new();
    cache.insert(
        "[\"u1\"]".to_owned(),
        "u1".to_owned(),
        resolved("r", json!("a")),
        Duration::from_mins(1),
    );
    cache.insert(
        "[\"u2\"]".to_owned(),
        "u2".to_owned(),
        resolved("r", json!("b")),
        Duration::from_mins(1),
    );

    cache.flush_all();

    assert_eq!(cache.len(), 0);
}

// ── Failure model against a mock store (DESIGN §5) ────────────────────────

/// Holds the **first** provision open until the test releases it, so a second
/// request can be driven through its whole `resolve` while the first is still
/// inside the statement (#1324).
///
/// Sequential requests cannot tell a design that caches the pre-provision
/// `ZeroRows` from one that does not — both answer every request correctly in
/// the end. This window is the only place they differ.
struct ProvisionGate {
    entered: tokio::sync::Semaphore,
    release: tokio::sync::Semaphore,
    seen:    AtomicUsize,
}

impl ProvisionGate {
    fn new() -> Self {
        Self {
            entered: tokio::sync::Semaphore::new(0),
            release: tokio::sync::Semaphore::new(0),
            seen:    AtomicUsize::new(0),
        }
    }

    /// Resolve once the first provision statement has started.
    async fn wait_until_provisioning(&self) {
        self.entered.acquire().await.unwrap().forget();
    }

    /// Let the held provision finish.
    fn release(&self) {
        self.release.add_permits(1);
    }
}

/// A store that returns a fixed row set (or a transient error) and counts calls,
/// so tests can assert both the classification and the caching behaviour.
///
/// The provisioning arm (#1324) makes it a small state machine: `fetch_rows`
/// answers `rows` until an `execute` succeeds and `provisioned_rows` answers
/// after — which is what a `provision` statement does to the table the `query`
/// reads.
struct MockStore {
    rows:             Vec<serde_json::Map<String, Value>>,
    provisioned_rows: Option<Vec<serde_json::Map<String, Value>>>,
    provisioned:      std::sync::atomic::AtomicBool,
    fail:             Option<ResolveError>,
    provision_fail:   Option<ResolveError>,
    calls:            AtomicUsize,
    provisions:       AtomicUsize,
    executed:         std::sync::Mutex<Vec<(String, Vec<Value>)>>,
    gate:             Option<Arc<ProvisionGate>>,
}

impl MockStore {
    fn returning(rows: Vec<serde_json::Map<String, Value>>) -> Self {
        Self {
            rows,
            provisioned_rows: None,
            provisioned: std::sync::atomic::AtomicBool::new(false),
            fail: None,
            provision_fail: None,
            calls: AtomicUsize::new(0),
            provisions: AtomicUsize::new(0),
            executed: std::sync::Mutex::new(Vec::new()),
            gate: None,
        }
    }

    fn failing() -> Self {
        Self {
            fail: Some(ResolveError::new("db unreachable")),
            ..Self::returning(Vec::new())
        }
    }

    /// What `fetch_rows` answers once a provision has run.
    fn provisioning_to(mut self, rows: Vec<serde_json::Map<String, Value>>) -> Self {
        self.provisioned_rows = Some(rows);
        self
    }

    /// A provision statement that raises — the operator's outage, not a
    /// refusal of the subject.
    fn failing_provision(mut self) -> Self {
        self.provision_fail = Some(ResolveError::new("provision function raised"));
        self
    }

    /// Hold the first provision open on `gate`.
    fn gated(mut self, gate: Arc<ProvisionGate>) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Whether this call is the first provision to arrive.
    fn first_provision(&self) -> bool {
        self.gate
            .as_ref()
            .is_some_and(|gate| gate.seen.fetch_add(1, Ordering::SeqCst) == 0)
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    fn provisions(&self) -> usize {
        self.provisions.load(Ordering::Relaxed)
    }

    /// The `(sql, binds)` of each executed provision statement.
    fn executed(&self) -> Vec<(String, Vec<Value>)> {
        self.executed.lock().unwrap().clone()
    }
}

impl IdentityStore for MockStore {
    fn fetch_rows<'a>(
        &'a self,
        _sql: &'a str,
        _binds: &'a [Value],
    ) -> BoxFuture<'a, Result<Vec<serde_json::Map<String, Value>>, ResolveError>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let rows = match &self.provisioned_rows {
            Some(after) if self.provisioned.load(Ordering::SeqCst) => after.clone(),
            _ => self.rows.clone(),
        };
        let result = self.fail.clone().map_or(Ok(rows), Err);
        Box::pin(async move { result })
    }

    fn execute<'a>(
        &'a self,
        sql: &'a str,
        binds: &'a [Value],
    ) -> BoxFuture<'a, Result<(), ResolveError>> {
        self.provisions.fetch_add(1, Ordering::Relaxed);
        self.executed.lock().unwrap().push((sql.to_owned(), binds.to_vec()));
        let first = self.first_provision();
        Box::pin(async move {
            if first {
                if let Some(gate) = &self.gate {
                    gate.entered.add_permits(1);
                    gate.release.acquire().await.unwrap().forget();
                }
            }
            if let Some(err) = self.provision_fail.clone() {
                return Err(err);
            }
            self.provisioned.store(true, Ordering::SeqCst);
            Ok(())
        })
    }
}

fn row(pairs: &[(&str, Value)]) -> serde_json::Map<String, Value> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
}

fn claims(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
}

fn config(query: &str, map: &[(&str, &str)]) -> EnrichmentQueryConfig {
    EnrichmentQueryConfig {
        enabled:           true,
        query:             query.to_owned(),
        map:               map.iter().map(|(c, f)| ((*c).to_owned(), (*f).to_owned())).collect(),
        cache_ttl_secs:    60,
        negative_ttl_secs: 5,
        provision:         None,
    }
}

/// A resolver over a `sub`-keyed query with the actor mapping, backed by `store`.
fn resolver(store: MockStore) -> IdentityResolver {
    IdentityResolver::new(
        config(
            "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub",
            &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
        ),
        Arc::new(store),
    )
}

fn sub_claims() -> HashMap<String, Value> {
    claims(&[("sub", json!("u1"))])
}

#[tokio::test]
async fn resolve_one_row_all_fields_present_resolves() {
    let store = MockStore::returning(vec![row(&[
        ("actor_id", json!("a-1")),
        ("actor_role", json!("manager")),
    ])]);
    let resolver = resolver(store);

    match resolver.resolve("u1", &sub_claims()).await {
        IdentityResolution::Resolved(map) => {
            assert_eq!(map["actor_id"], "a-1");
            assert_eq!(map["actor_role"], "manager");
        },
        other => panic!("expected Resolved, got {other:?}"),
    }
}

#[tokio::test]
async fn resolve_zero_rows_denies_unknown_subject() {
    let resolver = resolver(MockStore::returning(vec![]));

    match resolver.resolve("u1", &sub_claims()).await {
        IdentityResolution::Denied(DenyReason::ZeroRows) => {},
        other => panic!("expected Denied(ZeroRows), got {other:?}"),
    }
}

#[tokio::test]
async fn resolve_more_than_one_row_denies_ambiguous() {
    let store = MockStore::returning(vec![
        row(&[("actor_id", json!("a-1")), ("actor_role", json!("manager"))]),
        row(&[("actor_id", json!("a-2")), ("actor_role", json!("staff"))]),
    ]);

    match resolver(store).resolve("u1", &sub_claims()).await {
        IdentityResolution::Denied(DenyReason::Ambiguous) => {},
        other => panic!("expected Denied(Ambiguous), got {other:?}"),
    }
}

#[tokio::test]
async fn resolve_null_mapped_field_denies() {
    let store = MockStore::returning(vec![row(&[
        ("actor_id", json!("a-1")),
        ("actor_role", Value::Null),
    ])]);

    match resolver(store).resolve("u1", &sub_claims()).await {
        IdentityResolution::Denied(DenyReason::NullField(col)) => assert_eq!(col, "actor_role"),
        other => panic!("expected Denied(NullField), got {other:?}"),
    }
}

#[tokio::test]
async fn resolve_absent_mapped_field_denies() {
    // Row is present but omits `actor_role` entirely — treated as NULL.
    let store = MockStore::returning(vec![row(&[("actor_id", json!("a-1"))])]);

    match resolver(store).resolve("u1", &sub_claims()).await {
        IdentityResolution::Denied(DenyReason::NullField(col)) => assert_eq!(col, "actor_role"),
        other => panic!("expected Denied(NullField), got {other:?}"),
    }
}

#[tokio::test]
async fn resolve_missing_bound_param_denies() {
    // Query binds $email but the token has no email claim (DESIGN §9 item 8).
    let resolver = IdentityResolver::new(
        config(
            "SELECT actor_id FROM tb_actor WHERE sub = $sub AND email = $email",
            &[("actor_id", "actor_id")],
        ),
        Arc::new(MockStore::returning(vec![])),
    );

    match resolver.resolve("u1", &sub_claims()).await {
        IdentityResolution::Denied(DenyReason::MissingParam(name)) => assert_eq!(name, "email"),
        other => panic!("expected Denied(MissingParam), got {other:?}"),
    }
}

#[tokio::test]
async fn resolve_transient_error_is_unavailable_and_not_cached() {
    let resolver = resolver(MockStore::failing());

    assert!(matches!(
        resolver.resolve("u1", &sub_claims()).await,
        IdentityResolution::Unavailable(_)
    ));
    // A transient blip must not be cached — the second call re-hits the store.
    assert!(matches!(
        resolver.resolve("u1", &sub_claims()).await,
        IdentityResolution::Unavailable(_)
    ));
}

#[tokio::test]
async fn resolve_caches_resolved_positively() {
    let store = MockStore::returning(vec![row(&[
        ("actor_id", json!("a-1")),
        ("actor_role", json!("manager")),
    ])]);
    let store = Arc::new(store);
    let resolver = IdentityResolver::new(
        config(
            "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub",
            &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
        ),
        store.clone(),
    );

    let _ = resolver.resolve("u1", &sub_claims()).await;
    let _ = resolver.resolve("u1", &sub_claims()).await;

    assert_eq!(store.calls(), 1, "second resolve should be served from cache");
}

#[tokio::test]
async fn resolve_caches_denied_negatively() {
    let store = Arc::new(MockStore::returning(vec![]));
    let resolver = IdentityResolver::new(
        config("SELECT actor_id FROM tb_actor WHERE sub = $sub", &[("actor_id", "actor_id")]),
        store.clone(),
    );

    assert!(matches!(
        resolver.resolve("u1", &sub_claims()).await,
        IdentityResolution::Denied(DenyReason::ZeroRows)
    ));
    assert!(matches!(
        resolver.resolve("u1", &sub_claims()).await,
        IdentityResolution::Denied(DenyReason::ZeroRows)
    ));

    assert_eq!(store.calls(), 1, "a denial is negative-cached");
}

#[tokio::test]
async fn flush_evicts_subject_so_next_resolve_rehits() {
    let store = Arc::new(MockStore::returning(vec![row(&[
        ("actor_id", json!("a-1")),
        ("actor_role", json!("manager")),
    ])]));
    let resolver = IdentityResolver::new(
        config(
            "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub",
            &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
        ),
        store.clone(),
    );

    let _ = resolver.resolve("u1", &sub_claims()).await;
    resolver.flush("u1");
    let _ = resolver.resolve("u1", &sub_claims()).await;

    assert_eq!(store.calls(), 2, "flush forces a re-resolution");
}

#[tokio::test]
async fn cache_key_discriminates_by_bound_params() {
    // Same query, two different `$sub` bindings → two distinct cache keys, so the
    // store is hit for each (amendment A: no cross-subject sharing).
    let store = Arc::new(MockStore::returning(vec![row(&[
        ("actor_id", json!("a-1")),
        ("actor_role", json!("manager")),
    ])]));
    let resolver = IdentityResolver::new(
        config(
            "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub",
            &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
        ),
        store.clone(),
    );

    let _ = resolver.resolve("u1", &claims(&[("sub", json!("u1"))])).await;
    let _ = resolver.resolve("u2", &claims(&[("sub", json!("u2"))])).await;

    assert_eq!(store.calls(), 2, "distinct bound tuples do not share a cache entry");
}

// ── PgIdentityStore against a live Postgres (skip-clean) ──────────────────

/// Connect to the harness-provided Postgres (Dagger-bound in CI; a local spawn
/// with the `local-testcontainers` feature). `None` when no service is available
/// so the test skips cleanly — including when the connection itself fails, which
/// is announced rather than panicking inside test setup (#879).
async fn connect_pool() -> Option<(sqlx::PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    match sqlx::PgPool::connect(svc.url()).await {
        Ok(pool) => Some((pool, svc)),
        Err(e) => {
            eprintln!("SKIP: postgres reachable but connect failed ({e}); skipping");
            None
        },
    }
}

/// Create a fresh, uniquely-named actor table so parallel runs stay independent.
async fn make_actor_table(pool: &sqlx::PgPool) -> String {
    let table = format!("tb_actor_test_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE TABLE {table} (sub text, actor_id text, actor_role text)"))
        .execute(pool)
        .await
        .unwrap();
    table
}

fn actor_resolver(pool: &sqlx::PgPool, table: &str) -> IdentityResolver {
    IdentityResolver::new(
        config(
            &format!("SELECT actor_id, actor_role FROM {table} WHERE sub = $sub"),
            &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
        ),
        Arc::new(PgIdentityStore::new(pool.clone())),
    )
}

#[tokio::test]
async fn pg_store_resolves_and_renames_known_subject() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP pg_store_resolves_and_renames_known_subject: no postgres");
        return;
    };
    let table = make_actor_table(&pool).await;
    sqlx::query(&format!(
        "INSERT INTO {table} (sub, actor_id, actor_role) VALUES ('u1', 'a-1', 'manager')"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let resolver = actor_resolver(&pool, &table);
    match resolver.resolve("u1", &sub_claims()).await {
        IdentityResolution::Resolved(map) => {
            assert_eq!(map["actor_id"], "a-1");
            assert_eq!(map["actor_role"], "manager");
        },
        other => panic!("expected Resolved, got {other:?}"),
    }

    sqlx::query(&format!("DROP TABLE {table}")).execute(&pool).await.unwrap();
}

#[tokio::test]
async fn pg_store_denies_unknown_subject() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP pg_store_denies_unknown_subject: no postgres");
        return;
    };
    let table = make_actor_table(&pool).await;

    let resolver = actor_resolver(&pool, &table);
    assert!(matches!(
        resolver.resolve("nobody", &claims(&[("sub", json!("nobody"))])).await,
        IdentityResolution::Denied(DenyReason::ZeroRows)
    ));

    sqlx::query(&format!("DROP TABLE {table}")).execute(&pool).await.unwrap();
}

#[tokio::test]
async fn pg_store_denies_ambiguous_subject() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP pg_store_denies_ambiguous_subject: no postgres");
        return;
    };
    let table = make_actor_table(&pool).await;
    sqlx::query(&format!(
        "INSERT INTO {table} (sub, actor_id, actor_role) VALUES ('u1', 'a-1', 'manager'), ('u1', 'a-2', 'staff')"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let resolver = actor_resolver(&pool, &table);
    assert!(matches!(
        resolver.resolve("u1", &sub_claims()).await,
        IdentityResolution::Denied(DenyReason::Ambiguous)
    ));

    sqlx::query(&format!("DROP TABLE {table}")).execute(&pool).await.unwrap();
}

#[tokio::test]
async fn pg_store_denies_null_field() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP pg_store_denies_null_field: no postgres");
        return;
    };
    let table = make_actor_table(&pool).await;
    sqlx::query(&format!(
        "INSERT INTO {table} (sub, actor_id, actor_role) VALUES ('u1', 'a-1', NULL)"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let resolver = actor_resolver(&pool, &table);
    match resolver.resolve("u1", &sub_claims()).await {
        IdentityResolution::Denied(DenyReason::NullField(col)) => assert_eq!(col, "actor_role"),
        other => panic!("expected Denied(NullField), got {other:?}"),
    }

    sqlx::query(&format!("DROP TABLE {table}")).execute(&pool).await.unwrap();
}

#[tokio::test]
async fn pg_store_binds_hostile_subject_value_safely() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP pg_store_binds_hostile_subject_value_safely: no postgres");
        return;
    };
    let table = make_actor_table(&pool).await;
    sqlx::query(&format!(
        "INSERT INTO {table} (sub, actor_id, actor_role) VALUES ('u1', 'a-1', 'manager')"
    ))
    .execute(&pool)
    .await
    .unwrap();

    // A classic injection payload as the `sub` value must bind as data (matching
    // no row) — not drop the table.
    let hostile = format!("'; DROP TABLE {table}; --");
    let resolver = actor_resolver(&pool, &table);
    assert!(matches!(
        resolver.resolve(&hostile, &claims(&[("sub", json!(hostile))])).await,
        IdentityResolution::Denied(DenyReason::ZeroRows)
    ));

    // The table survives, proving the value never reached the SQL text.
    let (count,): (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    sqlx::query(&format!("DROP TABLE {table}")).execute(&pool).await.unwrap();
}

// ── #1324: provision on miss against a live Postgres ──────────────────────

/// A `sub`-keyed actor table with the conflict target a provisioning statement
/// needs — the same target any out-of-band writer (an `IdP`'s `user.created`
/// webhook) must use, or the two writers duplicate the row instead of agreeing
/// on it.
async fn make_provisionable_actor_table(pool: &sqlx::PgPool) -> String {
    let table = format!("tb_actor_prov_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!(
        "CREATE TABLE {table} (sub text PRIMARY KEY, actor_id text, actor_role text)"
    ))
    .execute(pool)
    .await
    .unwrap();
    table
}

#[tokio::test]
async fn pg_provision_function_serves_a_new_subject_and_scopes_its_read() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP pg_provision_function_serves_a_new_subject_and_scopes_its_read: no postgres"
        );
        return;
    };
    let suffix = uuid::Uuid::new_v4().simple();
    let actor = make_provisionable_actor_table(&pool).await;
    let item = format!("tb_item_prov_{suffix}");
    let view = format!("v_item_prov_{suffix}");
    let func = format!("fn_provision_actor_{suffix}");

    // The provision function's third parameter is `jsonb`, and it *reads* the
    // claim set to decide the role. A `$claims` bound as text resolves no such
    // function, and a `$claims` the function ignores would leave `actor_role`
    // the same whatever the token said.
    sqlx::query(&format!(
        "CREATE FUNCTION {func}(p_sub text, p_email text, p_claims jsonb) RETURNS void AS $$ \
           INSERT INTO {actor} (sub, actor_id, actor_role) \
           VALUES (p_sub, 'a-' || split_part(p_email, '@', 1), p_claims->>'department') \
           ON CONFLICT (sub) DO NOTHING; \
         $$ LANGUAGE sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!("CREATE TABLE {item} (id int, owner_actor_id text)"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("INSERT INTO {item} VALUES (1,'a-newcomer'),(2,'a-someone-else')"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "CREATE VIEW {view} AS SELECT * FROM {item} \
         WHERE current_setting('app.actor_role', true) = 'manager' \
            OR owner_actor_id = current_setting('app.actor_id', true)"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let mut cfg = config(
        &format!("SELECT actor_id, actor_role FROM {actor} WHERE sub = $sub"),
        &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
    );
    cfg.provision = Some(format!("SELECT {func}($sub, $email, $claims)"));
    let resolver = IdentityResolver::new(cfg, Arc::new(PgIdentityStore::new(pool.clone())));

    // A subject with no row: today's #539 answer is 403 until something outside
    // FraiseQL inserts it.
    let mut ctx = sec_ctx("newcomer-sub", &[("department", json!("staff"))]);
    ctx.email = Some("newcomer@acme.example".to_owned());
    assert_eq!(
        enrich_security_context(&resolver, &mut ctx).await,
        EnrichmentOutcome::Proceed,
        "a brand-new subject is served on its first request"
    );

    // The row is durable, not a value the resolver made up in memory.
    let (count,): (i64,) =
        sqlx::query_as(&format!("SELECT count(*) FROM {actor} WHERE sub = 'newcomer-sub'"))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "the provision statement committed the actor row");

    // And the resolved field reached the query: a bare 200 proves nothing,
    // because a route that reads no enriched field answers 200 regardless.
    assert_eq!(enriched(&ctx, "actor_id"), "a-newcomer");
    assert_eq!(
        enriched(&ctx, "actor_role"),
        "staff",
        "the role came out of the claim set the function read as jsonb"
    );
    assert_eq!(
        count_visible(&pool, &view, &enriched(&ctx, "actor_role"), &enriched(&ctx, "actor_id"))
            .await,
        1,
        "the newly provisioned identity scopes the read to its own row"
    );

    sqlx::query(&format!("DROP VIEW {view}")).execute(&pool).await.unwrap();
    sqlx::query(&format!("DROP TABLE {item}")).execute(&pool).await.unwrap();
    sqlx::query(&format!("DROP FUNCTION {func}(text, text, jsonb)"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP TABLE {actor}")).execute(&pool).await.unwrap();
}

/// Connect with a pool wide enough that N requests really are in flight at once,
/// rather than queueing on `PgPool::connect`'s default ceiling.
async fn connect_wide_pool(max: u32) -> Option<(sqlx::PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    match sqlx::postgres::PgPoolOptions::new()
        .max_connections(max)
        .connect(svc.url())
        .await
    {
        Ok(pool) => Some((pool, svc)),
        Err(e) => {
            eprintln!("SKIP: postgres reachable but connect failed ({e}); skipping");
            None
        },
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pg_sixteen_concurrent_first_requests_produce_one_row() {
    const N: usize = 16;
    const N_U32: u32 = 16;
    const N_I64: i64 = 16;

    let Some((pool, _svc)) = connect_wide_pool(N_U32).await else {
        eprintln!("SKIP pg_sixteen_concurrent_first_requests_produce_one_row: no postgres");
        return;
    };
    let actor = make_provisionable_actor_table(&pool).await;
    let attempts = format!("tb_provision_attempt_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE TABLE {attempts} (sub text)"))
        .execute(&pool)
        .await
        .unwrap();

    let mut cfg = config(
        &format!("SELECT actor_id, actor_role FROM {actor} WHERE sub = $sub"),
        &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
    );
    // The documented contract: idempotent under concurrency. The conflict target
    // is the column holding the `IdP` subject — the same one any out-of-band
    // writer must use, or the two writers duplicate the actor instead of
    // agreeing on it.
    //
    // The `pg_sleep` is what makes this a race rather than sixteen requests that
    // happen to run one after another: it holds every provision open long enough
    // that all N have read zero rows before the first insert commits. The
    // attempt log is the proof — without it, a run where fifteen requests simply
    // found the row already there would look identical.
    cfg.provision = Some(format!(
        "WITH attempt AS ( \
             INSERT INTO {attempts} (sub) SELECT $sub FROM (SELECT pg_sleep(0.5)) s \
             RETURNING sub \
         ) \
         INSERT INTO {actor} (sub, actor_id, actor_role) \
         SELECT sub, 'a-' || sub, 'staff' FROM attempt \
         ON CONFLICT (sub) DO NOTHING"
    ));
    let resolver =
        Arc::new(IdentityResolver::new(cfg, Arc::new(PgIdentityStore::new(pool.clone()))));

    let barrier = Arc::new(tokio::sync::Barrier::new(N));
    let mut tasks = Vec::with_capacity(N);
    for _ in 0..N {
        let resolver = Arc::clone(&resolver);
        let barrier = Arc::clone(&barrier);
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            resolver.resolve("u-storm", &claims(&[("sub", json!("u-storm"))])).await
        }));
    }

    for task in tasks {
        match task.await.unwrap() {
            IdentityResolution::Resolved(map) => assert_eq!(map["actor_id"], "a-u-storm"),
            other => panic!("every concurrent first request must be served, got {other:?}"),
        }
    }

    let (attempted,): (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM {attempts}"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        attempted, N_I64,
        "all {N} requests must have found zero rows and provisioned — a run where some found \
         the row already committed would prove nothing about the race"
    );
    let (count,): (i64,) =
        sqlx::query_as(&format!("SELECT count(*) FROM {actor} WHERE sub = 'u-storm'"))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "{N} provisions, one actor");

    sqlx::query(&format!("DROP TABLE {attempts}")).execute(&pool).await.unwrap();
    sqlx::query(&format!("DROP TABLE {actor}")).execute(&pool).await.unwrap();
}

#[tokio::test]
async fn pg_a_bare_insert_statement_provisions_too() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP pg_a_bare_insert_statement_provisions_too: no postgres");
        return;
    };
    let actor = make_provisionable_actor_table(&pool).await;

    // The issue's documented contract is an `INSERT … ON CONFLICT DO NOTHING`.
    // A data-modifying statement cannot be a `FROM` sub-query, so this only runs
    // if provisioning has an execute path of its own rather than reusing the
    // row-returning wrapper.
    let mut cfg = config(
        &format!("SELECT actor_id, actor_role FROM {actor} WHERE sub = $sub"),
        &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
    );
    cfg.provision = Some(format!(
        "INSERT INTO {actor} (sub, actor_id, actor_role) VALUES ($sub, 'a-' || $sub, 'staff') \
         ON CONFLICT (sub) DO NOTHING"
    ));
    let resolver = IdentityResolver::new(cfg, Arc::new(PgIdentityStore::new(pool.clone())));

    match resolver.resolve("u-insert", &claims(&[("sub", json!("u-insert"))])).await {
        IdentityResolution::Resolved(map) => assert_eq!(map["actor_id"], "a-u-insert"),
        other => panic!("expected Resolved after an INSERT provision, got {other:?}"),
    }

    sqlx::query(&format!("DROP TABLE {actor}")).execute(&pool).await.unwrap();
}

// ── Consumer A: enrich_security_context + config (DESIGN §3, §7) ───────────

fn sec_ctx(sub: &str, attrs: &[(&str, Value)]) -> SecurityContext {
    SecurityContext {
        user_id:          UserId::new(sub),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       attrs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
        request_id:       "req-test".to_owned(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// A store that records the binds it received (to prove `claims_for_binding`
/// surfaces the subject) and returns a fixed row set.
struct CapturingStore {
    rows:     Vec<serde_json::Map<String, Value>>,
    captured: std::sync::Mutex<Vec<Value>>,
}

impl IdentityStore for CapturingStore {
    fn fetch_rows<'a>(
        &'a self,
        _sql: &'a str,
        binds: &'a [Value],
    ) -> BoxFuture<'a, Result<Vec<serde_json::Map<String, Value>>, ResolveError>> {
        *self.captured.lock().unwrap() = binds.to_vec();
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }

    fn execute<'a>(
        &'a self,
        _sql: &'a str,
        _binds: &'a [Value],
    ) -> BoxFuture<'a, Result<(), ResolveError>> {
        panic!("the capturing store is used by profiles that configure no `provision`")
    }
}

fn actor_row() -> serde_json::Map<String, Value> {
    row(&[("actor_id", json!("a-1")), ("actor_role", json!("manager"))])
}

#[tokio::test]
async fn enrich_resolved_merges_under_reserved_namespace() {
    let mut ctx = sec_ctx("u1", &[]);
    let resolver = resolver(MockStore::returning(vec![actor_row()]));

    assert_eq!(enrich_security_context(&resolver, &mut ctx).await, EnrichmentOutcome::Proceed);
    assert_eq!(ctx.attributes[&format!("{ENRICHED_NAMESPACE_PREFIX}actor_role")], "manager");
    assert_eq!(ctx.attributes[&format!("{ENRICHED_NAMESPACE_PREFIX}actor_id")], "a-1");
}

#[tokio::test]
async fn enrich_denied_merges_nothing() {
    let mut ctx = sec_ctx("u1", &[]);
    let resolver = resolver(MockStore::returning(vec![])); // zero rows → Denied

    assert_eq!(enrich_security_context(&resolver, &mut ctx).await, EnrichmentOutcome::Denied);
    assert!(
        ctx.attributes.keys().all(|k| !k.starts_with(ENRICHED_NAMESPACE_PREFIX)),
        "a denial must merge nothing"
    );
}

#[tokio::test]
async fn enrich_unavailable_maps_to_unavailable() {
    let mut ctx = sec_ctx("u1", &[]);
    let resolver = resolver(MockStore::failing());

    assert_eq!(
        enrich_security_context(&resolver, &mut ctx).await,
        EnrichmentOutcome::Unavailable
    );
}

#[tokio::test]
async fn enrich_binds_subject_from_context() {
    let store = Arc::new(CapturingStore {
        rows:     vec![actor_row()],
        captured: std::sync::Mutex::new(Vec::new()),
    });
    let resolver = IdentityResolver::new(
        config(
            "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub",
            &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
        ),
        store.clone(),
    );
    let mut ctx = sec_ctx("subject-42", &[]);

    let _ = enrich_security_context(&resolver, &mut ctx).await;

    // The subject from the context bound `$sub` — the read scopes on a
    // DB-derived identity, not a client-asserted one.
    assert_eq!(*store.captured.lock().unwrap(), vec![json!("subject-42")]);
}

#[test]
fn identity_config_deserializes_from_toml() {
    let toml_src = r#"
[enrichment]
enabled = true
query = "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub"
map = { actor_id = "actor_id", actor_role = "actor_role" }
cache_ttl_secs = 30
"#;
    let cfg: IdentityConfig = toml::from_str(toml_src).unwrap();
    let enrichment = cfg.enrichment.unwrap();
    assert!(enrichment.enabled);
    assert_eq!(enrichment.cache_ttl_secs, 30);
    assert_eq!(enrichment.negative_ttl_secs, 5, "negative TTL defaults to 5s");
    assert_eq!(enrichment.map["actor_role"], "actor_role");
    assert!(cfg.sender.is_none());
}

#[test]
fn identity_config_rejects_unknown_field() {
    // deny_unknown_fields makes a mistyped/stranded key fail loud — the failure
    // mode that hid #242's absence (DESIGN §7).
    let toml_src = r#"
[enrichment]
enabled = true
query = "SELECT 1"
typo_field = "oops"
"#;
    assert!(toml::from_str::<IdentityConfig>(toml_src).is_err());
}

// ── Consumer B: DB-backed sender identity (DESIGN §4) ─────────────────────

use fraiseql_functions::SenderIdentityResolver;

use super::sender::DbSenderIdentityResolver;

fn sender_resolver(store: MockStore, display: bool) -> DbSenderIdentityResolver {
    let map: &[(&str, &str)] = if display {
        &[
            ("sending_address", "sending_address"),
            ("display_name", "display_name"),
        ]
    } else {
        &[("sending_address", "sending_address")]
    };
    let resolver = IdentityResolver::new(
        config(
            "SELECT sending_address, display_name FROM tb_sales_mailbox WHERE sub = $sub",
            map,
        ),
        Arc::new(store),
    );
    DbSenderIdentityResolver::new(
        resolver,
        "sending_address",
        display.then(|| "display_name".to_owned()),
    )
}

#[tokio::test]
async fn db_sender_resolves_verified_address_not_login_email() {
    let store = MockStore::returning(vec![row(&[
        ("sending_address", json!("sales@acme.example")),
        ("display_name", json!("Acme Sales")),
    ])]);
    let sender = sender_resolver(store, true);
    // The auth context's login email differs from the verified sending mailbox.
    let auth = json!({ "sub": "u1", "email": "rep.personal@acme.example" });

    let identity = sender.resolve_sender(&auth).await.unwrap();
    assert_eq!(identity.address, "sales@acme.example");
    assert_eq!(identity.display_name.as_deref(), Some("Acme Sales"));
}

#[tokio::test]
async fn db_sender_denies_unprovisioned_subject() {
    let sender = sender_resolver(MockStore::returning(vec![]), false);
    let auth = json!({ "sub": "nobody" });

    assert!(
        sender.resolve_sender(&auth).await.is_err(),
        "an unprovisioned subject must refuse, never fall back to a shared mailbox"
    );
}

#[tokio::test]
async fn db_sender_refuses_without_a_subject() {
    let sender = sender_resolver(MockStore::returning(vec![]), false);
    let auth = json!({ "email": "someone@acme.example" }); // no `sub`

    assert!(sender.resolve_sender(&auth).await.is_err());
}

#[tokio::test]
async fn db_sender_refuses_a_malformed_resolved_address() {
    let store = MockStore::returning(vec![row(&[("sending_address", json!("not-an-email"))])]);
    let sender = sender_resolver(store, false);
    let auth = json!({ "sub": "u1" });

    assert!(sender.resolve_sender(&auth).await.is_err());
}

// ── Acceptance: the two-role read boundary (DESIGN §9, self-contained) ─────
//
// The partner runs §9 against their live two-role setup at P04; this is the
// self-contained local proof of the same property. It exercises the real chain:
// enrich a known subject against a real `tb_actor`, then apply the enriched
// values as GUCs on a connection (exactly the keys the `Enrichment` session-var
// source reads) and assert RLS-view visibility.

/// Count rows visible under the enriched GUCs, transaction-locally so no GUC
/// leaks onto a pooled connection.
async fn count_visible(pool: &sqlx::PgPool, view: &str, role: &str, actor_id: &str) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.actor_role', $1, true)")
        .bind(role)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('app.actor_id', $1, true)")
        .bind(actor_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let (count,): (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM {view}"))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    count
}

fn enriched(ctx: &SecurityContext, field: &str) -> String {
    ctx.attributes[&format!("{ENRICHED_NAMESPACE_PREFIX}{field}")]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn acceptance_two_role_read_boundary() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP acceptance_two_role_read_boundary: no postgres");
        return;
    };
    let suffix = uuid::Uuid::new_v4().simple();
    let actor = format!("tb_actor_acc_{suffix}");
    let item = format!("tb_item_acc_{suffix}");
    let view = format!("v_item_acc_{suffix}");

    sqlx::query(&format!("CREATE TABLE {actor} (sub text, actor_id text, actor_role text)"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "INSERT INTO {actor} VALUES ('admin-sub','a-admin','manager'), ('staff-sub','a-staff','staff')"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!("CREATE TABLE {item} (id int, owner_actor_id text)"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("INSERT INTO {item} VALUES (1,'a-admin'),(2,'a-staff'),(3,'a-other')"))
        .execute(&pool)
        .await
        .unwrap();
    // The view scopes exactly as the partner's does: a `manager` sees all, any
    // other role sees only its own rows.
    sqlx::query(&format!(
        "CREATE VIEW {view} AS SELECT * FROM {item} \
         WHERE current_setting('app.actor_role', true) = 'manager' \
            OR owner_actor_id = current_setting('app.actor_id', true)"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let resolver = actor_resolver(&pool, &actor);

    // §9.1 — known sub → role-A (manager, unfiltered): sees all rows.
    let mut ctx_admin = sec_ctx("admin-sub", &[]);
    assert_eq!(
        enrich_security_context(&resolver, &mut ctx_admin).await,
        EnrichmentOutcome::Proceed
    );
    assert_eq!(
        count_visible(
            &pool,
            &view,
            &enriched(&ctx_admin, "actor_role"),
            &enriched(&ctx_admin, "actor_id")
        )
        .await,
        3,
        "the manager role sees every row"
    );

    // §9.2 — known sub → role-B (staff, own-scope): sees only its own row.
    let mut ctx_staff = sec_ctx("staff-sub", &[]);
    assert_eq!(
        enrich_security_context(&resolver, &mut ctx_staff).await,
        EnrichmentOutcome::Proceed
    );
    assert_eq!(
        count_visible(
            &pool,
            &view,
            &enriched(&ctx_staff, "actor_role"),
            &enriched(&ctx_staff, "actor_id")
        )
        .await,
        1,
        "the staff role sees only its own row"
    );

    // §9.3 — unknown sub → DENIED (fail-closed before dispatch), NOT an empty set.
    let mut ctx_unknown = sec_ctx("nobody", &[]);
    assert_eq!(
        enrich_security_context(&resolver, &mut ctx_unknown).await,
        EnrichmentOutcome::Denied
    );

    sqlx::query(&format!("DROP VIEW {view}")).execute(&pool).await.unwrap();
    sqlx::query(&format!("DROP TABLE {item}")).execute(&pool).await.unwrap();
    sqlx::query(&format!("DROP TABLE {actor}")).execute(&pool).await.unwrap();
}

// ── Admin flush endpoint (the immediate #539 follow-up) ────────────────────

fn actor_resolver_arc() -> std::sync::Arc<IdentityResolver> {
    std::sync::Arc::new(IdentityResolver::new(
        config(
            "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub",
            &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
        ),
        Arc::new(MockStore::returning(vec![actor_row()])),
    ))
}

#[tokio::test]
async fn identity_admin_router_constructs() {
    // axum validates path-capture syntax inside `Router::route`, so a bad literal
    // would panic here at build time (issue #316 prevention).
    let _ = identity_admin_router(actor_resolver_arc());
}

#[tokio::test]
async fn flush_endpoint_evicts_the_subject() {
    let store = Arc::new(MockStore::returning(vec![actor_row()]));
    let resolver = Arc::new(IdentityResolver::new(
        config(
            "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub",
            &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
        ),
        store.clone(),
    ));

    // Warm the cache, then flush the subject through the HTTP handler.
    let _ = resolver.resolve("u1", &sub_claims()).await;
    assert_eq!(store.calls(), 1);

    let response = identity_admin_router(resolver.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/identity/flush")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"sub":"u1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // The next resolve re-hits the store — the entry was evicted.
    let _ = resolver.resolve("u1", &sub_claims()).await;
    assert_eq!(store.calls(), 2, "flush must evict the subject's cache entry");
}

#[tokio::test]
async fn flush_all_endpoint_clears_the_cache() {
    let store = Arc::new(MockStore::returning(vec![actor_row()]));
    let resolver = Arc::new(IdentityResolver::new(
        config(
            "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub",
            &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
        ),
        store.clone(),
    ));

    let _ = resolver.resolve("u1", &sub_claims()).await;

    let response = identity_admin_router(resolver.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/identity/flush-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let _ = resolver.resolve("u1", &sub_claims()).await;
    assert_eq!(store.calls(), 2, "flush-all must clear the cache");
}

// ── #1324: provision on miss — configuration ──────────────────────────────

#[test]
fn provision_parses_on_the_enrichment_profile() {
    let toml_src = r#"
[enrichment]
enabled = true
query = "SELECT actor_id FROM tb_actor WHERE sub = $sub"
provision = "SELECT fn_provision_actor($sub, $iss, $email, $claims)"
map = { actor_id = "actor_id" }
"#;
    let cfg: IdentityConfig = toml::from_str(toml_src).unwrap();
    assert_eq!(
        cfg.enrichment.unwrap().provision.as_deref(),
        Some("SELECT fn_provision_actor($sub, $iss, $email, $claims)")
    );
}

#[test]
fn a_profile_without_provision_carries_none() {
    let toml_src = r#"
[enrichment]
enabled = true
query = "SELECT actor_id FROM tb_actor WHERE sub = $sub"
"#;
    let cfg: IdentityConfig = toml::from_str(toml_src).unwrap();
    assert!(
        cfg.enrichment.unwrap().provision.is_none(),
        "absent means absent — the provisioning path must be unreachable by default"
    );
}

#[test]
fn provision_on_the_sender_profile_is_refused_naming_the_profile() {
    // The two profiles share one query schema, so `provision` parses on the
    // sender profile — where it would silently provision a *sending mailbox* on
    // every unknown subject. The refusal has to name which profile is wrong and
    // where the key belongs, because the operator's fix is to move it.
    let cfg: IdentityConfig = toml::from_str(
        r#"
[sender]
enabled = true
query = "SELECT sending_address FROM tb_mailbox WHERE sub = $sub"
provision = "INSERT INTO tb_mailbox (sub) VALUES ($sub)"
"#,
    )
    .unwrap();

    let err = cfg.validate().expect_err("a provisioning sender profile must be refused");
    assert!(err.contains("[identity.sender]"), "names the offending profile: {err}");
    assert!(err.contains("[identity.enrichment]"), "and where the key belongs: {err}");
}

#[test]
fn provision_on_the_enrichment_profile_validates() {
    let cfg: IdentityConfig = toml::from_str(
        r#"
[enrichment]
enabled = true
query = "SELECT actor_id FROM tb_actor WHERE sub = $sub"
provision = "INSERT INTO tb_actor (sub) VALUES ($sub) ON CONFLICT (sub) DO NOTHING"
"#,
    )
    .unwrap();

    assert!(cfg.validate().is_ok());
}

/// A resolver whose profile also carries a `provision` statement.
fn provisioning_resolver(store: Arc<MockStore>, provision: &str) -> IdentityResolver {
    let mut cfg = config(
        "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub",
        &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
    );
    cfg.provision = Some(provision.to_owned());
    IdentityResolver::new(cfg, store)
}

fn provisioned_actor() -> Vec<serde_json::Map<String, Value>> {
    vec![row(&[
        ("actor_id", json!("a-new")),
        ("actor_role", json!("staff")),
    ])]
}

#[tokio::test]
async fn a_zero_row_miss_provisions_and_re_resolves() {
    let store = Arc::new(MockStore::returning(vec![]).provisioning_to(provisioned_actor()));
    let resolver = provisioning_resolver(
        store.clone(),
        "INSERT INTO tb_actor (sub) VALUES ($sub) ON CONFLICT (sub) DO NOTHING",
    );

    match resolver.resolve("u-new", &claims(&[("sub", json!("u-new"))])).await {
        IdentityResolution::Resolved(map) => assert_eq!(map["actor_id"], "a-new"),
        other => panic!("expected Resolved after provisioning, got {other:?}"),
    }
    assert_eq!(store.provisions(), 1, "the statement ran once");
    assert_eq!(
        store.calls(),
        2,
        "and the query ran again after it — the re-read is what decides the outcome"
    );
}

#[tokio::test]
async fn the_provision_statement_is_bound_from_the_claims_not_interpolated() {
    let store = Arc::new(MockStore::returning(vec![]).provisioning_to(provisioned_actor()));
    let resolver = provisioning_resolver(store.clone(), "SELECT fn_provision_actor($sub, $email)");

    let _ = resolver
        .resolve(
            "u-new",
            &claims(&[("sub", json!("u-new")), ("email", json!("new@example.com"))]),
        )
        .await;

    let executed = store.executed();
    assert_eq!(executed.len(), 1);
    let (sql, binds) = &executed[0];
    assert_eq!(
        sql, "SELECT fn_provision_actor($1, $2)",
        "the statement reaches the store with positional placeholders, never claim text"
    );
    assert_eq!(binds, &vec![json!("u-new"), json!("new@example.com")]);
}

#[tokio::test]
async fn a_provision_statement_referencing_an_absent_claim_denies() {
    let store = Arc::new(MockStore::returning(vec![]).provisioning_to(provisioned_actor()));
    let resolver = provisioning_resolver(store.clone(), "SELECT fn_provision_actor($sub, $org_id)");

    match resolver.resolve("u-new", &claims(&[("sub", json!("u-new"))])).await {
        IdentityResolution::Denied(DenyReason::MissingParam(name)) => assert_eq!(name, "org_id"),
        other => panic!("expected Denied(MissingParam), got {other:?}"),
    }
    assert_eq!(store.provisions(), 0, "an unbindable statement never reaches the database");
}

#[tokio::test]
async fn without_a_provision_statement_a_zero_row_miss_is_unchanged() {
    // The pin that the whole path is unreachable by default: same store, same
    // claims, no `provision` key.
    let store = Arc::new(MockStore::returning(vec![]).provisioning_to(provisioned_actor()));
    let resolver = IdentityResolver::new(
        config(
            "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub",
            &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
        ),
        store.clone(),
    );

    assert!(matches!(
        resolver.resolve("u-new", &claims(&[("sub", json!("u-new"))])).await,
        IdentityResolution::Denied(DenyReason::ZeroRows)
    ));
    assert_eq!(store.provisions(), 0);
    assert_eq!(store.calls(), 1);
}

#[tokio::test]
async fn a_request_arriving_while_another_provisions_is_not_refused() {
    // The race #1324 has to survive: an IdP hands a browser its token, the app
    // opens several requests at once, and none of them has a row yet.
    let gate = Arc::new(ProvisionGate::new());
    let store = Arc::new(
        MockStore::returning(vec![])
            .provisioning_to(provisioned_actor())
            .gated(Arc::clone(&gate)),
    );
    let resolver =
        Arc::new(provisioning_resolver(Arc::clone(&store), "SELECT fn_provision_actor($sub)"));

    let first = tokio::spawn({
        let resolver = Arc::clone(&resolver);
        async move { resolver.resolve("u-new", &claims(&[("sub", json!("u-new"))])).await }
    });
    gate.wait_until_provisioning().await;

    // The first request is *inside* its provision statement. Nothing it has done
    // on the way there may make this one fail closed — a `Denied(ZeroRows)`
    // cached before provisioning would 403 every concurrent request, and go on
    // doing it for the rest of `negative_ttl_secs`.
    match resolver.resolve("u-new", &claims(&[("sub", json!("u-new"))])).await {
        IdentityResolution::Resolved(map) => assert_eq!(map["actor_id"], "a-new"),
        other => panic!("a concurrent first request must not be refused, got {other:?}"),
    }

    gate.release();
    assert!(
        matches!(first.await.unwrap(), IdentityResolution::Resolved(_)),
        "and the request that did the provisioning is served too"
    );
    assert_eq!(
        store.provisions(),
        2,
        "each provisioned; the statement is what must be idempotent"
    );
}

#[tokio::test]
async fn a_provision_that_raises_is_an_outage_and_is_never_cached() {
    let store = Arc::new(MockStore::returning(vec![]).failing_provision());
    let resolver = provisioning_resolver(Arc::clone(&store), "SELECT fn_provision_actor($sub)");

    // 503, not 403: the operator's database said no, and the subject may well be
    // legitimate. Answering `Denied` here would negative-cache an outage.
    assert!(matches!(
        resolver.resolve("u-new", &claims(&[("sub", json!("u-new"))])).await,
        IdentityResolution::Unavailable(_)
    ));
    assert!(matches!(
        resolver.resolve("u-new", &claims(&[("sub", json!("u-new"))])).await,
        IdentityResolution::Unavailable(_)
    ));
    assert_eq!(store.provisions(), 2, "a blip must not pin a denial — the next request retries");
}

#[tokio::test]
async fn a_provision_that_inserts_nothing_refuses_once_per_negative_ttl() {
    // The statement *is* the policy: to refuse a subject it inserts nothing, and
    // the re-read denies. Without the negative cache that refusal would be one
    // database write per request, for as long as the token is valid.
    let store = Arc::new(MockStore::returning(vec![]));
    let resolver = provisioning_resolver(
        Arc::clone(&store),
        "INSERT INTO tb_actor (sub) SELECT $sub WHERE false",
    );

    for _ in 0..4 {
        assert!(matches!(
            resolver.resolve("u-refused", &claims(&[("sub", json!("u-refused"))])).await,
            IdentityResolution::Denied(DenyReason::ZeroRows)
        ));
    }
    assert_eq!(store.provisions(), 1, "the post-provision denial is what gets cached");
    assert_eq!(store.calls(), 2, "and no further reads either");
}

#[tokio::test]
async fn after_the_negative_ttl_a_refused_subject_is_offered_again() {
    // The other side of the window: the refusal is bounded, so a subject the
    // statement declines today is not locked out by a cache entry that never
    // expires.
    let store = Arc::new(MockStore::returning(vec![]));
    let mut cfg = config(
        "SELECT actor_id, actor_role FROM tb_actor WHERE sub = $sub",
        &[("actor_id", "actor_id"), ("actor_role", "actor_role")],
    );
    cfg.provision = Some("INSERT INTO tb_actor (sub) SELECT $sub WHERE false".to_owned());
    // Already elapsed by the next monotonic `get` (strict `<`), as in
    // `cache_expired_entry_returns_none`.
    cfg.negative_ttl_secs = 0;
    let resolver = IdentityResolver::new(cfg, store.clone() as Arc<dyn IdentityStore>);

    let _ = resolver.resolve("u-refused", &claims(&[("sub", json!("u-refused"))])).await;
    let _ = resolver.resolve("u-refused", &claims(&[("sub", json!("u-refused"))])).await;

    assert_eq!(store.provisions(), 2, "an expired refusal is offered the statement again");
}

#[tokio::test]
async fn a_provisioned_row_with_a_null_mapped_field_still_refuses() {
    // Provisioning writes the row; it does not lower the bar the row must clear.
    // A half-built actor is exactly the empty-string GUC the failure model exists
    // to prevent.
    let store = Arc::new(MockStore::returning(vec![]).provisioning_to(vec![row(&[
        ("actor_id", json!("a-new")),
        ("actor_role", Value::Null),
    ])]));
    let resolver = provisioning_resolver(Arc::clone(&store), "SELECT fn_provision_actor($sub)");

    match resolver.resolve("u-new", &claims(&[("sub", json!("u-new"))])).await {
        IdentityResolution::Denied(DenyReason::NullField(col)) => assert_eq!(col, "actor_role"),
        other => panic!("expected Denied(NullField), got {other:?}"),
    }
}

#[tokio::test]
async fn a_denial_that_is_not_zero_rows_never_provisions() {
    // The rule that keeps provisioning from inverting the failure model: these
    // are the denials of an identity that already exists, and no statement may
    // be given the chance to overwrite it into one that resolves.
    let ambiguous = Arc::new(MockStore::returning(vec![
        row(&[("actor_id", json!("a-1")), ("actor_role", json!("manager"))]),
        row(&[("actor_id", json!("a-2")), ("actor_role", json!("staff"))]),
    ]));
    let resolver = provisioning_resolver(Arc::clone(&ambiguous), "SELECT fn_provision_actor($sub)");
    assert!(matches!(
        resolver.resolve("u1", &sub_claims()).await,
        IdentityResolution::Denied(DenyReason::Ambiguous)
    ));
    assert_eq!(ambiguous.provisions(), 0, "an ambiguous identity is not a missing one");

    let null_field = Arc::new(MockStore::returning(vec![row(&[
        ("actor_id", json!("a-1")),
        ("actor_role", Value::Null),
    ])]));
    let resolver =
        provisioning_resolver(Arc::clone(&null_field), "SELECT fn_provision_actor($sub)");
    assert!(matches!(
        resolver.resolve("u1", &sub_claims()).await,
        IdentityResolution::Denied(DenyReason::NullField(_))
    ));
    assert_eq!(null_field.provisions(), 0, "a half-built actor is not a missing one");

    let missing_param = Arc::new(MockStore::returning(vec![]));
    let mut cfg = config(
        "SELECT actor_id FROM tb_actor WHERE sub = $sub AND org = $org_id",
        &[("actor_id", "actor_id")],
    );
    cfg.provision = Some("SELECT fn_provision_actor($sub)".to_owned());
    let resolver = IdentityResolver::new(cfg, missing_param.clone() as Arc<dyn IdentityStore>);
    assert!(matches!(
        resolver.resolve("u1", &sub_claims()).await,
        IdentityResolution::Denied(DenyReason::MissingParam(_))
    ));
    assert_eq!(missing_param.provisions(), 0, "a query that cannot be bound never ran");
    assert_eq!(missing_param.calls(), 0, "and never reached the database at all");
}

#[tokio::test]
async fn the_request_after_a_successful_provision_is_a_positive_cache_hit() {
    // The issue's fourth gate item, re-derived. It is written as "a subject
    // negative-cached before provisioning resolves immediately after", which
    // presumes the pre-provision miss is cached; it is not, so there is no
    // negative entry to clear. The property that item protects is asserted
    // directly instead: the next request costs nothing and is still enriched.
    let store = Arc::new(MockStore::returning(vec![]).provisioning_to(provisioned_actor()));
    let resolver = provisioning_resolver(Arc::clone(&store), "SELECT fn_provision_actor($sub)");

    let mut first = sec_ctx("u-new", &[]);
    assert_eq!(enrich_security_context(&resolver, &mut first).await, EnrichmentOutcome::Proceed);
    let reads_to_provision = store.calls();

    let mut second = sec_ctx("u-new", &[]);
    assert_eq!(
        enrich_security_context(&resolver, &mut second).await,
        EnrichmentOutcome::Proceed
    );
    assert_eq!(enriched(&second, "actor_id"), "a-new");
    assert_eq!(
        enriched(&second, "actor_role"),
        "staff",
        "a cache hit carries the whole mapped set, not a bare permission to proceed"
    );

    assert_eq!(store.calls(), reads_to_provision, "the second request read nothing");
    assert_eq!(store.provisions(), 1, "and no denial survived under that subject's tuple");
}

#[tokio::test]
async fn the_claims_parameter_binds_the_whole_verified_claim_set() {
    let store = Arc::new(MockStore::returning(vec![]).provisioning_to(provisioned_actor()));
    let resolver = provisioning_resolver(store.clone(), "SELECT fn_provision_actor($claims)");

    let mut ctx = sec_ctx("u-new", &[("department", json!("sales"))]);
    ctx.email = Some("new@example.com".to_owned());
    assert_eq!(enrich_security_context(&resolver, &mut ctx).await, EnrichmentOutcome::Proceed);

    let executed = store.executed();
    let bound = &executed[0].1[0];
    assert_eq!(bound["sub"], "u-new", "the well-known fields are in it: {bound}");
    assert_eq!(bound["email"], "new@example.com", "including the ones off the context: {bound}");
    assert_eq!(bound["department"], "sales", "and every forwarded attribute: {bound}");
    assert!(
        bound.get("claims").is_none(),
        "but not itself — the snapshot is of the claims, not of the binding: {bound}"
    );
}

#[test]
fn server_config_validate_refuses_a_provisioning_sender_profile() {
    // The rule is only worth anything if boot runs it: `ServerConfig::validate`
    // is the one call `main` makes before serving.
    let config = crate::ServerConfig {
        identity: Some(
            toml::from_str(
                r#"
[sender]
enabled = true
query = "SELECT sending_address FROM tb_mailbox WHERE sub = $sub"
provision = "INSERT INTO tb_mailbox (sub) VALUES ($sub)"
"#,
            )
            .unwrap(),
        ),
        ..Default::default()
    };

    let err = config.validate().expect_err("boot must refuse it");
    assert!(err.contains("provision"), "the boot refusal is the identity one: {err}");
}
