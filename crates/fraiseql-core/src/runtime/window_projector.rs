//! Window Function Result Projector
//!
//! Projects SQL window function results to GraphQL JSON.
//!
//! # Overview
//!
//! Window functions return rows with computed window values alongside regular columns.
//! This module transforms raw SQL results into GraphQL-compatible JSON format.
//!
//! # Example
//!
//! SQL Result:
//! ```text
//! | revenue | category    | rank | running_total |
//! |---------|-------------|------|---------------|
//! | 100.00  | Electronics | 1    | 100.00        |
//! | 150.00  | Electronics | 2    | 250.00        |
//! | 50.00   | Books       | 1    | 50.00         |
//! ```
//!
//! GraphQL Response:
//! ```json
//! {
//!   "data": {
//!     "sales_window": [
//!       {"revenue": 100.00, "category": "Electronics", "rank": 1, "running_total": 100.00},
//!       {"revenue": 150.00, "category": "Electronics", "rank": 2, "running_total": 250.00},
//!       {"revenue": 50.00, "category": "Books", "rank": 1, "running_total": 50.00}
//!     ]
//!   }
//! }
//! ```

use std::collections::HashMap;

use serde_json::Value;

use crate::{compiler::window_functions::WindowExecutionPlan, error::Result};

/// Window function result projector.
///
/// Transforms SQL query results into GraphQL-compatible JSON format.
pub struct WindowProjector;

impl WindowProjector {
    /// Project SQL window function results to GraphQL JSON.
    ///
    /// # Arguments
    ///
    /// * `rows` - SQL result rows as HashMaps (column name → value)
    /// * `plan` - Window execution plan (for metadata like aliases)
    ///
    /// # Returns
    ///
    /// GraphQL-compatible JSON array of objects
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let rows = vec![
    ///     hashmap!{
    ///         "revenue" => json!(100.00),
    ///         "category" => json!("Electronics"),
    ///         "rank" => json!(1)
    ///     }
    /// ];
    ///
    /// let result = WindowProjector::project(rows, &plan)?;
    /// // result: [{"revenue": 100.00, "category": "Electronics", "rank": 1}]
    /// ```
    pub fn project(
        rows: Vec<HashMap<String, Value>>,
        _plan: &WindowExecutionPlan,
    ) -> Result<Value> {
        // Simple projection: convert each row HashMap to JSON object
        // Future enhancements could include:
        // - Type coercion (ensure numbers are numbers, not strings)
        // - Null handling
        // - Alias mapping (SQL alias → GraphQL field name)
        // - Decimal precision handling

        let projected_rows: Vec<Value> = rows
            .into_iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();
                for (key, value) in row {
                    obj.insert(key, value);
                }
                Value::Object(obj)
            })
            .collect();

        Ok(Value::Array(projected_rows))
    }

    /// Wrap projected results in a GraphQL data envelope.
    ///
    /// # Arguments
    ///
    /// * `projected` - The projected JSON value (array of objects)
    /// * `query_name` - The GraphQL field name (e.g., "sales_window")
    ///
    /// # Returns
    ///
    /// Complete GraphQL response structure
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let projected = json!([{"rank": 1}, {"rank": 2}]);
    /// let response = WindowProjector::wrap_in_data_envelope(projected, "sales_window");
    /// // { "data": { "sales_window": [{"rank": 1}, {"rank": 2}] } }
    /// ```
    #[must_use]
    pub fn wrap_in_data_envelope(projected: Value, query_name: &str) -> Value {
        let mut data = serde_json::Map::new();
        data.insert(query_name.to_string(), projected);

        let mut response = serde_json::Map::new();
        response.insert("data".to_string(), Value::Object(data));

        Value::Object(response)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::compiler::window_functions::{
        SelectColumn, WindowExecutionPlan, WindowFunction, WindowFunctionType,
    };

    fn create_test_plan() -> WindowExecutionPlan {
        WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![
                SelectColumn {
                    expression: "revenue".to_string(),
                    alias:      "revenue".to_string(),
                },
                SelectColumn {
                    expression: "category".to_string(),
                    alias:      "category".to_string(),
                },
            ],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::RowNumber,
                alias:        "rank".to_string(),
                partition_by: vec!["category".to_string()],
                order_by:     vec![],
                frame:        None,
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        }
    }

    #[test]
    fn test_project_empty_results() {
        let plan = create_test_plan();
        let rows: Vec<HashMap<String, Value>> = vec![];

        let result = WindowProjector::project(rows, &plan).unwrap();
        assert_eq!(result, json!([]));
    }

    #[test]
    fn test_project_single_row() {
        let plan = create_test_plan();
        let mut row = HashMap::new();
        row.insert("revenue".to_string(), json!(100.00));
        row.insert("category".to_string(), json!("Electronics"));
        row.insert("rank".to_string(), json!(1));

        let rows = vec![row];
        let result = WindowProjector::project(rows, &plan).unwrap();

        let expected = json!([
            {"revenue": 100.00, "category": "Electronics", "rank": 1}
        ]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_project_multiple_rows() {
        let plan = create_test_plan();

        let mut row1 = HashMap::new();
        row1.insert("revenue".to_string(), json!(100.00));
        row1.insert("category".to_string(), json!("Electronics"));
        row1.insert("rank".to_string(), json!(1));

        let mut row2 = HashMap::new();
        row2.insert("revenue".to_string(), json!(150.00));
        row2.insert("category".to_string(), json!("Electronics"));
        row2.insert("rank".to_string(), json!(2));

        let mut row3 = HashMap::new();
        row3.insert("revenue".to_string(), json!(50.00));
        row3.insert("category".to_string(), json!("Books"));
        row3.insert("rank".to_string(), json!(1));

        let rows = vec![row1, row2, row3];
        let result = WindowProjector::project(rows, &plan).unwrap();

        let expected = json!([
            {"revenue": 100.00, "category": "Electronics", "rank": 1},
            {"revenue": 150.00, "category": "Electronics", "rank": 2},
            {"revenue": 50.00, "category": "Books", "rank": 1}
        ]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_wrap_in_data_envelope() {
        let projected = json!([{"rank": 1}, {"rank": 2}]);
        let response = WindowProjector::wrap_in_data_envelope(projected, "sales_window");

        let expected = json!({
            "data": {
                "sales_window": [{"rank": 1}, {"rank": 2}]
            }
        });
        assert_eq!(response, expected);
    }

    #[test]
    fn test_project_with_null_values() {
        let plan = create_test_plan();

        let mut row = HashMap::new();
        row.insert("revenue".to_string(), json!(null));
        row.insert("category".to_string(), json!("Unknown"));
        row.insert("rank".to_string(), json!(1));

        let rows = vec![row];
        let result = WindowProjector::project(rows, &plan).unwrap();

        let expected = json!([
            {"revenue": null, "category": "Unknown", "rank": 1}
        ]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_project_with_numeric_types() {
        let plan = create_test_plan();

        let mut row = HashMap::new();
        row.insert("revenue".to_string(), json!(1234.56));
        row.insert("category".to_string(), json!("Electronics"));
        row.insert("rank".to_string(), json!(1));
        row.insert("running_total".to_string(), json!(5000.00));
        row.insert("row_count".to_string(), json!(42));

        let rows = vec![row];
        let result = WindowProjector::project(rows, &plan).unwrap();

        // Verify numeric values are preserved
        let arr = result.as_array().unwrap();
        let first_row = &arr[0];
        assert_eq!(first_row["revenue"], json!(1234.56));
        assert_eq!(first_row["rank"], json!(1));
        assert_eq!(first_row["running_total"], json!(5000.00));
        assert_eq!(first_row["row_count"], json!(42));
    }
}
