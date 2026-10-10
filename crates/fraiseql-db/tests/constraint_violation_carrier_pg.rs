//! #1531: a database error that names a constraint carries its name, schema and table.
//!
//! PostgreSQL reports `CONSTRAINT NAME`, `SCHEMA NAME` and `TABLE NAME` with an
//! integrity-constraint violation; the error the engine received used to keep only the
//! message and the SQLSTATE, so the typed error a mutation serves for it (#1424) could not
//! say which rule failed.
//!
//! Two sites of a mutation's transaction can raise one: the function call (the issue's
//! partial unique index) and the `COMMIT`, where a deferred constraint is checked. Both are
//! driven here through `execute_write`, the write path a mutation takes.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1531_carrier` schema → run
//! `--test-threads=1`.
#![cfg(all(feature = "postgres", feature = "test-postgres"))]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use fraiseql_db::{PostgresAdapter, WriteRequest, Writer as _};
use fraiseql_error::{ConstraintViolation, FraiseQLError};
use tokio_postgres::NoTls;

const SCHEMA: &str = "p1531_carrier";

async fn setup() -> PostgresAdapter {
    let url = fraiseql_test_support::database_url();
    let (client, conn) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE TABLE {SCHEMA}.tb_user (id int PRIMARY KEY, email text, deleted_at timestamptz);
             CREATE UNIQUE INDEX tb_user_email_live_key ON {SCHEMA}.tb_user (email)
                 WHERE deleted_at IS NULL;
             INSERT INTO {SCHEMA}.tb_user VALUES (1, 'taken@example.com', NULL);
             CREATE TABLE {SCHEMA}.tb_slot (n int,
                 CONSTRAINT tb_slot_n_key UNIQUE (n) DEFERRABLE INITIALLY DEFERRED);
             INSERT INTO {SCHEMA}.tb_slot VALUES (1);
             CREATE FUNCTION {SCHEMA}.fn_create_user() RETURNS SETOF int LANGUAGE sql AS $$
                 INSERT INTO {SCHEMA}.tb_user VALUES (2, 'taken@example.com', NULL) RETURNING id
             $$;
             CREATE FUNCTION {SCHEMA}.fn_take_slot() RETURNS SETOF int LANGUAGE sql AS $$
                 INSERT INTO {SCHEMA}.tb_slot VALUES (1) RETURNING n
             $$;"
        ))
        .await
        .unwrap();
    PostgresAdapter::new(&url).await.unwrap()
}

async fn violation(adapter: &PostgresAdapter, function: &str) -> FraiseQLError {
    adapter
        .execute_write(&WriteRequest::new(&format!("{SCHEMA}.{function}"), &[]), &|_| Ok(()))
        .await
        .expect_err("the write must violate its constraint")
}

fn assert_names(err: &FraiseQLError, name: &str, table: &str) {
    let FraiseQLError::Database {
        sql_state,
        constraint,
        ..
    } = err
    else {
        panic!("expected a database error, got {err:?}");
    };
    assert_eq!(sql_state.as_deref(), Some("23505"), "{err:?}");
    assert_eq!(
        constraint.as_deref(),
        Some(&ConstraintViolation {
            name:   Some(name.to_string()),
            schema: Some(SCHEMA.to_string()),
            table:  Some(table.to_string()),
            column: None,
        }),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_violation_in_the_function_names_its_constraint() {
    let adapter = setup().await;
    let err = violation(&adapter, "fn_create_user").await;
    assert_names(&err, "tb_user_email_live_key", "tb_user");
}

#[tokio::test]
async fn a_deferred_violation_at_commit_names_its_constraint() {
    let adapter = setup().await;
    let err = violation(&adapter, "fn_take_slot").await;
    assert_names(&err, "tb_slot_n_key", "tb_slot");
}
