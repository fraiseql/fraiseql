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
    // `SAGA_STORE_TEST_URL` still wins, so a run CAN point this at a store of its
    // own — but there is no localhost fallback behind it. The old default was
    // `…@localhost:5433/test_fraiseql`, this box's developer database, and the
    // guard above only asked whether DATABASE_URL was SET: in a leg that binds
    // Postgres under a service alias the guard passed, the fallback dialled a
    // localhost nothing listens on, and the test panicked. It had never run
    // anywhere to show that — the only leg compiling it bound no database, so the
    // guard skipped it — until #1297 wired this module into `integration
    // (postgres)`.
    //
    // Same shape as #1295, and the same answer: the fallback is not a convenience,
    // it is the defect. Skipping when neither variable is set stays correct for a
    // developer with no database; dialling a host that happens to be right on one
    // machine is what made the failure invisible.
    let Some(url) = std::env::var("SAGA_STORE_TEST_URL")
        .ok()
        .or_else(fraiseql_test_support::try_database_url)
    else {
        eprintln!("Skipping: neither SAGA_STORE_TEST_URL nor DATABASE_URL is set");
        return;
    };
    let store = PostgresSagaStore::new(&url).await.expect("Failed to create store");
    store.health_check().await.expect("Health check failed");
}

/// The saga store's times as PostgreSQL holds them, and what a load reads back. Each
/// test skips when no database is configured, like `test_postgres_connection`.
mod stored_times {
    use super::*;

    type Instant = fraiseql_time::Timestamp;

    fn at(rfc3339: &str) -> Instant {
        Instant::parse_rfc3339(rfc3339).expect("a valid RFC 3339 instant")
    }

    fn database_url() -> Option<String> {
        std::env::var("SAGA_STORE_TEST_URL")
            .ok()
            .or_else(fraiseql_test_support::try_database_url)
    }

    async fn opened(url: &str) -> PostgresSagaStore {
        let store = PostgresSagaStore::new(url).await.expect("the store opens");
        store.migrate_schema().await.expect("the store's tables exist");
        store
    }

    /// A raw connection whose session renders `timestamptz` in UTC.
    async fn raw(url: &str) -> tokio_postgres::Client {
        let (client, conn) =
            tokio_postgres::connect(url, tokio_postgres::NoTls).await.expect("connect");
        tokio::spawn(conn);
        client.batch_execute("SET TIME ZONE 'UTC'").await.expect("set the session zone");
        client
    }

    fn saga(id: Uuid, created_at: Instant, completed_at: Option<Instant>) -> Saga {
        Saga {
            id,
            state: SagaState::Pending,
            created_at,
            completed_at,
            metadata: None,
        }
    }

    fn step(saga_id: Uuid, started_at: Option<Instant>, completed_at: Option<Instant>) -> SagaStep {
        SagaStep {
            id: Uuid::new_v4(),
            saga_id,
            order: 1,
            subgraph: "orders".to_string(),
            remote: false,
            mutation_type: MutationType::Create,
            mutation_name: None,
            typename: "Order".to_string(),
            variables: serde_json::json!({}),
            state: StepState::Pending,
            result: None,
            started_at,
            completed_at,
            compensation_mutation: None,
            compensation_variables: None,
            required_fields: Vec::new(),
            compensation_error: None,
        }
    }

    async fn stored_text(
        client: &tokio_postgres::Client,
        sql: &str,
        id: Uuid,
    ) -> Vec<Option<String>> {
        let row = client.query_one(sql, &[&id]).await.expect("the row is stored");
        (0..row.len()).map(|i| row.get(i)).collect()
    }

    /// A saga, its step and a recovery record store their times as these `timestamptz`
    /// values: whole microseconds, the nanoseconds below truncated. A load reads the
    /// same instants back.
    #[tokio::test]
    async fn saga_step_and_recovery_times_are_stored_as_today() {
        let Some(url) = database_url() else {
            eprintln!("Skipping: neither SAGA_STORE_TEST_URL nor DATABASE_URL is set");
            return;
        };
        let store = opened(&url).await;
        let client = raw(&url).await;

        let id = Uuid::new_v4();
        store
            .save_saga(&saga(
                id,
                at("2026-03-01T12:34:56.123456789Z"),
                Some(at("2026-03-01T12:35:00Z")),
            ))
            .await
            .expect("the saga saves");
        assert_eq!(
            stored_text(
                &client,
                "SELECT created_at::text, completed_at::text FROM tb_federation_sagas WHERE id = $1",
                id
            )
            .await,
            [Some("2026-03-01 12:34:56.123456+00".to_owned()), Some("2026-03-01 12:35:00+00".to_owned())],
            "the saga's stored times"
        );
        let loaded = store.load_saga(id).await.expect("the saga loads").expect("it exists");
        assert_eq!(loaded.created_at.to_rfc3339(), "2026-03-01T12:34:56.123456+00:00");
        assert_eq!(
            loaded.completed_at.map(|t| t.to_rfc3339()).as_deref(),
            Some("2026-03-01T12:35:00+00:00")
        );

        let s = step(id, Some(at("1999-12-31T23:59:59.999999Z")), None);
        store.save_saga_step(&s).await.expect("the step saves");
        assert_eq!(
            stored_text(
                &client,
                "SELECT started_at::text, completed_at::text FROM tb_federation_saga_steps WHERE id = $1",
                s.id
            )
            .await,
            [Some("1999-12-31 23:59:59.999999+00".to_owned()), None],
            "the step's stored times"
        );
        let loaded = store.load_saga_step(s.id).await.expect("the step loads").expect("it exists");
        assert_eq!(
            loaded.started_at.map(|t| t.to_rfc3339()).as_deref(),
            Some("1999-12-31T23:59:59.999999+00:00")
        );

        let recovery = SagaRecovery {
            id:            Uuid::new_v4(),
            saga_id:       id,
            recovery_type: "retry".to_string(),
            attempted_at:  at("1970-01-01T00:00:00.000001Z"),
            last_attempt:  Some(at("1969-12-31T23:59:59.5Z")),
            attempt_count: 1,
            last_error:    None,
        };
        store.save_recovery_record(&recovery).await.expect("the record saves");
        assert_eq!(
            stored_text(
                &client,
                "SELECT attempted_at::text, last_attempt::text FROM tb_federation_saga_recovery WHERE id = $1",
                recovery.id
            )
            .await,
            [Some("1970-01-01 00:00:00.000001+00".to_owned()), Some("1969-12-31 23:59:59.5+00".to_owned())],
            "the recovery record's stored times"
        );
        remove(&url, id).await;
    }

    /// Stores a saga, then sets `column` of its row (or of its step's row) to `stored`.
    async fn saga_with_stored(
        url: &str,
        table: &str,
        column: &str,
        stored: &str,
    ) -> (PostgresSagaStore, Uuid, Uuid) {
        let store = opened(url).await;
        let id = Uuid::new_v4();
        store
            .save_saga(&saga(id, at("2026-03-01T12:00:00Z"), None))
            .await
            .expect("the saga saves");
        let s = step(id, Some(at("2026-03-01T12:00:01Z")), None);
        store.save_saga_step(&s).await.expect("the step saves");
        let row_id = if table == "tb_federation_sagas" {
            id
        } else {
            s.id
        };
        raw(url)
            .await
            .execute(
                &format!("UPDATE {table} SET {column} = $2::text::timestamptz WHERE id = $1"),
                &[&row_id, &stored],
            )
            .await
            .expect("the time is set");
        (store, id, s.id)
    }

    /// Deletes a saga; its steps and recovery records go with it. The store's list loads
    /// read every saga in a state, so a row left unreadable here would fail them for every
    /// suite sharing the database.
    async fn remove(url: &str, id: Uuid) {
        raw(url)
            .await
            .execute("DELETE FROM tb_federation_sagas WHERE id = $1", &[&id])
            .await
            .expect("the saga is removed");
    }

    fn assert_corrupt<T: std::fmt::Debug>(loaded: Result<T>, column: &str, named: &str) {
        let err = loaded.expect_err("the load reports the stored time");
        assert!(
            matches!(&err, SagaStoreError::CorruptStoredValue { .. }),
            "expected a corrupt {column}, got {err:?}"
        );
        if let SagaStoreError::CorruptStoredValue { column: c, value } = err {
            assert_eq!(c, column, "the report names the column");
            assert!(value.contains(named), "the report names the value: {value}");
        }
    }

    /// A stored time the store cannot read is a corrupt value of that column, named
    /// with the value, on the load that reads it.
    async fn an_unreadable_time_is_a_corrupt_value(stored: &str, named: &str) {
        let Some(url) = database_url() else {
            eprintln!("Skipping: neither SAGA_STORE_TEST_URL nor DATABASE_URL is set");
            return;
        };
        let (store, id, _) =
            saga_with_stored(&url, "tb_federation_sagas", "created_at", stored).await;
        let loaded = store.load_saga(id).await;
        remove(&url, id).await;
        assert_corrupt(loaded, "created_at", named);

        let (store, id, step_id) =
            saga_with_stored(&url, "tb_federation_saga_steps", "started_at", stored).await;
        let loaded = store.load_saga_step(step_id).await;
        remove(&url, id).await;
        assert_corrupt(loaded, "started_at", named);
    }

    #[tokio::test]
    async fn an_infinite_stored_time_is_a_corrupt_value() {
        an_unreadable_time_is_a_corrupt_value("infinity", "infinity").await;
    }

    /// A saga's times hold years -9999 to 9999; PostgreSQL's `timestamptz` reaches
    /// further. A stored time past 9999 is out of that range.
    #[tokio::test]
    async fn a_stored_time_past_year_9999_is_a_corrupt_value() {
        an_unreadable_time_is_a_corrupt_value("10000-01-01 00:00:00+00", "252455616000000000")
            .await;
    }
}

/// A lease or staleness offset past the first or last instant holds there instead of
/// failing the claim.
#[test]
fn an_offset_past_the_range_holds_at_its_end() {
    let now = fraiseql_time::Timestamp::parse_rfc3339("2026-03-01T12:00:00Z").expect("an instant");
    assert_eq!(offset(now, 60).to_rfc3339(), "2026-03-01T12:01:00+00:00");
    assert_eq!(offset(now, -60).to_rfc3339(), "2026-03-01T11:59:00+00:00");
    assert_eq!(offset(now, i128::from(i64::MAX)), fraiseql_time::jiff::Timestamp::MAX.into());
    assert_eq!(offset(now, -i128::from(i64::MAX)), fraiseql_time::jiff::Timestamp::MIN.into());
}
