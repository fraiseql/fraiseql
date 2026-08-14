#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code

#[test]
fn database_adapter_is_send_sync() {
    // Static assertion: `dyn DatabaseAdapter` must be `Send + Sync`.
    // This test exists to catch accidental removal of `Send + Sync` bounds.
    // It only needs to compile — no runtime assertion required.
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<dyn super::DatabaseAdapter>();
}

/// The streaming defaults (#958).
///
/// An adapter that implements none of the streaming methods must still answer
/// them, with exactly the rows its collecting methods return — that is the whole
/// contract of the default, and it is what lets a stub adapter, a test double or
/// a future backend be dropped into a streaming caller without special-casing.
#[cfg(test)]
mod streaming_defaults {
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
    }

    // Reason: the trait is declared with the async-trait macro; impls must match.
    #[async_trait]
    impl DatabaseAdapter for CollectOnlyAdapter {
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
            Ok(Vec::new())
        }

        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn projection_default_streams_the_collected_rows_in_order() {
        let adapter = CollectOnlyAdapter {
            rows: vec![
                JsonbValue::new(json!({"id": 1})),
                JsonbValue::new(json!({"id": 2})),
                JsonbValue::new(json!({"id": 3})),
            ],
        };

        let stream = adapter
            .stream_with_projection(&ProjectionRequest::new("v_user"), &[], ReadRouting::Any)
            .await
            .expect("default streaming read");
        let got: Vec<_> = stream.map(|r| r.expect("row").into_value()).collect().await;

        assert_eq!(got, vec![json!({"id": 1}), json!({"id": 2}), json!({"id": 3})]);
    }

    #[tokio::test]
    async fn row_query_default_streams_the_collected_rows_in_order() {
        let adapter = CollectOnlyAdapter { rows: Vec::new() };
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
