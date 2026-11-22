//! A server with a function triggered after a write creates the function dispatch ledger
//! (`_fraiseql_function_dispatch`) at boot. A database role that may not create it stops the
//! boot, and the error names the table and the privilege the role is missing, so an operator
//! upgrading a deployment knows what to grant.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** owns a scratch database and a role of its own → `--test-threads=1`.
#![cfg(feature = "functions-runtime")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics/skips are fine

use std::{str::FromStr as _, sync::Arc};

use fraiseql_core::schema::CompiledSchema;
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use serde_json::json;
use sqlx::{PgPool, postgres::PgConnectOptions};

const DB: &str = "fraiseql_function_dispatch_boot";
const ROLE: &str = "fraiseql_dispatch_boot_no_create";
const PASSWORD: &str = "dispatch-boot-no-create";

#[tokio::test]
async fn a_role_that_cannot_create_the_ledger_stops_the_boot_naming_the_table_and_privilege() {
    let Some(url) = try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let admin = PgPool::connect(&url).await.expect("connect to admin database");
    // One statement each: `DROP DATABASE` refuses to run in a transaction block.
    for statement in [
        format!("DROP DATABASE IF EXISTS {DB} WITH (FORCE)"),
        format!("DROP ROLE IF EXISTS {ROLE}"),
        format!("CREATE ROLE {ROLE} LOGIN PASSWORD '{PASSWORD}'"),
        format!("CREATE DATABASE {DB}"),
    ] {
        // Reason: test SQL: names and values this test defines, or DDL the code under test
        // generated.
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(&admin)
            .await
            .unwrap();
    }
    let options = PgConnectOptions::from_str(&url).unwrap();
    let owner = PgPool::connect_with(options.clone().database(DB)).await.unwrap();
    // The role may connect and read, as a least-privilege serving role does, but not create.
    // Reason: test SQL: names and values this test defines, or DDL the code under test generated.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE CREATE ON SCHEMA public FROM PUBLIC;
         GRANT USAGE ON SCHEMA public TO {ROLE};"
    )))
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
    let pool = PgPool::connect_with(options.database(DB).username(ROLE).password(PASSWORD))
        .await
        .unwrap();

    let modules = tempfile::tempdir().unwrap();
    std::fs::write(modules.path().join("onOrder.wasm"), b"\0asm\x01\0\0\0").unwrap();
    let functions = serde_json::from_value(json!({
        "module_dir": modules.path(),
        "definitions": [{ "name": "onOrder", "trigger": "after:mutation:Order", "runtime": "Wasm" }]
    }))
    .unwrap();
    let config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let server = Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        Some(pool.clone()),
    ))
    .await
    .expect("the server is built")
    .with_functions_config(Some(functions));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

    let error = server
        .serve_on_listener(listener, std::future::pending::<()>())
        .await
        .expect_err("the boot stops")
        .to_string();

    pool.close().await;
    for statement in [
        format!("DROP DATABASE IF EXISTS {DB} WITH (FORCE)"),
        format!("DROP ROLE IF EXISTS {ROLE}"),
    ] {
        // Reason: test SQL: names and values this test defines, or DDL the code under test
        // generated.
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(&admin)
            .await
            .unwrap();
    }
    assert!(error.contains("_fraiseql_function_dispatch"), "names the table: {error}");
    assert!(error.contains("CREATE"), "names the missing privilege: {error}");
}
