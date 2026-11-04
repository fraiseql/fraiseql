//! Tests for RLS enforcement in aggregate and window query paths.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;

use crate::{
    compiler::fact_table::{
        DimensionColumn, FactRowScope, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
    },
    runtime::{Executor, RuntimeConfig, executor::test_support::CapturingMockAdapter},
    security::{DefaultRLSPolicy, SecurityContext},
};

pub(super) fn tenant_security_context(tenant_id: &str) -> SecurityContext {
    SecurityContext {
        user_id:          "user-42".into(),
        roles:            vec!["viewer".to_string()],
        tenant_id:        Some(tenant_id.into()),
        scopes:           vec![],
        attributes:       HashMap::from([("tenant_id".to_string(), serde_json::json!(tenant_id))]),
        request_id:       "req-001".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn admin_security_context() -> SecurityContext {
    SecurityContext {
        user_id:          "admin-1".into(),
        roles:            vec!["admin".to_string()],
        tenant_id:        Some("tenant-abc".into()),
        scopes:           vec![],
        attributes:       HashMap::from([(
            "tenant_id".to_string(),
            serde_json::json!("tenant-abc"),
        )]),
        request_id:       "req-002".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// Build a schema with a `tf_sales` fact table that includes `tenant_id` as a
/// denormalized filter column, so RLS can produce direct-column WHERE clauses.
pub(super) fn schema_with_fact_table() -> crate::schema::CompiledSchema {
    let mut schema = crate::schema::CompiledSchema::new();
    schema.add_fact_table(
        "tf_sales".to_string(),
        FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            type_name:                None,
            row_scope:                FactRowScope::Server,
            inject_params:            indexmap::IndexMap::default(),
            measures:                 vec![MeasureColumn {
                name:       "revenue".to_string(),
                sql_type:   SqlType::Decimal,
                nullable:   false,
                additivity: crate::compiler::fact_table::Additivity::Additive,
            }],
            dimensions:               DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![
                FilterColumn {
                    name:      "tenant_id".to_string(),
                    sql_type:  SqlType::Text,
                    indexed:   true,
                    hierarchy: None,
                },
                FilterColumn {
                    name:      "author_id".to_string(),
                    sql_type:  SqlType::Text,
                    indexed:   true,
                    hierarchy: None,
                },
            ],
            calendar_dimensions:      vec![],
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        },
    );
    schema
}

// ── Aggregate RLS tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn aggregate_query_with_rls_includes_tenant_filter_in_sql() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let executor = Executor::with_config(schema, adapter.clone(), config);

    let ctx = tenant_security_context("tenant-abc");
    let vars = serde_json::json!({ "aggregates": [{"count": true}] });
    let _result = executor
        .execute_with_security("{ sales_aggregate { count } }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("aggregate SQL should be captured");
    assert!(
        sql.contains("tenant_id"),
        "RLS tenant filter must appear in aggregate SQL, got: {sql}"
    );
}

#[tokio::test]
async fn aggregate_query_admin_bypasses_rls() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let executor = Executor::with_config(schema, adapter.clone(), config);

    let ctx = admin_security_context();
    let vars = serde_json::json!({ "aggregates": [{"count": true}] });
    let _result = executor
        .execute_with_security("{ sales_aggregate { count } }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("aggregate SQL should be captured");
    // Admin should bypass RLS — no tenant_id filter in SQL
    assert!(
        !sql.contains("tenant_id"),
        "admin should bypass RLS, but SQL contains tenant_id: {sql}"
    );
}

#[tokio::test]
async fn aggregate_query_no_rls_policy_returns_unfiltered() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    // No RLS policy configured
    let executor = Executor::new(schema, adapter.clone());

    let ctx = tenant_security_context("tenant-abc");
    let vars = serde_json::json!({ "aggregates": [{"count": true}] });
    let _result = executor
        .execute_with_security("{ sales_aggregate { count } }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("aggregate SQL should be captured");
    // No RLS policy means no tenant filter
    assert!(
        !sql.contains("tenant_id"),
        "without RLS policy, SQL should not contain tenant_id: {sql}"
    );
}

#[tokio::test]
async fn aggregate_rls_composes_with_user_where() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let executor = Executor::with_config(schema, adapter.clone(), config);

    let ctx = tenant_security_context("tenant-abc");
    // User-supplied WHERE on a denormalized filter, in the aggregate `field_operator` form.
    // The nested `{"tenant_id": {"eq": …}}` this test used to send was never an aggregate
    // filter: the parser skipped it and the test passed on the RLS predicate alone (#1460).
    let vars = serde_json::json!({
        "aggregates": [{"count": true}],
        "where": {"tenant_id_eq": "tenant-abc"}
    });
    let _result = executor
        .execute_with_security("{ sales_aggregate { count } }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("aggregate SQL should be captured");
    // Both RLS and user WHERE should be present (AND-composed)
    assert!(sql.contains("WHERE"), "combined WHERE expected in SQL: {sql}");
    assert!(sql.contains("AND"), "RLS + user WHERE should be AND-composed: {sql}");
    assert!(
        sql.matches("tenant_id").count() >= 2,
        "the RLS predicate and the user's own filter must both reach the SQL: {sql}"
    );
}

// ── Window RLS tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn window_query_with_rls_includes_tenant_filter_in_sql() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let executor = Executor::with_config(schema, adapter.clone(), config);

    let ctx = tenant_security_context("tenant-abc");
    let vars = serde_json::json!({
        "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
        "windows": [{
            "function": {"type": "row_number"},
            "alias": "rank",
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });
    let _result = executor
        .execute_with_security("{ sales_window }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("window SQL should be captured");
    assert!(
        sql.contains("tenant_id"),
        "RLS tenant filter must appear in window SQL, got: {sql}"
    );
}

#[tokio::test]
async fn window_query_admin_bypasses_rls() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let executor = Executor::with_config(schema, adapter.clone(), config);

    let ctx = admin_security_context();
    let vars = serde_json::json!({
        "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
        "windows": [{
            "function": {"type": "row_number"},
            "alias": "rank",
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });
    let _result = executor
        .execute_with_security("{ sales_window }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("window SQL should be captured");
    assert!(
        !sql.contains("tenant_id"),
        "admin should bypass RLS in window queries, but SQL contains tenant_id: {sql}"
    );
}

// ── mod gated_fact_tables: an aggregate or window reads its fact table's type (AA 3b) ──
//
// A fact table linked to a type (`type_name`, ruling AB 1) is read as that type: a field
// the caller may not read may neither be aggregated, grouped, filtered, ordered or
// partitioned by, nor selected by a window; a name the type does not declare is no field
// at all; and the type's own role gates the read. Today fields resolve against the fact
// table's metadata alone, which carries no gate. The link is written in JSON because the
// struct has no field for it yet: serde ignores the key until the fix gives it one.
mod gated_fact_tables {
    use super::*;
    use crate::schema::{
        CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, RoleDefinition,
        SecurityConfig, TypeDefinition,
    };

    /// `Sale`: `margin` masks without `read:margin`, `cost` refuses without `read:cost`,
    /// `segment` (a dimension) masks without `read:segment`, `note` is an `authorize` field.
    /// `tf_sales` declares each of them, and is linked to `Sale`.
    fn sale_type() -> TypeDefinition {
        let mut note = FieldDefinition::nullable("note", FieldType::String);
        note.authorize = true;
        TypeDefinition {
            fields: vec![
                FieldDefinition::new("revenue", FieldType::Float),
                FieldDefinition::nullable("margin", FieldType::Float)
                    .with_requires_scope("read:margin")
                    .with_on_deny(FieldDenyPolicy::Mask),
                FieldDefinition::nullable("cost", FieldType::Float)
                    .with_requires_scope("read:cost")
                    .with_on_deny(FieldDenyPolicy::Reject),
                FieldDefinition::new("category", FieldType::String),
                FieldDefinition::nullable("segment", FieldType::String)
                    .with_requires_scope("read:segment")
                    .with_on_deny(FieldDenyPolicy::Mask),
                note,
                FieldDefinition::new("tenant_id", FieldType::String),
                FieldDefinition::new("occurred_at", FieldType::String),
                FieldDefinition::nullable("closed_at", FieldType::String)
                    .with_requires_scope("read:closed")
                    .with_on_deny(FieldDenyPolicy::Mask),
            ],
            ..TypeDefinition::new("Sale", "v_sale")
        }
    }

    fn linked_fact_table() -> FactTableMetadata {
        let measure =
            |name: &str| serde_json::json!({"name": name, "sql_type": "Decimal", "nullable": true});
        let path = |name: &str| serde_json::json!({"name": name, "json_path": format!("data->>'{name}'"), "data_type": "text"});
        let filter =
            |name: &str| serde_json::json!({"name": name, "sql_type": "Text", "indexed": true});
        serde_json::from_value(serde_json::json!({
            "table_name": "tf_sales",
            "type_name": "Sale",
            "measures": [measure("revenue"), measure("margin"), measure("cost")],
            "dimensions": {"name": "data", "paths": [path("category"), path("segment"), path("note")]},
            "denormalized_filters": [filter("tenant_id"), filter("occurred_at"), filter("closed_at")]
        }))
        .unwrap()
    }

    fn schema() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.types.push(sale_type());
        schema.add_fact_table("tf_sales".to_string(), linked_fact_table());
        let mut security = SecurityConfig::default();
        security.add_role(RoleDefinition::new("analyst", vec!["read:margin".to_string()]));
        schema.security = Some(security);
        schema.build_indexes();
        schema
    }

    /// A principal with no role, so no scope.
    fn principal() -> SecurityContext {
        SecurityContext {
            roles: vec![],
            ..tenant_security_context("tenant-abc")
        }
    }

    async fn run(
        schema: CompiledSchema,
        root: &str,
        vars: &serde_json::Value,
        ctx: &SecurityContext,
    ) -> (crate::error::Result<serde_json::Value>, Option<String>) {
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let res = Executor::new(schema, adapter.clone())
            .execute_with_security(root, Some(vars), ctx)
            .await;
        (res, adapter.captured_aggregate_sql())
    }

    async fn assert_refused(vars: serde_json::Value, root: &str, what: &str) {
        let (res, sql) = run(schema(), root, &vars, &principal()).await;
        assert!(
            matches!(res, Err(crate::error::FraiseQLError::Authorization { .. })),
            "{what}: {res:?}"
        );
        assert!(sql.is_none(), "{what}: the statement reached the database: {sql:?}");
    }

    #[tokio::test]
    async fn aggregating_a_measure_the_caller_may_not_read_is_refused() {
        assert_refused(
            serde_json::json!({"aggregates": [{"margin_sum": true}]}),
            "{ sales_aggregate { count } }",
            "the sum of a masked measure discloses it",
        )
        .await;
        assert_refused(
            serde_json::json!({"aggregates": [{"cost_max": true}]}),
            "{ sales_aggregate { count } }",
            "the max of a refused measure is its value",
        )
        .await;
    }

    #[tokio::test]
    async fn grouping_by_a_dimension_the_caller_may_not_read_is_refused() {
        assert_refused(
            serde_json::json!({"groupBy": {"segment": true}, "aggregates": [{"count": true}]}),
            "{ sales_aggregate { count } }",
            "a group key is served as the value itself",
        )
        .await;
    }

    #[tokio::test]
    async fn an_aggregate_filter_on_a_field_the_caller_may_not_read_is_refused() {
        assert_refused(
            serde_json::json!({"where": {"margin_gt": 10}, "aggregates": [{"count": true}]}),
            "{ sales_aggregate { count } }",
            "a count filtered by a masked measure answers a question about it",
        )
        .await;
    }

    #[tokio::test]
    async fn an_aggregate_order_by_a_field_the_caller_may_not_read_is_refused() {
        assert_refused(
            serde_json::json!({
                "groupBy": {"category": true},
                "aggregates": [{"count": true}],
                "orderBy": {"margin": "DESC"}
            }),
            "{ sales_aggregate { count } }",
            "an order by a masked measure ranks by it",
        )
        .await;
    }

    #[tokio::test]
    async fn an_aggregate_over_an_authorize_field_is_refused() {
        assert_refused(
            serde_json::json!({"groupBy": {"note": true}, "aggregates": [{"count": true}]}),
            "{ sales_aggregate { count } }",
            "an authorize field is decided per row, after a read",
        )
        .await;
    }

    #[tokio::test]
    async fn a_window_over_a_field_the_caller_may_not_read_is_refused() {
        assert_refused(
            serde_json::json!({
                "select": [{"type": "measure", "name": "margin", "alias": "margin"}],
                "windows": []
            }),
            "{ sales_window }",
            "a window selecting a masked measure serves it",
        )
        .await;
        assert_refused(
            serde_json::json!({
                "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
                "windows": [{
                    "function": {"type": "running_sum", "measure": "cost"},
                    "alias": "running_cost",
                    "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
                }]
            }),
            "{ sales_window }",
            "a running sum of a refused measure discloses it",
        )
        .await;
        assert_refused(
            serde_json::json!({
                "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
                "windows": [{
                    "function": {"type": "row_number"},
                    "alias": "rank",
                    "partitionBy": [{"type": "dimension", "path": "segment"}],
                    "orderBy": [{"field": "margin", "direction": "DESC"}]
                }]
            }),
            "{ sales_window }",
            "a rank partitioned and ordered by gated fields ranks by them",
        )
        .await;
        assert_refused(
            serde_json::json!({
                "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
                "windows": [],
                "where": {"margin_gt": 10}
            }),
            "{ sales_window }",
            "a window filtered by a masked measure answers a question about it",
        )
        .await;
    }

    #[tokio::test]
    async fn a_name_the_linked_type_does_not_declare_is_refused() {
        let vars =
            serde_json::json!({"where": {"salary_gt": 100_000}, "aggregates": [{"count": true}]});
        let (res, sql) = run(schema(), "{ sales_aggregate { count } }", &vars, &principal()).await;
        assert!(
            matches!(res, Err(crate::error::FraiseQLError::Validation { .. })),
            "an undeclared JSONB key of a linked fact table is no field of its type: {res:?}"
        );
        assert!(sql.is_none(), "the statement reached the database: {sql:?}");
    }

    fn finance_gated() -> CompiledSchema {
        let mut schema = schema();
        schema.types.iter_mut().find(|t| t.name == "Sale").unwrap().requires_role =
            Some("finance".to_string());
        schema.build_indexes();
        schema
    }

    fn finance() -> SecurityContext {
        SecurityContext {
            roles: vec!["finance".to_string()],
            ..principal()
        }
    }

    fn message(res: &crate::error::Result<serde_json::Value>) -> String {
        match res {
            Err(crate::error::FraiseQLError::Validation { message, .. }) => message.clone(),
            other => format!("not a validation error: {other:?}"),
        }
    }

    /// A fact table read as a type whose role the caller lacks is absent to it, as a
    /// role-gated query is (#1122): its roots answer exactly as a table that does not exist,
    /// before anything else about the document is checked, so neither the refusal nor a
    /// suggestion tells the caller the table exists.
    #[tokio::test]
    async fn the_roots_of_a_fact_table_of_a_role_gated_type_are_absent_without_the_role() {
        let vars = serde_json::json!({"aggregates": [{"revenue_sum": true}]});
        for document in [
            "{ sales_aggregate { count } }",
            "{ sales_aggregate }",
            "{ sales_aggregate { bogus } }",
            "{ sales_window }",
            "{ sales_window { rank } }",
        ] {
            let (res, sql) = run(finance_gated(), document, &vars, &principal()).await;
            assert_eq!(
                message(&res),
                "Fact table 'tf_sales' not found in schema",
                "{document}: answered as a table that does not exist"
            );
            assert!(sql.is_none(), "{document}: the statement reached the database: {sql:?}");
        }
        let (res, _) =
            run(finance_gated(), "{ sale_aggregate { count } }", &vars, &principal()).await;
        assert_eq!(message(&res), "Fact table 'tf_sale' not found in schema", "no suggestion");
    }

    /// Control: the role's holder is served, and is suggested the table.
    #[tokio::test]
    async fn the_holder_of_the_role_is_served_the_roots() {
        let vars = serde_json::json!({"aggregates": [{"revenue_sum": true}]});
        let (res, sql) =
            run(finance_gated(), "{ sales_aggregate { count } }", &vars, &finance()).await;
        res.expect("`finance` is held");
        assert!(sql.is_some());
        let (res, _) =
            run(finance_gated(), "{ sale_aggregate { count } }", &vars, &finance()).await;
        assert!(message(&res).contains("Did you mean 'tf_sales'"), "{res:?}");
    }

    /// Introspection lists the roots and their row type only to a caller who may run them.
    #[tokio::test]
    async fn introspection_lists_the_roots_only_to_the_holder_of_the_role() {
        let document = r#"{ query: __type(name: "Query") { fields { name } }
                           row: __type(name: "SalesAggregateRow") { name } }"#;
        for (caller, listed) in [(principal(), false), (finance(), true)] {
            let (res, _) = run(finance_gated(), document, &serde_json::json!({}), &caller).await;
            let response = res.expect("introspection is served");
            let fields = response["data"]["query"]["fields"].to_string();
            assert!(response["data"]["query"]["fields"].is_array(), "{response}");
            assert_eq!(
                fields.contains("\"sales_aggregate\""),
                listed,
                "{:?}: {fields}",
                caller.roles
            );
            assert_eq!(fields.contains("\"sales_window\""), listed, "{:?}: {fields}", caller.roles);
            assert_eq!(
                response["data"]["row"].is_object(),
                listed,
                "{:?}: {response}",
                caller.roles
            );
        }
    }

    /// The schema a caller is shown, which the exports describe, lacks the roots and their row
    /// type, not only the introspection that prunes what nothing reaches.
    #[test]
    fn the_visible_schema_lacks_the_roots_and_row_type_without_the_role() {
        for (caller, shown) in [(principal(), false), (finance(), true)] {
            let visible =
                crate::security::role_gate::visible_schema(&finance_gated(), Some(&caller));
            assert_eq!(visible.fact_root("sales_aggregate").is_some(), shown, "{:?}", caller.roles);
            assert_eq!(
                visible.derived.fact_rows.iter().any(|t| t.name == "SalesAggregateRow"),
                shown,
                "{:?}",
                caller.roles
            );
            assert_eq!(
                visible.raw_schema().contains("type SalesAggregateRow"),
                shown,
                "{:?}",
                caller.roles
            );
        }
    }

    /// The embedder entries take the metadata, not a root: the type's role refuses there.
    #[tokio::test]
    async fn the_embedder_entry_refuses_a_fact_table_of_a_role_gated_type() {
        let schema = finance_gated();
        let metadata = schema.get_fact_table("tf_sales").unwrap().clone();
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let res = Executor::new(schema, adapter.clone())
            .execute_aggregate_query(
                &serde_json::json!({"aggregates": [{"revenue_sum": true}]}),
                "sales_aggregate",
                &metadata,
            )
            .await;
        assert!(
            matches!(res, Err(crate::error::FraiseQLError::Authorization { .. })),
            "`Sale` requires `finance`: {res:?}"
        );
        assert!(adapter.captured_aggregate_sql().is_none());
    }

    // A temporal bucket reads its source column.
    #[tokio::test]
    async fn a_temporal_bucket_of_a_column_the_caller_may_not_read_is_refused() {
        assert_refused(
            serde_json::json!({"groupBy": {"closed_at_day": true}, "aggregates": [{"count": true}]}),
            "{ sales_aggregate { count } }",
            "a day bucket of a masked timestamp is the timestamp, truncated",
        )
        .await;
    }

    // A native dimension mapping is read as the dimension key it maps.
    #[tokio::test]
    async fn a_natively_mapped_dimension_the_caller_may_not_read_is_refused() {
        let mut schema = schema();
        schema
            .fact_tables
            .get_mut("tf_sales")
            .unwrap()
            .native_dimension_mapping
            .insert("segment".to_string(), "segment_col".to_string());
        let vars =
            serde_json::json!({"groupBy": {"segment": true}, "aggregates": [{"count": true}]});
        let (res, sql) = run(schema, "{ sales_aggregate { count } }", &vars, &principal()).await;
        assert!(
            matches!(res, Err(crate::error::FraiseQLError::Authorization { .. })),
            "`segment_col` holds `Sale.segment`: {res:?}"
        );
        assert!(sql.is_none(), "the statement reached the database: {sql:?}");
    }

    // The schema's registration of the table decides: an embedder handing in metadata
    // without the link does not unlink it.
    #[tokio::test]
    async fn the_schemas_link_holds_for_metadata_handed_in_without_it() {
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let mut unlinked = linked_fact_table();
        unlinked.type_name = None;
        let res = Executor::new(schema(), adapter.clone())
            .execute_aggregate_query(
                &serde_json::json!({"aggregates": [{"margin_sum": true}]}),
                "sales_aggregate",
                &unlinked,
            )
            .await;
        assert!(
            matches!(res, Err(crate::error::FraiseQLError::Authorization { .. })),
            "the registered `tf_sales` is read as `Sale`: {res:?}"
        );
        assert!(adapter.captured_aggregate_sql().is_none());
    }

    // Value functions and the final order read their fields too.
    #[tokio::test]
    async fn a_window_value_function_or_final_order_over_a_gated_field_is_refused() {
        assert_refused(
            serde_json::json!({
                "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
                "windows": [{
                    "function": {"type": "lag", "field": "cost"},
                    "alias": "previous_cost",
                    "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
                }]
            }),
            "{ sales_window }",
            "the previous row's refused cost is its value",
        )
        .await;
        assert_refused(
            serde_json::json!({
                "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
                "windows": [],
                "orderBy": [{"field": "margin", "direction": "DESC"}]
            }),
            "{ sales_window }",
            "rows ordered by a masked measure are ranked by it",
        )
        .await;
    }

    // Control: a final order by a window alias references nothing new.
    #[tokio::test]
    async fn a_window_ordered_by_its_own_aliases_is_served() {
        let vars = serde_json::json!({
            "select": [{"type": "measure", "name": "revenue", "alias": "rev"}],
            "windows": [{
                "function": {"type": "row_number"},
                "alias": "rank",
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            }],
            "orderBy": [{"field": "rank", "direction": "ASC"}]
        });
        let (res, sql) = run(schema(), "{ sales_window }", &vars, &principal()).await;
        res.expect("aliases of readable outputs are no new reference");
        assert!(sql.is_some());
    }

    // A denormalized filter is a native column: its condition is a reference too.
    #[tokio::test]
    async fn a_filter_on_a_gated_native_column_is_refused() {
        assert_refused(
            serde_json::json!({"where": {"closed_at_gte": "2026-01-01"}, "aggregates": [{"count": true}]}),
            "{ sales_aggregate { count } }",
            "a count of rows closed after a date asks about the masked timestamp",
        )
        .await;
    }

    // Partitioning alone, and a window's own order alone, each read the field.
    #[tokio::test]
    async fn a_partition_or_a_window_order_over_a_gated_field_is_refused() {
        assert_refused(
            serde_json::json!({
                "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
                "windows": [{
                    "function": {"type": "row_number"},
                    "alias": "rank",
                    "partitionBy": [{"type": "dimension", "path": "segment"}],
                    "orderBy": [{"field": "revenue", "direction": "DESC"}]
                }]
            }),
            "{ sales_window }",
            "a rank per masked segment groups rows by it",
        )
        .await;
        assert_refused(
            serde_json::json!({
                "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
                "windows": [{
                    "function": {"type": "row_number"},
                    "alias": "rank",
                    "partitionBy": [{"type": "dimension", "path": "category"}],
                    "orderBy": [{"field": "margin", "direction": "DESC"}]
                }]
            }),
            "{ sales_window }",
            "a rank by a masked measure ranks by it",
        )
        .await;
    }

    // Control: readable fields of a linked fact table are served.
    #[tokio::test]
    async fn a_readable_measure_of_a_linked_fact_table_is_served() {
        let vars = serde_json::json!({
            "where": {"tenant_id_eq": "tenant-abc"},
            "groupBy": {"category": true},
            "aggregates": [{"revenue_sum": true}, {"count": true}],
            "orderBy": {"revenue_sum": "DESC"}
        });
        let (res, sql) = run(schema(), "{ sales_aggregate { count } }", &vars, &principal()).await;
        res.expect("an ungated aggregate is served");
        assert!(sql.is_some());
    }

    // Control: the scope's holder may aggregate the field.
    #[tokio::test]
    async fn a_holder_of_the_scope_may_aggregate_the_field() {
        let analyst = SecurityContext {
            roles: vec!["analyst".to_string()],
            ..principal()
        };
        let vars = serde_json::json!({"aggregates": [{"margin_sum": true}]});
        let (res, sql) = run(schema(), "{ sales_aggregate { count } }", &vars, &analyst).await;
        res.expect("`read:margin` is held");
        assert!(sql.is_some());
    }
}

// ── AB 3: an anonymous aggregate or window under a row policy fails closed (#784 parity) ──
//
// The regular read refuses an anonymous request when a row policy is configured: the policy
// cannot be evaluated without a principal (#784). The aggregate and window runners skip the
// policy when the principal is absent, and read every row.
mod anonymous_under_a_row_policy {
    use super::*;

    fn executor(adapter: &Arc<CapturingMockAdapter>) -> Executor {
        let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
        Executor::with_config(schema_with_fact_table(), Arc::clone(adapter), config)
    }

    #[tokio::test]
    async fn an_anonymous_aggregate_is_refused() {
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let vars = serde_json::json!({"aggregates": [{"count": true}]});
        let res = executor(&adapter).execute("{ sales_aggregate { count } }", Some(&vars)).await;
        assert!(res.is_err(), "no principal, no policy to evaluate: {res:?}");
        let sql = adapter.captured_aggregate_sql();
        assert!(sql.is_none(), "every tenant's rows were read: {sql:?}");
    }

    #[tokio::test]
    async fn an_anonymous_window_is_refused() {
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let vars = serde_json::json!({
            "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
            "windows": []
        });
        let res = executor(&adapter).execute("{ sales_window }", Some(&vars)).await;
        assert!(res.is_err(), "no principal, no policy to evaluate: {res:?}");
        let sql = adapter.captured_aggregate_sql();
        assert!(sql.is_none(), "every tenant's rows were read: {sql:?}");
    }

    #[tokio::test]
    async fn the_embedder_entries_are_refused() {
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let metadata = schema_with_fact_table().get_fact_table("tf_sales").unwrap().clone();
        let exec = executor(&adapter);
        let agg = exec
            .execute_aggregate_query(
                &serde_json::json!({"aggregates": [{"count": true}]}),
                "sales_aggregate",
                &metadata,
            )
            .await;
        assert!(agg.is_err(), "aggregate embedder entry: {agg:?}");
        let win = exec
            .execute_window_query(
                &serde_json::json!({
                    "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
                    "windows": []
                }),
                "sales_window",
                &metadata,
            )
            .await;
        assert!(win.is_err(), "window embedder entry: {win:?}");
        let sql = adapter.captured_aggregate_sql();
        assert!(sql.is_none(), "every tenant's rows were read: {sql:?}");
    }

    // Control: with a principal, the policy is composed and the read runs.
    #[tokio::test]
    async fn an_authenticated_aggregate_is_served_under_the_policy() {
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let vars = serde_json::json!({"aggregates": [{"count": true}]});
        executor(&adapter)
            .execute_with_security(
                "{ sales_aggregate { count } }",
                Some(&vars),
                &tenant_security_context("tenant-abc"),
            )
            .await
            .expect("the policy is evaluated for the principal");
        assert!(adapter.captured_aggregate_sql().is_some());
    }
}

// ── Tenant-scoped fact tables (#1799) ───────────────────────────────────────
//
// An aggregate or a window root has no `inject_params`, so in a schema that scopes a fact
// table's rows by tenant, nothing but a row policy can scope it: without one it is refused,
// from every door, before any SQL runs.
mod tenant_scoped_fact_tables {
    use super::*;
    use crate::{
        error::FraiseQLError,
        schema::{
            CompiledSchema, FieldDefinition, FieldType, InjectedParamSource, QueryDefinition,
            TypeDefinition,
        },
    };

    const REFUSAL: &str = "no row policy scopes its aggregate and window roots";

    fn aggregate_vars() -> serde_json::Value {
        serde_json::json!({ "aggregates": [{"count": true}] })
    }

    fn window_vars() -> serde_json::Value {
        serde_json::json!({
            "select": [{"type": "filter", "name": "tenant_id", "alias": "tenant_id"}],
            "windows": [{"function": {"type": "row_number"}, "alias": "rank"}]
        })
    }

    /// A query injecting `column` from the token, returning `return_type`.
    fn injecting_query(name: &str, return_type: &str, column: &str) -> QueryDefinition {
        let mut query = QueryDefinition::new(name, return_type);
        query
            .inject_params
            .insert(column.to_string(), InjectedParamSource::Jwt("tid".to_string()));
        query
    }

    /// `tf_sales` (with its `tenant_id` filter) beside a list root injecting `tenant_id`.
    fn injected_column_schema() -> CompiledSchema {
        let mut schema = schema_with_fact_table();
        schema.queries.push(injecting_query("rows", "Row", "tenant_id"));
        schema.build_indexes();
        schema
    }

    fn with_row_scope(mut schema: CompiledSchema, row_scope: FactRowScope) -> CompiledSchema {
        let mut table = schema.get_fact_table("tf_sales").unwrap().clone();
        table.row_scope = row_scope;
        schema.add_fact_table("tf_sales".to_string(), table);
        schema
    }

    /// `metadata` with every name that could tie it to a tenant removed: what an embedder
    /// could hand the public entries.
    fn stripped(metadata: &FactTableMetadata) -> FactTableMetadata {
        FactTableMetadata {
            type_name: None,
            denormalized_filters: vec![],
            ..metadata.clone()
        }
    }

    async fn graphql(
        schema: CompiledSchema,
        config: RuntimeConfig,
        root: &str,
        vars: &serde_json::Value,
    ) -> (crate::error::Result<serde_json::Value>, Option<String>) {
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let ctx = tenant_security_context("tenant-abc");
        let res = Executor::with_config(schema, adapter.clone(), config)
            .execute_with_security(root, Some(vars), &ctx)
            .await;
        (res, adapter.captured_aggregate_sql())
    }

    fn assert_refused(
        res: &crate::error::Result<serde_json::Value>,
        sql: Option<&String>,
        what: &str,
    ) {
        assert!(
            refusal(res).contains(REFUSAL),
            "{what}: expected the #1799 refusal, got {res:?}"
        );
        assert!(sql.is_none(), "{what}: the statement reached the database: {sql:?}");
    }

    /// The message of a validation refusal; empty for anything else.
    fn refusal(res: &crate::error::Result<serde_json::Value>) -> &str {
        match res {
            Err(FraiseQLError::Validation { message, .. }) => message,
            _ => "",
        }
    }

    #[tokio::test]
    async fn an_aggregate_over_a_fact_table_with_an_injected_column_is_refused() {
        let (res, sql) = graphql(
            injected_column_schema(),
            RuntimeConfig::default(),
            "{ sales_aggregate { count } }",
            &aggregate_vars(),
        )
        .await;
        assert_refused(&res, sql.as_ref(), "aggregate");
        let message = refusal(&res);
        assert!(
            message.contains("its column 'tenant_id' is injected by query 'rows'"),
            "the reason names the matched inject key: {message}"
        );
    }

    #[tokio::test]
    async fn a_window_over_a_fact_table_with_an_injected_column_is_refused() {
        let (res, sql) = graphql(
            injected_column_schema(),
            RuntimeConfig::default(),
            "{ sales_window }",
            &window_vars(),
        )
        .await;
        assert_refused(&res, sql.as_ref(), "window");
    }

    /// A fact table read as a type its queries inject is scoped, whatever its columns are
    /// called: here the injected column is not one of the fact table's.
    #[tokio::test]
    async fn an_aggregate_over_a_fact_table_read_as_an_injected_type_is_refused() {
        let mut schema = schema_with_fact_table();
        schema.types.push(TypeDefinition {
            fields: vec![
                FieldDefinition::new("revenue", FieldType::Float),
                FieldDefinition::new("tenant_id", FieldType::String),
            ],
            ..TypeDefinition::new("Sale", "v_sale")
        });
        schema.queries.push(injecting_query("sales", "Sale", "owner_id"));
        let mut table = schema.get_fact_table("tf_sales").unwrap().clone();
        table.type_name = Some("Sale".to_string());
        schema.add_fact_table("tf_sales".to_string(), table);
        schema.build_indexes();

        let (res, sql) = graphql(
            schema,
            RuntimeConfig::default(),
            "{ sales_aggregate { count } }",
            &aggregate_vars(),
        )
        .await;
        assert_refused(&res, sql.as_ref(), "linked aggregate");
        let message = refusal(&res);
        assert!(
            message.contains("type 'Sale', which query 'sales' reads injecting 'owner_id'"),
            "{message}"
        );
    }

    /// The type's own `inject_params` scope it as its queries' do.
    #[tokio::test]
    async fn an_aggregate_over_a_fact_table_read_as_a_type_injecting_itself_is_refused() {
        let mut schema = schema_with_fact_table();
        let mut sale = TypeDefinition {
            fields: vec![
                FieldDefinition::new("revenue", FieldType::Float),
                FieldDefinition::new("tenant_id", FieldType::String),
            ],
            ..TypeDefinition::new("Sale", "v_sale")
        };
        sale.inject_params
            .insert("owner_id".to_string(), InjectedParamSource::Jwt("sub".to_string()));
        schema.types.push(sale);
        let mut table = schema.get_fact_table("tf_sales").unwrap().clone();
        table.type_name = Some("Sale".to_string());
        schema.add_fact_table("tf_sales".to_string(), table);
        schema.build_indexes();

        let (res, sql) = graphql(
            schema,
            RuntimeConfig::default(),
            "{ sales_aggregate { count } }",
            &aggregate_vars(),
        )
        .await;
        assert_refused(&res, sql.as_ref(), "type-level inject");
    }

    #[tokio::test]
    async fn the_public_aggregate_entry_is_refused_even_with_stripped_metadata() {
        let schema = injected_column_schema();
        let registered = schema.get_fact_table("tf_sales").unwrap().clone();
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter.clone());
        for metadata in [registered.clone(), stripped(&registered)] {
            let res = executor
                .execute_aggregate_query(&aggregate_vars(), "sales_aggregate", &metadata)
                .await;
            assert_refused(&res, adapter.captured_aggregate_sql().as_ref(), "embedder aggregate");
        }
    }

    #[tokio::test]
    async fn the_public_window_entry_is_refused_even_with_stripped_metadata() {
        let schema = injected_column_schema();
        let registered = schema.get_fact_table("tf_sales").unwrap().clone();
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter.clone());
        for metadata in [registered.clone(), stripped(&registered)] {
            let res =
                executor.execute_window_query(&window_vars(), "sales_window", &metadata).await;
            assert_refused(&res, adapter.captured_aggregate_sql().as_ref(), "embedder window");
        }
    }

    /// A row policy scopes the roots: the read is served, under the policy's predicate.
    #[tokio::test]
    async fn a_row_policy_scopes_the_roots_instead() {
        let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
        let (res, sql) = graphql(
            injected_column_schema(),
            config,
            "{ sales_aggregate { count } }",
            &aggregate_vars(),
        )
        .await;
        assert!(res.is_ok(), "{res:?}");
        assert!(sql.unwrap().contains("tenant_id"), "the policy's predicate scopes it");
    }

    /// A relation that filters its own rows is served, and the server adds no predicate.
    #[tokio::test]
    async fn a_relation_scoped_fact_table_is_served_unfiltered_by_the_server() {
        let schema = with_row_scope(injected_column_schema(), FactRowScope::Relation);
        let (res, sql) = graphql(
            schema,
            RuntimeConfig::default(),
            "{ sales_aggregate { count } }",
            &aggregate_vars(),
        )
        .await;
        assert!(res.is_ok(), "{res:?}");
        assert!(!sql.unwrap().contains("tenant_id"), "the relation scopes itself");
    }

    /// An embedder's metadata cannot declare the relation scoped for a table the schema
    /// registers as scoped by the server.
    #[tokio::test]
    async fn embedder_metadata_cannot_declare_a_registered_table_relation_scoped() {
        let schema = injected_column_schema();
        let mut metadata = schema.get_fact_table("tf_sales").unwrap().clone();
        metadata.row_scope = FactRowScope::Relation;
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let res = Executor::new(schema, adapter.clone())
            .execute_aggregate_query(&aggregate_vars(), "sales_aggregate", &metadata)
            .await;
        assert_refused(&res, adapter.captured_aggregate_sql().as_ref(), "embedder opt-out");
    }

    /// `row_scope = "relation"` on a fact table no injected read concerns changes nothing.
    #[tokio::test]
    async fn row_scope_on_a_fact_table_that_is_not_tenant_scoped_is_harmless() {
        for row_scope in [FactRowScope::Server, FactRowScope::Relation] {
            let schema = with_row_scope(schema_with_fact_table(), row_scope);
            let (res, sql) = graphql(
                schema,
                RuntimeConfig::default(),
                "{ sales_aggregate { count } }",
                &aggregate_vars(),
            )
            .await;
            assert!(res.is_ok(), "{row_scope:?}: {res:?}");
            assert!(sql.is_some(), "{row_scope:?}: served");
        }
    }
}

/// The request surface of a fact table's roots (#1800): the response key, the document around
/// the root, and the arguments it reads.
mod fact_table_roots {
    use super::*;
    use crate::error::FraiseQLError;

    const COUNT: &str = "aggregates: [{count: true}]";
    const RANK: &str = r#"select: [{type: "filter", name: "tenant_id", alias: "tenant_id"}], windows: [{function: {type: "row_number"}, alias: "rank"}]"#;

    async fn run(
        document: &str,
        vars: &serde_json::Value,
    ) -> (crate::error::Result<serde_json::Value>, Option<String>) {
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let res = Executor::new(schema_with_fact_table(), adapter.clone())
            .execute_with_security(document, Some(vars), &tenant_security_context("tenant-abc"))
            .await;
        (res, adapter.captured_aggregate_sql())
    }

    fn validation_message(res: &crate::error::Result<serde_json::Value>) -> &str {
        match res {
            Err(FraiseQLError::Validation { message, .. }) => message,
            _ => "",
        }
    }

    /// The positive control: a root written with no `table` is served.
    #[tokio::test]
    async fn a_root_without_table_is_served() {
        for document in [
            format!("{{ sales_aggregate({COUNT}) {{ count }} }}"),
            format!("{{ sales_window({RANK}) }}"),
        ] {
            let (res, sql) = run(&document, &serde_json::json!({})).await;
            assert!(res.is_ok(), "{document}: {res:?}");
            assert!(sql.is_some(), "{document}: reached the database");
        }
    }

    /// An aliased root answers under its alias, as every field does (§ 2.7).
    #[tokio::test]
    async fn an_aliased_root_answers_under_its_alias() {
        for (document, alias) in [
            (format!("{{ totals: sales_aggregate({COUNT}) {{ count }} }}"), "totals"),
            (format!("{{ ranked: sales_window({RANK}) }}"), "ranked"),
        ] {
            let (res, _) = run(&document, &serde_json::json!({})).await;
            assert!(res.is_ok(), "{document}: {res:?}");
            let data = res.unwrap()["data"].clone();
            assert!(data.get(alias).is_some(), "{document}: answers under `{alias}`: {data}");
            assert_eq!(data.as_object().map(serde_json::Map::len), Some(1), "{document}: {data}");
        }
    }

    /// The root names its fact table, so `table` is refused, written inline or sent as a
    /// variable of that name, rather than checked against it.
    #[tokio::test]
    async fn a_table_argument_is_refused() {
        let expected = "`table` is no longer an argument: the root reads fact table `tf_sales`";
        for (document, vars) in [
            (
                format!(r#"{{ sales_aggregate(table: "tf_sales", {COUNT}) {{ count }} }}"#),
                serde_json::json!({}),
            ),
            (
                format!("{{ sales_aggregate({COUNT}) {{ count }} }}"),
                serde_json::json!({"table": "tf_sales"}),
            ),
            (
                format!(r#"{{ sales_window(table: "tf_sales", {RANK}) }}"#),
                serde_json::json!({}),
            ),
            (format!("{{ sales_window({RANK}) }}"), serde_json::json!({"table": "tf_sales"})),
        ] {
            let (res, sql) = run(&document, &vars).await;
            let message = validation_message(&res);
            assert!(message.contains(expected), "{document} {vars}: {res:?}");
            assert!(sql.is_none(), "{document} {vars}: reached the database: {sql:?}");
        }
    }

    /// The public embedder entries read the same request, so they refuse it too.
    #[tokio::test]
    async fn the_embedder_entries_refuse_a_table_key() {
        let schema = schema_with_fact_table();
        let metadata = schema.get_fact_table("tf_sales").unwrap().clone();
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter.clone());
        let aggregate = executor
            .execute_aggregate_query(
                &serde_json::json!({"table": "tf_sales", "aggregates": [{"count": true}]}),
                "sales_aggregate",
                &metadata,
            )
            .await;
        let window = executor
            .execute_window_query(
                &serde_json::json!({"table": "tf_sales", "windows": []}),
                "sales_window",
                &metadata,
            )
            .await;
        for res in [aggregate, window] {
            assert!(
                validation_message(&res).contains("`table` is no longer an argument"),
                "{res:?}"
            );
        }
        assert!(adapter.captured_aggregate_sql().is_none(), "nothing reached the database");
    }

    /// A fact-table root is executed alone: a document holding it and any other root was
    /// answered for the first root only, and the others were silently dropped. Refused, naming
    /// the roots, whichever comes first.
    #[tokio::test]
    async fn a_document_with_a_fact_table_root_and_another_root_is_refused() {
        let (res, _) = run(
            &format!(
                "{{ totals: sales_aggregate({COUNT}) {{ count }} orders {{ id }} __typename }}"
            ),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(
            validation_message(&res),
            "`totals` must be the only root of its document; it was sent with `orders`, \
             `__typename`: send each in its own request",
            "names the root by its response key and every other root: {res:?}"
        );
        for document in [
            format!("{{ sales_aggregate({COUNT}) {{ count }} orders {{ id }} }}"),
            format!("{{ orders {{ id }} sales_aggregate({COUNT}) {{ count }} }}"),
            format!(
                "{{ a: sales_aggregate({COUNT}) {{ count }} b: sales_aggregate({COUNT}) {{ count }} }}"
            ),
            format!("{{ sales_window({RANK}) __typename }}"),
            format!("{{ __typename sales_window({RANK}) }}"),
        ] {
            let (res, sql) = run(&document, &serde_json::json!({})).await;
            let message = validation_message(&res);
            assert!(
                message.contains("must be the only root of its document"),
                "{document}: {res:?}"
            );
            assert!(sql.is_none(), "{document}: reached the database: {sql:?}");
        }
    }

    /// A mutation is routed by the mutation branch, which runs every root: a mutation whose
    /// name ends in `_aggregate` is not a fact-table root, so the rule does not refuse it.
    #[tokio::test]
    async fn a_mutation_document_is_not_held_to_the_fact_table_root_rule() {
        let (res, _) =
            run("mutation { log_aggregate { id } other { id } }", &serde_json::json!({})).await;
        assert!(
            !validation_message(&res).contains("must be the only root"),
            "a mutation is not a fact-table root: {res:?}"
        );
    }
}

/// An aggregate root's rows are its published `<Stem>AggregateRow` objects (#1800): each row
/// holds the fields the selection set selects, under their response keys, and nothing else.
mod row_projection {
    use super::*;
    use crate::error::FraiseQLError;

    const ARGS: &str = "groupBy: {tenant_id: true}, aggregates: [{count: true}]";

    fn published_schema() -> crate::schema::CompiledSchema {
        let mut schema = schema_with_fact_table();
        schema.build_indexes();
        schema
    }

    async fn run(
        document: &str,
        rows: &[serde_json::Value],
    ) -> (crate::error::Result<serde_json::Value>, Option<String>) {
        let rows = rows
            .iter()
            .map(|row| serde_json::from_value(row.clone()).expect("a row is an object"))
            .collect();
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_aggregate_rows(rows));
        let res = Executor::new(published_schema(), adapter.clone())
            .execute_with_security(document, None, &tenant_security_context("tenant-abc"))
            .await;
        (res, adapter.captured_aggregate_sql())
    }

    fn validation_message(res: &crate::error::Result<serde_json::Value>) -> &str {
        match res {
            Err(FraiseQLError::Validation { message, .. }) => message,
            _ => "",
        }
    }

    /// Aliases and `__typename` are honoured, a key the selection does not select is dropped,
    /// and a selected field the arguments did not compute is `null`.
    #[tokio::test]
    async fn a_row_holds_exactly_the_selected_fields_under_their_response_keys() {
        let (res, _) = run(
            &format!(
                "{{ s: sales_aggregate({ARGS}) {{ t: tenant_id count __typename revenue_sum \
                 again: count }} }}"
            ),
            &[serde_json::json!({"tenant_id": "a", "count": 3, "other": 1})],
        )
        .await;
        let response = res.expect("served");
        assert_eq!(
            response["data"]["s"],
            serde_json::json!([{
                "t": "a", "count": 3, "__typename": "SalesAggregateRow", "revenue_sum": null,
                "again": 3
            }]),
            "{response}"
        );
        assert!(response.get("errors").is_none(), "{response}");
        let keys: Vec<&String> = response["data"]["s"][0].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["t", "count", "__typename", "revenue_sum", "again"], "document order");
    }

    /// Fragments on the row type apply, and `@skip`/`@include` drop what they exclude.
    #[tokio::test]
    async fn fragments_and_directives_select_as_on_any_object() {
        for document in [
            format!(
                "{{ sales_aggregate({ARGS}) {{ ... on SalesAggregateRow {{ count }} \
                 tenant_id @skip(if: true) }} }}"
            ),
            format!(
                "{{ sales_aggregate({ARGS}) {{ ...Counts tenant_id @include(if: false) }} }} \
                 fragment Counts on SalesAggregateRow {{ count }}"
            ),
        ] {
            let (res, _) =
                run(&document, &[serde_json::json!({"tenant_id": "a", "count": 3})]).await;
            assert!(res.is_ok(), "{document}: {res:?}");
            let response = res.unwrap();
            assert_eq!(
                response["data"]["sales_aggregate"],
                serde_json::json!([{"count": 3}]),
                "{document}"
            );
        }
    }

    /// An `Int` holds a signed 32-bit integer (§ 3.5.1), and a count or an `int8` key is wider:
    /// a value outside the range is a field error at its path, its field `null`, never a
    /// silently wrong number.
    #[tokio::test]
    async fn an_int_outside_the_32_bit_range_is_a_field_error() {
        for (count, served) in [
            (serde_json::json!(2_147_483_647_i64), true),
            (serde_json::json!(-2_147_483_648_i64), true),
            (serde_json::json!(2_147_483_648_i64), false),
            (serde_json::json!(-2_147_483_649_i64), false),
        ] {
            let (res, _) = run(
                &format!("{{ s: sales_aggregate({ARGS}) {{ tenant_id n: count }} }}"),
                &[
                    serde_json::json!({"tenant_id": "a", "count": 1}),
                    serde_json::json!({"tenant_id": "b", "count": count}),
                ],
            )
            .await;
            let response = res.expect("served");
            let rows = &response["data"]["s"];
            assert_eq!(rows[0]["n"], 1, "{response}");
            assert_eq!(rows[1]["tenant_id"], "b", "the row stays: {response}");
            if served {
                assert_eq!(rows[1]["n"], count, "{response}");
                assert!(response.get("errors").is_none(), "{response}");
            } else {
                assert_eq!(rows[1]["n"], serde_json::Value::Null, "{response}");
                let errors = response["errors"].as_array().expect("an error");
                assert_eq!(errors.len(), 1, "{response}");
                assert_eq!(errors[0]["path"], serde_json::json!(["s", 1, "n"]), "{response}");
                let message = errors[0]["message"].as_str().unwrap();
                assert!(
                    message.contains("SalesAggregateRow.count")
                        && message.contains("32-bit")
                        && message.contains(&count.to_string()),
                    "{message}"
                );
            }
        }
    }

    /// A row is an object (§ 5.3.3): a root with no selection set, a field the row does not
    /// publish (§ 5.3.1) and a selection set on a leaf are refused before the database is read.
    #[tokio::test]
    async fn a_selection_the_row_type_does_not_publish_is_refused() {
        for (document, expected) in [
            (
                format!("{{ s: sales_aggregate({ARGS}) }}"),
                "`s` returns `[SalesAggregateRow!]!` and must select its fields",
            ),
            (
                format!("{{ sales_aggregate({ARGS}) {{ count bogus revenue_total }} }}"),
                "`SalesAggregateRow` has no field `bogus`, `revenue_total`",
            ),
            (
                format!("{{ sales_aggregate({ARGS}) {{ count {{ value }} }} }}"),
                "`SalesAggregateRow.count` is a leaf and takes no selection set",
            ),
        ] {
            let (res, sql) = run(&document, &[]).await;
            assert!(validation_message(&res).contains(expected), "{document}: {res:?}");
            assert!(sql.is_none(), "{document}: reached the database: {sql:?}");
        }
    }

    /// A window root returns `[JSON!]!`, a leaf: its rows are the `select` and `windows`
    /// aliases, and a selection set on it is refused rather than ignored.
    #[tokio::test]
    async fn a_window_root_takes_no_selection_set() {
        let (res, sql) = run(
            r#"{ sales_window(select: [{type: "filter", name: "tenant_id", alias: "tenant_id"}]) { tenant_id } }"#,
            &[],
        )
        .await;
        assert!(
            validation_message(&res)
                .contains("`sales_window` returns `[JSON!]!`, a leaf, and takes no selection set"),
            "{res:?}"
        );
        assert!(sql.is_none(), "reached the database: {sql:?}");
    }
}
