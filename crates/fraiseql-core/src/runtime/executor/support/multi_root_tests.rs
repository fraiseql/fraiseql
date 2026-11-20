//! A multi-root document (GraphQL § 6.4.4): every root is planned before any is read, a
//! root refused while planned refuses the document, and a root that fails while read is a
//! field error at its path while the others are served.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use crate::{
    error::FraiseQLError,
    runtime::{
        Executor, RuntimeConfig,
        executor::test_support::{MockAdapter, mock_user_results, test_schema},
    },
    schema::{CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, TypeDefinition},
    security::SecurityContext,
};

/// The text a failed read's database error carries: a relation name a client must not see
/// when the embedder renders the error.
const DRIVER_TEXT: &str = "relation \"core.v_account_internal\" does not exist";

/// `users: [User!]!` on `v_user`, and `account: User` (nullable, one object) on
/// `v_account`. `User.salary` refuses a caller without `read:salary`, `User.email` is
/// masked for a caller without `read:email`.
fn schema() -> CompiledSchema {
    let mut schema = test_schema();
    let mut account = schema.queries[0].clone();
    account.name = "account".to_string();
    account.returns_list = false;
    account.nullable = true;
    account.sql_source = Some("v_account".to_string());
    schema.queries.push(account);
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::String),
            FieldDefinition::new("name", FieldType::String),
            FieldDefinition::nullable("salary", FieldType::Float)
                .with_requires_scope("read:salary")
                .with_on_deny(FieldDenyPolicy::Reject),
            FieldDefinition::nullable("email", FieldType::String)
                .with_requires_scope("read:email")
                .with_on_deny(FieldDenyPolicy::Mask),
        ],
        ..TypeDefinition::new("User", "v_user")
    });
    schema.build_indexes();
    schema
}

fn account_row() -> Vec<crate::backend::types::JsonbValue> {
    vec![crate::backend::types::JsonbValue::new(serde_json::json!({
        "id": "9", "name": "Carol", "salary": 1.5, "email": "carol@example.test"
    }))]
}

fn adapter() -> MockAdapter {
    MockAdapter::new(mock_user_results()).with_view("v_account", account_row())
}

fn executor_with(adapter: MockAdapter, config: RuntimeConfig) -> (Executor, Arc<MockAdapter>) {
    let adapter = Arc::new(adapter);
    (Executor::with_config(schema(), Arc::clone(&adapter), config), adapter)
}

/// A principal holding no scope.
fn unscoped() -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::default(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        authenticated_at: chrono::Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// A nullable root whose read fails is `null`, with one error at its path, and the root
/// beside it is served.
#[tokio::test]
async fn a_nullable_root_that_fails_is_null_beside_the_roots_served() {
    let (executor, _) = executor_with(
        adapter().with_failing_view("v_account", DRIVER_TEXT),
        RuntimeConfig::default(),
    );
    let response = executor.execute("{ users { id } account { id } }", None).await.unwrap();
    assert_eq!(
        response["data"],
        serde_json::json!({ "users": [{ "id": "1" }, { "id": "2" }], "account": null }),
        "{response}"
    );
    let errors = response["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{response}");
    assert_eq!(errors[0]["path"], serde_json::json!(["account"]), "{response}");
}

/// A root published non-null that fails nulls `data` (§ 6.4.4), and its error is reported
/// once: not again as a `null` in a non-null position.
#[tokio::test]
async fn a_non_null_root_that_fails_nulls_data_with_its_one_error() {
    let (executor, _) =
        executor_with(adapter().with_failing_view("v_user", DRIVER_TEXT), RuntimeConfig::default());
    let response = executor.execute("{ account { id } list: users { id } }", None).await.unwrap();
    assert_eq!(response["data"], serde_json::Value::Null, "{response}");
    let errors = response["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{response}");
    assert_eq!(errors[0]["path"], serde_json::json!(["list"]), "under its alias: {response}");
}

/// The failed root's error is the embedder's rendering of it, the treatment a failed
/// request gets: the server's renderer withholds the database's text.
#[tokio::test]
async fn a_failed_roots_error_is_rendered_by_the_embedders_renderer() {
    let config = RuntimeConfig {
        root_error_renderer: Some(Arc::new(|error: &FraiseQLError| {
            serde_json::json!({
                "message": "An internal error occurred",
                "extensions": { "code": error.error_code() },
            })
        })),
        ..RuntimeConfig::default()
    };
    let (executor, _) =
        executor_with(adapter().with_failing_view("v_account", DRIVER_TEXT), config);
    let response = executor.execute("{ users { id } account { id } }", None).await.unwrap();
    assert!(!response.to_string().contains("v_account_internal"), "{response}");
    assert_eq!(response["errors"][0]["message"], "An internal error occurred", "{response}");
    assert_eq!(response["errors"][0]["path"], serde_json::json!(["account"]), "{response}");
}

/// A root no query answers refuses the document, before any root is read.
#[tokio::test]
async fn a_root_refused_while_planned_refuses_the_document_before_any_read() {
    let (executor, adapter) = executor_with(adapter(), RuntimeConfig::default());
    let refused = executor.execute("{ users { id } nope { id } }", None).await;
    assert!(matches!(refused, Err(FraiseQLError::Validation { .. })), "{refused:?}");
    assert!(adapter.served_name_buffers.lock().unwrap().is_empty(), "a root was read");
}

/// A field the caller may not read refuses the document whichever root selects it, as it
/// refuses that root sent alone, and nothing is read.
#[tokio::test]
async fn a_refused_field_on_any_root_refuses_the_document_as_it_does_alone() {
    let alone = executor_with(adapter(), RuntimeConfig::default())
        .0
        .execute_with_security("{ account { id salary } }", None, &unscoped())
        .await
        .unwrap_err();
    assert!(matches!(alone, FraiseQLError::Authorization { .. }), "{alone:?}");
    for document in [
        "{ users { id } account { id salary } }",
        "{ account { id salary } users { id } }",
        "{ users { id } a: account { id } b: account { id salary } }",
    ] {
        let (executor, adapter) = executor_with(adapter(), RuntimeConfig::default());
        let refused =
            executor.execute_with_security(document, None, &unscoped()).await.unwrap_err();
        assert_eq!(refused.to_string(), alone.to_string(), "{document}");
        assert!(
            adapter.served_name_buffers.lock().unwrap().is_empty(),
            "{document}: a root was read"
        );
    }
}

/// A masked field is masked on whichever root selects it, as on that root alone.
#[tokio::test]
async fn a_masked_field_on_any_root_is_masked_as_it_is_alone() {
    let (executor, _) = executor_with(adapter(), RuntimeConfig::default());
    let alone = executor
        .execute_with_security("{ account { id email } }", None, &unscoped())
        .await;
    let alone = alone.unwrap();
    assert_eq!(alone["data"]["account"]["email"], serde_json::Value::Null, "{alone}");
    let both = executor
        .execute_with_security("{ users { id } account { id email } }", None, &unscoped())
        .await
        .unwrap();
    assert_eq!(both["data"]["account"], alone["data"]["account"], "{both}");
}

/// A document with one root keeps refusing as a whole when its root fails.
#[tokio::test]
async fn a_single_root_that_fails_still_fails_the_request() {
    let (executor, _) = executor_with(
        adapter().with_failing_view("v_account", DRIVER_TEXT),
        RuntimeConfig::default(),
    );
    let failed = executor.execute("{ account { id } }", None).await;
    assert!(matches!(failed, Err(FraiseQLError::Database { .. })), "{failed:?}");
}

/// A fact table's aggregate and window roots are roots like the others: planned with their
/// own gates, read beside the other roots, and answered under their response keys.
mod fact_table_roots {
    use super::*;
    use crate::{
        error::Result as FqlResult,
        security::{Authorizer, AuthzDecision, AuthzRequest},
    };

    const COUNT: &str = "aggregates: [{count: true}]";
    const COST: &str = "aggregates: [{cost_sum: true}]";
    const RANK: &str = r#"select: [{type: "filter", name: "region", alias: "region"}], windows: [{function: {type: "row_number"}, alias: "rank"}]"#;

    /// The multi-root schema, with `tf_sales` linked to `Sale`: `cost` refuses a caller
    /// without `read:cost`. `role` gates `Sale`, and with it the fact table's roots.
    fn schema_with_sales(role: Option<&str>) -> CompiledSchema {
        let mut schema = schema();
        schema.types.push(TypeDefinition {
            fields: vec![
                FieldDefinition::new("revenue", FieldType::Float),
                FieldDefinition::nullable("cost", FieldType::Float)
                    .with_requires_scope("read:cost")
                    .with_on_deny(FieldDenyPolicy::Reject),
                FieldDefinition::new("region", FieldType::String),
            ],
            requires_role: role.map(str::to_string),
            ..TypeDefinition::new("Sale", "v_sale")
        });
        let measure =
            |name: &str| serde_json::json!({"name": name, "sql_type": "Decimal", "nullable": true});
        schema.add_fact_table(
            "tf_sales".to_string(),
            serde_json::from_value(serde_json::json!({
                "table_name": "tf_sales",
                "type_name": "Sale",
                "measures": [measure("revenue"), measure("cost")],
                "dimensions": {"name": "data", "paths": []},
                "denormalized_filters": [{"name": "region", "sql_type": "Text", "indexed": true}]
            }))
            .unwrap(),
        );
        schema.build_indexes();
        schema
    }

    fn sales_executor(adapter: MockAdapter) -> (Executor, Arc<MockAdapter>) {
        executor_on(schema_with_sales(None), adapter, RuntimeConfig::default())
    }

    fn executor_on(
        schema: CompiledSchema,
        adapter: MockAdapter,
        config: RuntimeConfig,
    ) -> (Executor, Arc<MockAdapter>) {
        let adapter = Arc::new(adapter);
        (Executor::with_config(schema, Arc::clone(&adapter), config), adapter)
    }

    fn sales_adapter() -> MockAdapter {
        adapter().with_aggregate_rows(vec![serde_json::json!({"count": 3})])
    }

    fn nothing_read(adapter: &MockAdapter) -> bool {
        adapter.served_name_buffers.lock().unwrap().is_empty()
            && adapter.aggregate_statements().is_empty()
    }

    /// Each root is answered under its response key, whichever comes first, beside a
    /// regular root and `__typename`.
    #[tokio::test]
    async fn an_aggregate_or_window_root_is_served_beside_other_roots() {
        let users = serde_json::json!([{ "id": "1" }, { "id": "2" }]);
        for (document, key, rows) in [
            (
                format!(
                    "{{ totals: sales_aggregate({COUNT}) {{ count }} users {{ id }} __typename }}"
                ),
                "totals",
                serde_json::json!([{ "count": 3 }]),
            ),
            (
                format!("{{ users {{ id }} sales_aggregate({COUNT}) {{ count }} }}"),
                "sales_aggregate",
                serde_json::json!([{ "count": 3 }]),
            ),
            (
                format!("{{ users {{ id }} ranked: sales_window({RANK}) }}"),
                "ranked",
                serde_json::json!([{ "count": 3 }]),
            ),
        ] {
            let (executor, adapter) = sales_executor(sales_adapter());
            let response = executor.execute(&document, None).await.unwrap();
            assert!(response.get("errors").is_none(), "{document}: {response}");
            assert_eq!(response["data"]["users"], users, "{document}: {response}");
            assert_eq!(response["data"][key], rows, "{document}: {response}");
            assert_eq!(adapter.aggregate_statements().len(), 1, "{document}");
        }
    }

    /// Two fact-table roots are two reads, each answered under its own key: two aggregates,
    /// or an aggregate and a window.
    #[tokio::test]
    async fn two_fact_table_roots_are_two_reads() {
        for (document, keys) in [
            (
                format!(
                    "{{ a: sales_aggregate({COUNT}) {{ count }} \
                     b: sales_aggregate(groupBy: {{region: true}}, {COUNT}) {{ region count }} }}"
                ),
                ["a", "b"],
            ),
            (
                format!("{{ s: sales_aggregate({COUNT}) {{ count }} w: sales_window({RANK}) }}"),
                ["s", "w"],
            ),
        ] {
            let (executor, adapter) = sales_executor(sales_adapter());
            let response = executor.execute(&document, None).await.unwrap();
            assert!(response.get("errors").is_none(), "{document}: {response}");
            for key in keys {
                assert!(response["data"][key].is_array(), "{document}: {key}: {response}");
            }
            let statements = adapter.aggregate_statements();
            assert_eq!(statements.len(), 2, "{document}: {statements:?}");
            assert_ne!(statements[0], statements[1], "{document}: two distinct reads");
        }
    }

    /// An aggregate root's read failing nulls `data` (it is `[Row!]!`), with its one error;
    /// a nullable regular root failing beside an aggregate root leaves the aggregate served.
    #[tokio::test]
    async fn a_failing_root_beside_a_fact_table_root_is_a_field_error() {
        let (executor, _) =
            sales_executor(sales_adapter().with_failing_view("tf_sales", DRIVER_TEXT));
        let response = executor
            .execute(
                &format!("{{ totals: sales_aggregate({COUNT}) {{ count }} users {{ id }} }}"),
                None,
            )
            .await
            .unwrap();
        assert_eq!(response["data"], serde_json::Value::Null, "{response}");
        let errors = response["errors"].as_array().unwrap();
        assert_eq!(errors.len(), 1, "{response}");
        assert_eq!(errors[0]["path"], serde_json::json!(["totals"]), "{response}");

        let (executor, _) =
            sales_executor(sales_adapter().with_failing_view("v_account", DRIVER_TEXT));
        let response = executor
            .execute(
                &format!("{{ totals: sales_aggregate({COUNT}) {{ count }} account {{ id }} }}"),
                None,
            )
            .await
            .unwrap();
        assert_eq!(response["data"]["totals"], serde_json::json!([{ "count": 3 }]), "{response}");
        assert_eq!(response["data"]["account"], serde_json::Value::Null, "{response}");
        assert_eq!(response["errors"][0]["path"], serde_json::json!(["account"]), "{response}");
    }

    /// The field errors an aggregate root's rows raise are kept, at their paths, beside the
    /// other roots.
    #[tokio::test]
    async fn an_aggregate_roots_own_field_errors_are_kept() {
        let (executor, _) = sales_executor(adapter().with_aggregate_rows(vec![
            serde_json::json!({"count": 1}),
            serde_json::json!({"count": 2_147_483_648_i64}),
        ]));
        let response = executor
            .execute(
                &format!("{{ users {{ id }} s: sales_aggregate({COUNT}) {{ n: count }} }}"),
                None,
            )
            .await
            .unwrap();
        assert!(response["data"]["users"].is_array(), "{response}");
        assert_eq!(response["data"]["s"][1]["n"], serde_json::Value::Null, "{response}");
        let errors = response["errors"].as_array().expect("the row's error");
        assert_eq!(errors.len(), 1, "{response}");
        assert_eq!(errors[0]["path"], serde_json::json!(["s", 1, "n"]), "{response}");
    }

    /// A fact-table root refused while planned refuses the document as it refuses that root
    /// alone, and nothing is read: an argument it does not take, a field of its linked type
    /// the caller may not read, a role the caller lacks.
    #[tokio::test]
    async fn a_fact_table_root_refused_while_planned_refuses_the_document_as_alone() {
        for (schema, root, kind) in [
            (
                schema_with_sales(None),
                format!("sales_aggregate(table: \"tf_sales\", {COUNT}) {{ count }}"),
                "validation",
            ),
            (
                schema_with_sales(None),
                format!("sales_aggregate(nope: 1, {COUNT}) {{ count }}"),
                "validation",
            ),
            (
                schema_with_sales(None),
                format!("sales_aggregate({COST}) {{ count }}"),
                "authorization",
            ),
            (
                schema_with_sales(Some("finance")),
                format!("sales_aggregate({COUNT}) {{ count }}"),
                "validation",
            ),
        ] {
            let alone = executor_on(schema.clone(), sales_adapter(), RuntimeConfig::default())
                .0
                .execute_with_security(&format!("{{ {root} }}"), None, &unscoped())
                .await
                .unwrap_err();
            let expected = match kind {
                "authorization" => matches!(alone, FraiseQLError::Authorization { .. }),
                _ => matches!(alone, FraiseQLError::Validation { .. }),
            };
            assert!(expected, "{root} alone: {alone:?}");
            for document in [
                format!("{{ users {{ id }} {root} }}"),
                format!("{{ {root} users {{ id }} }}"),
            ] {
                let (executor, adapter) =
                    executor_on(schema.clone(), sales_adapter(), RuntimeConfig::default());
                let refused =
                    executor.execute_with_security(&document, None, &unscoped()).await.unwrap_err();
                assert_eq!(refused.to_string(), alone.to_string(), "{document}");
                assert!(nothing_read(&adapter), "{document}: a root was read");
            }
        }
    }

    struct DenyNamed(&'static str);
    impl Authorizer for DenyNamed {
        fn authorize(&self, req: &AuthzRequest<'_>) -> FqlResult<AuthzDecision> {
            Ok(if req.name == self.0 {
                AuthzDecision::Deny {
                    reason: "named-deny".into(),
                }
            } else {
                AuthzDecision::Allow
            })
        }
    }

    /// The operation authorizer is asked about a fact-table root in a document of several,
    /// by its name, as when it is sent alone, and its deny refuses the document unread.
    #[tokio::test]
    async fn the_authorizer_denies_a_fact_table_root_beside_other_roots() {
        let (executor, adapter) = executor_on(
            schema_with_sales(None),
            sales_adapter(),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyNamed("sales_aggregate"))),
        );
        let refused = executor
            .execute(&format!("{{ users {{ id }} t: sales_aggregate({COUNT}) {{ count }} }}"), None)
            .await
            .unwrap_err();
        assert!(
            matches!(
                &refused,
                FraiseQLError::Authorization { resource, .. }
                    if resource.as_deref() == Some("sales_aggregate")
            ),
            "{refused:?}"
        );
        assert!(nothing_read(&adapter), "a root was read");
    }

    /// A document holding a fact-table root is not reusable, as the root alone is not,
    /// whatever the TTL of the roots beside it.
    #[test]
    fn a_fact_table_root_makes_the_response_unreusable() {
        let mut schema = schema_with_sales(None);
        schema.queries[0].cache_ttl_seconds = Some(60);
        schema.build_indexes();
        let (executor, _) = executor_on(schema, sales_adapter(), RuntimeConfig::default());
        assert_eq!(executor.response_cache_ttl("{ users { id } }", None), Some(60));
        assert_eq!(
            executor.response_cache_ttl(
                &format!("{{ users {{ id }} sales_aggregate({COUNT}) {{ count }} }}"),
                None
            ),
            None
        );
    }
}
