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

use chrono::Utc;
use fraiseql_core::{
    security::{ENRICHED_NAMESPACE_PREFIX, SecurityContext},
    types::UserId,
};
use serde_json::{Value, json};

use super::{
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
        Duration::from_secs(60),
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
        Duration::from_secs(60),
    );
    cache.insert(
        "[\"u1\",\"x\"]".to_owned(),
        "u1".to_owned(),
        resolved("r", json!("a")),
        Duration::from_secs(60),
    );
    cache.insert(
        "[\"u2\"]".to_owned(),
        "u2".to_owned(),
        resolved("r", json!("b")),
        Duration::from_secs(60),
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
        Duration::from_secs(60),
    );
    cache.insert(
        "[\"u2\"]".to_owned(),
        "u2".to_owned(),
        resolved("r", json!("b")),
        Duration::from_secs(60),
    );

    cache.flush_all();

    assert_eq!(cache.len(), 0);
}

// ── Failure model against a mock store (DESIGN §5) ────────────────────────

/// A store that returns a fixed row set (or a transient error) and counts calls,
/// so tests can assert both the classification and the caching behaviour.
struct MockStore {
    rows:  Vec<serde_json::Map<String, Value>>,
    fail:  Option<ResolveError>,
    calls: AtomicUsize,
}

impl MockStore {
    fn returning(rows: Vec<serde_json::Map<String, Value>>) -> Self {
        Self {
            rows,
            fail: None,
            calls: AtomicUsize::new(0),
        }
    }

    fn failing() -> Self {
        Self {
            rows:  Vec::new(),
            fail:  Some(ResolveError::new("db unreachable")),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl IdentityStore for MockStore {
    fn fetch_rows<'a>(
        &'a self,
        _sql: &'a str,
        _binds: &'a [Value],
    ) -> BoxFuture<'a, Result<Vec<serde_json::Map<String, Value>>, ResolveError>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let result = self.fail.clone().map_or_else(|| Ok(self.rows.clone()), Err);
        Box::pin(async move { result })
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
/// so the test skips cleanly.
async fn connect_pool() -> Option<(sqlx::PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = sqlx::PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
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
