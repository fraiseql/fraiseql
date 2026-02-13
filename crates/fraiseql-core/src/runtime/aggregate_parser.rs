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

use serde_json::Value;

use crate::{
    compiler::{
        aggregate_types::{AggregateFunction, HavingOperator, TemporalBucket},
        aggregation::{
            AggregateSelection, AggregationRequest, GroupBySelection, HavingCondition,
            OrderByClause, OrderDirection,
        },
        fact_table::FactTableMetadata,
    },
    db::where_clause::{WhereClause, WhereOperator},
    error::{FraiseQLError, Result},
};

/// Aggregate query parser
pub struct AggregateQueryParser;

impl AggregateQueryParser {
    /// Parse a simplified aggregate query into AggregationRequest.
    ///
    /// For we'll accept a JSON structure that represents the query:
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
    pub fn parse(query_json: &Value, metadata: &FactTableMetadata) -> Result<AggregationRequest> {
        // Extract table name
        let table_name = query_json
            .get("table")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Validation {
                message: "Missing 'table' field in aggregate query".to_string(),
                path:    None,
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
        let limit = query_json.get("limit").and_then(|v| v.as_u64()).map(|n| n as u32);

        // Parse OFFSET
        let offset = query_json.get("offset").and_then(|v| v.as_u64()).map(|n| n as u32);

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
    ///
    /// For aggregate queries, WHERE works on denormalized filter columns only.
    /// Expected format: `{ "field_operator": value }`
    /// Example: `{ "customer_id_eq": "123", "occurred_at_gte": "2024-01-01" }`
    fn parse_where_clause(where_obj: &Value) -> Result<WhereClause> {
        let Some(obj) = where_obj.as_object() else {
            return Ok(WhereClause::And(vec![]));
        };

        let mut conditions = Vec::new();

        for (key, value) in obj {
            // Parse field_operator format (e.g., "customer_id_eq" -> field="customer_id",
            // operator="eq")
            if let Some((field, operator_str)) = Self::parse_where_field_and_operator(key)? {
                let operator = WhereOperator::from_str(operator_str)?;

                conditions.push(WhereClause::Field {
                    path: vec![field.to_string()],
                    operator,
                    value: value.clone(),
                });
            }
        }

        Ok(WhereClause::And(conditions))
    }

    /// Parse WHERE field and operator from key (e.g., "customer_id_eq" -> ("customer_id", "eq"))
    fn parse_where_field_and_operator(key: &str) -> Result<Option<(&str, &str)>> {
        // Find last underscore to split field from operator
        if let Some(last_underscore) = key.rfind('_') {
            let field = &key[..last_underscore];
            let operator = &key[last_underscore + 1..];

            // Validate operator is known
            match WhereOperator::from_str(operator) {
                Ok(_) => Ok(Some((field, operator))),
                Err(_) => {
                    // Not a valid operator suffix, treat entire key as field (might be used
                    // elsewhere)
                    Ok(None)
                },
            }
        } else {
            // No underscore, not a WHERE condition
            Ok(None)
        }
    }

    /// Parse GROUP BY selections
    ///
    /// Supports two formats:
    /// 1. Boolean true: {"category": true} -> regular dimension
    /// 2. Boolean true with suffix: {"occurred_at_day": true} -> temporal bucket
    /// 3. String bucket name: {"occurred_at": "day"} -> temporal bucket
    fn parse_group_by(
        group_by_obj: &Value,
        metadata: &FactTableMetadata,
    ) -> Result<Vec<GroupBySelection>> {
        let mut selections = Vec::new();

        if let Some(obj) = group_by_obj.as_object() {
            for (key, value) in obj {
                if value.as_bool() == Some(true) {
                    // Format 1 & 2: Boolean true (with or without suffix)
                    // Priority 1: Try calendar dimension first (highest performance)
                    if let Some(calendar_sel) = Self::try_parse_calendar_bucket(key, metadata)? {
                        selections.push(calendar_sel);
                    } else if let Some(bucket_sel) = Self::parse_temporal_bucket(key, metadata)? {
                        // Priority 2: Fall back to DATE_TRUNC if no calendar dimension
                        selections.push(bucket_sel);
                    } else {
                        // Priority 3: Regular dimension
                        selections.push(GroupBySelection::Dimension {
                            path:  key.clone(),
                            alias: key.clone(),
                        });
                    }
                } else if let Some(bucket_str) = value.as_str() {
                    // Format 3: String bucket name {"occurred_at": "day"}
                    let bucket = TemporalBucket::from_str(bucket_str)?;

                    // Priority 1: Try calendar dimension first
                    if let Some(calendar_sel) =
                        Self::try_find_calendar_bucket(key, bucket, metadata)
                    {
                        selections.push(calendar_sel);
                    } else {
                        // Priority 2: Fall back to DATE_TRUNC
                        // Verify this column exists in denormalized_filters
                        let column_exists =
                            metadata.denormalized_filters.iter().any(|f| f.name == *key);

                        if !column_exists {
                            return Err(FraiseQLError::Validation {
                                message: format!(
                                    "Temporal bucketing column '{}' not found in denormalized filters",
                                    key
                                ),
                                path:    None,
                            });
                        }

                        selections.push(GroupBySelection::TemporalBucket {
                            column: key.clone(),
                            bucket,
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
                        alias:  key.to_string(),
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Try to parse calendar dimension from key pattern (e.g., "occurred_at_day")
    ///
    /// Checks if the key matches a calendar dimension pattern and returns
    /// a CalendarDimension selection if available, otherwise None.
    fn try_parse_calendar_bucket(
        key: &str,
        metadata: &FactTableMetadata,
    ) -> Result<Option<GroupBySelection>> {
        for calendar_dim in &metadata.calendar_dimensions {
            // Check all temporal bucket suffixes
            for (suffix, bucket_type) in &[
                ("_second", TemporalBucket::Second),
                ("_minute", TemporalBucket::Minute),
                ("_hour", TemporalBucket::Hour),
                ("_day", TemporalBucket::Day),
                ("_week", TemporalBucket::Week),
                ("_month", TemporalBucket::Month),
                ("_quarter", TemporalBucket::Quarter),
                ("_year", TemporalBucket::Year),
            ] {
                let expected_key = format!("{}{}", calendar_dim.source_column, suffix);
                if key == expected_key {
                    // Find matching calendar bucket
                    if let Some((gran, bucket)) =
                        Self::find_calendar_bucket(calendar_dim, *bucket_type)
                    {
                        return Ok(Some(GroupBySelection::CalendarDimension {
                            source_column:   calendar_dim.source_column.clone(),
                            calendar_column: gran.column_name.clone(),
                            json_key:        bucket.json_key.clone(),
                            bucket:          bucket.bucket_type,
                            alias:           key.to_string(),
                        }));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Try to find calendar bucket for explicit temporal request
    ///
    /// Used when user provides explicit bucket like {"occurred_at": "day"}
    fn try_find_calendar_bucket(
        column: &str,
        bucket: TemporalBucket,
        metadata: &FactTableMetadata,
    ) -> Option<GroupBySelection> {
        for calendar_dim in &metadata.calendar_dimensions {
            if calendar_dim.source_column == column {
                if let Some((gran, cal_bucket)) = Self::find_calendar_bucket(calendar_dim, bucket) {
                    return Some(GroupBySelection::CalendarDimension {
                        source_column:   calendar_dim.source_column.clone(),
                        calendar_column: gran.column_name.clone(),
                        json_key:        cal_bucket.json_key.clone(),
                        bucket:          cal_bucket.bucket_type,
                        alias:           column.to_string(),
                    });
                }
            }
        }
        None
    }

    /// Find calendar bucket in available granularities
    ///
    /// Searches through calendar dimension granularities to find a matching bucket type.
    /// Returns the granularity and bucket if found.
    fn find_calendar_bucket(
        calendar_dim: &crate::compiler::fact_table::CalendarDimension,
        bucket: TemporalBucket,
    ) -> Option<(
        &crate::compiler::fact_table::CalendarGranularity,
        &crate::compiler::fact_table::CalendarBucket,
    )> {
        for granularity in &calendar_dim.granularities {
            for cal_bucket in &granularity.buckets {
                if cal_bucket.bucket_type == bucket {
                    return Some((granularity, cal_bucket));
                }
            }
        }
        None
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

        // Handle COUNT_DISTINCT: supports both "count_distinct" (defaults to first dimension)
        // and "field_count_distinct" pattern (e.g., "product_id_count_distinct")
        if agg_name == "count_distinct" {
            // Default to first dimension path, or "id" if none available
            let default_field = Self::extract_dimension_paths(metadata)
                .first()
                .cloned()
                .unwrap_or_else(|| "id".to_string());
            return Ok(AggregateSelection::CountDistinct {
                field: default_field,
                alias: "count_distinct".to_string(),
            });
        }

        // Handle field_count_distinct pattern (e.g., "customer_id_count_distinct")
        if let Some(stripped) = agg_name.strip_suffix("_count_distinct") {
            // Check if the stripped part matches a dimension path
            let dimension_paths = Self::extract_dimension_paths(metadata);
            if dimension_paths.iter().any(|p| p == stripped) {
                return Ok(AggregateSelection::CountDistinct {
                    field: stripped.to_string(),
                    alias: agg_name.to_string(),
                });
            }
            // Also allow count distinct on measures
            if metadata.measures.iter().any(|m| m.name == stripped) {
                return Ok(AggregateSelection::CountDistinct {
                    field: stripped.to_string(),
                    alias: agg_name.to_string(),
                });
            }
            // If no match found, return error with helpful message
            return Err(FraiseQLError::Validation {
                message: format!(
                    "COUNT DISTINCT field '{}' not found in dimensions or measures. Available: {:?}",
                    stripped, dimension_paths
                ),
                path:    None,
            });
        }

        // Handle boolean aggregates (BOOL_AND, BOOL_OR)
        // e.g., "is_active_bool_and", "has_discount_bool_or"
        for dimension_path in Self::extract_dimension_paths(metadata) {
            if let Some(stripped) = agg_name.strip_suffix("_bool_and") {
                if stripped == dimension_path {
                    return Ok(AggregateSelection::BoolAggregate {
                        field:    dimension_path.clone(),
                        function: crate::compiler::aggregate_types::BoolAggregateFunction::And,
                        alias:    agg_name.to_string(),
                    });
                }
            }
            if let Some(stripped) = agg_name.strip_suffix("_bool_or") {
                if stripped == dimension_path {
                    return Ok(AggregateSelection::BoolAggregate {
                        field:    dimension_path.clone(),
                        function: crate::compiler::aggregate_types::BoolAggregateFunction::Or,
                        alias:    agg_name.to_string(),
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
                // Advanced aggregates
                ("_array_agg", AggregateFunction::ArrayAgg),
                ("_json_agg", AggregateFunction::JsonAgg),
                ("_jsonb_agg", AggregateFunction::JsonbAgg),
                ("_string_agg", AggregateFunction::StringAgg),
            ] {
                let expected_name = format!("{}{}", measure.name, func.0);
                if agg_name == expected_name {
                    return Ok(AggregateSelection::MeasureAggregate {
                        measure:  measure.name.clone(),
                        function: func.1,
                        alias:    agg_name.to_string(),
                    });
                }
            }
        }

        // Check for dimension-level advanced aggregates
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
                        measure:  dimension_path.clone(),
                        function: func.1,
                        alias:    agg_name.to_string(),
                    });
                }
            }
        }

        Err(FraiseQLError::Validation {
            message: format!("Unknown aggregate selection: {agg_name}"),
            path:    None,
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
                            path:    None,
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
                    Some("ASC" | "asc") => OrderDirection::Asc,
                    Some("DESC" | "desc") => OrderDirection::Desc,
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
    use serde_json::json;

    use super::*;
    use crate::compiler::fact_table::{DimensionColumn, FilterColumn, MeasureColumn, SqlType};

    fn create_test_metadata() -> FactTableMetadata {
        use crate::compiler::fact_table::DimensionPath;

        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![
                MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name:     "quantity".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![
                    DimensionPath {
                        name:      "category".to_string(),
                        json_path: "data->>'category'".to_string(),
                        data_type: "text".to_string(),
                    },
                    DimensionPath {
                        name:      "product".to_string(),
                        json_path: "data->>'product'".to_string(),
                        data_type: "text".to_string(),
                    },
                ],
            },
            denormalized_filters: vec![FilterColumn {
                name:     "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            }],
            calendar_dimensions:  vec![],
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
            },
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
            GroupBySelection::TemporalBucket {
                column,
                bucket,
                alias,
            } => {
                assert_eq!(column, "occurred_at");
                assert_eq!(*bucket, TemporalBucket::Day);
                assert_eq!(alias, "occurred_at_day");
            },
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

    #[test]
    fn test_parse_count_distinct_default() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"count_distinct": {}}
            ]
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.aggregates.len(), 1);
        match &request.aggregates[0] {
            AggregateSelection::CountDistinct { field, alias } => {
                // Defaults to first dimension: "category"
                assert_eq!(field, "category");
                assert_eq!(alias, "count_distinct");
            },
            _ => panic!("Expected CountDistinct selection"),
        }
    }

    #[test]
    fn test_parse_count_distinct_with_field() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"product_count_distinct": {}}
            ]
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.aggregates.len(), 1);
        match &request.aggregates[0] {
            AggregateSelection::CountDistinct { field, alias } => {
                assert_eq!(field, "product");
                assert_eq!(alias, "product_count_distinct");
            },
            _ => panic!("Expected CountDistinct selection"),
        }
    }

    #[test]
    fn test_parse_count_distinct_on_measure() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"revenue_count_distinct": {}}
            ]
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.aggregates.len(), 1);
        match &request.aggregates[0] {
            AggregateSelection::CountDistinct { field, alias } => {
                assert_eq!(field, "revenue");
                assert_eq!(alias, "revenue_count_distinct");
            },
            _ => panic!("Expected CountDistinct selection"),
        }
    }

    #[test]
    fn test_parse_count_distinct_invalid_field() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"nonexistent_count_distinct": {}}
            ]
        });

        let result = AggregateQueryParser::parse(&query, &metadata);

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            FraiseQLError::Validation { message, .. } => {
                assert!(message.contains("COUNT DISTINCT field 'nonexistent' not found"));
            },
            _ => panic!("Expected Validation error"),
        }
    }

    #[test]
    fn test_parse_multiple_count_distinct() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"count": {}},
                {"category_count_distinct": {}},
                {"product_count_distinct": {}},
                {"revenue_sum": {}}
            ]
        });

        let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.aggregates.len(), 4);
        assert_eq!(request.aggregates[0].alias(), "count");
        assert_eq!(request.aggregates[1].alias(), "category_count_distinct");
        assert_eq!(request.aggregates[2].alias(), "product_count_distinct");
        assert_eq!(request.aggregates[3].alias(), "revenue_sum");
    }
}
