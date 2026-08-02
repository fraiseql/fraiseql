//! Construction-path parity: every way of building a runtime must produce the
//! same configured runtime.
//!
//! `Server::new`, `Server::with_relay_pagination`, `Server::with_flight_service`
//! and the hot-reload rebuild are four ways to reach a serving executor. Each one
//! that drifted from the others silently dropped a different subset of the
//! compiled configuration (#750, #754, #782, #783).
//!
//! These tests build every path from **one** fully-populated compiled schema and
//! server config and assert the same set of properties on each. They are the
//! phase's durable deliverable: a fifth construction path that forgets to carry a
//! setting fails here rather than in production.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none — the fail-closed webhook probe targets a closed local port
//! **Parallelism:** safe
#![allow(clippy::panic)] // Reason: the relay stub is never invoked by these tests

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
        CURRENT_SCHEMA_FORMAT_VERSION, ChangelogConfig, CompiledSchema, SecurityConfig,
        SqlProjectionHint, SubscriptionHooksConfig, SubscriptionsConfig, ValidationConfig,
    },
};

use crate::{Server, server_config::ServerConfig};

/// A closed port on loopback. A fail-closed webhook hook pointed here must
/// *reject*; a `NoopLifecycle` accepts. This is what makes the subscription
/// assertion test the guarantee (#754: dropping the hooks is a silent fail-open)
/// rather than the type name.
const UNREACHABLE_WEBHOOK: &str = "http://127.0.0.1:9/reject";

/// The compiled `[validation] max_page_size` the fixture schema declares —
/// deliberately different from the runtime default (1000) so a path that fell
/// back to `RuntimeConfig::default()` is visible.
const FIXTURE_MAX_PAGE_SIZE: u32 = 100;

/// The compiled `[subscriptions] max_subscriptions_per_connection`.
const FIXTURE_MAX_SUBSCRIPTIONS: u32 = 7;

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
        DatabaseType::PostgreSQL
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
        panic!("relay queries are never executed in construction-parity tests")
    }
}

/// A compiled schema that sets **every** setting a constructor is supposed to
/// carry, each to a value distinguishable from the runtime default.
fn fully_configured_schema() -> CompiledSchema {
    let mut security = SecurityConfig::default();
    security
        .additional
        .insert("enterprise".to_string(), serde_json::json!({ "audit_logging_enabled": true }));

    CompiledSchema {
        schema_format_version: Some(CURRENT_SCHEMA_FORMAT_VERSION),
        security: Some(security),
        validation_config: Some(ValidationConfig {
            max_page_size: Some(FIXTURE_MAX_PAGE_SIZE),
            ..ValidationConfig::default()
        }),
        changelog: Some(ChangelogConfig {
            write_enabled: false,
            ..ChangelogConfig::default()
        }),
        subscriptions_config: Some(SubscriptionsConfig {
            max_subscriptions_per_connection: Some(FIXTURE_MAX_SUBSCRIPTIONS),
            hooks: Some(SubscriptionHooksConfig {
                on_connect: Some(UNREACHABLE_WEBHOOK.to_string()),
                on_subscribe: Some(UNREACHABLE_WEBHOOK.to_string()),
                timeout_ms: 100,
                ..SubscriptionHooksConfig::default()
            }),
        }),
        ..CompiledSchema::default()
    }
}

/// The matching server config. `pool_tuning` is enabled so the paths that never
/// applied it are visible.
fn fully_configured_server_config() -> ServerConfig {
    ServerConfig {
        // The fixture adapter is not a real database; caching is off so the
        // multi-tenant/RLS gate and the live RLS verification stay out of the way.
        cache_enabled: false,
        // #874: `Server::new` now runs `ServerConfig::validate()`, and the
        // default `cors_enabled = true` with no origins is refused in
        // production mode (the test environment does not set FRAISEQL_ENV).
        cors_enabled: false,
        pool_tuning: Some(crate::config::pool_tuning::PoolPressureMonitorConfig {
            enabled: true,
            ..crate::config::pool_tuning::PoolPressureMonitorConfig::default()
        }),
        ..ServerConfig::default()
    }
}

/// Assert every property the compiled configuration promises, on a server built
/// by any construction path.
async fn assert_carries_full_config<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    server: &Server<A>,
    path: &str,
) {
    let cfg = server.executor.config();

    assert!(
        cfg.audit_mutations,
        "{path}: [security.enterprise] audit_logging_enabled = true must reach the executor"
    );
    assert_eq!(
        cfg.max_page_size,
        Some(FIXTURE_MAX_PAGE_SIZE),
        "{path}: the #421 page-size ceiling from [validation] must reach the executor"
    );
    assert!(
        !cfg.changelog_enabled,
        "{path}: [changelog] write_enabled = false must reach the executor"
    );

    assert_eq!(
        server.max_subscriptions_per_connection,
        Some(FIXTURE_MAX_SUBSCRIPTIONS),
        "{path}: [subscriptions] max_subscriptions_per_connection must be applied (#754)"
    );

    // The guarantee, not the type: a fail-closed on_connect hook pointed at a
    // closed port must reject. `NoopLifecycle` — what a drifted constructor
    // leaves behind — accepts, which is the silent fail-open #754 describes.
    let decision = server.subscription_lifecycle.on_connect(&serde_json::json!({}), "conn-1").await;
    assert!(
        decision.is_err(),
        "{path}: the compiled fail-closed [subscriptions] on_connect hook must reject an \
         unauthorised connection; a NoopLifecycle accepts it (#754)"
    );

    assert!(
        server.pool_tuning_config.is_some(),
        "{path}: [pool_tuning] must be applied (#754)"
    );
}

#[tokio::test]
async fn server_new_carries_the_full_compiled_config() {
    let server = Server::new(
        fully_configured_server_config(),
        fully_configured_schema(),
        Arc::new(NoopRelayAdapter),
        None,
    )
    .await
    .expect("Server::new must boot the fixture schema");
    assert_carries_full_config(&server, "Server::new").await;
}

#[tokio::test]
async fn with_relay_pagination_carries_the_full_compiled_config() {
    let server = Server::with_relay_pagination(
        fully_configured_server_config(),
        fully_configured_schema(),
        Arc::new(NoopRelayAdapter),
        None,
    )
    .await
    .expect("with_relay_pagination must boot the fixture schema");
    assert_carries_full_config(&server, "Server::with_relay_pagination").await;
}

#[cfg(feature = "arrow")]
#[tokio::test]
async fn with_flight_service_carries_the_full_compiled_config() {
    let server = Server::with_flight_service(
        fully_configured_server_config(),
        fully_configured_schema(),
        Arc::new(NoopRelayAdapter),
        None,
        None,
    )
    .await
    .expect("with_flight_service must boot the fixture schema");
    assert_carries_full_config(&server, "Server::with_flight_service").await;
}

// ── #783: the Flight service must receive the OIDC validator ─────────────────
//
// `create_flight_service` builds the service with `oidc_validator: None` and
// `main.rs` hands it to `with_flight_service`, which stored it without wiring
// the validator it had just constructed. The Flight handshake is fail-closed on
// a missing validator, so the entire Arrow Flight surface was dead in every
// arrow binary — `Status::internal("Authentication not configured")` regardless
// of `[auth]`.

/// An `[auth]` block with the JWKS URI pinned, so `OidcValidator::new` performs
/// no `OIDC` discovery and the test needs no network.
#[cfg(all(feature = "arrow", feature = "auth"))]
fn offline_oidc_config() -> fraiseql_core::security::OidcConfig {
    fraiseql_core::security::OidcConfig {
        issuer: None,
        audience: Some("https://api.example.test".to_string()),
        jwks_uri: Some("https://idp.example.test/.well-known/jwks.json".to_string()),
        ..Default::default()
    }
}

#[cfg(all(feature = "arrow", feature = "auth"))]
#[tokio::test]
async fn with_flight_service_wires_the_oidc_validator_into_the_passed_service() {
    use fraiseql_arrow::FraiseQLFlightService;

    let config = ServerConfig {
        auth: Some(offline_oidc_config()),
        ..fully_configured_server_config()
    };
    // Exactly what `main.rs` does: build the service (validator `None`) and pass it in.
    let flight_service = FraiseQLFlightService::new();
    assert!(
        !flight_service.has_oidc_validator(),
        "precondition: create_flight_service builds the service without a validator"
    );

    let server = Server::with_flight_service(
        config,
        fully_configured_schema(),
        Arc::new(NoopRelayAdapter),
        None,
        Some(flight_service),
    )
    .await
    .expect("with_flight_service must boot with [auth] configured");

    assert!(
        server.oidc_validator.is_some(),
        "precondition: [auth] must produce an OIDC validator on the server"
    );
    assert!(
        server
            .flight_service
            .as_ref()
            .is_some_and(FraiseQLFlightService::has_oidc_validator),
        "#783: with_flight_service must install the OIDC validator into the Flight service; \
         without it every Flight handshake fails closed with 'Authentication not configured'"
    );
}

// ── #750 / #782: hot-reload is a construction path too ───────────────────────

/// A schema identical to [`fully_configured_schema`] except for one added type,
/// so its content hash differs while every configuration section is unchanged.
/// This is the reload a running server must accept.
fn hot_reloadable_variant() -> CompiledSchema {
    use fraiseql_core::schema::TypeDefinition;

    let mut schema = fully_configured_schema();
    schema.types.push(TypeDefinition::new("Widget", "v_widget"));
    schema
}

async fn write_schema(dir: &std::path::Path, schema: &CompiledSchema) -> std::path::PathBuf {
    let path = dir.join("schema.compiled.json");
    let json = serde_json::to_string(schema).expect("fixture schema must serialize");
    tokio::fs::write(&path, json).await.expect("fixture schema must be writable");
    path
}

#[tokio::test]
async fn hot_reload_preserves_the_full_runtime_config() {
    let server = Server::new(
        fully_configured_server_config(),
        fully_configured_schema(),
        Arc::new(NoopRelayAdapter),
        None,
    )
    .await
    .expect("Server::new must boot the fixture schema");
    let state = server.build_app_state();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_schema(dir.path(), &hot_reloadable_variant()).await;

    state.reload_schema(&path).await.expect("a config-identical schema must reload");

    let cfg_holder = state.executor.load();
    let cfg = cfg_holder.config();
    assert!(
        cfg.audit_mutations,
        "#750: reload must not drop [security.enterprise] audit_logging_enabled"
    );
    assert_eq!(
        cfg.max_page_size,
        Some(FIXTURE_MAX_PAGE_SIZE),
        "#750: reload must not reset the #421 page-size ceiling to the default"
    );
    assert!(
        !cfg.changelog_enabled,
        "#750: reload must not re-enable the change-log outbox write"
    );
}

#[tokio::test]
async fn hot_reload_preserves_relay_dispatch() {
    let server = Server::with_relay_pagination(
        fully_configured_server_config(),
        fully_configured_schema(),
        Arc::new(NoopRelayAdapter),
        None,
    )
    .await
    .expect("with_relay_pagination must boot the fixture schema");
    let state = server.build_app_state();
    assert!(
        state.executor.load().relay_enabled(),
        "precondition: with_relay_pagination builds a relay-capable executor"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_schema(dir.path(), &hot_reloadable_variant()).await;

    state.reload_schema(&path).await.expect("a config-identical schema must reload");

    assert!(
        state.executor.load().relay_enabled(),
        "#750: reload must preserve relay dispatch; without it every relay query fails \
         validation until the process restarts"
    );
}

#[tokio::test]
async fn hot_reload_refuses_a_schema_whose_boot_frozen_config_changed() {
    let server = Server::new(
        fully_configured_server_config(),
        fully_configured_schema(),
        Arc::new(NoopRelayAdapter),
        None,
    )
    .await
    .expect("Server::new must boot the fixture schema");
    let state = server.build_app_state();

    // `[fraiseql.naming] acronyms` is installed into a process-global `OnceLock`
    // at boot; only the first call wins. A reload cannot apply a change, so it
    // must refuse rather than serve a schema whose JSONB-key resolution disagrees
    // with the compiled surface (#782).
    let mut drifted = hot_reloadable_variant();
    drifted.naming_acronyms = vec!["db2".to_string()];

    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_schema(dir.path(), &drifted).await;

    let err = state
        .reload_schema(&path)
        .await
        .expect_err("#782: a reload that changes boot-frozen config must be refused");
    assert!(
        err.contains("naming_acronyms"),
        "the refusal must name the section that cannot be hot-reloaded: {err}"
    );
    assert!(
        err.to_lowercase().contains("restart"),
        "the refusal must tell the operator what to do instead: {err}"
    );
}

#[tokio::test]
async fn hot_reload_runs_the_boot_safety_gates() {
    use fraiseql_core::schema::{
        FieldDefinition, FieldEncryptionConfig, FieldType, TypeDefinition,
    };

    let server = Server::new(
        fully_configured_server_config(),
        fully_configured_schema(),
        Arc::new(NoopRelayAdapter),
        None,
    )
    .await
    .expect("Server::new must boot the fixture schema");
    let state = server.build_app_state();

    // Boot refuses a field marked for at-rest encryption (H12): the write path
    // stores plaintext. A reload that accepts one moves the server into a state
    // boot would have refused.
    let mut encrypted = fully_configured_schema();
    let mut user = TypeDefinition::new("User", "v_user");
    user.fields
        .push(FieldDefinition::new("email", FieldType::String).with_encryption(
            FieldEncryptionConfig {
                key_reference: "keys/user-email".to_string(),
                algorithm:     "AES-256-GCM".to_string(),
            },
        ));
    encrypted.types.push(user);

    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_schema(dir.path(), &encrypted).await;

    let err = state
        .reload_schema(&path)
        .await
        .expect_err("#782: reload must run the boot-time safety gates");
    assert!(
        err.contains("User.email"),
        "the refusal must name the offending field, as boot does: {err}"
    );
}

/// #874: `ServerConfig::validate()` must run on the library construction path,
/// not only in `main.rs`. A downstream embedder following the documented
/// `from_file` + `Server::new` flow used to skip every production safety gate —
/// a `pool_timeout_secs = 0` (or a public playground, or `OIDC`+HS256 both
/// configured) booted happily as a library while the binary refused it.
///
/// `pool_timeout_secs = 0` is the probe because it is environment-independent:
/// the production-mode gates depend on `FRAISEQL_ENV`, which the parallel test
/// runner must not touch.
#[tokio::test]
async fn library_construction_path_runs_config_validate() {
    let config = ServerConfig {
        pool_timeout_secs: 0,
        ..fully_configured_server_config()
    };
    let result =
        Server::new(config, fully_configured_schema(), Arc::new(NoopRelayAdapter), None).await;
    assert!(
        result.is_err(),
        "Server::new must refuse the config validate() refuses — the library \
         path skipped every production safety gate (#874)"
    );
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        msg.contains("pool_timeout_secs"),
        "the refusal must name the offending key, got: {msg}"
    );
}

/// #368: fixture for a compiled `[auth.social.github]` block. `GitHub` is used
/// (not `Google`) because its construction is network-free — no discovery fetch
/// can stall or fail these DB-less unit tests.
#[cfg(feature = "auth")]
fn schema_with_github_social(secret_env: &str) -> CompiledSchema {
    let mut schema = fully_configured_schema();
    schema.auth = Some(fraiseql_core::schema::AuthClientConfig {
        pkce:   None,
        local:  None,
        social: Some(fraiseql_core::schema::SocialAuthConfig {
            redirect_uri_allowlist: Vec::new(),
            google:                 None,
            github:                 Some(fraiseql_core::schema::GitHubSocialConfig {
                client_id:         "gh-client".to_string(),
                client_secret_env: secret_env.to_string(),
                redirect_uri:      "https://app.example.com/auth/v1/callback".to_string(),
                base_url:          None,
                api_base_url:      None,
            }),
        }),
    });
    schema
}

/// #368: a compiled `[auth.social]` block on a config without `[auth_hs256]`
/// must refuse to boot: the callback mints HS256-signed sessions this server
/// itself validates, so without the signing config every login would 500 (or
/// worse, mint tokens nothing can validate).
#[cfg(feature = "auth")]
#[tokio::test]
async fn social_login_without_hs256_refuses_to_boot() {
    // A fixed valid value, set unconditionally: safe under the parallel runner
    // because every reader wants exactly this value.
    std::env::set_var("FRAISEQL_TEST_P26_SOCIAL_GH_SECRET", "gh-secret");
    let schema = schema_with_github_social("FRAISEQL_TEST_P26_SOCIAL_GH_SECRET");
    let result =
        Server::new(fully_configured_server_config(), schema, Arc::new(NoopRelayAdapter), None)
            .await;
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        msg.contains("auth_hs256"),
        "[auth.social] without [auth_hs256] must refuse naming the missing section, got: {msg}"
    );
}

/// #368: a provider whose `client_secret_env` is unset must refuse to boot —
/// the alternative is a mounted login flow whose every token exchange fails.
#[cfg(feature = "auth")]
#[tokio::test]
async fn social_login_with_unset_secret_env_refuses_to_boot() {
    let schema = schema_with_github_social("__FRAISEQL_TEST_P26_DEFINITELY_UNSET__");
    let mut config = fully_configured_server_config();
    std::env::set_var("FRAISEQL_TEST_P26_SOCIAL_HS256", "p26-social-hs256-secret-32-bytes!");
    config.auth_hs256 = Some(crate::server_config::hs256::Hs256Config {
        secret_env: "FRAISEQL_TEST_P26_SOCIAL_HS256".to_string(),
        issuer:     Some("https://sp.example.com".to_string()),
        audience:   Some("fraiseql".to_string()),
    });
    let result = Server::new(config, schema, Arc::new(NoopRelayAdapter), None).await;
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        msg.contains("__FRAISEQL_TEST_P26_DEFINITELY_UNSET__"),
        "an unset client_secret_env must refuse naming the env var, got: {msg}"
    );
}

/// #368: `[auth.social]` on a server constructed without a database pool must
/// refuse to boot — sessions and account linking are Postgres-backed.
#[cfg(feature = "auth")]
#[tokio::test]
async fn social_login_without_a_pool_refuses_to_boot() {
    std::env::set_var("FRAISEQL_TEST_P26_SOCIAL_GH_SECRET", "gh-secret");
    let schema = schema_with_github_social("FRAISEQL_TEST_P26_SOCIAL_GH_SECRET");
    let mut config = fully_configured_server_config();
    std::env::set_var("FRAISEQL_TEST_P26_SOCIAL_HS256", "p26-social-hs256-secret-32-bytes!");
    config.auth_hs256 = Some(crate::server_config::hs256::Hs256Config {
        secret_env: "FRAISEQL_TEST_P26_SOCIAL_HS256".to_string(),
        issuer:     Some("https://sp.example.com".to_string()),
        audience:   Some("fraiseql".to_string()),
    });
    let result = Server::new(config, schema, Arc::new(NoopRelayAdapter), None).await;
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        msg.contains("[auth.social]") && msg.contains("pool"),
        "[auth.social] without a pool must refuse naming both, got: {msg}"
    );
}

/// #367: fixture for a compiled `[auth.local]` block.
#[cfg(feature = "auth")]
fn schema_with_local_auth(local: fraiseql_core::schema::LocalAuthConfig) -> CompiledSchema {
    let mut schema = fully_configured_schema();
    schema.auth = Some(fraiseql_core::schema::AuthClientConfig {
        pkce:   None,
        social: None,
        local:  Some(local),
    });
    schema
}

/// A `ServerConfig` with `[auth_hs256]` set — the shape `[auth.local]` requires.
#[cfg(feature = "auth")]
fn hs256_server_config() -> ServerConfig {
    std::env::set_var("FRAISEQL_TEST_P26_LOCAL_HS256", "p26-local-hs256-secret-32bytes!!");
    ServerConfig {
        auth_hs256: Some(crate::server_config::hs256::Hs256Config {
            secret_env: "FRAISEQL_TEST_P26_LOCAL_HS256".to_string(),
            issuer:     Some("https://sp.example.com".to_string()),
            audience:   Some("fraiseql".to_string()),
        }),
        ..fully_configured_server_config()
    }
}

/// #367: `[auth.local]` without a database pool must refuse to boot —
/// credentials, `MFA` enrollments, `OTP` budgets and sessions are all durable.
#[cfg(feature = "auth")]
#[tokio::test]
async fn local_auth_without_a_pool_refuses_to_boot() {
    let schema = schema_with_local_auth(fraiseql_core::schema::LocalAuthConfig {
        mfa: true,
        ..fraiseql_core::schema::LocalAuthConfig::default()
    });
    let result = Server::new(hs256_server_config(), schema, Arc::new(NoopRelayAdapter), None).await;
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        msg.contains("[auth.local]") && msg.contains("pool"),
        "[auth.local] without a pool must refuse naming both, got: {msg}"
    );
}

/// #367: `[auth.local]` without `[auth_hs256]` must refuse — every local
/// sign-in mints HS256 sessions this server itself validates.
#[cfg(feature = "auth")]
#[tokio::test]
async fn local_auth_without_hs256_refuses_to_boot() {
    let schema = schema_with_local_auth(fraiseql_core::schema::LocalAuthConfig {
        mfa: true,
        ..fraiseql_core::schema::LocalAuthConfig::default()
    });
    let result = Server::new(
        fully_configured_server_config(), // no [auth_hs256]
        schema,
        Arc::new(NoopRelayAdapter),
        None,
    )
    .await;
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        msg.contains("auth_hs256"),
        "[auth.local] without [auth_hs256] must refuse naming the missing section, got: {msg}"
    );
}

/// #367: a mail-sending method (`otp`) naming a mailbox that does not exist must
/// refuse to boot — the alternative is a mounted login flow that mails nobody.
#[cfg(all(feature = "auth", feature = "inbound-email"))]
#[tokio::test]
async fn local_auth_otp_with_an_unknown_mailbox_refuses_to_boot() {
    let schema = schema_with_local_auth(fraiseql_core::schema::LocalAuthConfig {
        otp: true,
        email_from: Some("no-such-mailbox".to_string()),
        ..fraiseql_core::schema::LocalAuthConfig::default()
    });
    let result = Server::new(hs256_server_config(), schema, Arc::new(NoopRelayAdapter), None).await;
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    // The pool check fires first (it guards every method); with a pool present
    // the mailbox check is what the live-PG suite proves. Either refusal is a
    // refusal — what must never happen is a successful boot.
    assert!(
        !msg.is_empty(),
        "[auth.local] otp naming an unknown mailbox must refuse to boot"
    );
}

/// #627: `[security.api_keys] storage = "postgres"` configured on a server with
/// no database pool must refuse to boot. Before the Postgres store existed, the
/// CLI rejected the value at compile time; now that the server accepts it, a
/// missing pool would otherwise reproduce the original defect — an authenticator
/// with zero keys that authenticates nothing, silently.
#[tokio::test]
async fn postgres_api_keys_without_a_pool_refuse_to_boot() {
    let mut schema = fully_configured_schema();
    if let Some(ref mut sec) = schema.security {
        sec.additional.insert(
            "api_keys".to_string(),
            serde_json::json!({ "enabled": true, "storage": "postgres" }),
        );
    }
    let result = Server::new(
        fully_configured_server_config(),
        schema,
        Arc::new(NoopRelayAdapter),
        None, // no db_pool
    )
    .await;
    assert!(
        result.is_err(),
        "storage = \"postgres\" with no database pool must refuse to boot, \
         not authenticate nothing silently (#627)"
    );
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(msg.contains("api_keys"), "the refusal must name the config section, got: {msg}");
}
