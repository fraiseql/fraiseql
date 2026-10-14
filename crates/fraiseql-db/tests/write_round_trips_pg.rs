//! What one mutation costs on the wire, and what its function sees (#1572).
//!
//! A mutation runs in one transaction: `BEGIN`, its configuration (session variables, the
//! #366 CDC marker, the timing variable, the outbox's `started_at`), the function call,
//! `COMMIT`. These tests route the adapter through a proxy that counts, on its one
//! connection, how often the client speaks after the server has answered (a round trip) and
//! how many statements it asks PostgreSQL to parse (a plan).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL`.

#![cfg(all(feature = "postgres", feature = "test-postgres"))]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use fraiseql_db::{
    PostgresAdapter, Writer as _,
    changelog::{CDC_MEDIATED_ON, CDC_MEDIATED_VAR, STARTED_AT_VAR},
};
use fraiseql_test_support::wire::{CountingProxy, Wire};

/// A direct connection to the test database, for fixtures.
async fn fixture_client() -> tokio_postgres::Client {
    let (client, connection) =
        tokio_postgres::connect(&fraiseql_test_support::database_url(), tokio_postgres::NoTls)
            .await
            .unwrap();
    tokio::spawn(connection);
    client
}

/// `fn_1572_echo` returns what a mutation's function body reads of its configuration;
/// `fn_1572_outbox` returns the `mutation_response` the outbox reads, its `message` carrying
/// the `started_at` the function saw.
async fn install_fixtures() {
    let client = fixture_client().await;
    client
        .batch_execute(
            "CREATE OR REPLACE FUNCTION public.fn_1572_echo() \
             RETURNS TABLE(cdc_mediated text, timer text, started_at text, tenant text) \
             LANGUAGE sql AS $$ SELECT current_setting('fraiseql.cdc_mediated', true), \
             current_setting('app.timer_1572', true), \
             current_setting('fraiseql.started_at', true), \
             current_setting('app.tenant_1572', true) $$;
             CREATE TABLE IF NOT EXISTS public.t_1572 (tag text);
             CREATE OR REPLACE FUNCTION public.fn_1572_insert(p_tag text) \
             RETURNS TABLE(tag text) LANGUAGE sql AS $$ \
             INSERT INTO public.t_1572 VALUES (p_tag) RETURNING tag $$;",
        )
        .await
        .unwrap();
    client
        .batch_execute(
            "CREATE SCHEMA IF NOT EXISTS app;
             DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
             'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
             'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
             DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, \
             state_changed BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, \
             http_status SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
             updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
             EXCEPTION WHEN duplicate_object THEN NULL; END $$;
             CREATE OR REPLACE FUNCTION public.fn_1572_outbox(p_id uuid) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             v.succeeded := true; v.state_changed := true; v.entity_type := 'Outbox1572'; \
             v.entity_id := p_id; v.message := current_setting('fraiseql.started_at', true); \
             RETURN v; END; $$;",
        )
        .await
        .unwrap();
    client
        .batch_execute(&fraiseql_test_support::changelog::entity_change_log_provision_sql())
        .await
        .unwrap();
}

/// One adapter connection, through a counting proxy.
async fn proxied_adapter() -> (PostgresAdapter, CountingProxy) {
    install_fixtures().await;
    let proxy = CountingProxy::start(&fraiseql_test_support::database_url());
    let adapter = PostgresAdapter::with_pool_size(proxy.url(), 1).await.unwrap();
    (adapter, proxy)
}

/// Run `request` `times` times after one warm-up call (which prepares what the connection
/// caches), and return the round trips and parses per call.
async fn cost_of(
    adapter: &PostgresAdapter,
    wire: &Wire,
    request: &fraiseql_db::WriteRequest<'_>,
    times: u64,
) -> (u64, u64) {
    adapter.execute_write(request, &|_| Ok(())).await.unwrap();
    wire.reset();
    for _ in 0..times {
        adapter.execute_write(request, &|_| Ok(())).await.unwrap();
    }
    let (round_trips, parses) = (wire.round_trips(), wire.parses());
    assert_eq!(round_trips % times, 0, "{round_trips} round trips over {times} identical calls");
    (round_trips / times, parses)
}

/// Every call is single-threaded so the driver's writes reach the proxy in the order it
/// issued them: the count is the protocol's, not the scheduler's.
#[tokio::test(flavor = "current_thread")]
async fn a_plain_mutation_takes_three_round_trips_and_plans_nothing() {
    let (adapter, proxy) = proxied_adapter().await;
    let request = fraiseql_db::WriteRequest::new("public.fn_1572_echo", &[]);

    let (round_trips, parses) = cost_of(&adapter, proxy.wire(), &request, 10).await;

    assert_eq!(
        (round_trips, parses),
        (3, 0),
        "BEGIN, the configuration and the call in one flight, COMMIT; nothing re-planned"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_mutation_with_session_variables_and_timing_takes_three_round_trips() {
    let (adapter, proxy) = proxied_adapter().await;
    let adapter = adapter.with_mutation_timing("app.timer_1572");
    let session = [("app.tenant_1572", "t-1")];
    let request =
        fraiseql_db::WriteRequest::new("public.fn_1572_echo", &[]).with_session_vars(&session);

    let (round_trips, parses) = cost_of(&adapter, proxy.wire(), &request, 10).await;

    assert_eq!((round_trips, parses), (3, 0));
}

/// What the function body reads, by column.
async fn echo(
    adapter: &PostgresAdapter,
    session: &[(&str, &str)],
) -> std::collections::HashMap<String, serde_json::Value> {
    let request =
        fraiseql_db::WriteRequest::new("public.fn_1572_echo", &[]).with_session_vars(session);
    adapter.execute_write(&request, &|_| Ok(())).await.unwrap().remove(0)
}

#[tokio::test(flavor = "current_thread")]
async fn the_function_sees_the_cdc_marker() {
    let (adapter, _proxy) = proxied_adapter().await;

    let seen = echo(&adapter, &[]).await;

    assert_eq!(seen["cdc_mediated"], CDC_MEDIATED_ON, "{CDC_MEDIATED_VAR} on every mutation");
}

#[tokio::test(flavor = "current_thread")]
async fn the_function_sees_the_timing_variable_stamped_by_the_database() {
    let (adapter, _proxy) = proxied_adapter().await;
    let adapter = adapter.with_mutation_timing("app.timer_1572");

    let seen = echo(&adapter, &[("app.tenant_1572", "t-1")]).await;

    let timer = seen["timer"].as_str().expect("the timing variable is set");
    timer
        .parse::<chrono::DateTime<chrono::FixedOffset>>()
        .or_else(|_| chrono::DateTime::parse_from_str(timer, "%Y-%m-%d %H:%M:%S%.f%#z"))
        .unwrap_or_else(|_| panic!("the timing variable `{timer}` is a database timestamp"));
    assert_eq!(seen["tenant"], "t-1", "and the session variables beside it");
}

#[tokio::test(flavor = "current_thread")]
async fn without_an_outbox_started_at_is_not_set() {
    let (adapter, _proxy) = proxied_adapter().await;

    let seen = echo(&adapter, &[]).await;

    // With timing off nothing stamps it either: the default timing variable is this one.
    assert!(
        seen["started_at"].is_null() || seen["started_at"] == "",
        "only the outbox (or timing) reads it: got {}",
        seen["started_at"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn an_outbox_mutation_takes_three_round_trips() {
    let (adapter, proxy) = proxied_adapter().await;
    let id = serde_json::json!(uuid::Uuid::new_v4().to_string());
    let args = [id];
    let changelog = fraiseql_db::ChangeLogWrite::new("Outbox1572", "UPDATE");
    let session = [("app.tenant_1572", "t-1")];
    let request = fraiseql_db::WriteRequest::new("public.fn_1572_outbox", &args)
        .with_session_vars(&session)
        .with_changelog(Some(&changelog));

    let (round_trips, parses) = cost_of(&adapter, proxy.wire(), &request, 10).await;

    assert_eq!((round_trips, parses), (3, 0));
}

/// The outbox computes the mutation's duration from `started_at`, so the adapter stamps it
/// when the session does not carry one, and only then.
async fn started_at_seen_by_an_outbox_mutation(session: &[(&str, &str)]) -> String {
    let (adapter, _proxy) = proxied_adapter().await;
    let args = [serde_json::json!(uuid::Uuid::new_v4().to_string())];
    let changelog = fraiseql_db::ChangeLogWrite::new("Outbox1572", "UPDATE");
    let request = fraiseql_db::WriteRequest::new("public.fn_1572_outbox", &args)
        .with_session_vars(session)
        .with_changelog(Some(&changelog));
    let mut rows = adapter.execute_write(&request, &|_| Ok(())).await.unwrap();
    rows.remove(0)["message"].as_str().unwrap_or_default().to_owned()
}

#[tokio::test(flavor = "current_thread")]
async fn an_outbox_mutation_stamps_started_at_when_the_session_has_none() {
    let started_at = started_at_seen_by_an_outbox_mutation(&[]).await;

    assert!(!started_at.is_empty(), "the outbox reads started_at without missing_ok");
}

#[tokio::test(flavor = "current_thread")]
async fn a_started_at_the_session_carries_is_not_stamped_over() {
    // A second ago on this machine, which the duration computation accepts.
    let carried = (chrono::Utc::now() - chrono::Duration::seconds(1))
        .format("%Y-%m-%d %H:%M:%S%.6f+00")
        .to_string();

    let started_at = started_at_seen_by_an_outbox_mutation(&[(STARTED_AT_VAR, &carried)]).await;

    assert_eq!(started_at, carried);
}

#[tokio::test(flavor = "current_thread")]
async fn a_configuration_that_fails_returns_its_own_error_and_writes_nothing() {
    let (adapter, _proxy) = proxied_adapter().await;
    let tag = uuid::Uuid::new_v4().to_string();
    let args = [serde_json::json!(tag)];
    let request = fraiseql_db::WriteRequest::new("public.fn_1572_insert", &args)
        .with_session_vars(&[("not a variable name", "x")]);

    let refused = adapter.execute_write(&request, &|_| Ok(())).await;

    match refused {
        Err(fraiseql_error::FraiseQLError::Database { sql_state, .. }) => assert_eq!(
            sql_state.as_deref(),
            Some("42704"),
            "the configuration's own error, not the call's 25P02 (transaction aborted)"
        ),
        other => panic!("expected the configuration's database error, got {other:?}"),
    }
    let written: i64 = fixture_client()
        .await
        .query_one("SELECT count(*) FROM public.t_1572 WHERE tag = $1", &[&tag])
        .await
        .unwrap()
        .get(0);
    assert_eq!(written, 0, "the call must not commit without its configuration");
}
