//! Aggregate Type Generation Module
//!
//! This module generates GraphQL types for aggregation queries from fact table metadata.
//!
//! # Generated Types
//!
//! For a fact table `tf_sales` with measures `revenue`, `quantity`:
//!
//! ```graphql
//! # Aggregate result type
//! type SalesAggregate {
//!   count: Int!
//!   revenue_sum: Float
//!   revenue_avg: Float
//!   revenue_min: Float
//!   revenue_max: Float
//!   quantity_sum: Int
//!   quantity_avg: Float
//!   # ... grouped dimensions
//!   category: String
//!   region: String
//!   occurred_at_day: String
//! }
//!
//! # GROUP BY input
//! input SalesGroupBy {
//!   category: Boolean
//!   region: Boolean
//!   occurred_at_day: Boolean
//!   occurred_at_week: Boolean
//!   occurred_at_month: Boolean
//! }
//!
//! # HAVING input
//! input SalesHaving {
//!   revenue_sum_gt: Float
//!   revenue_avg_gte: Float
//!   count_eq: Int
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::{
    compiler::fact_table::{FactTableMetadata, SqlType},
    error::{FraiseQLError, Result},
};

/// Aggregate function type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AggregateFunction {
    /// COUNT(*) - count rows
    Count,
    /// COUNT(DISTINCT field) - count unique values
    CountDistinct,
    /// SUM(field) - sum values
    Sum,
    /// AVG(field) - average values
    Avg,
    /// MIN(field) - minimum value
    Min,
    /// MAX(field) - maximum value
    Max,
    /// STDDEV(field) - standard deviation (PostgreSQL, SQL Server)
    Stddev,
    /// VARIANCE(field) - variance (PostgreSQL, SQL Server)
    Variance,

    // Advanced aggregates
    /// `ARRAY_AGG(field)` - collect values into array
    ArrayAgg,
    /// `JSON_AGG(expr)` - aggregate into JSON array (PostgreSQL)
    JsonAgg,
    /// `JSONB_AGG(expr)` - aggregate into JSONB array (PostgreSQL)
    JsonbAgg,
    /// `STRING_AGG(field`, delimiter) - concatenate strings
    StringAgg,
}

impl AggregateFunction {
    /// Get all basic aggregate functions (supported by all databases)
    #[must_use]
    pub const fn basic_functions() -> &'static [Self] {
        &[
            Self::Count,
            Self::CountDistinct,
            Self::Sum,
            Self::Avg,
            Self::Min,
            Self::Max,
        ]
    }

    /// Get statistical functions (PostgreSQL, SQL Server only)
    #[must_use]
    pub const fn statistical_functions() -> &'static [Self] {
        &[Self::Stddev, Self::Variance]
    }

    /// Get advanced aggregate functions
    #[must_use]
    pub const fn advanced_functions() -> &'static [Self] {
        &[
            Self::ArrayAgg,
            Self::JsonAgg,
            Self::JsonbAgg,
            Self::StringAgg,
        ]
    }

    /// Get GraphQL field name for this function
    #[must_use]
    pub const fn field_name(&self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::CountDistinct => "count_distinct",
            Self::Sum => "sum",
            Self::Avg => "avg",
            Self::Min => "min",
            Self::Max => "max",
            Self::Stddev => "stddev",
            Self::Variance => "variance",
            Self::ArrayAgg => "array_agg",
            Self::JsonAgg => "json_agg",
            Self::JsonbAgg => "jsonb_agg",
            Self::StringAgg => "string_agg",
        }
    }

    /// Get SQL function name for this function
    #[must_use]
    pub const fn sql_name(&self) -> &'static str {
        match self {
            Self::Count | Self::CountDistinct => "COUNT",
            Self::Sum => "SUM",
            Self::Avg => "AVG",
            Self::Min => "MIN",
            Self::Max => "MAX",
            Self::Stddev => "STDDEV",
            Self::Variance => "VARIANCE",
            Self::ArrayAgg => "ARRAY_AGG",
            Self::JsonAgg => "JSON_AGG",
            Self::JsonbAgg => "JSONB_AGG",
            Self::StringAgg => "STRING_AGG",
        }
    }
}

/// Temporal bucket for time-based grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TemporalBucket {
    /// Second-level grouping
    Second,
    /// Minute-level grouping
    Minute,
    /// Hour-level grouping
    Hour,
    /// Day-level grouping
    Day,
    /// Week-level grouping
    Week,
    /// Month-level grouping
    Month,
    /// Quarter-level grouping
    Quarter,
    /// Year-level grouping
    Year,
}

impl TemporalBucket {
    /// Get all temporal buckets
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Second,
            Self::Minute,
            Self::Hour,
            Self::Day,
            Self::Week,
            Self::Month,
            Self::Quarter,
            Self::Year,
        ]
    }

    /// Get field suffix for this bucket
    #[must_use]
    pub const fn field_suffix(&self) -> &'static str {
        match self {
            Self::Second => "second",
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Quarter => "quarter",
            Self::Year => "year",
        }
    }

    /// Get PostgreSQL `DATE_TRUNC` argument
    #[must_use]
    pub const fn postgres_arg(&self) -> &'static str {
        match self {
            Self::Second => "second",
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Quarter => "quarter",
            Self::Year => "year",
        }
    }

    /// Parse temporal bucket from string
    ///
    /// # Errors
    ///
    /// Returns error if bucket name is unknown
    #[allow(clippy::should_implement_trait)] // Reason: returns FraiseQLError (schema-specific), not a FromStr-compatible error type
    pub fn from_str(s: &str) -> crate::error::Result<Self> {
        match s.to_lowercase().as_str() {
            "second" => Ok(Self::Second),
            "minute" => Ok(Self::Minute),
            "hour" => Ok(Self::Hour),
            "day" => Ok(Self::Day),
            "week" => Ok(Self::Week),
            "month" => Ok(Self::Month),
            "quarter" => Ok(Self::Quarter),
            "year" => Ok(Self::Year),
            _ => Err(crate::error::FraiseQLError::parse(format!("Invalid temporal bucket: {}", s))),
        }
    }
}

/// Boolean aggregate function (AND/OR)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BoolAggregateFunction {
    /// `BOOL_AND` - all values must be true
    And,
    /// `BOOL_OR` - at least one value must be true
    Or,
}

impl BoolAggregateFunction {
    /// Get SQL function name
    #[must_use]
    pub const fn sql_name(&self) -> &'static str {
        match self {
            Self::And => "BOOL_AND",
            Self::Or => "BOOL_OR",
        }
    }

    /// Get GraphQL field suffix
    #[must_use]
    pub const fn field_suffix(&self) -> &'static str {
        match self {
            Self::And => "all",
            Self::Or => "any",
        }
    }
}

/// GraphQL type for aggregate results
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateType {
    /// Type name (e.g., "`SalesAggregate`")
    pub name: String,
    /// Fields in the aggregate result
    pub fields: Vec<AggregateField>,
}

/// Field in an aggregate result type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateField {
    /// Field name
    pub name: String,
    /// GraphQL type
    pub field_type: String,
    /// Is nullable
    pub nullable: bool,
    /// Field kind
    pub kind: AggregateFieldKind,
}

/// Kind of aggregate field
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AggregateFieldKind {
    /// count field (always Int!)
    Count,
    /// Aggregate function on a measure
    MeasureAggregate {
        /// Measure column name
        measure: String,
        /// Aggregate function
        function: AggregateFunction,
    },
    /// Grouped dimension from JSONB
    Dimension {
        /// JSONB path
        path: String,
    },
    /// Temporal bucket field
    TemporalBucket {
        /// Column name
        column: String,
        /// Bucket type
        bucket: TemporalBucket,
    },
}

/// GraphQL input type for GROUP BY
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupByInput {
    /// Input type name (e.g., "`SalesGroupBy`")
    pub name: String,
    /// Fields in the GROUP BY input
    pub fields: Vec<GroupByField>,
}

/// Field in a GROUP BY input
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupByField {
    /// Field name
    pub name: String,
    /// Field kind
    pub kind: GroupByFieldKind,
}

/// Kind of GROUP BY field
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum GroupByFieldKind {
    /// JSONB dimension
    Dimension {
        /// JSONB path
        path: String,
    },
    /// Temporal bucket
    TemporalBucket {
        /// Column name
        column: String,
        /// Bucket type
        bucket: TemporalBucket,
    },
}

/// GraphQL input type for HAVING
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HavingInput {
    /// Input type name (e.g., "`SalesHaving`")
    pub name: String,
    /// Fields in the HAVING input
    pub fields: Vec<HavingField>,
}

/// Field in a HAVING input
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HavingField {
    /// Field name (e.g., "`revenue_sum_gt`")
    pub name: String,
    /// Measure column
    pub measure: String,
    /// Aggregate function
    pub function: AggregateFunction,
    /// Comparison operator
    pub operator: HavingOperator,
    /// GraphQL type for the comparison value
    pub value_type: String,
}

/// HAVING comparison operator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HavingOperator {
    /// Equal (=)
    Eq,
    /// Not equal (!=)
    Neq,
    /// Greater than (>)
    Gt,
    /// Greater than or equal (>=)
    Gte,
    /// Less than (<)
    Lt,
    /// Less than or equal (<=)
    Lte,
}

impl HavingOperator {
    /// Get all HAVING operators
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Eq,
            Self::Neq,
            Self::Gt,
            Self::Gte,
            Self::Lt,
            Self::Lte,
        ]
    }

    /// Get field suffix for this operator
    #[must_use]
    pub const fn field_suffix(&self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Neq => "neq",
            Self::Gt => "gt",
            Self::Gte => "gte",
            Self::Lt => "lt",
            Self::Lte => "lte",
        }
    }

    /// Get SQL operator
    #[must_use]
    pub const fn sql_operator(&self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Neq => "!=",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
        }
    }
}

/// Generator for aggregate GraphQL types
pub struct AggregateTypeGenerator;

impl AggregateTypeGenerator {
    /// Generate aggregate types from fact table metadata
    ///
    /// # Arguments
    ///
    /// * `metadata` - Fact table metadata
    /// * `include_statistical` - Include statistical functions (PostgreSQL, SQL Server)
    ///
    /// # Returns
    ///
    /// Tuple of (`AggregateType`, `GroupByInput`, `HavingInput`)
    ///
    /// # Errors
    ///
    /// Returns error if metadata is invalid or type name generation fails.
    pub fn generate(
        metadata: &FactTableMetadata,
        include_statistical: bool,
    ) -> Result<(AggregateType, GroupByInput, HavingInput)> {
        let type_name = Self::extract_type_name(&metadata.table_name)?;

        let aggregate_type =
            Self::generate_aggregate_type(metadata, &type_name, include_statistical)?;
        let group_by_input = Self::generate_group_by_input(metadata, &type_name)?;
        let having_input = Self::generate_having_input(metadata, &type_name, include_statistical)?;

        Ok((aggregate_type, group_by_input, having_input))
    }

    /// Extract type name from table name (`tf_sales` -> Sales)
    pub(crate) fn extract_type_name(table_name: &str) -> Result<String> {
        if !table_name.starts_with("tf_") {
            return Err(FraiseQLError::Validation {
                message: format!("Table '{}' is not a fact table", table_name),
                path: None,
            });
        }

        let name = &table_name[3..]; // Remove "tf_" prefix
        let pascal_case = Self::to_pascal_case(name);
        Ok(pascal_case)
    }

    /// Convert `snake_case` to `PascalCase`
    pub(crate) fn to_pascal_case(s: &str) -> String {
        s.split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect()
    }

    /// Generate `AggregateType`
    fn generate_aggregate_type(
        metadata: &FactTableMetadata,
        type_name: &str,
        include_statistical: bool,
    ) -> Result<AggregateType> {
        let mut fields = Vec::new();

        // Add count field (always present)
        fields.push(AggregateField {
            name: "count".to_string(),
            field_type: "Int".to_string(),
            nullable: false,
            kind: AggregateFieldKind::Count,
        });

        // Add aggregate fields for each measure
        for measure in &metadata.measures {
            let graphql_type = Self::sql_type_to_graphql(&measure.sql_type);

            // Add basic aggregates
            for function in AggregateFunction::basic_functions() {
                if *function == AggregateFunction::Count
                    || *function == AggregateFunction::CountDistinct
                {
                    continue; // Skip count variants for measures
                }

                fields.push(AggregateField {
                    name: format!("{}_{}", measure.name, function.field_name()),
                    field_type: if *function == AggregateFunction::Avg {
                        "Float".to_string()
                    } else {
                        graphql_type.clone()
                    },
                    nullable: true,
                    kind: AggregateFieldKind::MeasureAggregate {
                        measure: measure.name.clone(),
                        function: *function,
                    },
                });
            }

            // Add statistical aggregates if requested
            if include_statistical {
                for function in AggregateFunction::statistical_functions() {
                    fields.push(AggregateField {
                        name: format!("{}_{}", measure.name, function.field_name()),
                        field_type: "Float".to_string(),
                        nullable: true,
                        kind: AggregateFieldKind::MeasureAggregate {
                            measure: measure.name.clone(),
                            function: *function,
                        },
                    });
                }
            }
        }

        // Add dimension fields (from JSONB paths)
        for dim_path in &metadata.dimensions.paths {
            fields.push(AggregateField {
                name: dim_path.name.clone(),
                field_type: Self::dimension_type_to_graphql(&dim_path.data_type),
                nullable: true, // Dimension fields are nullable in aggregates
                kind: AggregateFieldKind::Dimension {
                    path: dim_path.json_path.clone(),
                },
            });
        }

        // Add temporal bucket fields (from calendar dimensions)
        for calendar_dim in &metadata.calendar_dimensions {
            for granularity in &calendar_dim.granularities {
                for bucket in &granularity.buckets {
                    // Create field name like "occurred_at_day", "occurred_at_month"
                    let field_name = format!(
                        "{}_{}",
                        calendar_dim.source_column,
                        bucket.bucket_type.field_suffix()
                    );

                    // Skip duplicates (multiple calendar columns may have overlapping buckets)
                    if fields.iter().any(|f| f.name == field_name) {
                        continue;
                    }

                    fields.push(AggregateField {
                        name: field_name,
                        field_type: Self::calendar_bucket_to_graphql(&bucket.data_type),
                        nullable: true,
                        kind: AggregateFieldKind::TemporalBucket {
                            column: granularity.column_name.clone(),
                            bucket: bucket.bucket_type,
                        },
                    });
                }
            }
        }

        // If no calendar dimensions but we have timestamp filter columns, add DATE_TRUNC-based
        // buckets
        if metadata.calendar_dimensions.is_empty() {
            for filter in &metadata.denormalized_filters {
                if matches!(filter.sql_type, SqlType::Timestamp | SqlType::Date) {
                    // Add common temporal buckets for timestamp columns
                    for bucket in &[
                        TemporalBucket::Day,
                        TemporalBucket::Week,
                        TemporalBucket::Month,
                        TemporalBucket::Year,
                    ] {
                        let field_name = format!("{}_{}", filter.name, bucket.field_suffix());
                        fields.push(AggregateField {
                            name: field_name,
                            field_type: "String".to_string(), /* DATE_TRUNC returns timestamp as
                                                               * string */
                            nullable: true,
                            kind: AggregateFieldKind::TemporalBucket {
                                column: filter.name.clone(),
                                bucket: *bucket,
                            },
                        });
                    }
                }
            }
        }

        Ok(AggregateType {
            name: format!("{}Aggregate", type_name),
            fields,
        })
    }

    /// Generate `GroupByInput`
    fn generate_group_by_input(
        metadata: &FactTableMetadata,
        type_name: &str,
    ) -> Result<GroupByInput> {
        let mut fields = Vec::new();

        // Add dimension fields (from JSONB paths)
        for dim_path in &metadata.dimensions.paths {
            fields.push(GroupByField {
                name: dim_path.name.clone(),
                kind: GroupByFieldKind::Dimension {
                    path: dim_path.json_path.clone(),
                },
            });
        }

        // Add temporal bucket fields (from calendar dimensions)
        for calendar_dim in &metadata.calendar_dimensions {
            for granularity in &calendar_dim.granularities {
                for bucket in &granularity.buckets {
                    // Create field name like "occurred_at_day", "occurred_at_month"
                    let field_name = format!(
                        "{}_{}",
                        calendar_dim.source_column,
                        bucket.bucket_type.field_suffix()
                    );

                    // Skip duplicates
                    if fields.iter().any(|f| f.name == field_name) {
                        continue;
                    }

                    fields.push(GroupByField {
                        name: field_name,
                        kind: GroupByFieldKind::TemporalBucket {
                            column: granularity.column_name.clone(),
                            bucket: bucket.bucket_type,
                        },
                    });
                }
            }
        }

        // If no calendar dimensions but we have timestamp filter columns, add DATE_TRUNC-based
        // buckets
        if metadata.calendar_dimensions.is_empty() {
            for filter in &metadata.denormalized_filters {
                if matches!(filter.sql_type, SqlType::Timestamp | SqlType::Date) {
                    for bucket in &[
                        TemporalBucket::Day,
                        TemporalBucket::Week,
                        TemporalBucket::Month,
                        TemporalBucket::Year,
                    ] {
                        let field_name = format!("{}_{}", filter.name, bucket.field_suffix());
                        fields.push(GroupByField {
                            name: field_name,
                            kind: GroupByFieldKind::TemporalBucket {
                                column: filter.name.clone(),
                                bucket: *bucket,
                            },
                        });
                    }
                }
            }
        }

        Ok(GroupByInput {
            name: format!("{}GroupBy", type_name),
            fields,
        })
    }

    /// Generate `HavingInput`
    fn generate_having_input(
        metadata: &FactTableMetadata,
        type_name: &str,
        include_statistical: bool,
    ) -> Result<HavingInput> {
        let mut fields = Vec::new();

        // Add HAVING fields for count
        for operator in HavingOperator::all() {
            fields.push(HavingField {
                name: format!("count_{}", operator.field_suffix()),
                measure: String::new(),
                function: AggregateFunction::Count,
                operator: *operator,
                value_type: "Int".to_string(),
            });
        }

        // Add HAVING fields for each measure
        for measure in &metadata.measures {
            let graphql_type = Self::sql_type_to_graphql(&measure.sql_type);

            // Add basic aggregate HAVING fields
            for function in AggregateFunction::basic_functions() {
                if *function == AggregateFunction::Count
                    || *function == AggregateFunction::CountDistinct
                {
                    continue;
                }

                for operator in HavingOperator::all() {
                    fields.push(HavingField {
                        name: format!(
                            "{}_{}_{}",
                            measure.name,
                            function.field_name(),
                            operator.field_suffix()
                        ),
                        measure: measure.name.clone(),
                        function: *function,
                        operator: *operator,
                        value_type: if *function == AggregateFunction::Avg {
                            "Float".to_string()
                        } else {
                            graphql_type.clone()
                        },
                    });
                }
            }

            // Add statistical HAVING fields if requested
            if include_statistical {
                for function in AggregateFunction::statistical_functions() {
                    for operator in HavingOperator::all() {
                        fields.push(HavingField {
                            name: format!(
                                "{}_{}_{}",
                                measure.name,
                                function.field_name(),
                                operator.field_suffix()
                            ),
                            measure: measure.name.clone(),
                            function: *function,
                            operator: *operator,
                            value_type: "Float".to_string(),
                        });
                    }
                }
            }
        }

        Ok(HavingInput {
            name: format!("{}Having", type_name),
            fields,
        })
    }

    /// Convert SQL type to GraphQL type
    pub(crate) fn sql_type_to_graphql(sql_type: &SqlType) -> String {
        match sql_type {
            SqlType::Int | SqlType::BigInt => "Int".to_string(),
            SqlType::Decimal | SqlType::Float => "Float".to_string(),
            SqlType::Boolean => "Boolean".to_string(),
            SqlType::Jsonb | SqlType::Json => "JSON".to_string(),
            SqlType::Uuid => "ID".to_string(),
            SqlType::Text | SqlType::Timestamp | SqlType::Date | SqlType::Other(_) => {
                "String".to_string()
            },
        }
    }

    /// Convert dimension data type hint to GraphQL type
    pub(crate) fn dimension_type_to_graphql(data_type: &str) -> String {
        match data_type.to_lowercase().as_str() {
            "integer" | "int" | "number" => "Int".to_string(),
            "float" | "decimal" | "double" => "Float".to_string(),
            "boolean" | "bool" => "Boolean".to_string(),
            // date/timestamp/datetime and all unknown types map to String
            _ => "String".to_string(),
        }
    }

    /// Convert calendar bucket data type to GraphQL type
    pub(crate) fn calendar_bucket_to_graphql(data_type: &str) -> String {
        match data_type.to_lowercase().as_str() {
            "integer" | "int" => "Int".to_string(),
            // Dates returned as ISO strings; all other types default to String
            _ => "String".to_string(),
        }
    }
}
