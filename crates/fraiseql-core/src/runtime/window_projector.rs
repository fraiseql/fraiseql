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
    /// * `rows` - SQL result rows as `HashMaps` (column name → value)
    /// * `plan` - Window execution plan (for metadata like aliases)
    ///
    /// # Errors
    ///
    /// Currently infallible; reserved for future extension (e.g., type coercion failures).
    ///
    /// # Returns
    ///
    /// GraphQL-compatible JSON array of objects
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a WindowExecutionPlan built from compiled schema metadata.
    /// // See: tests/integration/ for runnable examples.
    /// use std::collections::HashMap;
    /// use serde_json::json;
    /// # use fraiseql_core::runtime::WindowProjector;
    ///
    /// let mut row = HashMap::new();
    /// row.insert("revenue".to_string(), json!(100.00));
    /// row.insert("category".to_string(), json!("Electronics"));
    /// row.insert("rank".to_string(), json!(1));
    /// let rows = vec![row];
    /// // let result = WindowProjector::project(rows, &plan)?;
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
    /// * `query_name` - The GraphQL field name (e.g., "`sales_window`")
    ///
    /// # Returns
    ///
    /// Complete GraphQL response structure
    ///
    /// # Example
    ///
    /// ```rust
    /// # use fraiseql_core::runtime::WindowProjector;
    /// # use serde_json::json;
    /// let projected = json!([{"rank": 1}, {"rank": 2}]);
    /// let response = WindowProjector::wrap_in_data_envelope(projected, "sales_window");
    /// // { "data": { "sales_window": [{"rank": 1}, {"rank": 2}] } }
    /// assert!(response.get("data").is_some());
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
