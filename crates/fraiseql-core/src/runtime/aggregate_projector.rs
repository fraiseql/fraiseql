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

#[allow(unused_imports)] // Reason: used only in doc links for `# Errors` sections
use crate::error::FraiseQLError;
use crate::{compiler::aggregation::AggregationPlan, error::Result};

/// Aggregation result projector
pub struct AggregationProjector;

impl AggregationProjector {
    /// Project SQL aggregate results to GraphQL JSON.
    ///
    /// # Arguments
    ///
    /// * `rows` - SQL result rows as `HashMaps`
    /// * `plan` - Aggregation execution plan (for metadata)
    ///
    /// # Returns
    ///
    /// GraphQL-compatible JSON response
    ///
    /// # Errors
    ///
    /// Currently infallible; reserved for future extension (e.g., type coercion failures).
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: an AggregationPlan built from compiled schema metadata.
    /// // See: tests/integration/ for runnable examples.
    /// use std::collections::HashMap;
    /// use serde_json::{json, Value};
    /// # use fraiseql_core::runtime::AggregationProjector;
    ///
    /// let mut row = HashMap::new();
    /// row.insert("category".to_string(), json!("Electronics"));
    /// row.insert("count".to_string(), json!(42));
    /// row.insert("revenue_sum".to_string(), json!(5280.50));
    /// let rows = vec![row];
    /// // let result = AggregationProjector::project(rows, &plan)?;
    /// // result: [{"category": "Electronics", "count": 42, "revenue_sum": 5280.50}]
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Internal`] if JSON serialization of the projected
    /// rows fails (should not occur for well-formed input).
    pub fn project(rows: Vec<HashMap<String, Value>>, _plan: &AggregationPlan) -> Result<Value> {
        // For simple projection: just convert rows to JSON array
        // Future improvements could include:
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
    /// * `query_name` - GraphQL query field name (e.g., "`sales_aggregate`")
    ///
    /// # Returns
    ///
    /// Complete GraphQL response with `{"data": {...}}` wrapper
    ///
    /// # Example
    ///
    /// ```rust
    /// # use fraiseql_core::runtime::AggregationProjector;
    /// # use serde_json::json;
    /// let projected = json!([{"count": 42}]);
    /// let response = AggregationProjector::wrap_in_data_envelope(projected, "sales_aggregate");
    /// // response: {"data": {"sales_aggregate": [{"count": 42}]}}
    /// assert!(response.get("data").is_some());
    /// ```
    #[allow(clippy::needless_pass_by_value)] // Reason: projected is moved into serde_json::json! and consumed by value
    #[must_use] 
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
    /// # Errors
    ///
    /// Currently infallible; reserved for future extension (e.g., type coercion failures).
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: an AggregationPlan built from compiled schema metadata.
    /// // See: tests/integration/ for runnable examples.
    /// use std::collections::HashMap;
    /// use serde_json::json;
    /// # use fraiseql_core::runtime::AggregationProjector;
    ///
    /// let mut row = HashMap::new();
    /// row.insert("count".to_string(), json!(100));
    /// row.insert("revenue_sum".to_string(), json!(5000.0));
    /// // let result = AggregationProjector::project_single(row, &plan)?;
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
