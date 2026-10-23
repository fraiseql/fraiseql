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
