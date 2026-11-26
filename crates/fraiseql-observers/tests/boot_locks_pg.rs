//! A second boot over a current observer schema takes no lock a live write holds.
//!
//! Each replica applies these scripts at boot (the checkpoint table, the observer
//! management tables, the source cursor and the dispatch ledger) while the others serve
//! traffic. Each test applies one script to a database of its own, then applies it again
//! while a live write holds every table in `core` ([`fraiseql_test_support::boot_locks`]):
//! a statement that needs a `SHARE` or stronger table lock on a current schema fails it,
//! named.
//!
//! The guards skip only what is already there, and a trigger, a policy or a default only
//! when its whole definition matches: each script's `…_boot_builds_the_pinned_shape`
//! (which also boots over each of its policies changed in one attribute),
//! [`a_trigger_differing_in_one_attribute_is_replaced`] and
//! [`a_changed_column_definition_is_put_back`] prove it.
//!
//! Self-skips when no Postgres is available; it runs in the Dagger `integration: observers`
//! suite.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each test creates its own database → safe in parallel.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::path::PathBuf;

use fraiseql_observers::migrations::{
    observer_dispatch_sql, observer_management_sql, source_cursor_sql,
};
use fraiseql_test_support::boot_locks::{
    FreshDatabase, boot_restores_the_pinned_shape, fresh_database, second_init_waits_on_nothing,
};
use sqlx::PgPool;

/// The checkpoint script, as `fraiseql_observers::checkpoint::migration_sql` includes it.
/// Read from the file, not through that function: it sits behind the `checkpoint` feature,
/// which the leg running this suite does not build the crate with.
const CHECKPOINT_SQL: &str = include_str!("../migrations/02_create_observer_checkpoints.sql");

/// The schema every observer table lives in.
const SCHEMAS: &[&str] = &["core"];

/// Apply `script` the way a boot does: whole, behind the workspace's DDL lock.
async fn apply(pool: PgPool, script: &'static str) -> Result<(), sqlx::Error> {
    // Reason: the crate's own migration DDL, behind the workspace's fixed lock statement.
    sqlx::raw_sql(sqlx::AssertSqlSafe(fraiseql_guard::ddl::lock_prefixed(script)))
        .execute(&pool)
        .await
        .map(drop)
}

/// Run `statements` on `pool`, each named in the panic if it fails.
async fn execute(pool: &PgPool, statements: &[&str]) {
    for statement in statements {
        // Reason: statements this file defines.
        sqlx::raw_sql(sqlx::AssertSqlSafe(*statement))
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("{statement}: {e}"));
    }
}

async fn database(tag: &str) -> Option<FreshDatabase> {
    let db = fresh_database(tag).await;
    if db.is_none() {
        eprintln!("SKIP {tag}: no postgres");
    }
    db
}

async fn second_boot_waits_on_nothing(tag: &str, script: &'static str) {
    let Some(db) = database(tag).await else {
        return;
    };
    second_init_waits_on_nothing(&db, SCHEMAS, move |pool| apply(pool, script)).await;
    db.drop().await;
}

async fn boot_builds_the_pinned_shape(tag: &str, script: &'static str) {
    let Some(db) = database(&format!("{tag}_shape")).await else {
        return;
    };
    let fixture: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
        "boot_shape",
        &format!("{tag}.txt"),
    ]
    .iter()
    .collect();
    boot_restores_the_pinned_shape(&db, SCHEMAS, &[script], &fixture, move |pool| {
        apply(pool, script)
    })
    .await;
    db.drop().await;
}

#[tokio::test]
async fn checkpoints_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("checkpoints", CHECKPOINT_SQL).await;
}

#[tokio::test]
async fn checkpoints_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("checkpoints", CHECKPOINT_SQL).await;
}

#[tokio::test]
async fn observer_management_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("management", observer_management_sql()).await;
}

#[tokio::test]
async fn observer_management_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("management", observer_management_sql()).await;
}

#[tokio::test]
async fn source_cursor_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("source_cursor", source_cursor_sql()).await;
}

#[tokio::test]
async fn source_cursor_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("source_cursor", source_cursor_sql()).await;
}

#[tokio::test]
async fn observer_dispatch_second_boot_waits_on_nothing() {
    second_boot_waits_on_nothing("dispatch", observer_dispatch_sql()).await;
}

#[tokio::test]
async fn observer_dispatch_boot_builds_the_pinned_shape() {
    boot_builds_the_pinned_shape("dispatch", observer_dispatch_sql()).await;
}

/// The definitions of the triggers on `relation`, by name.
async fn triggers(pool: &PgPool, relation: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT pg_get_triggerdef(oid) || ' ' || tgenabled::text FROM pg_trigger \
         WHERE tgrelid = $1::regclass AND NOT tgisinternal ORDER BY tgname",
    )
    .bind(relation)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// A trigger the guard compares by its catalog fields, and the same trigger changed in
/// each of them: what each change makes, and what it leaves beside the table to remove
/// once the boot has run.
struct Trigger {
    script:   &'static str,
    relation: &'static str,
    name:     &'static str,
    variants: &'static [(&'static str, &'static str, &'static str)],
}

/// A function no guard asks for, for the function variant.
const OTHER_FUNCTION: &str = "CREATE FUNCTION core.fq_other_trigger() RETURNS trigger \
                              LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$";

const UPDATED_AT: Trigger = Trigger {
    script:   observer_management_sql(),
    relation: "core.tb_observer",
    name:     "tr_observer_updated_at",
    variants: &[
        (
            "function",
            "DROP TRIGGER tr_observer_updated_at ON core.tb_observer; \
             CREATE TRIGGER tr_observer_updated_at BEFORE UPDATE ON core.tb_observer \
             FOR EACH ROW EXECUTE FUNCTION core.fq_other_trigger()",
            "",
        ),
        (
            "timing",
            "DROP TRIGGER tr_observer_updated_at ON core.tb_observer; \
             CREATE TRIGGER tr_observer_updated_at AFTER UPDATE ON core.tb_observer \
             FOR EACH ROW EXECUTE FUNCTION core.fn_update_timestamp()",
            "",
        ),
        (
            "events",
            "DROP TRIGGER tr_observer_updated_at ON core.tb_observer; \
             CREATE TRIGGER tr_observer_updated_at BEFORE INSERT OR UPDATE ON core.tb_observer \
             FOR EACH ROW EXECUTE FUNCTION core.fn_update_timestamp()",
            "",
        ),
        (
            "level",
            "DROP TRIGGER tr_observer_updated_at ON core.tb_observer; \
             CREATE TRIGGER tr_observer_updated_at BEFORE UPDATE ON core.tb_observer \
             FOR EACH STATEMENT EXECUTE FUNCTION core.fn_update_timestamp()",
            "",
        ),
        (
            "enabled",
            "ALTER TABLE core.tb_observer DISABLE TRIGGER tr_observer_updated_at",
            "",
        ),
        (
            "condition",
            "DROP TRIGGER tr_observer_updated_at ON core.tb_observer; \
             CREATE TRIGGER tr_observer_updated_at BEFORE UPDATE ON core.tb_observer \
             FOR EACH ROW WHEN (OLD IS DISTINCT FROM NEW) \
             EXECUTE FUNCTION core.fn_update_timestamp()",
            "",
        ),
        (
            "columns",
            "DROP TRIGGER tr_observer_updated_at ON core.tb_observer; \
             CREATE TRIGGER tr_observer_updated_at BEFORE UPDATE OF name ON core.tb_observer \
             FOR EACH ROW EXECUTE FUNCTION core.fn_update_timestamp()",
            "",
        ),
        (
            "arguments",
            "DROP TRIGGER tr_observer_updated_at ON core.tb_observer; \
             CREATE TRIGGER tr_observer_updated_at BEFORE UPDATE ON core.tb_observer \
             FOR EACH ROW EXECUTE FUNCTION core.fn_update_timestamp('x')",
            "",
        ),
        (
            "name",
            "ALTER TRIGGER tr_observer_updated_at ON core.tb_observer RENAME TO tr_moved",
            "DROP TRIGGER tr_moved ON core.tb_observer",
        ),
        (
            "table",
            "CREATE TABLE core.tb_observer_elsewhere (LIKE core.tb_observer); \
             CREATE TRIGGER tr_observer_updated_at BEFORE UPDATE ON core.tb_observer_elsewhere \
             FOR EACH ROW EXECUTE FUNCTION core.fn_update_timestamp(); \
             DROP TRIGGER tr_observer_updated_at ON core.tb_observer",
            "DROP TABLE core.tb_observer_elsewhere",
        ),
    ],
};

const CHECKPOINTS_AUDIT: Trigger = Trigger {
    script:   CHECKPOINT_SQL,
    relation: "core.observer_checkpoints",
    name:     "observer_checkpoints_audit_trigger",
    variants: &[
        (
            "function",
            "DROP TRIGGER observer_checkpoints_audit_trigger ON core.observer_checkpoints; \
             CREATE TRIGGER observer_checkpoints_audit_trigger AFTER INSERT OR UPDATE \
             ON core.observer_checkpoints FOR EACH ROW EXECUTE FUNCTION core.fq_other_trigger()",
            "",
        ),
        (
            "timing",
            "DROP TRIGGER observer_checkpoints_audit_trigger ON core.observer_checkpoints; \
             CREATE TRIGGER observer_checkpoints_audit_trigger BEFORE INSERT OR UPDATE \
             ON core.observer_checkpoints FOR EACH ROW \
             EXECUTE FUNCTION core.observer_checkpoints_audit()",
            "",
        ),
        (
            "events",
            "DROP TRIGGER observer_checkpoints_audit_trigger ON core.observer_checkpoints; \
             CREATE TRIGGER observer_checkpoints_audit_trigger AFTER UPDATE \
             ON core.observer_checkpoints FOR EACH ROW \
             EXECUTE FUNCTION core.observer_checkpoints_audit()",
            "",
        ),
        (
            "level",
            "DROP TRIGGER observer_checkpoints_audit_trigger ON core.observer_checkpoints; \
             CREATE TRIGGER observer_checkpoints_audit_trigger AFTER INSERT OR UPDATE \
             ON core.observer_checkpoints FOR EACH STATEMENT \
             EXECUTE FUNCTION core.observer_checkpoints_audit()",
            "",
        ),
        (
            "enabled",
            "ALTER TABLE core.observer_checkpoints \
             DISABLE TRIGGER observer_checkpoints_audit_trigger",
            "",
        ),
        (
            "condition",
            "DROP TRIGGER observer_checkpoints_audit_trigger ON core.observer_checkpoints; \
             CREATE TRIGGER observer_checkpoints_audit_trigger AFTER INSERT OR UPDATE \
             ON core.observer_checkpoints FOR EACH ROW WHEN (NEW.event_count > 0) \
             EXECUTE FUNCTION core.observer_checkpoints_audit()",
            "",
        ),
        (
            "columns",
            "DROP TRIGGER observer_checkpoints_audit_trigger ON core.observer_checkpoints; \
             CREATE TRIGGER observer_checkpoints_audit_trigger AFTER INSERT OR UPDATE OF \
             event_count ON core.observer_checkpoints FOR EACH ROW \
             EXECUTE FUNCTION core.observer_checkpoints_audit()",
            "",
        ),
        (
            "arguments",
            "DROP TRIGGER observer_checkpoints_audit_trigger ON core.observer_checkpoints; \
             CREATE TRIGGER observer_checkpoints_audit_trigger AFTER INSERT OR UPDATE \
             ON core.observer_checkpoints FOR EACH ROW \
             EXECUTE FUNCTION core.observer_checkpoints_audit('x')",
            "",
        ),
        (
            "name",
            "ALTER TRIGGER observer_checkpoints_audit_trigger ON core.observer_checkpoints \
             RENAME TO tr_moved",
            "DROP TRIGGER tr_moved ON core.observer_checkpoints",
        ),
        (
            "table",
            "CREATE TABLE core.observer_checkpoints_elsewhere (LIKE core.observer_checkpoints); \
             CREATE TRIGGER observer_checkpoints_audit_trigger AFTER INSERT OR UPDATE \
             ON core.observer_checkpoints_elsewhere FOR EACH ROW \
             EXECUTE FUNCTION core.observer_checkpoints_audit(); \
             DROP TRIGGER observer_checkpoints_audit_trigger ON core.observer_checkpoints",
            "DROP TABLE core.observer_checkpoints_elsewhere",
        ),
    ],
};

/// A trigger that differs from the wanted one in a single catalog field the guard reads
/// (function, timing, events, row or statement level, enabled state, condition, columns,
/// arguments), or that sits under another name or on another table, is replaced by a boot.
#[tokio::test]
async fn a_trigger_differing_in_one_attribute_is_replaced() {
    let Some(db) = database("trigger_attr").await else {
        return;
    };
    let pool = db.pool().await;
    let mut kept = Vec::new();
    for trigger in [UPDATED_AT, CHECKPOINTS_AUDIT] {
        apply(pool.clone(), trigger.script).await.unwrap();
        let wanted = triggers(&pool, trigger.relation).await;
        assert!(
            wanted.iter().any(|definition| definition.contains(trigger.name)),
            "{} is not on {}: {wanted:#?}",
            trigger.name,
            trigger.relation
        );
        execute(
            &pool,
            &[
                "DROP FUNCTION IF EXISTS core.fq_other_trigger()",
                OTHER_FUNCTION,
            ],
        )
        .await;
        for (attribute, change, cleanup) in trigger.variants {
            execute(&pool, &[change]).await;
            assert_ne!(
                triggers(&pool, trigger.relation).await,
                wanted,
                "the {attribute} variant of {} changed nothing",
                trigger.name
            );
            apply(pool.clone(), trigger.script).await.unwrap();
            if !cleanup.is_empty() {
                execute(&pool, &[cleanup]).await;
            }
            if triggers(&pool, trigger.relation).await != wanted {
                kept.push(format!("{} with another {attribute}", trigger.name));
            }
        }
    }
    pool.close().await;
    db.drop().await;
    assert!(
        kept.is_empty(),
        "a boot kept a trigger that differs from the wanted one: {kept:#?}"
    );
}

/// The definition of `column` on `core.tb_observer`: its type and its default.
async fn column(pool: &PgPool, column: &str) -> String {
    sqlx::query_scalar(
        "SELECT format_type(a.atttypid, a.atttypmod) || ' default ' \
                || coalesce(pg_get_expr(d.adbin, d.adrelid), '-') \
         FROM pg_attribute a \
         LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
         WHERE a.attrelid = 'core.tb_observer'::regclass AND a.attname = $1",
    )
    .bind(column)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A column whose type or default a boot reconciles, held changed, is put back by the
/// next boot: the guards compare the definition, not only that a type or default exists.
#[tokio::test]
async fn a_changed_column_definition_is_put_back() {
    const CHANGES: &[(&str, &str)] = &[
        (
            "retry_config",
            "ALTER TABLE core.tb_observer ALTER COLUMN retry_config \
             SET DEFAULT '{\"max_attempts\": 5}'::jsonb",
        ),
        (
            "retry_config",
            "ALTER TABLE core.tb_observer ALTER COLUMN retry_config DROP DEFAULT",
        ),
        (
            "managed_by",
            "ALTER TABLE core.tb_observer ALTER COLUMN managed_by TYPE varchar(64)",
        ),
        (
            "created_by",
            "ALTER TABLE core.tb_observer ALTER COLUMN created_by TYPE varchar(255)",
        ),
        (
            "updated_by",
            "ALTER TABLE core.tb_observer ALTER COLUMN updated_by TYPE varchar(255)",
        ),
    ];
    let Some(db) = database("column_change").await else {
        return;
    };
    let pool = db.pool().await;
    let script = observer_management_sql();
    apply(pool.clone(), script).await.unwrap();
    let mut kept = Vec::new();
    for (name, change) in CHANGES {
        let wanted = column(&pool, name).await;
        execute(&pool, &[change]).await;
        assert_ne!(column(&pool, name).await, wanted, "{change} changed nothing");
        apply(pool.clone(), script).await.unwrap();
        let rebuilt = column(&pool, name).await;
        if rebuilt != wanted {
            kept.push(format!("{change}: {rebuilt}, wanted {wanted}"));
        }
    }
    pool.close().await;
    db.drop().await;
    assert!(kept.is_empty(), "a boot kept a changed column: {kept:#?}");
}
