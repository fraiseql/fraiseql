//! Aggregation Result Projector
//!
//! Projects SQL aggregate results to GraphQL JSON responses.
//!
//! # SQL Result Format
//!
//! SQL returns rows as `Vec<HashMap<String, Value>>`:
//! ```json
//! [
//!   {
//!     "category": "Electronics",
//!     "occurred_at_day": "2025-01-01T00:00:00Z",
//!     "count": 42,
//!     "revenue_sum": 5280.50,
//!     "revenue_avg": 125.73
//!   }
//! ]
//! ```
//!
//! # GraphQL Response Format
//!
//! Projected to GraphQL response:
//! ```json
//! {
//!   "data": {
//!     "sales_aggregate": [
//!       {
//!         "category": "Electronics",
//!         "occurred_at_day": "2025-01-01T00:00:00Z",
//!         "count": 42,
//!         "revenue_sum": 5280.50,
//!         "revenue_avg": 125.73
//!       }
//!     ]
//!   }
//! }
//! ```

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::{compiler::aggregation::AggregationPlan, error::Result};

/// Aggregation result projector
pub struct AggregationProjector;

impl AggregationProjector {
    /// Project SQL aggregate results to GraphQL JSON.
    ///
    /// # Arguments
    ///
    /// * `rows` - SQL result rows as HashMaps
    /// * `plan` - Aggregation execution plan (for metadata)
    ///
    /// # Returns
    ///
    /// GraphQL-compatible JSON response
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let rows = vec![
    ///     hashmap!{
    ///         "category" => json!("Electronics"),
    ///         "count" => json!(42),
    ///         "revenue_sum" => json!(5280.50)
    ///     }
    /// ];
    ///
    /// let result = AggregationProjector::project(rows, &plan)?;
    /// // result: [{"category": "Electronics", "count": 42, "revenue_sum": 5280.50}]
    /// ```
    pub fn project(rows: Vec<HashMap<String, Value>>, _plan: &AggregationPlan) -> Result<Value> {
        // For Phase 5, simple projection: just convert rows to JSON array
        // In future phases, this could include:
        // - Type coercion (ensure numbers are numbers, not strings)
        // - Null handling
        // - Nested object construction
        // - Date formatting

        let projected_rows: Vec<Value> = rows
            .into_iter()
            .map(|row| {
                // Convert HashMap to JSON object
                let mut obj = serde_json::Map::new();
                for (key, value) in row {
                    obj.insert(key, value);
                }
                Value::Object(obj)
            })
            .collect();

        Ok(Value::Array(projected_rows))
    }

    /// Wrap projected results in GraphQL data envelope.
    ///
    /// # Arguments
    ///
    /// * `projected` - Projected result array
    /// * `query_name` - GraphQL query field name (e.g., "sales_aggregate")
    ///
    /// # Returns
    ///
    /// Complete GraphQL response with `{"data": {...}}` wrapper
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let projected = json!([{"count": 42}]);
    /// let response = AggregationProjector::wrap_in_data_envelope(projected, "sales_aggregate");
    /// // response: {"data": {"sales_aggregate": [{"count": 42}]}}
    /// ```
    pub fn wrap_in_data_envelope(projected: Value, query_name: &str) -> Value {
        json!({
            "data": {
                query_name: projected
            }
        })
    }

    /// Project a single aggregate result (no GROUP BY).
    ///
    /// When there's no GROUP BY, the result is a single object, not an array.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let row = hashmap!{"count" => json!(100), "revenue_sum" => json!(5000.0)};
    /// let result = AggregationProjector::project_single(row, &plan)?;
    /// // result: {"count": 100, "revenue_sum": 5000.0}
    /// ```
    pub fn project_single(row: HashMap<String, Value>, _plan: &AggregationPlan) -> Result<Value> {
        // Convert HashMap to JSON object
        let mut obj = serde_json::Map::new();
        for (key, value) in row {
            obj.insert(key, value);
        }
        Ok(Value::Object(obj))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{
        aggregate_types::AggregateFunction,
        aggregation::{
            AggregateExpression, AggregateSelection, AggregationRequest, GroupByExpression,
            GroupBySelection,
        },
        fact_table::{DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType},
    };

    fn create_test_plan() -> AggregationPlan {
        use crate::compiler::fact_table::DimensionPath;

        let metadata = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![DimensionPath {
                    name:      "category".to_string(),
                    json_path: "data->>'category'".to_string(),
                    data_type: "text".to_string(),
                }],
            },
            denormalized_filters: vec![FilterColumn {
                name:     "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            }],
            calendar_dimensions:  vec![],
        };

        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::Dimension {
                path:  "category".to_string(),
                alias: "category".to_string(),
            }],
            aggregates:   vec![
                AggregateSelection::Count {
                    alias: "count".to_string(),
                },
                AggregateSelection::MeasureAggregate {
                    measure:  "revenue".to_string(),
                    function: AggregateFunction::Sum,
                    alias:    "revenue_sum".to_string(),
                },
            ],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        AggregationPlan {
            metadata,
            request,
            group_by_expressions: vec![GroupByExpression::JsonbPath {
                jsonb_column: "data".to_string(),
                path:         "category".to_string(),
                alias:        "category".to_string(),
            }],
            aggregate_expressions: vec![
                AggregateExpression::Count {
                    alias: "count".to_string(),
                },
                AggregateExpression::MeasureAggregate {
                    column:   "revenue".to_string(),
                    function: AggregateFunction::Sum,
                    alias:    "revenue_sum".to_string(),
                },
            ],
            having_conditions: vec![],
        }
    }

    #[test]
    fn test_project_simple_result() {
        let plan = create_test_plan();
        let rows = vec![
            {
                let mut row = HashMap::new();
                row.insert("category".to_string(), json!("Electronics"));
                row.insert("count".to_string(), json!(42));
                row.insert("revenue_sum".to_string(), json!(5280.50));
                row
            },
            {
                let mut row = HashMap::new();
                row.insert("category".to_string(), json!("Books"));
                row.insert("count".to_string(), json!(15));
                row.insert("revenue_sum".to_string(), json!(450.25));
                row
            },
        ];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);

        assert_eq!(arr[0]["category"], "Electronics");
        assert_eq!(arr[0]["count"], 42);
        assert_eq!(arr[0]["revenue_sum"], 5280.50);

        assert_eq!(arr[1]["category"], "Books");
        assert_eq!(arr[1]["count"], 15);
        assert_eq!(arr[1]["revenue_sum"], 450.25);
    }

    #[test]
    fn test_project_empty_result() {
        let plan = create_test_plan();
        let rows = vec![];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 0);
    }

    #[test]
    fn test_wrap_in_data_envelope() {
        let projected = json!([
            {"category": "Electronics", "count": 42}
        ]);

        let response = AggregationProjector::wrap_in_data_envelope(projected, "sales_aggregate");

        assert!(response.get("data").is_some());
        assert!(response["data"].get("sales_aggregate").is_some());
        assert!(response["data"]["sales_aggregate"].is_array());
        assert_eq!(response["data"]["sales_aggregate"][0]["category"], "Electronics");
    }

    #[test]
    fn test_project_single() {
        let plan = create_test_plan();
        let mut row = HashMap::new();
        row.insert("count".to_string(), json!(100));
        row.insert("revenue_sum".to_string(), json!(10000.0));

        let result = AggregationProjector::project_single(row, &plan).unwrap();

        assert!(result.is_object());
        assert_eq!(result["count"], 100);
        assert_eq!(result["revenue_sum"], 10000.0);
    }

    #[test]
    fn test_project_with_temporal_bucket() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            row.insert("occurred_at_day".to_string(), json!("2025-01-01"));
            row.insert("count".to_string(), json!(25));
            row.insert("revenue_sum".to_string(), json!(3000.0));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["occurred_at_day"], "2025-01-01");
    }

    #[test]
    fn test_project_with_null_values() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), Value::Null);
            row.insert("count".to_string(), json!(10));
            row.insert("revenue_sum".to_string(), json!(500.0));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], Value::Null);
        assert_eq!(arr[0]["count"], 10);
    }

    // ========================================
    // Phase 6: Advanced Aggregates Projection Tests
    // ========================================

    #[test]
    fn test_project_array_agg_result() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            row.insert("count".to_string(), json!(10));
            // PostgreSQL ARRAY_AGG result
            row.insert("products".to_string(), json!(["prod_1", "prod_2", "prod_3"]));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], "Electronics");
        assert_eq!(arr[0]["products"], json!(["prod_1", "prod_2", "prod_3"]));
    }

    #[test]
    fn test_project_json_agg_result() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            row.insert("count".to_string(), json!(10));
            // PostgreSQL JSON_AGG result
            row.insert(
                "items".to_string(),
                json!([
                    {"product": "prod_1", "revenue": 1500},
                    {"product": "prod_2", "revenue": 1200}
                ]),
            );
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], "Electronics");
        assert!(arr[0]["items"].is_array());
        let items = arr[0]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["product"], "prod_1");
        assert_eq!(items[0]["revenue"], 1500);
    }

    #[test]
    fn test_project_string_agg_result() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            row.insert("count".to_string(), json!(10));
            // PostgreSQL STRING_AGG result
            row.insert("product_names".to_string(), json!("Laptop, Phone, Tablet"));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], "Electronics");
        assert_eq!(arr[0]["product_names"], "Laptop, Phone, Tablet");
    }

    #[test]
    fn test_project_bool_agg_result() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            row.insert("count".to_string(), json!(10));
            // PostgreSQL BOOL_AND result
            row.insert("all_active".to_string(), json!(true));
            // PostgreSQL BOOL_OR result
            row.insert("any_discounted".to_string(), json!(false));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], "Electronics");
        assert_eq!(arr[0]["all_active"], true);
        assert_eq!(arr[0]["any_discounted"], false);
    }

    #[test]
    fn test_project_mixed_aggregates() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            // Basic aggregates
            row.insert("count".to_string(), json!(42));
            row.insert("revenue_sum".to_string(), json!(5280.50));
            row.insert("revenue_avg".to_string(), json!(125.73));
            // Advanced aggregates
            row.insert("products".to_string(), json!(["prod_1", "prod_2"]));
            row.insert("product_names".to_string(), json!("Laptop, Phone"));
            row.insert("all_active".to_string(), json!(true));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        // Verify basic aggregates
        assert_eq!(arr[0]["count"], 42);
        assert_eq!(arr[0]["revenue_sum"], 5280.50);
        // Verify advanced aggregates
        assert_eq!(arr[0]["products"], json!(["prod_1", "prod_2"]));
        assert_eq!(arr[0]["product_names"], "Laptop, Phone");
        assert_eq!(arr[0]["all_active"], true);
    }

    #[test]
    fn test_project_empty_array_agg() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Empty"));
            row.insert("count".to_string(), json!(0));
            // Empty ARRAY_AGG result (NULL in PostgreSQL, [] in others)
            row.insert("products".to_string(), Value::Null);
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], "Empty");
        assert!(arr[0]["products"].is_null());
    }
}
