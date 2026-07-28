//! H16 — server constructor drift regression guards.
//!
//! Every public `Server` constructor that boots an executor must route through
//! the single `RuntimeConfig::from_compiled_schema` seam, so it cannot skip the
//! schema-format-version validation (or the audit / page-size / change-log
//! config) the way `with_relay_pagination` and `with_flight_service` previously
//! did by building the executor with `RuntimeConfig::default()`.
//!
//! These tests assert the headline regression: an incompatible compiled-schema
//! format version is now refused by *every* constructor, not just `Server::new`.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test functions, panics are expected
#![allow(clippy::unimplemented)] // Reason: the relay query stub is never invoked by these tests
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        DatabaseAdapter, DatabaseType, SupportsMutations, WhereClause,
        traits::{CursorValue, RelayDatabaseAdapter, RelayPageResult},
        types::{JsonbValue, OrderByClause, PoolMetrics},
    },
    error::Result as FraiseQLResult,
    schema::{
        CURRENT_SCHEMA_FORMAT_VERSION, CompiledSchema, SecurityConfig, SqlProjectionHint,
        TenancyConfig, TenancyMode,
    },
};
use fraiseql_server::{Server, server_config::ServerConfig};

/// A relay-capable no-op adapter. Its query methods are never invoked by these
/// tests — construction either fails fast on the format-version check or builds
/// the server without running a query — so the bodies are trivial/unimplemented.
#[derive(Debug, Clone)]
struct NoopRelayAdapter;

#[async_trait]
impl DatabaseAdapter for NoopRelayAdapter {
    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> FraiseQLResult<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> FraiseQLResult<Vec<JsonbValue>> {
        Ok(vec![])
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SQLite
    }

    async fn health_check(&self) -> FraiseQLResult<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics::default()
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> FraiseQLResult<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> FraiseQLResult<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

impl SupportsMutations for NoopRelayAdapter {}

// `RelayDatabaseAdapter` is a native RPIT async trait (no `#[async_trait]`).
impl RelayDatabaseAdapter for NoopRelayAdapter {
    #[allow(clippy::too_many_arguments)] // Reason: mirrors the trait's full cursor/filter/sort signature
    async fn execute_relay_page(
        &self,
        _view: &str,
        _cursor_column: &str,
        _after: Option<CursorValue>,
        _before: Option<CursorValue>,
        _limit: u32,
        _forward: bool,
        _where_clause: Option<&WhereClause>,
        _order_by: Option<&[OrderByClause]>,
        _include_total_count: bool,
    ) -> FraiseQLResult<RelayPageResult> {
        unimplemented!("relay queries are never executed in constructor-drift tests")
    }
}

fn incompatible_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.schema_format_version = Some(CURRENT_SCHEMA_FORMAT_VERSION + 1);
    schema
}

fn current_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.schema_format_version = Some(CURRENT_SCHEMA_FORMAT_VERSION);
    schema
}

/// A schema that declares row-level tenancy and nothing else. With caching on,
/// every constructor that builds a result cache must refuse it: cache keys carry no
/// tenant, so without RLS two tenants issuing the same query share one entry.
fn multi_tenant_schema_without_rls() -> CompiledSchema {
    let mut schema = current_schema();
    schema.security = Some(SecurityConfig {
        tenancy: TenancyConfig {
            mode: TenancyMode::Row,
            ..TenancyConfig::default()
        },
        ..SecurityConfig::default()
    });
    schema
}

fn caching_config() -> ServerConfig {
    ServerConfig {
        cache_enabled: true,
        ..ServerConfig::default()
    }
}

#[tokio::test]
async fn server_new_refuses_incompatible_schema_format() {
    let config = ServerConfig {
        cache_enabled: false,
        ..ServerConfig::default()
    };
    let result = Server::new(config, incompatible_schema(), Arc::new(NoopRelayAdapter), None).await;
    assert!(
        result.is_err(),
        "Server::new must refuse an incompatible compiled-schema version"
    );
}

#[tokio::test]
async fn with_relay_pagination_refuses_incompatible_schema_format() {
    // H16 regression: the relay constructor previously built the executor with
    // RuntimeConfig::default() and so never validated the schema format version.
    let config = ServerConfig {
        cache_enabled: false,
        ..ServerConfig::default()
    };
    let result = Server::with_relay_pagination(
        config,
        incompatible_schema(),
        Arc::new(NoopRelayAdapter),
        None,
    )
    .await;
    assert!(
        result.is_err(),
        "with_relay_pagination must refuse an incompatible compiled-schema version (H16)"
    );
}

#[tokio::test]
async fn with_relay_pagination_accepts_current_schema_format() {
    // Guard against over-rejection: a current-version schema still boots.
    let config = ServerConfig {
        cache_enabled: false,
        ..ServerConfig::default()
    };
    let result =
        Server::with_relay_pagination(config, current_schema(), Arc::new(NoopRelayAdapter), None)
            .await;
    assert!(result.is_ok(), "with_relay_pagination must accept a current-version schema");
}

#[cfg(feature = "arrow")]
#[tokio::test]
async fn with_flight_service_refuses_incompatible_schema_format() {
    // H16 regression: the Arrow Flight constructor likewise used
    // RuntimeConfig::default() and skipped format-version validation.
    let config = ServerConfig {
        cache_enabled: false,
        ..ServerConfig::default()
    };
    let result = Server::with_flight_service(
        config,
        incompatible_schema(),
        Arc::new(NoopRelayAdapter),
        None,
        None,
    )
    .await;
    assert!(
        result.is_err(),
        "with_flight_service must refuse an incompatible compiled-schema version (H16)"
    );
}

// ── #758: the multi-tenant cache gate must fire from every constructor ────────
//
// This gate existed inline in `Server::new` and `with_relay_pagination` and was
// absent from `with_flight_service`. The drift was invisible because nothing could
// set `security.multi_tenant`, so `is_multi_tenant()` was false everywhere and all
// three constructors agreed by accident. Giving it a producer makes the difference
// real, so the check now lives in one place and each constructor is pinned here.

#[tokio::test]
async fn server_new_refuses_multi_tenant_caching_without_rls() {
    let result = Server::new(
        caching_config(),
        multi_tenant_schema_without_rls(),
        Arc::new(NoopRelayAdapter),
        None,
    )
    .await;
    let Err(e) = result else {
        panic!("#758: Server::new must refuse multi-tenant + cache + no RLS");
    };
    let msg = e.to_string();
    assert!(msg.contains("multi-tenant"), "the refusal must say why: {msg}");
    assert!(
        msg.contains("[security.rls]"),
        "the refusal must name a knob that exists: {msg}"
    );
}

#[tokio::test]
async fn with_relay_pagination_refuses_multi_tenant_caching_without_rls() {
    let result = Server::with_relay_pagination(
        caching_config(),
        multi_tenant_schema_without_rls(),
        Arc::new(NoopRelayAdapter),
        None,
    )
    .await;
    assert!(
        result.is_err(),
        "#758: with_relay_pagination must refuse multi-tenant + cache + no RLS"
    );
}

/// Counterweight: the gate must not refuse a single-tenant schema, or it would
/// break every non-tenanted deployment while proving nothing about isolation.
#[tokio::test]
async fn a_single_tenant_schema_still_boots_with_caching() {
    let result =
        Server::new(caching_config(), current_schema(), Arc::new(NoopRelayAdapter), None).await;
    assert!(result.is_ok(), "single-tenant + cache must still boot: {:?}", result.err());
}

/// Counterweight: declaring RLS lets a multi-tenant schema boot. The live catalog
/// verification that follows the declaration is exercised against a real database
/// in `cache_rls_isolation_test`; this adapter answers no rows, which the check
/// treats as "relation does not exist" — so the declaration path is asserted here
/// only up to the point where it hands off.
#[tokio::test]
async fn declaring_rls_changes_the_outcome() {
    let mut schema = multi_tenant_schema_without_rls();
    if let Some(security) = schema.security.as_mut() {
        security.rls.enabled = true;
    }
    let refused_without = Server::new(
        caching_config(),
        multi_tenant_schema_without_rls(),
        Arc::new(NoopRelayAdapter),
        None,
    )
    .await
    .is_err();
    assert!(refused_without, "precondition: the undeclared schema is refused");

    // With RLS declared the *declaration* gate passes; the schema below has no
    // queries, so the live check has no relation to inspect and also passes.
    let result = Server::new(caching_config(), schema, Arc::new(NoopRelayAdapter), None).await;
    assert!(
        result.is_ok(),
        "declaring [security.rls] must change the outcome: {:?}",
        result.err()
    );
}
