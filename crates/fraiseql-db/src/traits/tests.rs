#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code

#[test]
fn database_adapter_is_send_sync() {
    // Static assertion: `dyn DatabaseAdapter` must be `Send + Sync`.
    // This test exists to catch accidental removal of `Send + Sync` bounds.
    // It only needs to compile — no runtime assertion required.
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<dyn super::DatabaseAdapter>();
}

/// What the trait's defaults hand an adapter that implements nothing.
///
/// The streaming defaults (#958): an adapter that implements none of the streaming
/// methods must still answer them, with exactly the rows its collecting methods
/// return — that is the whole contract of the default, and it is what lets a stub
/// adapter, a test double or a future backend be dropped into a streaming caller
/// without special-casing.
///
/// The mutation default is the opposite kind of question, and must answer the
/// opposite way: silence about a capability that writes must mean refused.
#[cfg(test)]
mod trait_defaults {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use fraiseql_error::Result;
    use futures::StreamExt as _;
    use serde_json::json;

    use crate::{
        dialect::RowViewColumnType,
        traits::{DatabaseAdapter, ProjectionRequest},
        types::{
            ColumnSpec, ColumnValue, DatabaseType, JsonbValue, PoolMetrics, ReadRouting,
            sql_hints::OrderByClause,
        },
        where_clause::WhereClause,
    };

    /// Implements only the *collecting* half of the trait.
    struct CollectOnlyAdapter {
        rows: Vec<JsonbValue>,
        /// What `applies_session_variables` reports.
        applies_session_variables: bool,
    }

    impl CollectOnlyAdapter {
        const fn new(rows: Vec<JsonbValue>) -> Self {
            Self {
                rows,
                applies_session_variables: true,
            }
        }
    }

    // Reason: the trait is declared with the async-trait macro; impls must match.
    #[async_trait]
    impl DatabaseAdapter for CollectOnlyAdapter {
        fn applies_session_variables(&self) -> bool {
            self.applies_session_variables
        }

        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(self.rows.clone())
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::types::sql_hints::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(self.rows.clone())
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::PostgreSQL
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics {
                total_connections:  0,
                idle_connections:   0,
                active_connections: 0,
                waiting_requests:   0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            Ok(vec![
                HashMap::from([("id".to_string(), json!(1))]),
                HashMap::from([("id".to_string(), json!(2))]),
            ])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            Ok(vec![HashMap::from([
                ("total".to_string(), json!(12.5)),
                ("count".to_string(), json!(2)),
                ("region".to_string(), json!("north")),
                ("empty".to_string(), serde_json::Value::Null),
            ])])
        }
    }

    #[tokio::test]
    async fn exact_aggregate_default_prints_only_the_named_numbers() {
        let adapter = CollectOnlyAdapter::new(Vec::new());

        let rows = adapter
            .execute_parameterized_aggregate_exact(
                "SELECT 1",
                &[],
                &[],
                ReadRouting::Any,
                &["total", "region", "empty"],
            )
            .await
            .expect("default exact aggregate");

        assert_eq!(
            rows,
            [HashMap::from([
                ("total".to_string(), json!("12.5")),
                ("count".to_string(), json!(2)),
                ("region".to_string(), json!("north")),
                ("empty".to_string(), serde_json::Value::Null),
            ])]
        );
    }

    #[tokio::test]
    async fn projection_default_streams_the_collected_rows_in_order() {
        let adapter = CollectOnlyAdapter::new(vec![
            JsonbValue::new(json!({"id": 1})),
            JsonbValue::new(json!({"id": 2})),
            JsonbValue::new(json!({"id": 3})),
        ]);

        let stream = adapter
            .stream_with_projection(&ProjectionRequest::new("v_user"), &[], ReadRouting::Any)
            .await
            .expect("default streaming read");
        let got: Vec<_> = stream.map(|r| r.expect("row").into_value()).collect().await;

        assert_eq!(got, vec![json!({"id": 1}), json!({"id": 2}), json!({"id": 3})]);
    }

    #[tokio::test]
    async fn row_query_default_streams_the_collected_rows_in_order() {
        let adapter = CollectOnlyAdapter::new(Vec::new());
        let columns = [ColumnSpec {
            name:        "id".to_string(),
            column_type: RowViewColumnType::Int64,
        }];

        let stream = adapter
            .stream_row_query("v_user", &columns, None, None, None, None)
            .await
            .expect("default streaming row read");
        let got: Vec<_> = stream.map(|r| r.expect("row")).collect().await;

        // `ColumnValue` carries no `PartialEq`, so the shape is asserted directly.
        let ids: Vec<i64> = got
            .iter()
            .map(|row| match row.as_slice() {
                [ColumnValue::Int64(id)] => *id,
                other => panic!("expected one Int64 column, got {other:?}"),
            })
            .collect();
        assert_eq!(ids, vec![1, 2]);
    }

    // Reason: the trait's required method returns a future; a double with no database
    // answers it immediately.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    impl crate::traits::RelayDatabaseAdapter for CollectOnlyAdapter {
        #[allow(clippy::too_many_arguments)] // Reason: mirrors the trait's signature
        async fn execute_relay_page(
            &self,
            _view: &str,
            _cursor_column: &str,
            _after: Option<crate::traits::RelayCursor>,
            _before: Option<crate::traits::RelayCursor>,
            _limit: u32,
            _forward: bool,
            _where_clause: Option<&WhereClause>,
            _order_by: Option<&[OrderByClause]>,
            _include_total_count: bool,
        ) -> Result<crate::traits::RelayPageResult> {
            Ok(crate::traits::RelayPageResult {
                rows:        self.rows.clone(),
                total_count: None,
            })
        }
    }

    /// A relay read carrying session variables, on an adapter that inherits the default
    /// and cannot apply them, is refused like every sibling `*_with_session` default:
    /// served without them, it would ignore every policy that reads them.
    #[tokio::test]
    async fn the_default_relay_with_session_refuses_session_variables() {
        use crate::traits::RelayDatabaseAdapter as _;

        let adapter = CollectOnlyAdapter {
            applies_session_variables: false,
            ..CollectOnlyAdapter::new(vec![JsonbValue::new(json!({"id": 1}))])
        };

        let result = adapter
            .execute_relay_page_with_session(
                "v_user",
                "id",
                None,
                None,
                10,
                true,
                None,
                None,
                false,
                &[("app.tenant_id", "t1")],
                ReadRouting::Any,
                None,
            )
            .await;

        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Unsupported { .. })),
            "the default must refuse session variables it cannot apply, got {result:?}"
        );
    }
}

/// The SQL both row-query paths share (#958).
#[cfg(test)]
mod row_query_sql {
    use crate::traits::build_row_query_sql;

    #[test]
    fn renders_every_clause_in_sql_order() {
        assert_eq!(
            build_row_query_sql(
                "v_user",
                Some("data->>'x' = '1'"),
                Some("id ASC"),
                Some(10),
                Some(5)
            ),
            "SELECT * FROM \"v_user\" WHERE data->>'x' = '1' ORDER BY id ASC LIMIT 10 OFFSET 5"
        );
    }

    #[test]
    fn omits_absent_clauses() {
        assert_eq!(
            build_row_query_sql("v_user", None, None, None, None),
            "SELECT * FROM \"v_user\""
        );
    }
}

/// #1314: a counted page gives back its rows without the count, and the count; the
/// marker an empty page carries is no row.
#[test]
fn take_matched_strips_the_count_and_the_empty_marker() {
    use serde_json::json;

    use super::{EMPTY_PAGE_KEY, JsonbValue, MATCHED_KEY, take_matched};

    let page = [
        JsonbValue::new(json!({ "id": 1, MATCHED_KEY: 7 })),
        JsonbValue::new(json!({ "id": 2, MATCHED_KEY: 7 })),
    ];
    let (rows, matched) = take_matched(&page);
    assert_eq!(matched, Some(7));
    let rows: Vec<_> = rows.iter().map(|r| r.data.clone()).collect();
    assert_eq!(rows, [json!({ "id": 1 }), json!({ "id": 2 })]);

    let empty = [JsonbValue::new(
        json!({ EMPTY_PAGE_KEY: true, MATCHED_KEY: 4 }),
    )];
    let (rows, matched) = take_matched(&empty);
    assert!(rows.is_empty());
    assert_eq!(matched, Some(4));

    let plain = [JsonbValue::new(json!({ "id": 1 }))];
    assert_eq!(take_matched(&plain).1, None);
}

/// `pg_tviews`' report read as the rows a write changed (#1576): updated and deleted alike,
/// ids as text, a truncated type wholly, an unreadable id as its whole type.
#[test]
fn a_report_reads_as_the_rows_it_names_and_the_types_it_truncated() {
    let report = serde_json::json!({
        "updated": [{"__typename": "User", "id": "a"}, {"__typename": "Tag", "id": 7}],
        "deleted": [{"__typename": "User", "id": "b"}, {"__typename": "Post", "id": null}],
        "invalidated_types": ["Comment"],
        "truncated": true,
    });
    let changed = super::ChangedRows::from_report(&report).unwrap();
    let ids = |t: &str| changed.rows.get(t).map(|ids| ids.iter().cloned().collect::<Vec<_>>());
    assert_eq!(ids("User"), Some(vec!["a".to_string(), "b".to_string()]));
    assert_eq!(ids("Tag"), Some(vec!["7".to_string()]));
    assert_eq!(ids("Post"), None);
    assert_eq!(
        changed.types.iter().cloned().collect::<Vec<_>>(),
        ["Comment".to_string(), "Post".to_string()]
    );
}

/// What cannot be read as a report says nothing about what changed: `None`, so the caller
/// keeps the sweep.
#[test]
fn what_is_not_a_report_reads_as_unknown() {
    for report in [
        serde_json::json!(null),
        serde_json::json!([]),
        serde_json::json!({"updated": "User"}),
        serde_json::json!({"updated": [{"id": "a"}]}),
        serde_json::json!({"invalidated_types": [1]}),
    ] {
        assert_eq!(super::ChangedRows::from_report(&report), None, "{report}");
    }
    assert_eq!(
        super::ChangedRows::from_report(&serde_json::json!({})),
        Some(super::ChangedRows {
            collections: None,
            ..super::ChangedRows::default()
        }),
        "a report of nothing changed, which captured no collections"
    );
}

/// The parents of each collection the report names, as text by `<child type>.<column>`
/// (#1603).
#[test]
fn a_report_reads_as_the_collections_it_names() {
    let report = serde_json::json!({
        "updated": [{"__typename": "Comment", "id": "c1"}],
        "collections": {"Comment.fk_post": ["p2", "p1"], "Tag.fk_post": [7]},
    });
    let changed = super::ChangedRows::from_report(&report).unwrap();
    let collections = changed.collections.expect("captured");
    let parents =
        |key: &str| collections.get(key).map(|ids| ids.iter().cloned().collect::<Vec<_>>());
    assert_eq!(parents("Comment.fk_post"), Some(vec!["p1".to_string(), "p2".to_string()]));
    assert_eq!(parents("Tag.fk_post"), Some(vec!["7".to_string()]));
    assert!(changed.types.is_empty());
}

/// No `collections` key: the rows were written without `pg_tviews.publish_changes = 'rows'`
/// and pg_tviews captured none. Which collections changed is unknown, not "none" (#1603).
#[test]
fn a_report_without_collections_captured_none() {
    let report = serde_json::json!({"updated": [{"__typename": "Comment", "id": "c1"}]});
    let changed = super::ChangedRows::from_report(&report).unwrap();
    assert_eq!(changed.collections, None);
    assert!(changed.rows.contains_key("Comment"), "the rows are read all the same");
}

/// A parent id that is neither text nor a number names its child type wholly, which stands
/// for every collection of it; a key that is not `<type>.<column>`, or collections that are
/// not lists, make what changed unknown (#1603).
#[test]
fn an_unreadable_collection_evicts_more_never_less() {
    let report = serde_json::json!({"collections": {"Comment.fk_post": ["p1", {"x": 1}]}});
    let changed = super::ChangedRows::from_report(&report).unwrap();
    assert!(changed.types.contains("Comment"), "{changed:?}");
    for report in [
        serde_json::json!({"collections": {"Comment": ["p1"]}}),
        serde_json::json!({"collections": {".fk_post": ["p1"]}}),
        serde_json::json!({"collections": {"Comment.": ["p1"]}}),
    ] {
        let changed = super::ChangedRows::from_report(&report).unwrap();
        assert_eq!(changed.collections, None, "{report}");
    }
    for report in [
        serde_json::json!({"collections": ["Comment.fk_post"]}),
        serde_json::json!({"collections": {"Comment.fk_post": "p1"}}),
    ] {
        assert_eq!(super::ChangedRows::from_report(&report), None, "{report}");
    }
}

/// The calls of one transaction: their collections together, unless one call's report
/// captured none, which makes the whole transaction's unknown (#1603).
#[test]
fn absorbed_reports_name_their_collections_together_or_none() {
    let read = |report: serde_json::Value| super::ChangedRows::from_report(&report).unwrap();
    let mut all = super::ChangedRows::default();
    all.absorb(read(serde_json::json!({"collections": {"Comment.fk_post": ["p1"]}})));
    all.absorb(read(serde_json::json!({"collections": {"Comment.fk_post": ["p2"]}})));
    let parents = all.collections.clone().unwrap()["Comment.fk_post"].len();
    assert_eq!(parents, 2, "{all:?}");

    all.absorb(read(serde_json::json!({"updated": []})));
    assert_eq!(all.collections, None, "a call that captured none");
    all.absorb(read(serde_json::json!({"collections": {"Comment.fk_post": ["p3"]}})));
    assert_eq!(all.collections, None, "stays unknown");
}

/// What a write that changed nothing reports: no rows, no types, and no collection.
#[test]
fn the_default_is_a_write_that_changed_nothing() {
    let nothing = super::ChangedRows::default();
    assert!(nothing.rows.is_empty() && nothing.types.is_empty());
    assert_eq!(nothing.collections, Some(std::collections::BTreeMap::new()));
}
