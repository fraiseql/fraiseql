//! The boot provisioning table: how its failures surface.

#![allow(clippy::unwrap_used)] // Reason: test code
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available.

use std::{sync::Arc, time::Duration};

use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use sqlx::postgres::PgPoolOptions;

use crate::{Server, ServerError, server_config::ServerConfig};

/// A pool that can never connect: port 1 refuses, and the acquire gives up quickly.
fn unreachable_pool() -> sqlx::PgPool {
    PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy("postgres://fraiseql:fraiseql@127.0.0.1:1/fraiseql")
        .unwrap()
}

#[tokio::test]
async fn a_database_that_refuses_the_boot_ddl_is_a_database_error() {
    // The admin-token table is provisioned on every feature set, so this is the one
    // step every build runs once a pool and a token are configured.
    let config = ServerConfig {
        admin_token: Some("a".repeat(32)),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let mut server = Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        Some(unreachable_pool()),
    ))
    .await
    .unwrap();

    let error = server.provisioned_app_state().await.err().expect("the boot must fail");

    assert!(
        matches!(error, ServerError::Database(_)),
        "an unreachable database is not a configuration mistake; got {error:?}"
    );
}

#[tokio::test]
async fn a_script_two_steps_need_runs_once_under_the_first() {
    use super::provision::{ProvisionStep, without_repeats};

    let pool = unreachable_pool();
    let step = |name, scripts: &[&'static str]| ProvisionStep::new(name, &pool, scripts);
    let steps = without_repeats(vec![
        step("first", &["sessions", "accounts"]),
        step("second", &["accounts", "otp", "sessions"]),
        step("third", &["otp"]),
    ]);

    let planned: Vec<_> = steps.iter().map(|s| (s.name, s.scripts.clone())).collect();
    assert_eq!(
        planned,
        vec![
            ("first", vec!["sessions", "accounts"]),
            ("second", vec!["otp"])
        ],
        "each script runs once, under the first step that needs it; a step left with \
         nothing to run is dropped"
    );
}

/// A database of its own: the race needs the tables absent.
const SCRATCH_DB: &str = "provision_steps_race";

/// Four replicas booting together against one database must all provision, cold and
/// warm. The scripts are `IF NOT EXISTS` DDL, which is not atomic: unlocked, concurrent
/// runners lose on the catalogue's unique indexes.
#[tokio::test]
async fn replicas_booting_together_all_provision() {
    use fraiseql_auth::{account_linking, session_postgres::SESSIONS_SCHEMA_SQL};

    use super::provision::{ProvisionStep, run};

    let Some(svc) = fraiseql_test_support::postgres().await else {
        eprintln!("SKIP replicas_booting_together_all_provision: no postgres");
        return;
    };
    let base = svc.url();
    let (prefix, tail) = base.rsplit_once('/').expect("a PostgreSQL URL carries a database path");
    let query = tail.find('?').map_or("", |i| &tail[i..]);
    let admin = sqlx::PgPool::connect(base).await.unwrap();
    for ddl in [
        format!("DROP DATABASE IF EXISTS {SCRATCH_DB} WITH (FORCE)"),
        format!("CREATE DATABASE {SCRATCH_DB}"),
    ] {
        sqlx::query(&ddl).execute(&admin).await.unwrap();
    }
    let pool = sqlx::PgPool::connect(&format!("{prefix}/{SCRATCH_DB}{query}")).await.unwrap();

    for phase in ["cold", "warm"] {
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..4 {
            let steps = vec![
                ProvisionStep::new(
                    "RBAC",
                    &pool,
                    &[crate::api::rbac_management::db_backend::RBAC_SCHEMA_SQL],
                ),
                ProvisionStep::new(
                    "admin-token",
                    &pool,
                    &[crate::api::admin_principal::SCHEMA_SQL],
                ),
                ProvisionStep::new(
                    "SCIM",
                    &pool,
                    &[
                        account_linking::SCHEMA_SQL,
                        SESSIONS_SCHEMA_SQL,
                        fraiseql_auth::scim::PG_SCIM_SCHEMA_SQL,
                    ],
                ),
            ];
            set.spawn(run(steps));
        }
        let mut errors = Vec::new();
        while let Some(joined) = set.join_next().await {
            if let Err(e) = joined.unwrap() {
                errors.push(e.to_string());
            }
        }
        assert!(
            errors.is_empty(),
            "{phase} start: {} of 4 boots failed: {errors:?}",
            errors.len()
        );
    }

    pool.close().await;
    sqlx::query(&format!("DROP DATABASE IF EXISTS {SCRATCH_DB} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();
}

/// A database of its own: the probe needs every table absent.
const CTOR_SCRATCH_DB: &str = "server_ctor_runs_no_ddl";

/// Tables outside the system catalogues.
async fn user_tables(pool: &sqlx::PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT schemaname || '.' || tablename FROM pg_tables \
         WHERE schemaname NOT IN ('pg_catalog', 'information_schema') ORDER BY 1",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Constructing a server creates no table: every boot script is a provisioning step, so
/// it runs under the boot lock when the server is served, and a server that is built and
/// never served leaves the database as it found it. Provisioning then creates them.
#[tokio::test]
async fn tables_are_created_by_provisioning_not_by_the_constructor() {
    let Some(svc) = fraiseql_test_support::postgres().await else {
        eprintln!("SKIP tables_are_created_by_provisioning_not_by_the_constructor: no postgres");
        return;
    };
    let base = svc.url();
    let (prefix, tail) = base.rsplit_once('/').expect("a PostgreSQL URL carries a database path");
    let query = tail.find('?').map_or("", |i| &tail[i..]);
    let admin = sqlx::PgPool::connect(base).await.unwrap();
    for ddl in [
        format!("DROP DATABASE IF EXISTS {CTOR_SCRATCH_DB} WITH (FORCE)"),
        format!("CREATE DATABASE {CTOR_SCRATCH_DB}"),
    ] {
        sqlx::query(&ddl).execute(&admin).await.unwrap();
    }
    let pool = sqlx::PgPool::connect(&format!("{prefix}/{CTOR_SCRATCH_DB}{query}"))
        .await
        .unwrap();
    let config: ServerConfig = serde_json::from_value(serde_json::json!({
        "cors_enabled": false,
        "session_state": { "backend": "postgres" },
        "async_operations": { "operations": ["largeExport"] },
    }))
    .unwrap();

    let mut server = Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        Some(pool.clone()),
    ))
    .await
    .unwrap();
    let constructed = user_tables(&pool).await;
    let provisioned = server.provisioned_app_state().await.map(|_| ());
    let served = user_tables(&pool).await;

    drop(server);
    pool.close().await;
    sqlx::query(&format!("DROP DATABASE IF EXISTS {CTOR_SCRATCH_DB} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();
    assert_eq!(constructed, Vec::<String>::new(), "the constructor ran DDL");
    provisioned.unwrap();
    for table in ["_system.async_operations", "_system.session_state"] {
        assert!(
            served.iter().any(|t| t == table),
            "provisioning did not create {table}: {served:?}"
        );
    }
}

/// The steps `server` plans, by name.
fn planned(server: &Server) -> Vec<(&'static str, Vec<&'static str>)> {
    server
        .provision_steps()
        .into_iter()
        .map(|step| (step.name, step.scripts))
        .collect()
}

async fn server_over(schema: CompiledSchema) -> Server {
    Box::pin(Server::new(
        ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        },
        schema,
        Arc::new(FailingAdapter::new()),
        Some(unreachable_pool()),
    ))
    .await
    .unwrap()
}

/// Each configured capability plans its own tables. A shared test database already holds
/// them, created by whichever suite ran first, so a step that went missing would still
/// boot there: the plan is what can be checked.
#[tokio::test]
async fn each_configured_capability_plans_its_own_tables() {
    let nothing = planned(&server_over(CompiledSchema::new()).await);
    assert_eq!(nothing, Vec::new(), "nothing configured, nothing planned");

    #[cfg(feature = "inbound")]
    {
        let mut server = server_over(CompiledSchema::new()).await;
        server.config.webhooks = serde_json::from_value(serde_json::json!({
            "stripe": { "provider": "stripe", "secret_env": "STRIPE_SECRET" }
        }))
        .unwrap();
        assert_eq!(
            planned(&server),
            vec![(
                "inbound webhook",
                vec![
                    fraiseql_functions::migrations::inbound_migration_sql(),
                    fraiseql_webhooks::idempotency::SCHEMA_SQL,
                ]
            )]
        );
    }

    #[cfg(feature = "inbound-email")]
    {
        let mut server = server_over(CompiledSchema::new()).await;
        server.config.mailbox = serde_json::from_value(serde_json::json!({
            "support": { "imap": {
                "host": "imap.example.com", "username": "support", "password_env": "IMAP_PASSWORD"
            } }
        }))
        .unwrap();
        assert_eq!(
            planned(&server),
            vec![(
                "inbound email",
                vec![
                    fraiseql_functions::migrations::inbound_migration_sql(),
                    fraiseql_observers::migrations::source_cursor_sql(),
                    fraiseql_functions::migrations::send_tracking_migration_sql(),
                ]
            )]
        );
    }

    // #1710: the observer runtime's own tables. The change log it reads belongs to the
    // application (`fraiseql setup` installs it), so it is checked at start, not created.
    #[cfg(feature = "observers")]
    {
        let mut server = server_over(CompiledSchema::new()).await;
        server.config.observers =
            Some(serde_json::from_value(serde_json::json!({ "enabled": true })).unwrap());
        assert_eq!(
            planned(&server),
            vec![(
                "observers",
                vec![
                    fraiseql_observers::migrations::observer_management_sql(),
                    fraiseql_observers::migrations::observer_dispatch_sql(),
                    fraiseql_observers::checkpoint::migration_sql(),
                ]
            )]
        );
    }

    #[cfg(feature = "sources")]
    {
        let mut schema = CompiledSchema::new();
        schema.sources = vec![
            serde_json::from_value(serde_json::json!({
                "name": "orders", "schedule": "*/5 * * * *", "function": "pullOrders"
            }))
            .unwrap(),
        ];
        assert_eq!(
            planned(&server_over(schema).await),
            vec![("source cursor", vec![fraiseql_observers::migrations::source_cursor_sql()])]
        );
    }

    #[cfg(feature = "functions-runtime")]
    {
        let functions: crate::schema::loader::FunctionsConfig =
            serde_json::from_value(serde_json::json!({
                "module_dir": "functions",
                "definitions": [
                    { "name": "nightly", "trigger": "cron:0 3 * * *", "runtime": "Wasm" }
                ]
            }))
            .unwrap();
        let server =
            server_over(CompiledSchema::new()).await.with_functions_config(Some(functions));
        assert_eq!(
            planned(&server),
            vec![("cron state", vec![fraiseql_functions::migrations::cron_migration_sql()])]
        );
    }

    #[cfg(feature = "cdc-outbound")]
    {
        let mut server = server_over(CompiledSchema::new()).await;
        server.config.cdc_outbound = Some(crate::server_config::CdcOutboundConfig::default());
        assert_eq!(
            planned(&server),
            vec![("cdc outbound", vec![fraiseql_cdc_sinks::outbox_sink_state_migration_sql()])]
        );
    }
}
