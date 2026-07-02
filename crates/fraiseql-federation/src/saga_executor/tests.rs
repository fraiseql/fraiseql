#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use uuid::Uuid;

use super::SagaExecutor;
use crate::saga_store::SagaStoreError;

#[test]
fn test_saga_executor_creation() {
    let executor = SagaExecutor::new();
    drop(executor);
}

#[test]
fn test_saga_executor_default() {
    let _executor = SagaExecutor::default();
    // Default should work
}

#[test]
fn test_saga_executor_with_store() {
    // Test that we can create an executor; full store testing requires a
    // database (integration tests).
    let executor = SagaExecutor::new();
    assert!(!executor.has_store());
}

#[test]
fn test_saga_executor_has_store_method() {
    let executor = SagaExecutor::new();
    assert!(!executor.has_store());
}

/// H32: `execute_step` must fail loud — distributed saga execution is unwired,
/// so it must never fabricate a result or persist a step transition.
#[tokio::test]
async fn execute_step_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor
        .execute_step(saga_id, 1, "testMutation", &serde_json::json!({}), "test-service")
        .await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_step must fail loud, got: {result:?}"
    );
}

/// H32: the no-store path must also fail loud (it previously returned a
/// fabricated placeholder success).
#[tokio::test]
async fn execute_step_without_store_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor
        .execute_step(saga_id, 1, "createOrder", &serde_json::json!({}), "orders-service")
        .await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_step without store must fail loud, got: {result:?}"
    );
}

/// H32: the forward-phase driver must fail loud rather than reporting empty or
/// fabricated step results.
#[tokio::test]
async fn execute_saga_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor.execute_saga(saga_id).await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_saga must fail loud, got: {result:?}"
    );
}

/// H32: `get_execution_state` derived its values from fabricated step states; it
/// must now fail loud.
#[tokio::test]
async fn get_execution_state_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor.get_execution_state(saga_id).await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "get_execution_state must fail loud, got: {result:?}"
    );
}

/// H32: the `operation` string must identify the failing entry point so callers
/// and logs can tell the unwired paths apart.
#[tokio::test]
async fn execute_step_operation_is_descriptive() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor
        .execute_step(saga_id, 1, "mutation", &serde_json::json!({}), "service")
        .await;

    match result {
        Err(SagaStoreError::NotImplemented { operation }) => {
            assert_eq!(operation, "SagaExecutor::execute_step");
        },
        other => panic!("expected NotImplemented, got: {other:?}"),
    }
}

/// Wired forward-phase execution (`unstable-saga`). `execute_step_local` dispatches
/// the step's real local mutation through a `DatabaseAdapter` and reports the
/// outcome without fabricating success. Proven here against an in-memory SQLite
/// adapter (single connection so the schema is shared) — no external service.
#[cfg(feature = "unstable-saga")]
mod wired {
    use std::sync::Arc;

    use fraiseql_db::sqlite::SqliteAdapter;
    use serde_json::json;
    use uuid::Uuid;

    use crate::{
        mutation_executor::FederationMutationExecutor,
        saga_executor::{RetryPolicy, SagaExecutor},
        saga_store::{MutationType, SagaStep, StepState},
        types::{FederatedType, FederationMetadata, KeyDirective},
    };

    fn order_metadata() -> FederationMetadata {
        FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types: vec![FederatedType {
                name:                "Order".to_string(),
                keys:                vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:          false,
                external_fields:     Vec::new(),
                shareable_fields:    Vec::new(),
                inaccessible_fields: Vec::new(),
                field_directives:    std::collections::HashMap::new(),
                type_shareable:      false,
            }],
            remote_subscription_fields: std::collections::HashMap::new(),
        }
    }

    fn order_step(mutation_type: MutationType, variables: serde_json::Value) -> SagaStep {
        SagaStep {
            id: Uuid::new_v4(),
            saga_id: Uuid::new_v4(),
            order: 0,
            subgraph: "orders".to_string(),
            mutation_type,
            mutation_name: None,
            typename: "Order".to_string(),
            variables,
            state: StepState::Pending,
            result: None,
            started_at: None,
            completed_at: None,
            compensation_mutation: None,
            compensation_variables: None,
        }
    }

    /// Single-connection in-memory SQLite with an `"order"` table — the
    /// `lowercase(typename)` table the federation mutation builder targets.
    /// Returns the wired executor plus a handle to the same adapter so a test can
    /// read the database back directly.
    async fn order_table_executor()
    -> (FederationMutationExecutor<SqliteAdapter>, Arc<SqliteAdapter>) {
        use fraiseql_db::traits::DatabaseAdapter;

        // A single connection keeps the schema visible across queries: each
        // `sqlite::memory:` connection is otherwise a separate database.
        let adapter =
            Arc::new(SqliteAdapter::with_pool_config("sqlite::memory:", 1, 1).await.unwrap());
        adapter
            .execute_raw_query("CREATE TABLE \"order\" (id TEXT PRIMARY KEY, total TEXT)")
            .await
            .unwrap();
        let executor =
            FederationMutationExecutor::new(Arc::clone(&adapter), order_metadata(), false);
        (executor, adapter)
    }

    #[tokio::test]
    async fn execute_step_local_dispatches_real_create() {
        use fraiseql_db::traits::DatabaseAdapter;

        let executor = SagaExecutor::new();
        let (mutation_executor, adapter) = order_table_executor().await;
        let step = order_step(MutationType::Create, json!({"id": "o1", "total": "100"}));

        let result = executor.execute_step_local(&mutation_executor, &step).await;

        assert!(result.success, "a successful create must report success: {result:?}");
        assert_eq!(result.step_number, 1, "0-based order maps to 1-indexed step number");
        let data = result.data.expect("a successful step must carry the read-back entity");
        assert_eq!(data["id"], "o1", "result must reflect the real inserted row: {data}");
        assert_eq!(data["__typename"], "Order");

        // The row really landed in the database — not a fabricated response.
        let rows = adapter
            .execute_raw_query("SELECT id FROM \"order\" WHERE id = 'o1'")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "the create must have persisted a real row");
    }

    #[tokio::test]
    async fn execute_step_local_failed_mutation_reports_failure_not_fabricated_success() {
        let executor = SagaExecutor::new();
        let (mutation_executor, _adapter) = order_table_executor().await;
        // UPDATE targeting an id that does not exist → 0 rows → NotFound. The
        // step must report failure, never a fabricated Completed (audit H32).
        let step = order_step(MutationType::Update, json!({"id": "missing", "total": "5"}));

        let result = executor.execute_step_local(&mutation_executor, &step).await;

        assert!(!result.success, "a 0-row update must report failure: {result:?}");
        assert!(result.data.is_none(), "a failed step must not fabricate result data");
        assert!(result.error.is_some(), "a failed step must carry the error: {result:?}");
    }

    // ── Remote step dispatch via HttpMutationClient (#429 Phase 04) ────────────
    //
    // `dispatch_step(_, _, Some((client, url)))` routes the step over HTTP to a
    // peer subgraph instead of the local SQL adapter. A `new_for_test` client
    // skips the SSRF guard so the mock can be a loopback http server; the SQLite
    // executor here only supplies federation metadata (its adapter is untouched on
    // the remote path).

    use reqwest::Url;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_string_contains, method, path},
    };

    use crate::mutation_http_client::{HttpMutationClient, HttpMutationConfig};

    /// A remote mock subgraph returning `data.create` maps to a `success: true`
    /// step carrying the mock's response entity — proving the step ran over HTTP,
    /// not against the local adapter.
    #[tokio::test]
    async fn dispatch_step_remote_success_returns_mock_response() {
        use fraiseql_db::traits::DatabaseAdapter;

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": { "create": { "__typename": "Order", "id": "remote-1" } }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let (mutation_executor, adapter) = order_table_executor().await;
        let client = HttpMutationClient::new_for_test(HttpMutationConfig::default()).unwrap();
        let url = Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        // Create step; the store persists only the verb, so `create` is the op name.
        let step = order_step(MutationType::Create, json!({"id": "remote-1", "total": "42"}));

        let (result, state) =
            SagaExecutor::dispatch_step(&mutation_executor, &step, Some((&client, &url))).await;

        assert!(result.success, "a 200 mock response must succeed: {result:?}");
        assert_eq!(state, StepState::Completed);
        let data = result.data.expect("a successful remote step carries the mock entity");
        assert_eq!(data["id"], "remote-1", "the remote response is returned, not a local row");

        // The row must NOT exist locally — the step went over HTTP, not to SQLite.
        let rows = adapter
            .execute_raw_query("SELECT id FROM \"order\" WHERE id = 'remote-1'")
            .await
            .unwrap();
        assert!(rows.is_empty(), "a remote step must not touch the local table");
    }

    /// A remote mock returning HTTP 500 maps to a real `success: false` step
    /// (never fabricated success), with the error captured.
    #[tokio::test]
    async fn dispatch_step_remote_failure_reports_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let (mutation_executor, _adapter) = order_table_executor().await;
        // Single attempt with a tiny delay keeps the failure test fast.
        let config = HttpMutationConfig {
            timeout_ms:     2000,
            max_retries:    1,
            retry_delay_ms: 1,
        };
        let client = HttpMutationClient::new_for_test(config).unwrap();
        let url = Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let step = order_step(MutationType::Create, json!({"id": "remote-x", "total": "1"}));

        let (result, state) =
            SagaExecutor::dispatch_step(&mutation_executor, &step, Some((&client, &url))).await;

        assert!(!result.success, "a 500 mock response must fail the step: {result:?}");
        assert_eq!(state, StepState::Failed);
        assert!(result.data.is_none(), "a failed remote step fabricates no result data");
        assert!(result.error.is_some(), "a failed remote step carries the error: {result:?}");
    }

    /// When a step carries a full `mutation_name`, remote dispatch sends THAT as
    /// the GraphQL operation name — not the coarse mutation-kind verb (#429
    /// hardening: full remote mutation-name persistence). The mock only matches a
    /// request body containing `createOrder`, so a request that sent the verb
    /// `create` would not match and the step would fail.
    #[tokio::test]
    async fn dispatch_step_remote_uses_full_mutation_name_when_present() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_string_contains("createOrder"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": { "createOrder": { "__typename": "Order", "id": "o-named" } }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let (mutation_executor, _adapter) = order_table_executor().await;
        let client = HttpMutationClient::new_for_test(HttpMutationConfig::default()).unwrap();
        let url = Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let mut step = order_step(MutationType::Create, json!({"id": "o-named", "total": "9"}));
        step.mutation_name = Some("createOrder".to_string());

        let (result, state) =
            SagaExecutor::dispatch_step(&mutation_executor, &step, Some((&client, &url))).await;

        assert!(result.success, "the named remote mutation must succeed: {result:?}");
        assert_eq!(state, StepState::Completed);
        let data = result.data.expect("a successful remote step carries the mock entity");
        assert_eq!(data["id"], "o-named", "the createOrder response is returned");
        // `.expect(1)` on the `createOrder`-body matcher asserts the op name on drop.
    }

    /// With no `mutation_name` set, remote dispatch falls back to the mutation-kind
    /// verb (`create`) — backwards-compatible with pre-migration step rows.
    #[tokio::test]
    async fn dispatch_step_remote_falls_back_to_verb_when_name_absent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_string_contains("{ create("))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": { "create": { "__typename": "Order", "id": "o-verb" } }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let (mutation_executor, _adapter) = order_table_executor().await;
        let client = HttpMutationClient::new_for_test(HttpMutationConfig::default()).unwrap();
        let url = Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        // mutation_name left None → fall back to the verb.
        let step = order_step(MutationType::Create, json!({"id": "o-verb", "total": "3"}));

        let (result, state) =
            SagaExecutor::dispatch_step(&mutation_executor, &step, Some((&client, &url))).await;

        assert!(result.success, "the verb-named remote mutation must succeed: {result:?}");
        assert_eq!(state, StepState::Completed);
        let data = result.data.expect("a successful remote step carries the mock entity");
        assert_eq!(data["id"], "o-verb", "the create response is returned");
    }

    // ── Retry-with-backoff + per-step timeout (#429 hardening P06) ──────────────
    //
    // `dispatch_step_with_retry` retries a failed dispatch under the executor's
    // `RetryPolicy` before giving up, so a transient step failure does not needlessly
    // roll back the saga. Store-free (mirrors `dispatch_step`), so these run fast.
    // The `HttpMutationClient` is built with `max_retries: 1` (no internal retry) so
    // one dispatch attempt = exactly one HTTP request and the counts are unambiguous.

    fn single_attempt_client() -> HttpMutationClient {
        HttpMutationClient::new_for_test(HttpMutationConfig {
            timeout_ms:     5000,
            max_retries:    1,
            retry_delay_ms: 1,
        })
        .unwrap()
    }

    /// A step that succeeds on the first try is dispatched exactly once — the retry
    /// budget is never spent on success.
    #[tokio::test]
    async fn dispatch_step_with_retry_succeeds_first_try_no_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": { "create": { "__typename": "Order", "id": "o-ok" } }
            })))
            .mount(&server)
            .await;

        let (mutation_executor, _adapter) = order_table_executor().await;
        let client = single_attempt_client();
        let url = Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let step = order_step(MutationType::Create, json!({"id": "o-ok", "total": "1"}));
        let executor = SagaExecutor::new().with_retry_policy(RetryPolicy {
            max_retries:     3,
            base_delay_ms:   1,
            step_timeout_ms: None,
        });

        let (result, state) = executor
            .dispatch_step_with_retry(&mutation_executor, &step, Some((&client, &url)))
            .await;

        assert!(result.success, "first-try success must not fail: {result:?}");
        assert_eq!(state, StepState::Completed);
        let posts = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/graphql")
            .count();
        assert_eq!(posts, 1, "a successful step is dispatched exactly once: {posts}");
    }

    /// A transient failure is recovered by a retry: the peer returns 500 once, then
    /// 200; the step ultimately succeeds after exactly two attempts.
    #[tokio::test]
    async fn dispatch_step_with_retry_recovers_after_transient_failure() {
        let server = MockServer::start().await;
        // Higher-priority 500 fires once, then is exhausted; the default-priority 200
        // answers the retry.
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": { "create": { "__typename": "Order", "id": "o-retry" } }
            })))
            .mount(&server)
            .await;

        let (mutation_executor, _adapter) = order_table_executor().await;
        let client = single_attempt_client();
        let url = Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let step = order_step(MutationType::Create, json!({"id": "o-retry", "total": "1"}));
        let executor = SagaExecutor::new().with_retry_policy(RetryPolicy {
            max_retries:     2,
            base_delay_ms:   1,
            step_timeout_ms: None,
        });

        let (result, state) = executor
            .dispatch_step_with_retry(&mutation_executor, &step, Some((&client, &url)))
            .await;

        assert!(result.success, "a retry must recover the transient failure: {result:?}");
        assert_eq!(state, StepState::Completed);
        let posts = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/graphql")
            .count();
        assert_eq!(posts, 2, "one failed attempt + one successful retry: {posts}");
    }

    /// A step that keeps failing exhausts its retry budget and is reported failed —
    /// never a fabricated success. Attempts = `max_retries + 1`.
    #[tokio::test]
    async fn dispatch_step_with_retry_exhausts_budget_then_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let (mutation_executor, _adapter) = order_table_executor().await;
        let client = single_attempt_client();
        let url = Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let step = order_step(MutationType::Create, json!({"id": "o-fail", "total": "1"}));
        let executor = SagaExecutor::new().with_retry_policy(RetryPolicy {
            max_retries:     2,
            base_delay_ms:   1,
            step_timeout_ms: None,
        });

        let (result, state) = executor
            .dispatch_step_with_retry(&mutation_executor, &step, Some((&client, &url)))
            .await;

        assert!(!result.success, "an exhausted retry budget must report failure: {result:?}");
        assert_eq!(state, StepState::Failed);
        assert!(result.data.is_none(), "a failed step fabricates no data");
        let posts = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/graphql")
            .count();
        assert_eq!(posts, 3, "max_retries=2 → 1 initial + 2 retries = 3 attempts: {posts}");
    }

    /// A step whose dispatch exceeds the per-step timeout is a real failed attempt
    /// (a timeout error), never a fabricated success.
    #[tokio::test]
    async fn dispatch_step_with_retry_times_out() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(std::time::Duration::from_millis(400))
                    .set_body_json(
                        json!({ "data": { "create": { "__typename": "Order", "id": "x" } } }),
                    ),
            )
            .mount(&server)
            .await;

        let (mutation_executor, _adapter) = order_table_executor().await;
        let client = single_attempt_client();
        let url = Url::parse(&format!("{}/graphql", server.uri())).unwrap();
        let step = order_step(MutationType::Create, json!({"id": "o-slow", "total": "1"}));
        // 50ms step timeout vs a 400ms server delay → the attempt times out.
        let executor = SagaExecutor::new().with_retry_policy(RetryPolicy {
            max_retries:     0,
            base_delay_ms:   0,
            step_timeout_ms: Some(50),
        });

        let (result, state) = executor
            .dispatch_step_with_retry(&mutation_executor, &step, Some((&client, &url)))
            .await;

        assert!(!result.success, "a timed-out step must report failure: {result:?}");
        assert_eq!(state, StepState::Failed);
        assert!(
            result.error.as_deref().unwrap_or("").contains("timed out"),
            "the failure must name the timeout: {result:?}"
        );
    }
}
