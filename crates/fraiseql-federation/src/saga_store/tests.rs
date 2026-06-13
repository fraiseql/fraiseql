#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: CLI / test / example / bench code prints to stdout/stderr by design
use super::*;

/// S44b: `cleanup_all` must be crate-private; only `cleanup_all_for_testing` is pub.
/// This test confirms the testing wrapper exists and is callable from within the crate.
#[test]
fn test_cleanup_all_for_testing_is_accessible() {
    #[allow(dead_code)] // Reason: static-existence check for cleanup_all_for_testing; never called to avoid DB requirement
    async fn _check(store: &PostgresSagaStore) {
        let _ = store.cleanup_all_for_testing().await;
    }
}

/// S44a: schema DDL must use single-prefix table names (trinity convention: tb_<entity>).
#[test]
fn test_schema_ddl_uses_single_prefix_table_names() {
    assert_eq!(
        PostgresSagaStore::TABLE_SAGAS,
        "tb_federation_sagas",
        "main saga table must follow trinity single-prefix convention"
    );
    assert_eq!(
        PostgresSagaStore::TABLE_STEPS,
        "tb_federation_saga_steps",
        "saga steps table must follow trinity single-prefix convention"
    );
    assert_eq!(
        PostgresSagaStore::TABLE_RECOVERY,
        "tb_federation_saga_recovery",
        "saga recovery table must follow trinity single-prefix convention"
    );
}

/// M-saga-store-defaults: an unrecognised stored state string must NOT coerce
/// to a default.
///
/// `map_saga_row` / `map_saga_step_row` now propagate
/// [`SagaStoreError::CorruptStoredValue`] when these `from_str` parsers return
/// `None` (replacing the old `.unwrap_or(default)` coercion). The mappers
/// themselves require a `tokio_postgres::Row`, which cannot be constructed
/// outside a live query, so this test pins the parser contract the mappers
/// depend on: an unknown value yields `None`, which the mappers turn into
/// `CorruptStoredValue`.
#[test]
fn test_unknown_stored_enum_values_do_not_coerce_to_default() {
    assert!(SagaState::from_str("bogus").is_none(), "unknown saga state must not coerce");
    assert!(StepState::from_str("bogus").is_none(), "unknown step state must not coerce");
    assert!(
        MutationType::from_str("bogus").is_none(),
        "unknown mutation type must not coerce"
    );

    // Sanity: known values still parse.
    assert_eq!(SagaState::from_str("completed"), Some(SagaState::Completed));
    assert_eq!(StepState::from_str("failed"), Some(StepState::Failed));
    assert_eq!(MutationType::from_str("delete"), Some(MutationType::Delete));
}

/// `CorruptStoredValue` must surface the offending column and value in its
/// `Display` output for operator triage.
#[test]
fn test_corrupt_stored_value_display() {
    let err = SagaStoreError::CorruptStoredValue {
        column: "state".into(),
        value:  "bogus".into(),
    };
    let rendered = err.to_string();
    assert!(rendered.contains("state"), "display must name the column: {rendered}");
    assert!(rendered.contains("bogus"), "display must show the value: {rendered}");
}

#[tokio::test]
async fn test_postgres_connection() {
    if std::env::var("DATABASE_URL").is_err() {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    }
    // Use SAGA_STORE_TEST_URL (postgres-specific) so this test is unaffected
    // when the suite is invoked with DATABASE_URL pointing to MySQL/SQL Server.
    let url = std::env::var("SAGA_STORE_TEST_URL").unwrap_or_else(|_| {
        "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql".to_string()
    });
    let store = PostgresSagaStore::new(&url).await.expect("Failed to create store");
    store.health_check().await.expect("Health check failed");
}
