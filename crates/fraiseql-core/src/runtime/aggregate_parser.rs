//! Aggregate Query Parser
//!
//! Parses GraphQL aggregate queries into `AggregationRequest` for execution.
//!
//! # GraphQL Query Format
//!
//! ```graphql
//! query {
//!   sales_aggregate(
//!     where: { customer_id: { _eq: "uuid-123" } }
//!     groupBy: { category: true, occurred_at_day: true }
//!     having: { revenue_sum_gt: 1000 }
//!     orderBy: { revenue_sum: DESC }
//!     limit: 10
//!   ) {
//!     category
//!     occurred_at_day
//!     count
//!     revenue_sum
//!     revenue_avg
//!   }
//! }
//! ```
//!
//! # Parsed Result
//!
//! ```rust,ignore
//! AggregationRequest {
//!     table_name: "tf_sales",
//!     where_clause: Some(...),
//!     group_by: vec![
//!         GroupBySelection::Dimension { path: "category", alias: "category" },
//!         GroupBySelection::TemporalBucket { column: "occurred_at", bucket: Day, alias: "occurred_at_day" },
//!     ],
//!     aggregates: vec![
//!         AggregateSelection::Count { alias: "count" },
//!         AggregateSelection::MeasureAggregate { measure: "revenue", function: Sum, alias: "revenue_sum" },
//!         AggregateSelection::MeasureAggregate { measure: "revenue", function: Avg, alias: "revenue_avg" },
//!     ],
//!     having: vec![...],
//!     order_by: vec![...],
//!     limit: Some(10),
//!     offset: None,
//! }
//! ```

use crate::compiler::aggregate_types::{AggregateFunction, HavingOperator, TemporalBucket};
use crate::compiler::aggregation::{
    AggregateSelection, AggregationRequest, GroupBySelection, HavingCondition, OrderByClause,
    OrderDirection,
};
use crate::compiler::fact_table::FactTableMetadata;
use crate::db::where_clause::WhereClause;
use crate::error::{FraiseQLError, Result};
use serde_json::Value;

/// Aggregate query parser
pub struct AggregateQueryParser;

impl AggregateQueryParser {
    /// Parse a simplified aggregate query into AggregationRequest.
    ///
    /// For Phase 5, we'll accept a JSON structure that represents the query:
    /// ```json
    /// {
    ///   "table": "tf_sales",
    ///   "groupBy": {
    ///     "category": true,
    ///     "occurred_at_day": true
    ///   },
    ///   "aggregates": [
    ///     {"count": {}},
    ///     {"revenue_sum": {}}
    ///   ],
    ///   "having": {
    ///     "revenue_sum_gt": 1000
    ///   },
    ///   "orderBy": {
    ///     "revenue_sum": "DESC"
    ///   },
    ///   "limit": 10
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns error if the query structure is invalid or references non-existent measures.
    pub fn parse(
        query_json: &Value,
        metadata: &FactTableMetadata,
    ) -> Result<AggregationRequest> {
        // Extract table name
        let table_name = query_json
            .get("table")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Validation {
                message: "Missing 'table' field in aggregate query".to_string(),
                path: None,
            })?
            .to_string();

        // Parse WHERE clause (if present)
        let where_clause = if let Some(where_obj) = query_json.get("where") {
            Some(Self::parse_where_clause(where_obj)?)
        } else {
            None
        };

        // Parse GROUP BY selections
        let group_by = if let Some(group_by_obj) = query_json.get("groupBy") {
            Self::parse_group_by(group_by_obj, metadata)?
        } else {
            vec![]
        };

        // Parse aggregate selections from requested fields
        let aggregates = if let Some(agg_array) = query_json.get("aggregates") {
            Self::parse_aggregates(agg_array, metadata)?
        } else {
            vec![]
        };

        // Parse HAVING conditions
        let having = if let Some(having_obj) = query_json.get("having") {
            Self::parse_having(having_obj, &aggregates, metadata)?
        } else {
            vec![]
        };

        // Parse ORDER BY clauses
        let order_by = if let Some(order_obj) = query_json.get("orderBy") {
            Self::parse_order_by(order_obj)?
        } else {
            vec![]
        };

        // Parse LIMIT
        let limit = query_json
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        // Parse OFFSET
        let offset = query_json
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        Ok(AggregationRequest {
            table_name,
            where_clause,
            group_by,
            aggregates,
            having,
            order_by,
            limit,
            offset,
        })
    }

    /// Parse WHERE clause from JSON
    fn parse_where_clause(_where_obj: &Value) -> Result<WhereClause> {
        // TODO: Implement full WHERE clause parsing
        // For Phase 5, this is a placeholder
        // In a real implementation, this would parse the WHERE object
        // and build a WhereClause AST
        Ok(WhereClause::And(vec![]))
    }

    /// Parse GROUP BY selections
    fn parse_group_by(
        group_by_obj: &Value,
        metadata: &FactTableMetadata,
    ) -> Result<Vec<GroupBySelection>> {
        let mut selections = Vec::new();

        if let Some(obj) = group_by_obj.as_object() {
            for (key, value) in obj {
                if value.as_bool() == Some(true) {
                    // Check if this is a temporal bucket (ends with _day, _week, etc.)
                    if let Some(bucket_selection) = Self::parse_temporal_bucket(key, metadata)? {
                        selections.push(bucket_selection);
                    } else {
                        // Regular dimension
                        selections.push(GroupBySelection::Dimension {
                            path: key.clone(),
                            alias: key.clone(),
                        });
                    }
                }
            }
        }

        Ok(selections)
    }

    /// Parse temporal bucket if the key matches pattern
    fn parse_temporal_bucket(
        key: &str,
        metadata: &FactTableMetadata,
    ) -> Result<Option<GroupBySelection>> {
        // Check for temporal bucket patterns: column_day, column_week, etc.
        for filter_col in &metadata.denormalized_filters {
            for bucket in &[
                ("_second", TemporalBucket::Second),
                ("_minute", TemporalBucket::Minute),
                ("_hour", TemporalBucket::Hour),
                ("_day", TemporalBucket::Day),
                ("_week", TemporalBucket::Week),
                ("_month", TemporalBucket::Month),
                ("_quarter", TemporalBucket::Quarter),
                ("_year", TemporalBucket::Year),
            ] {
                let expected_key = format!("{}{}", filter_col.name, bucket.0);
                if key == expected_key {
                    return Ok(Some(GroupBySelection::TemporalBucket {
                        column: filter_col.name.clone(),
                        bucket: bucket.1,
                        alias: key.to_string(),
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Parse aggregate selections
    fn parse_aggregates(
        agg_array: &Value,
        metadata: &FactTableMetadata,
    ) -> Result<Vec<AggregateSelection>> {
        let mut aggregates = Vec::new();

        if let Some(arr) = agg_array.as_array() {
            for item in arr {
                if let Some(obj) = item.as_object() {
                    // Each object should have one key (the aggregate name)
                    for (agg_name, _value) in obj {
                        aggregates.push(Self::parse_aggregate_selection(agg_name, metadata)?);
                    }
                }
            }
        }

        Ok(aggregates)
    }

    /// Parse a single aggregate selection
    fn parse_aggregate_selection(
        agg_name: &str,
        metadata: &FactTableMetadata,
    ) -> Result<AggregateSelection> {
        // Handle COUNT
        if agg_name == "count" {
            return Ok(AggregateSelection::Count {
                alias: "count".to_string(),
            });
        }

        // Handle COUNT_DISTINCT
        if agg_name == "count_distinct" {
            // TODO: Parse which field to count distinct
            return Ok(AggregateSelection::CountDistinct {
                field: "id".to_string(),
                alias: "count_distinct".to_string(),
            });
        }

        // Phase 6: Handle boolean aggregates (BOOL_AND, BOOL_OR)
        // e.g., "is_active_bool_and", "has_discount_bool_or"
        for dimension_path in Self::extract_dimension_paths(metadata) {
            if let Some(stripped) = agg_name.strip_suffix("_bool_and") {
                if stripped == dimension_path {
                    return Ok(AggregateSelection::BoolAggregate {
                        field: dimension_path.clone(),
                        function: crate::compiler::aggregate_types::BoolAggregateFunction::And,
                        alias: agg_name.to_string(),
                    });
                }
            }
            if let Some(stripped) = agg_name.strip_suffix("_bool_or") {
                if stripped == dimension_path {
                    return Ok(AggregateSelection::BoolAggregate {
                        field: dimension_path.clone(),
                        function: crate::compiler::aggregate_types::BoolAggregateFunction::Or,
                        alias: agg_name.to_string(),
                    });
                }
            }
        }

        // Handle measure aggregates: revenue_sum, revenue_avg, etc.
        for measure in &metadata.measures {
            for func in &[
                ("_sum", AggregateFunction::Sum),
                ("_avg", AggregateFunction::Avg),
                ("_min", AggregateFunction::Min),
                ("_max", AggregateFunction::Max),
                ("_stddev", AggregateFunction::Stddev),
                ("_variance", AggregateFunction::Variance),
                // Phase 6: Advanced aggregates
                ("_array_agg", AggregateFunction::ArrayAgg),
                ("_json_agg", AggregateFunction::JsonAgg),
                ("_jsonb_agg", AggregateFunction::JsonbAgg),
                ("_string_agg", AggregateFunction::StringAgg),
            ] {
                let expected_name = format!("{}{}", measure.name, func.0);
                if agg_name == expected_name {
                    return Ok(AggregateSelection::MeasureAggregate {
                        measure: measure.name.clone(),
                        function: func.1,
                        alias: agg_name.to_string(),
                    });
                }
            }
        }

        // Phase 6: Check for dimension-level advanced aggregates
        // e.g., "product_id_array_agg", "product_name_string_agg"
        for dimension_path in Self::extract_dimension_paths(metadata) {
            for func in &[
                ("_array_agg", AggregateFunction::ArrayAgg),
                ("_json_agg", AggregateFunction::JsonAgg),
                ("_jsonb_agg", AggregateFunction::JsonbAgg),
                ("_string_agg", AggregateFunction::StringAgg),
            ] {
                let expected_name = format!("{}{}", dimension_path, func.0);
                if agg_name == expected_name {
                    // For dimension aggregates, store the path as the "measure"
                    return Ok(AggregateSelection::MeasureAggregate {
                        measure: dimension_path.clone(),
                        function: func.1,
                        alias: agg_name.to_string(),
                    });
                }
            }
        }

        Err(FraiseQLError::Validation {
            message: format!("Unknown aggregate selection: {agg_name}"),
            path: None,
        })
    }

    /// Extract dimension paths from metadata for advanced aggregate parsing
    fn extract_dimension_paths(metadata: &FactTableMetadata) -> Vec<String> {
        let mut paths = Vec::new();

        // Add dimension paths from JSONB column
        for dim_path in &metadata.dimensions.paths {
            paths.push(dim_path.name.clone());
        }

        // Add denormalized filter columns (these can also be aggregated)
        for filter in &metadata.denormalized_filters {
            paths.push(filter.name.clone());
        }

        paths
    }

    /// Parse HAVING conditions
    fn parse_having(
        having_obj: &Value,
        aggregates: &[AggregateSelection],
        _metadata: &FactTableMetadata,
    ) -> Result<Vec<HavingCondition>> {
        let mut conditions = Vec::new();

        if let Some(obj) = having_obj.as_object() {
            for (key, value) in obj {
                // Parse condition: revenue_sum_gt: 1000
                if let Some((agg_name, operator)) = Self::parse_having_key(key) {
                    // Find the aggregate
                    let aggregate = aggregates
                        .iter()
                        .find(|a| a.alias() == agg_name)
                        .ok_or_else(|| FraiseQLError::Validation {
                            message: format!(
                                "HAVING condition references non-selected aggregate: {agg_name}"
                            ),
                            path: None,
                        })?
                        .clone();

                    conditions.push(HavingCondition {
                        aggregate,
                        operator,
                        value: value.clone(),
                    });
                }
            }
        }

        Ok(conditions)
    }

    /// Parse HAVING key to extract aggregate name and operator
    fn parse_having_key(key: &str) -> Option<(&str, HavingOperator)> {
        for (suffix, op) in &[
            ("_gt", HavingOperator::Gt),
            ("_gte", HavingOperator::Gte),
            ("_lt", HavingOperator::Lt),
            ("_lte", HavingOperator::Lte),
            ("_eq", HavingOperator::Eq),
            ("_neq", HavingOperator::Neq),
        ] {
            if let Some(agg_name) = key.strip_suffix(suffix) {
                return Some((agg_name, *op));
            }
        }
        None
    }

    /// Parse ORDER BY clauses
    fn parse_order_by(order_obj: &Value) -> Result<Vec<OrderByClause>> {
        let mut clauses = Vec::new();

        if let Some(obj) = order_obj.as_object() {
            for (field, value) in obj {
                let direction = match value.as_str() {
                    Some("ASC") | Some("asc") => OrderDirection::Asc,
                    Some("DESC") | Some("desc") => OrderDirection::Desc,
                    _ => OrderDirection::Asc, // Default to ASC
                };

                clauses.push(OrderByClause {
                    field: field.clone(),
                    direction,
                });
            }
        }

        Ok(clauses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::fact_table::{DimensionColumn, FilterColumn, MeasureColumn, SqlType};
    use serde_json::json;

    fn create_test_metadata() -> FactTableMetadata {
        use crate::compiler::fact_table::DimensionPath;

        FactTableMetadata {
            table_name: "tf_sales".to_string(),
            measures: vec![
                MeasureColumn {
                    name: "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name: "quantity".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            dimensions: DimensionColumn {
                name: "data".to_string(),
                paths: vec![
                    DimensionPath {
                        name: "category".to_string(),
                        json_path: "data->>'category'".to_string(),
                        data_type: "text".to_string(),
                    },
                    DimensionPath {
                        name: "product".to_string(),
                        json_path: "data->>'product'".to_string(),
                        data_type: "text".to_string(),
                    },
                ],
            },
            denormalized_filters: vec![FilterColumn {
                name: "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed: true,
            }],
        }
    }

    #[test]
    fn test_parse_simple_count() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"count": {}}
            ]
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.table_name, "tf_sales");
        assert_eq!(request.aggregates.len(), 1);
        assert_eq!(request.aggregates[0].alias(), "count");
    }

    #[test]
    fn test_parse_group_by_dimension() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "groupBy": {
                "category": true
            },
            "aggregates": [
                {"count": {}}
            ]
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.group_by.len(), 1);
        match &request.group_by[0] {
            GroupBySelection::Dimension { path, alias } => {
                assert_eq!(path, "category");
                assert_eq!(alias, "category");
            }
            _ => panic!("Expected Dimension selection"),
        }
    }

    #[test]
    fn test_parse_temporal_bucket() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "groupBy": {
                "occurred_at_day": true
            },
            "aggregates": [
                {"count": {}}
            ]
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.group_by.len(), 1);
        match &request.group_by[0] {
            GroupBySelection::TemporalBucket { column, bucket, alias } => {
                assert_eq!(column, "occurred_at");
                assert_eq!(*bucket, TemporalBucket::Day);
                assert_eq!(alias, "occurred_at_day");
            }
            _ => panic!("Expected TemporalBucket selection"),
        }
    }

    #[test]
    fn test_parse_multiple_aggregates() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"count": {}},
                {"revenue_sum": {}},
                {"revenue_avg": {}},
                {"quantity_max": {}}
            ]
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.aggregates.len(), 4);
        assert_eq!(request.aggregates[0].alias(), "count");
        assert_eq!(request.aggregates[1].alias(), "revenue_sum");
        assert_eq!(request.aggregates[2].alias(), "revenue_avg");
        assert_eq!(request.aggregates[3].alias(), "quantity_max");
    }

    #[test]
    fn test_parse_having_condition() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"revenue_sum": {}}
            ],
            "having": {
                "revenue_sum_gt": 1000
            }
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.having.len(), 1);
        assert_eq!(request.having[0].operator, HavingOperator::Gt);
        assert_eq!(request.having[0].value, json!(1000));
    }

    #[test]
    fn test_parse_order_by() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"revenue_sum": {}}
            ],
            "orderBy": {
                "revenue_sum": "DESC"
            }
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.order_by.len(), 1);
        assert_eq!(request.order_by[0].field, "revenue_sum");
        assert_eq!(request.order_by[0].direction, OrderDirection::Desc);
    }

    #[test]
    fn test_parse_limit_offset() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"count": {}}
            ],
            "limit": 10,
            "offset": 5
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.limit, Some(10));
        assert_eq!(request.offset, Some(5));
    }

    #[test]
    fn test_parse_complex_query() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "groupBy": {
                "category": true,
                "occurred_at_month": true
            },
            "aggregates": [
                {"count": {}},
                {"revenue_sum": {}},
                {"revenue_avg": {}},
                {"quantity_sum": {}}
            ],
            "having": {
                "revenue_sum_gt": 1000,
                "count_gte": 5
            },
            "orderBy": {
                "revenue_sum": "DESC",
                "count": "ASC"
            },
            "limit": 20
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.table_name, "tf_sales");
        assert_eq!(request.group_by.len(), 2);
        assert_eq!(request.aggregates.len(), 4);
        assert_eq!(request.having.len(), 2);
        assert_eq!(request.order_by.len(), 2);
        assert_eq!(request.limit, Some(20));
    }
}
