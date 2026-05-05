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

use serde::{Deserialize, Serialize};

pub use crate::types::{OrderByClause, OrderDirection};
use crate::{
    compiler::{
        aggregate_types::{AggregateFunction, HavingOperator, TemporalBucket},
        fact_table::FactTableMetadata,
    },
    db::where_clause::WhereClause,
    error::{FraiseQLError, Result},
};

/// Aggregation request from GraphQL query
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregationRequest {
    /// Fact table name
    pub table_name:   String,
    /// WHERE clause filters (applied before GROUP BY)
    pub where_clause: Option<WhereClause>,
    /// GROUP BY selections
    pub group_by:     Vec<GroupBySelection>,
    /// Aggregate selections (what to compute)
    pub aggregates:   Vec<AggregateSelection>,
    /// HAVING clause filters (applied after GROUP BY)
    pub having:       Vec<HavingCondition>,
    /// ORDER BY clauses
    pub order_by:     Vec<OrderByClause>,
    /// LIMIT
    pub limit:        Option<u32>,
    /// OFFSET
    pub offset:       Option<u32>,
}

/// GROUP BY selection
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum GroupBySelection {
    /// Group by JSONB dimension
    Dimension {
        /// JSONB path (e.g., "category")
        path:  String,
        /// Alias for result
        alias: String,
    },
    /// Group by temporal bucket
    TemporalBucket {
        /// Column name (e.g., "`occurred_at`")
        column: String,
        /// Bucket type
        bucket: TemporalBucket,
        /// Alias for result
        alias:  String,
    },
    /// Group by pre-computed calendar dimension
    CalendarDimension {
        /// Source timestamp column (e.g., "`occurred_at`")
        source_column:   String,
        /// Calendar JSONB column (e.g., "`date_info`")
        calendar_column: String,
        /// JSON key within calendar column (e.g., "month")
        json_key:        String,
        /// Temporal bucket type
        bucket:          TemporalBucket,
        /// Alias for result
        alias:           String,
    },
    /// Group by a native SQL column (not JSONB-extracted).
    ///
    /// Produced by [`crate::runtime::AggregateQueryParser`] when the GROUP BY field
    /// matches an entry in the query's `native_columns` map.
    NativeDimension {
        /// Column name as it appears in the CREATE VIEW DDL.
        column:  String,
        /// PostgreSQL type for cast expressions (e.g. `"int8"`).
        pg_cast: String,
    },
}

impl GroupBySelection {
    /// Get the result alias for this selection
    #[must_use]
    pub fn alias(&self) -> &str {
        match self {
            Self::Dimension { alias, .. }
            | Self::TemporalBucket { alias, .. }
            | Self::CalendarDimension { alias, .. } => alias,
            // NativeDimension uses the column name as its alias by convention.
            Self::NativeDimension { column, .. } => column,
        }
    }
}

/// Aggregate selection (what to compute)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
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
        measure:  String,
        /// Aggregate function
        function: AggregateFunction,
        /// Alias for result
        alias:    String,
    },
    /// Boolean aggregate
    BoolAggregate {
        /// Field to aggregate
        field:    String,
        /// Boolean aggregate function
        function: crate::compiler::aggregate_types::BoolAggregateFunction,
        /// Alias for result
        alias:    String,
    },
}

impl AggregateSelection {
    /// Get the result alias for this selection
    #[must_use]
    pub fn alias(&self) -> &str {
        match self {
            Self::Count { alias }
            | Self::CountDistinct { alias, .. }
            | Self::MeasureAggregate { alias, .. }
            | Self::BoolAggregate { alias, .. } => alias,
        }
    }
}

/// HAVING condition (post-aggregation filter)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HavingCondition {
    /// Aggregate to filter on
    pub aggregate: AggregateSelection,
    /// Comparison operator
    pub operator:  HavingOperator,
    /// Value to compare against
    pub value:     serde_json::Value,
}

/// Validated and optimized aggregation execution plan
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregationPlan {
    /// Fact table metadata
    pub metadata:              FactTableMetadata,
    /// Original request
    pub request:               AggregationRequest,
    /// Validated GROUP BY expressions
    pub group_by_expressions:  Vec<GroupByExpression>,
    /// Validated aggregate expressions
    pub aggregate_expressions: Vec<AggregateExpression>,
    /// Validated HAVING conditions
    pub having_conditions:     Vec<ValidatedHavingCondition>,
}

/// Validated GROUP BY expression
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum GroupByExpression {
    /// JSONB dimension extraction
    JsonbPath {
        /// JSONB column name (usually "data")
        jsonb_column: String,
        /// Path to extract (e.g., "category")
        path:         String,
        /// Result alias
        alias:        String,
    },
    /// Temporal bucket with `DATE_TRUNC`
    TemporalBucket {
        /// Timestamp column name
        column: String,
        /// Bucket type
        bucket: TemporalBucket,
        /// Result alias
        alias:  String,
    },
    /// Pre-computed calendar dimension extraction
    CalendarPath {
        /// Calendar JSONB column (e.g., "`date_info`")
        calendar_column: String,
        /// JSON key within calendar column (e.g., "month")
        json_key:        String,
        /// Result alias
        alias:           String,
    },
    /// A native SQL column on the view/fact table — referenced directly,
    /// not via JSONB extraction. Generates a dialect-quoted column reference in
    /// GROUP BY / SELECT, enabling btree index usage.
    NativeColumn {
        /// Column name as it appears in the CREATE VIEW DDL.
        column:  String,
        /// PostgreSQL type suffix for casting (e.g. `"uuid"`, `"int8"`, `""`).
        pg_cast: String,
        /// Alias used in SELECT and referenced by ORDER BY.
        alias:   String,
    },
}

/// Validated aggregate expression
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
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
        alias:  String,
    },
    /// Aggregate function on measure column
    MeasureAggregate {
        /// Measure column name
        column:   String,
        /// Aggregate function
        function: AggregateFunction,
        /// Result alias
        alias:    String,
    },
    /// Advanced aggregate with optional parameters
    AdvancedAggregate {
        /// Column to aggregate
        column:    String,
        /// Aggregate function
        function:  AggregateFunction,
        /// Result alias
        alias:     String,
        /// Optional delimiter for `STRING_AGG`
        delimiter: Option<String>,
        /// Optional ORDER BY for `ARRAY_AGG/STRING_AGG`
        order_by:  Option<Vec<OrderByClause>>,
    },
    /// Boolean aggregate (`BOOL_AND/BOOL_OR`)
    BoolAggregate {
        /// Column to aggregate (boolean expression)
        column:   String,
        /// Boolean aggregate function
        function: crate::compiler::aggregate_types::BoolAggregateFunction,
        /// Result alias
        alias:    String,
    },
}

/// Validated HAVING condition
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedHavingCondition {
    /// Aggregate expression to filter on
    pub aggregate: AggregateExpression,
    /// Comparison operator
    pub operator:  HavingOperator,
    /// Value to compare against
    pub value:     serde_json::Value,
}

impl AggregationPlan {
    /// Returns the set of alias strings that correspond to native SQL column
    /// `GROUP BY` expressions (not JSONB-derived aliases).
    ///
    /// Used by the ORDER BY clause builder to document that native columns are
    /// referenced by alias rather than JSONB path, preventing accidental regressions
    /// if the ORDER BY logic is ever refactored.
    #[must_use]
    pub fn native_aliases(&self) -> std::collections::HashSet<&str> {
        self.group_by_expressions
            .iter()
            .filter_map(|e| {
                if let GroupByExpression::NativeColumn { alias, .. } = e {
                    Some(alias.as_str())
                } else {
                    None
                }
            })
            .collect()
    }
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
    pub fn plan(
        request: AggregationRequest,
        metadata: FactTableMetadata,
    ) -> Result<AggregationPlan> {
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
                    // When the schema declares dimension paths, validate against the allowlist.
                    // This prevents unrecognised paths from reaching `jsonb_extract_sql` even
                    // after SQL-level escaping (defence in depth). If no paths are declared,
                    // all paths are accepted — escaping in the runtime layer still applies.
                    let known_paths = &metadata.dimensions.paths;
                    if !known_paths.is_empty() && !known_paths.iter().any(|p| p.name == *path) {
                        return Err(FraiseQLError::Validation {
                            message: format!(
                                "Dimension '{}' not found in fact table '{}'",
                                path, metadata.table_name
                            ),
                            path:    None,
                        });
                    }
                    expressions.push(GroupByExpression::JsonbPath {
                        jsonb_column: metadata.dimensions.name.clone(),
                        path:         path.clone(),
                        alias:        alias.clone(),
                    });
                },
                GroupBySelection::TemporalBucket {
                    column,
                    bucket,
                    alias,
                } => {
                    // Validate column exists in denormalized filters
                    let filter_exists =
                        metadata.denormalized_filters.iter().any(|f| f.name == *column);

                    if !filter_exists {
                        return Err(FraiseQLError::Validation {
                            message: format!(
                                "Column '{}' not found in fact table '{}'",
                                column, metadata.table_name
                            ),
                            path:    None,
                        });
                    }

                    expressions.push(GroupByExpression::TemporalBucket {
                        column: column.clone(),
                        bucket: *bucket,
                        alias:  alias.clone(),
                    });
                },
                GroupBySelection::CalendarDimension {
                    calendar_column,
                    json_key,
                    alias,
                    ..
                } => {
                    // Calendar dimension - use pre-computed JSONB field
                    expressions.push(GroupByExpression::CalendarPath {
                        calendar_column: calendar_column.clone(),
                        json_key:        json_key.clone(),
                        alias:           alias.clone(),
                    });
                },
                GroupBySelection::NativeDimension { column, pg_cast } => {
                    // Native SQL column — alias equals the column name by convention.
                    expressions.push(GroupByExpression::NativeColumn {
                        alias:   column.clone(),
                        column:  column.clone(),
                        pg_cast: pg_cast.clone(),
                    });
                },
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
                },
                AggregateSelection::CountDistinct { field, alias } => {
                    // Validate field is a measure
                    let measure_exists = metadata.measures.iter().any(|m| m.name == *field);

                    if !measure_exists {
                        return Err(FraiseQLError::Validation {
                            message: format!(
                                "Measure '{}' not found in fact table '{}'",
                                field, metadata.table_name
                            ),
                            path:    None,
                        });
                    }

                    expressions.push(AggregateExpression::CountDistinct {
                        column: field.clone(),
                        alias:  alias.clone(),
                    });
                },
                AggregateSelection::MeasureAggregate {
                    measure,
                    function,
                    alias,
                } => {
                    // Validate measure exists (or is a dimension path for advanced aggregates)
                    let measure_exists = metadata.measures.iter().any(|m| m.name == *measure);
                    let is_dimension = metadata.dimensions.paths.iter().any(|p| p.name == *measure);
                    let is_filter =
                        metadata.denormalized_filters.iter().any(|f| f.name == *measure);

                    if !measure_exists && !is_dimension && !is_filter {
                        return Err(FraiseQLError::Validation {
                            message: format!(
                                "Measure or field '{}' not found in fact table '{}'",
                                measure, metadata.table_name
                            ),
                            path:    None,
                        });
                    }

                    // For advanced aggregates, create AdvancedAggregate variant
                    if matches!(
                        function,
                        AggregateFunction::ArrayAgg
                            | AggregateFunction::JsonAgg
                            | AggregateFunction::JsonbAgg
                            | AggregateFunction::StringAgg
                    ) {
                        expressions.push(AggregateExpression::AdvancedAggregate {
                            column:    measure.clone(),
                            function:  *function,
                            alias:     alias.clone(),
                            delimiter: if *function == AggregateFunction::StringAgg {
                                Some(", ".to_string())
                            } else {
                                None
                            },
                            order_by:  None,
                        });
                    } else {
                        expressions.push(AggregateExpression::MeasureAggregate {
                            column:   measure.clone(),
                            function: *function,
                            alias:    alias.clone(),
                        });
                    }
                },
                AggregateSelection::BoolAggregate {
                    field,
                    function,
                    alias,
                } => {
                    // Validate field exists
                    let field_exists = metadata.dimensions.paths.iter().any(|p| p.name == *field)
                        || metadata.denormalized_filters.iter().any(|f| f.name == *field);

                    if !field_exists {
                        return Err(FraiseQLError::Validation {
                            message: format!(
                                "Boolean field '{}' not found in fact table '{}'",
                                field, metadata.table_name
                            ),
                            path:    None,
                        });
                    }

                    expressions.push(AggregateExpression::BoolAggregate {
                        column:   field.clone(),
                        function: *function,
                        alias:    alias.clone(),
                    });
                },
            }
        }

        Ok(expressions)
    }

    /// Validate HAVING conditions
    fn validate_having(
        conditions: &[HavingCondition],
        _aggregate_expressions: &[AggregateExpression],
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
                        alias:  alias.clone(),
                    }
                },
                AggregateSelection::MeasureAggregate {
                    measure,
                    function,
                    alias,
                } => {
                    // For advanced aggregates in HAVING, create AdvancedAggregate variant
                    if matches!(
                        function,
                        AggregateFunction::ArrayAgg
                            | AggregateFunction::JsonAgg
                            | AggregateFunction::JsonbAgg
                            | AggregateFunction::StringAgg
                    ) {
                        AggregateExpression::AdvancedAggregate {
                            column:    measure.clone(),
                            function:  *function,
                            alias:     alias.clone(),
                            delimiter: if *function == AggregateFunction::StringAgg {
                                Some(", ".to_string())
                            } else {
                                None
                            },
                            order_by:  None,
                        }
                    } else {
                        AggregateExpression::MeasureAggregate {
                            column:   measure.clone(),
                            function: *function,
                            alias:    alias.clone(),
                        }
                    }
                },
                AggregateSelection::BoolAggregate {
                    field,
                    function,
                    alias,
                } => AggregateExpression::BoolAggregate {
                    column:   field.clone(),
                    function: *function,
                    alias:    alias.clone(),
                },
            };

            // Note: We don't strictly require the aggregate to be in the SELECT list
            // Some databases allow filtering on aggregates not in SELECT

            validated.push(ValidatedHavingCondition {
                aggregate: aggregate_expr,
                operator:  condition.operator,
                value:     condition.value.clone(),
            });
        }

        Ok(validated)
    }
}
