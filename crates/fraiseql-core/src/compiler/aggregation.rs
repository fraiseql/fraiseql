//! Aggregation Execution Plan Module
//!
//! This module generates execution plans for GROUP BY queries with aggregations.
//!
//! # Execution Plan Flow
//!
//! ```text
//! GraphQL Query
//!      ↓
//! AggregationRequest (parsed)
//!      ↓
//! AggregationPlan (validated, optimized)
//!      ↓
//! SQL Generation (database-specific)
//!      ↓
//! Query Execution
//! ```
//!
//! # Example
//!
//! ```graphql
//! query {
//!   sales_aggregate(
//!     where: { customer_id: { _eq: "uuid-123" } }
//!     groupBy: { category: true, occurred_at_day: true }
//!     having: { revenue_sum_gt: 1000 }
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
//! Generates:
//!
//! ```sql
//! SELECT
//!   data->>'category' AS category,
//!   DATE_TRUNC('day', occurred_at) AS occurred_at_day,
//!   COUNT(*) AS count,
//!   SUM(revenue) AS revenue_sum,
//!   AVG(revenue) AS revenue_avg
//! FROM tf_sales
//! WHERE customer_id = $1
//! GROUP BY data->>'category', DATE_TRUNC('day', occurred_at)
//! HAVING SUM(revenue) > $2
//! ```

use crate::compiler::aggregate_types::{AggregateFunction, TemporalBucket, HavingOperator};
use crate::compiler::fact_table::FactTableMetadata;
use crate::db::where_clause::WhereClause;
use crate::error::{FraiseQLError, Result};
use serde::{Deserialize, Serialize};

/// Aggregation request from GraphQL query
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregationRequest {
    /// Fact table name
    pub table_name: String,
    /// WHERE clause filters (applied before GROUP BY)
    pub where_clause: Option<WhereClause>,
    /// GROUP BY selections
    pub group_by: Vec<GroupBySelection>,
    /// Aggregate selections (what to compute)
    pub aggregates: Vec<AggregateSelection>,
    /// HAVING clause filters (applied after GROUP BY)
    pub having: Vec<HavingCondition>,
    /// ORDER BY clauses
    pub order_by: Vec<OrderByClause>,
    /// LIMIT
    pub limit: Option<u32>,
    /// OFFSET
    pub offset: Option<u32>,
}

/// GROUP BY selection
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GroupBySelection {
    /// Group by JSONB dimension
    Dimension {
        /// JSONB path (e.g., "category")
        path: String,
        /// Alias for result
        alias: String,
    },
    /// Group by temporal bucket
    TemporalBucket {
        /// Column name (e.g., "occurred_at")
        column: String,
        /// Bucket type
        bucket: TemporalBucket,
        /// Alias for result
        alias: String,
    },
}

impl GroupBySelection {
    /// Get the result alias for this selection
    #[must_use]
    pub fn alias(&self) -> &str {
        match self {
            Self::Dimension { alias, .. } | Self::TemporalBucket { alias, .. } => alias,
        }
    }
}

/// Aggregate selection (what to compute)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AggregateSelection {
    /// COUNT(*)
    Count {
        /// Alias for result
        alias: String,
    },
    /// COUNT(DISTINCT field)
    CountDistinct {
        /// Field to count
        field: String,
        /// Alias for result
        alias: String,
    },
    /// Aggregate function on a measure
    MeasureAggregate {
        /// Measure column name
        measure: String,
        /// Aggregate function
        function: AggregateFunction,
        /// Alias for result
        alias: String,
    },
}

impl AggregateSelection {
    /// Get the result alias for this selection
    #[must_use]
    pub fn alias(&self) -> &str {
        match self {
            Self::Count { alias } | Self::CountDistinct { alias, .. } | Self::MeasureAggregate { alias, .. } => alias,
        }
    }
}

/// HAVING condition (post-aggregation filter)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HavingCondition {
    /// Aggregate to filter on
    pub aggregate: AggregateSelection,
    /// Comparison operator
    pub operator: HavingOperator,
    /// Value to compare against
    pub value: serde_json::Value,
}

/// ORDER BY clause
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderByClause {
    /// Field to order by (can be dimension, aggregate, or temporal bucket)
    pub field: String,
    /// Sort direction
    pub direction: OrderDirection,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderDirection {
    /// Ascending (A-Z, 0-9)
    Asc,
    /// Descending (Z-A, 9-0)
    Desc,
}

/// Validated and optimized aggregation execution plan
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregationPlan {
    /// Fact table metadata
    pub metadata: FactTableMetadata,
    /// Original request
    pub request: AggregationRequest,
    /// Validated GROUP BY expressions
    pub group_by_expressions: Vec<GroupByExpression>,
    /// Validated aggregate expressions
    pub aggregate_expressions: Vec<AggregateExpression>,
    /// Validated HAVING conditions
    pub having_conditions: Vec<ValidatedHavingCondition>,
}

/// Validated GROUP BY expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GroupByExpression {
    /// JSONB dimension extraction
    JsonbPath {
        /// JSONB column name (usually "data")
        jsonb_column: String,
        /// Path to extract (e.g., "category")
        path: String,
        /// Result alias
        alias: String,
    },
    /// Temporal bucket with DATE_TRUNC
    TemporalBucket {
        /// Timestamp column name
        column: String,
        /// Bucket type
        bucket: TemporalBucket,
        /// Result alias
        alias: String,
    },
}

/// Validated aggregate expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AggregateExpression {
    /// COUNT(*)
    Count {
        /// Result alias
        alias: String,
    },
    /// COUNT(DISTINCT field)
    CountDistinct {
        /// Column to count
        column: String,
        /// Result alias
        alias: String,
    },
    /// Aggregate function on measure column
    MeasureAggregate {
        /// Measure column name
        column: String,
        /// Aggregate function
        function: AggregateFunction,
        /// Result alias
        alias: String,
    },
}

/// Validated HAVING condition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidatedHavingCondition {
    /// Aggregate expression to filter on
    pub aggregate: AggregateExpression,
    /// Comparison operator
    pub operator: HavingOperator,
    /// Value to compare against
    pub value: serde_json::Value,
}

/// Aggregation plan generator
pub struct AggregationPlanner;

impl AggregationPlanner {
    /// Generate execution plan from request
    ///
    /// # Arguments
    ///
    /// * `request` - Aggregation request from GraphQL
    /// * `metadata` - Fact table metadata
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Request references non-existent measures or dimensions
    /// - GROUP BY selections are invalid
    /// - HAVING conditions reference non-computed aggregates
    pub fn plan(request: AggregationRequest, metadata: FactTableMetadata) -> Result<AggregationPlan> {
        // Validate and convert GROUP BY selections
        let group_by_expressions = Self::validate_group_by(&request.group_by, &metadata)?;

        // Validate and convert aggregate selections
        let aggregate_expressions = Self::validate_aggregates(&request.aggregates, &metadata)?;

        // Validate HAVING conditions
        let having_conditions = Self::validate_having(&request.having, &aggregate_expressions)?;

        Ok(AggregationPlan {
            metadata,
            request,
            group_by_expressions,
            aggregate_expressions,
            having_conditions,
        })
    }

    /// Validate GROUP BY selections
    fn validate_group_by(
        selections: &[GroupBySelection],
        metadata: &FactTableMetadata,
    ) -> Result<Vec<GroupByExpression>> {
        let mut expressions = Vec::new();

        for selection in selections {
            match selection {
                GroupBySelection::Dimension { path, alias } => {
                    // Validate dimension exists in metadata (for now, just accept any path)
                    // TODO: Check against metadata.dimensions.paths when we add path discovery
                    expressions.push(GroupByExpression::JsonbPath {
                        jsonb_column: metadata.dimensions.name.clone(),
                        path: path.clone(),
                        alias: alias.clone(),
                    });
                }
                GroupBySelection::TemporalBucket { column, bucket, alias } => {
                    // Validate column exists in denormalized filters
                    let filter_exists = metadata
                        .denormalized_filters
                        .iter()
                        .any(|f| f.name == *column);

                    if !filter_exists {
                        return Err(FraiseQLError::Validation {
                            message: format!(
                                "Column '{}' not found in fact table '{}'",
                                column, metadata.table_name
                            ),
                            path: None,
                        });
                    }

                    expressions.push(GroupByExpression::TemporalBucket {
                        column: column.clone(),
                        bucket: *bucket,
                        alias: alias.clone(),
                    });
                }
            }
        }

        Ok(expressions)
    }

    /// Validate aggregate selections
    fn validate_aggregates(
        selections: &[AggregateSelection],
        metadata: &FactTableMetadata,
    ) -> Result<Vec<AggregateExpression>> {
        let mut expressions = Vec::new();

        for selection in selections {
            match selection {
                AggregateSelection::Count { alias } => {
                    expressions.push(AggregateExpression::Count {
                        alias: alias.clone(),
                    });
                }
                AggregateSelection::CountDistinct { field, alias } => {
                    // Validate field is a measure
                    let measure_exists = metadata.measures.iter().any(|m| m.name == *field);

                    if !measure_exists {
                        return Err(FraiseQLError::Validation {
                            message: format!(
                                "Measure '{}' not found in fact table '{}'",
                                field, metadata.table_name
                            ),
                            path: None,
                        });
                    }

                    expressions.push(AggregateExpression::CountDistinct {
                        column: field.clone(),
                        alias: alias.clone(),
                    });
                }
                AggregateSelection::MeasureAggregate { measure, function, alias } => {
                    // Validate measure exists
                    let measure_exists = metadata.measures.iter().any(|m| m.name == *measure);

                    if !measure_exists {
                        return Err(FraiseQLError::Validation {
                            message: format!(
                                "Measure '{}' not found in fact table '{}'",
                                measure, metadata.table_name
                            ),
                            path: None,
                        });
                    }

                    expressions.push(AggregateExpression::MeasureAggregate {
                        column: measure.clone(),
                        function: *function,
                        alias: alias.clone(),
                    });
                }
            }
        }

        Ok(expressions)
    }

    /// Validate HAVING conditions
    fn validate_having(
        conditions: &[HavingCondition],
        aggregate_expressions: &[AggregateExpression],
    ) -> Result<Vec<ValidatedHavingCondition>> {
        let mut validated = Vec::new();

        for condition in conditions {
            // Convert the aggregate selection to an expression
            let aggregate_expr = match &condition.aggregate {
                AggregateSelection::Count { alias } => AggregateExpression::Count {
                    alias: alias.clone(),
                },
                AggregateSelection::CountDistinct { field, alias } => {
                    AggregateExpression::CountDistinct {
                        column: field.clone(),
                        alias: alias.clone(),
                    }
                }
                AggregateSelection::MeasureAggregate { measure, function, alias } => {
                    AggregateExpression::MeasureAggregate {
                        column: measure.clone(),
                        function: *function,
                        alias: alias.clone(),
                    }
                }
            };

            // Note: We don't strictly require the aggregate to be in the SELECT list
            // Some databases allow filtering on aggregates not in SELECT

            validated.push(ValidatedHavingCondition {
                aggregate: aggregate_expr,
                operator: condition.operator,
                value: condition.value.clone(),
            });
        }

        Ok(validated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::fact_table::{DimensionColumn, FilterColumn, MeasureColumn, SqlType};

    fn create_test_metadata() -> FactTableMetadata {
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
                paths: vec![],
            },
            denormalized_filters: vec![
                FilterColumn {
                    name: "customer_id".to_string(),
                    sql_type: SqlType::Uuid,
                    indexed: true,
                },
                FilterColumn {
                    name: "occurred_at".to_string(),
                    sql_type: SqlType::Timestamp,
                    indexed: true,
                },
            ],
        }
    }

    #[test]
    fn test_plan_simple_aggregation() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name: "tf_sales".to_string(),
            where_clause: None,
            group_by: vec![],
            aggregates: vec![
                AggregateSelection::Count {
                    alias: "count".to_string(),
                },
                AggregateSelection::MeasureAggregate {
                    measure: "revenue".to_string(),
                    function: AggregateFunction::Sum,
                    alias: "revenue_sum".to_string(),
                },
            ],
            having: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
        };

        let plan = AggregationPlanner::plan(request, metadata).unwrap();

        assert_eq!(plan.aggregate_expressions.len(), 2);
        assert!(matches!(
            plan.aggregate_expressions[0],
            AggregateExpression::Count { .. }
        ));
        assert!(matches!(
            plan.aggregate_expressions[1],
            AggregateExpression::MeasureAggregate { .. }
        ));
    }

    #[test]
    fn test_plan_with_group_by() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name: "tf_sales".to_string(),
            where_clause: None,
            group_by: vec![
                GroupBySelection::Dimension {
                    path: "category".to_string(),
                    alias: "category".to_string(),
                },
                GroupBySelection::TemporalBucket {
                    column: "occurred_at".to_string(),
                    bucket: TemporalBucket::Day,
                    alias: "occurred_at_day".to_string(),
                },
            ],
            aggregates: vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
        };

        let plan = AggregationPlanner::plan(request, metadata).unwrap();

        assert_eq!(plan.group_by_expressions.len(), 2);
        assert!(matches!(
            plan.group_by_expressions[0],
            GroupByExpression::JsonbPath { .. }
        ));
        assert!(matches!(
            plan.group_by_expressions[1],
            GroupByExpression::TemporalBucket { .. }
        ));
    }

    #[test]
    fn test_plan_with_having() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name: "tf_sales".to_string(),
            where_clause: None,
            group_by: vec![GroupBySelection::Dimension {
                path: "category".to_string(),
                alias: "category".to_string(),
            }],
            aggregates: vec![AggregateSelection::MeasureAggregate {
                measure: "revenue".to_string(),
                function: AggregateFunction::Sum,
                alias: "revenue_sum".to_string(),
            }],
            having: vec![HavingCondition {
                aggregate: AggregateSelection::MeasureAggregate {
                    measure: "revenue".to_string(),
                    function: AggregateFunction::Sum,
                    alias: "revenue_sum".to_string(),
                },
                operator: HavingOperator::Gt,
                value: serde_json::json!(1000),
            }],
            order_by: vec![],
            limit: None,
            offset: None,
        };

        let plan = AggregationPlanner::plan(request, metadata).unwrap();

        assert_eq!(plan.having_conditions.len(), 1);
        assert_eq!(plan.having_conditions[0].operator, HavingOperator::Gt);
    }

    #[test]
    fn test_validate_invalid_measure() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name: "tf_sales".to_string(),
            where_clause: None,
            group_by: vec![],
            aggregates: vec![AggregateSelection::MeasureAggregate {
                measure: "nonexistent".to_string(),
                function: AggregateFunction::Sum,
                alias: "nonexistent_sum".to_string(),
            }],
            having: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
        };

        let result = AggregationPlanner::plan(request, metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_validate_invalid_temporal_column() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name: "tf_sales".to_string(),
            where_clause: None,
            group_by: vec![GroupBySelection::TemporalBucket {
                column: "nonexistent".to_string(),
                bucket: TemporalBucket::Day,
                alias: "day".to_string(),
            }],
            aggregates: vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having: vec![],
            order_by: vec![],
            limit: None,
            offset: None,
        };

        let result = AggregationPlanner::plan(request, metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
