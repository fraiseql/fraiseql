//! Tests for RLS enforcement in aggregate and window query paths.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;

use crate::{
    compiler::fact_table::{
        DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
    },
    runtime::{Executor, RuntimeConfig, executor::test_support::CapturingMockAdapter},
    security::{DefaultRLSPolicy, SecurityContext},
};

fn tenant_security_context(tenant_id: &str) -> SecurityContext {
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
fn schema_with_fact_table() -> crate::schema::CompiledSchema {
    let mut schema = crate::schema::CompiledSchema::new();
    schema.add_fact_table(
        "tf_sales".to_string(),
        FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            type_name:                None,
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
    let vars = serde_json::json!({ "table": "tf_sales", "aggregates": [{"count": {}}] });
    let _result = executor
        .execute_with_security("{ sales_aggregate }", Some(&vars), &ctx)
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
    let vars = serde_json::json!({ "table": "tf_sales", "aggregates": [{"count": {}}] });
    let _result = executor
        .execute_with_security("{ sales_aggregate }", Some(&vars), &ctx)
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
    let vars = serde_json::json!({ "table": "tf_sales", "aggregates": [{"count": {}}] });
    let _result = executor
        .execute_with_security("{ sales_aggregate }", Some(&vars), &ctx)
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
        "table": "tf_sales",
        "aggregates": [{"count": {}}],
        "where": {"tenant_id_eq": "tenant-abc"}
    });
    let _result = executor
        .execute_with_security("{ sales_aggregate }", Some(&vars), &ctx)
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
        "table": "tf_sales",
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
        "table": "tf_sales",
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
        let mut vars = vars.clone();
        vars["table"] = serde_json::json!("tf_sales");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let res = Executor::new(schema, adapter.clone())
            .execute_with_security(root, Some(&vars), ctx)
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
            serde_json::json!({"aggregates": [{"margin_sum": {}}]}),
            "{ sales_aggregate }",
            "the sum of a masked measure discloses it",
        )
        .await;
        assert_refused(
            serde_json::json!({"aggregates": [{"cost_max": {}}]}),
            "{ sales_aggregate }",
            "the max of a refused measure is its value",
        )
        .await;
    }

    #[tokio::test]
    async fn grouping_by_a_dimension_the_caller_may_not_read_is_refused() {
        assert_refused(
            serde_json::json!({"groupBy": {"segment": true}, "aggregates": [{"count": {}}]}),
            "{ sales_aggregate }",
            "a group key is served as the value itself",
        )
        .await;
    }

    #[tokio::test]
    async fn an_aggregate_filter_on_a_field_the_caller_may_not_read_is_refused() {
        assert_refused(
            serde_json::json!({"where": {"margin_gt": 10}, "aggregates": [{"count": {}}]}),
            "{ sales_aggregate }",
            "a count filtered by a masked measure answers a question about it",
        )
        .await;
    }

    #[tokio::test]
    async fn an_aggregate_order_by_a_field_the_caller_may_not_read_is_refused() {
        assert_refused(
            serde_json::json!({
                "groupBy": {"category": true},
                "aggregates": [{"count": {}}],
                "orderBy": {"margin": "DESC"}
            }),
            "{ sales_aggregate }",
            "an order by a masked measure ranks by it",
        )
        .await;
    }

    #[tokio::test]
    async fn an_aggregate_over_an_authorize_field_is_refused() {
        assert_refused(
            serde_json::json!({"groupBy": {"note": true}, "aggregates": [{"count": {}}]}),
            "{ sales_aggregate }",
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
            serde_json::json!({"where": {"salary_gt": 100_000}, "aggregates": [{"count": {}}]});
        let (res, sql) = run(schema(), "{ sales_aggregate }", &vars, &principal()).await;
        assert!(
            matches!(res, Err(crate::error::FraiseQLError::Validation { .. })),
            "an undeclared JSONB key of a linked fact table is no field of its type: {res:?}"
        );
        assert!(sql.is_none(), "the statement reached the database: {sql:?}");
    }

    #[tokio::test]
    async fn a_fact_table_of_a_role_gated_type_is_refused() {
        let mut schema = schema();
        schema.types.iter_mut().find(|t| t.name == "Sale").unwrap().requires_role =
            Some("finance".to_string());
        schema.build_indexes();
        let vars = serde_json::json!({"aggregates": [{"revenue_sum": {}}]});
        let (res, sql) = run(schema, "{ sales_aggregate }", &vars, &principal()).await;
        assert!(
            matches!(res, Err(crate::error::FraiseQLError::Authorization { .. })),
            "`Sale` requires `finance`: {res:?}"
        );
        assert!(sql.is_none(), "the statement reached the database: {sql:?}");
    }

    // A temporal bucket reads its source column.
    #[tokio::test]
    async fn a_temporal_bucket_of_a_column_the_caller_may_not_read_is_refused() {
        assert_refused(
            serde_json::json!({"groupBy": {"closed_at_day": true}, "aggregates": [{"count": {}}]}),
            "{ sales_aggregate }",
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
        let vars = serde_json::json!({"groupBy": {"segment": true}, "aggregates": [{"count": {}}]});
        let (res, sql) = run(schema, "{ sales_aggregate }", &vars, &principal()).await;
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
                &serde_json::json!({"table": "tf_sales", "aggregates": [{"margin_sum": {}}]}),
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
            serde_json::json!({"where": {"closed_at_gte": "2026-01-01"}, "aggregates": [{"count": {}}]}),
            "{ sales_aggregate }",
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
            "aggregates": [{"revenue_sum": {}}, {"count": {}}],
            "orderBy": {"revenue_sum": "DESC"}
        });
        let (res, sql) = run(schema(), "{ sales_aggregate }", &vars, &principal()).await;
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
        let vars = serde_json::json!({"aggregates": [{"margin_sum": {}}]});
        let (res, sql) = run(schema(), "{ sales_aggregate }", &vars, &analyst).await;
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
        let vars = serde_json::json!({"table": "tf_sales", "aggregates": [{"count": {}}]});
        let res = executor(&adapter).execute("{ sales_aggregate }", Some(&vars)).await;
        assert!(res.is_err(), "no principal, no policy to evaluate: {res:?}");
        let sql = adapter.captured_aggregate_sql();
        assert!(sql.is_none(), "every tenant's rows were read: {sql:?}");
    }

    #[tokio::test]
    async fn an_anonymous_window_is_refused() {
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
        let vars = serde_json::json!({
            "table": "tf_sales",
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
                &serde_json::json!({"table": "tf_sales", "aggregates": [{"count": {}}]}),
                "sales_aggregate",
                &metadata,
            )
            .await;
        assert!(agg.is_err(), "aggregate embedder entry: {agg:?}");
        let win = exec
            .execute_window_query(
                &serde_json::json!({
                    "table": "tf_sales",
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
        let vars = serde_json::json!({"table": "tf_sales", "aggregates": [{"count": {}}]});
        executor(&adapter)
            .execute_with_security(
                "{ sales_aggregate }",
                Some(&vars),
                &tenant_security_context("tenant-abc"),
            )
            .await
            .expect("the policy is evaluated for the principal");
        assert!(adapter.captured_aggregate_sql().is_some());
    }
}
