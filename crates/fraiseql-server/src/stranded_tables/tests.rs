#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available.

use super::{Strandable, move_into_core};

#[test]
fn a_table_with_a_companion_moves_with_it_in_one_transaction() {
    let observers = Strandable {
        table:     "tb_observer",
        companion: Some("tb_observer_log"),
    };
    assert_eq!(
        move_into_core("public", &observers),
        "  BEGIN;\n  DROP TABLE IF EXISTS core.tb_observer_log, core.tb_observer CASCADE;\n  \
         ALTER TABLE public.tb_observer SET SCHEMA core;\n  \
         ALTER TABLE IF EXISTS public.tb_observer_log SET SCHEMA core;\n  COMMIT;"
    );
}

#[test]
fn a_table_alone_moves_alone() {
    let cursor = Strandable {
        table:     "_fraiseql_source_cursor",
        companion: None,
    };
    assert_eq!(
        move_into_core("public", &cursor),
        "  BEGIN;\n  DROP TABLE IF EXISTS core._fraiseql_source_cursor CASCADE;\n  \
         ALTER TABLE public._fraiseql_source_cursor SET SCHEMA core;\n  COMMIT;"
    );
}

/// A schema name the operator copies into psql is quoted when it must be.
#[test]
fn a_schema_name_that_needs_quoting_is_quoted() {
    let cursor = Strandable {
        table:     "_fraiseql_source_cursor",
        companion: None,
    };
    assert!(
        move_into_core("Old \"Home\"", &cursor)
            .contains("ALTER TABLE \"Old \"\"Home\"\"\"._fraiseql_source_cursor SET SCHEMA core")
    );
}

#[cfg(any(feature = "sources", feature = "inbound-email"))]
mod source_cursor {
    use sqlx::PgPool;

    use super::*;
    use crate::stranded_tables::{refuse_stranded_source_cursor, stranded_moves};

    async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
        let svc = fraiseql_test_support::postgres().await?;
        let pool = PgPool::connect(svc.url()).await.unwrap();
        Some((pool, svc))
    }

    /// The cursor table boot creates, emptied, and `schema` holding a populated copy of
    /// it, as an earlier release left it.
    async fn strand_a_cursor_in(pool: &PgPool, schema: &str) {
        sqlx::raw_sql(fraiseql_observers::migrations::source_cursor_sql())
            .execute(pool)
            .await
            .unwrap();
        sqlx::raw_sql(&format!(
            "TRUNCATE core._fraiseql_source_cursor;
             DROP SCHEMA IF EXISTS {schema} CASCADE;
             CREATE SCHEMA {schema};
             CREATE TABLE {schema}._fraiseql_source_cursor
                 (LIKE core._fraiseql_source_cursor INCLUDING ALL);
             INSERT INTO {schema}._fraiseql_source_cursor (source_name, cursor_value, version)
                 VALUES ('left-behind', '{{\"page\": 3}}', 3);"
        ))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn drop_schema(pool: &PgPool, schema: &str) {
        sqlx::raw_sql(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .execute(pool)
            .await
            .unwrap();
    }

    /// The boot that wrote a cursor ran with another schema first on its search path,
    /// one today's path need not include: the check finds it anywhere outside `core`, and
    /// its refusal names the statements that move it.
    #[tokio::test]
    async fn a_populated_cursor_outside_core_beside_an_empty_core_one_is_refused() {
        const OLD_HOME: &str = "old_home_cursor";
        let Some((pool, _svc)) = connect_pool().await else {
            eprintln!("SKIP a_populated_cursor_outside_core_beside_an_empty_core_one: no postgres");
            return;
        };
        strand_a_cursor_in(&pool, OLD_HOME).await;

        let outcome = refuse_stranded_source_cursor(&pool).await;

        let Err(crate::ServerError::ConfigError(message)) = outcome else {
            drop_schema(&pool, OLD_HOME).await;
            panic!("expected the refusal, got {outcome:?}");
        };
        let statements = format!(
            "  BEGIN;\n  DROP TABLE IF EXISTS core._fraiseql_source_cursor CASCADE;\n  \
             ALTER TABLE {OLD_HOME}._fraiseql_source_cursor SET SCHEMA core;\n  COMMIT;"
        );
        assert!(message.contains(&statements), "names the move: {message}");

        // The statements it names are the whole migration: run them, and the cursor is
        // in `core` and the boot goes on.
        sqlx::raw_sql(statements.trim()).execute(&pool).await.unwrap();
        let moved: (i64,) = sqlx::query_as(
            "SELECT version FROM core._fraiseql_source_cursor WHERE source_name = 'left-behind'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let after = refuse_stranded_source_cursor(&pool).await;
        sqlx::raw_sql("DELETE FROM core._fraiseql_source_cursor WHERE source_name = 'left-behind'")
            .execute(&pool)
            .await
            .unwrap();
        drop_schema(&pool, OLD_HOME).await;
        assert_eq!(moved.0, 3, "the moved cursor keeps its version");
        assert!(after.is_ok(), "{after:?}");
    }

    /// Once `core` holds a cursor, a copy left elsewhere is not a reason to refuse.
    #[tokio::test]
    async fn a_populated_core_cursor_table_starts_whatever_copy_is_left_elsewhere() {
        const OLD_HOME: &str = "stale_copy_cursor";
        let Some((pool, _svc)) = connect_pool().await else {
            eprintln!("SKIP a_populated_core_cursor_table_starts: no postgres");
            return;
        };
        strand_a_cursor_in(&pool, OLD_HOME).await;
        sqlx::query(
            "INSERT INTO core._fraiseql_source_cursor (source_name, cursor_value, version) \
             VALUES ('lives-in-core', '{}', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let outcome = refuse_stranded_source_cursor(&pool).await;
        sqlx::raw_sql(
            "DELETE FROM core._fraiseql_source_cursor WHERE source_name = 'lives-in-core'",
        )
        .execute(&pool)
        .await
        .unwrap();
        drop_schema(&pool, OLD_HOME).await;

        assert!(outcome.is_ok(), "{outcome:?}");
    }

    /// An empty copy elsewhere holds nothing to lose.
    #[tokio::test]
    async fn an_empty_copy_outside_core_is_not_stranded() {
        const OLD_HOME: &str = "empty_copy_cursor";
        let Some((pool, _svc)) = connect_pool().await else {
            eprintln!("SKIP an_empty_copy_outside_core_is_not_stranded: no postgres");
            return;
        };
        strand_a_cursor_in(&pool, OLD_HOME).await;
        sqlx::raw_sql(&format!("TRUNCATE {OLD_HOME}._fraiseql_source_cursor"))
            .execute(&pool)
            .await
            .unwrap();

        let moves = stranded_moves(
            &pool,
            &[Strandable {
                table:     "_fraiseql_source_cursor",
                companion: None,
            }],
        )
        .await;
        drop_schema(&pool, OLD_HOME).await;

        assert!(!moves.unwrap().iter().any(|m| m.contains(OLD_HOME)));
    }
}
