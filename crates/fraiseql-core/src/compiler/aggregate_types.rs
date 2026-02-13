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
    /// ARRAY_AGG(field) - collect values into array
    ArrayAgg,
    /// JSON_AGG(expr) - aggregate into JSON array (PostgreSQL)
    JsonAgg,
    /// JSONB_AGG(expr) - aggregate into JSONB array (PostgreSQL)
    JsonbAgg,
    /// STRING_AGG(field, delimiter) - concatenate strings
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
            Self::Count => "COUNT",
            Self::CountDistinct => "COUNT",
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

    /// Get PostgreSQL DATE_TRUNC argument
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
pub enum BoolAggregateFunction {
    /// BOOL_AND - all values must be true
    And,
    /// BOOL_OR - at least one value must be true
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregateType {
    /// Type name (e.g., "SalesAggregate")
    pub name:   String,
    /// Fields in the aggregate result
    pub fields: Vec<AggregateField>,
}

/// Field in an aggregate result type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregateField {
    /// Field name
    pub name:       String,
    /// GraphQL type
    pub field_type: String,
    /// Is nullable
    pub nullable:   bool,
    /// Field kind
    pub kind:       AggregateFieldKind,
}

/// Kind of aggregate field
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AggregateFieldKind {
    /// count field (always Int!)
    Count,
    /// Aggregate function on a measure
    MeasureAggregate {
        /// Measure column name
        measure:  String,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupByInput {
    /// Input type name (e.g., "SalesGroupBy")
    pub name:   String,
    /// Fields in the GROUP BY input
    pub fields: Vec<GroupByField>,
}

/// Field in a GROUP BY input
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupByField {
    /// Field name
    pub name: String,
    /// Field kind
    pub kind: GroupByFieldKind,
}

/// Kind of GROUP BY field
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HavingInput {
    /// Input type name (e.g., "SalesHaving")
    pub name:   String,
    /// Fields in the HAVING input
    pub fields: Vec<HavingField>,
}

/// Field in a HAVING input
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HavingField {
    /// Field name (e.g., "revenue_sum_gt")
    pub name:       String,
    /// Measure column
    pub measure:    String,
    /// Aggregate function
    pub function:   AggregateFunction,
    /// Comparison operator
    pub operator:   HavingOperator,
    /// GraphQL type for the comparison value
    pub value_type: String,
}

/// HAVING comparison operator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Tuple of (AggregateType, GroupByInput, HavingInput)
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

    /// Extract type name from table name (tf_sales -> Sales)
    fn extract_type_name(table_name: &str) -> Result<String> {
        if !table_name.starts_with("tf_") {
            return Err(FraiseQLError::Validation {
                message: format!("Table '{}' is not a fact table", table_name),
                path:    None,
            });
        }

        let name = &table_name[3..]; // Remove "tf_" prefix
        let pascal_case = Self::to_pascal_case(name);
        Ok(pascal_case)
    }

    /// Convert snake_case to PascalCase
    fn to_pascal_case(s: &str) -> String {
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

    /// Generate AggregateType
    fn generate_aggregate_type(
        metadata: &FactTableMetadata,
        type_name: &str,
        include_statistical: bool,
    ) -> Result<AggregateType> {
        let mut fields = Vec::new();

        // Add count field (always present)
        fields.push(AggregateField {
            name:       "count".to_string(),
            field_type: "Int".to_string(),
            nullable:   false,
            kind:       AggregateFieldKind::Count,
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
                    name:       format!("{}_{}", measure.name, function.field_name()),
                    field_type: if *function == AggregateFunction::Avg {
                        "Float".to_string()
                    } else {
                        graphql_type.clone()
                    },
                    nullable:   true,
                    kind:       AggregateFieldKind::MeasureAggregate {
                        measure:  measure.name.clone(),
                        function: *function,
                    },
                });
            }

            // Add statistical aggregates if requested
            if include_statistical {
                for function in AggregateFunction::statistical_functions() {
                    fields.push(AggregateField {
                        name:       format!("{}_{}", measure.name, function.field_name()),
                        field_type: "Float".to_string(),
                        nullable:   true,
                        kind:       AggregateFieldKind::MeasureAggregate {
                            measure:  measure.name.clone(),
                            function: *function,
                        },
                    });
                }
            }
        }

        // Add dimension fields (from JSONB paths)
        for dim_path in &metadata.dimensions.paths {
            fields.push(AggregateField {
                name:       dim_path.name.clone(),
                field_type: Self::dimension_type_to_graphql(&dim_path.data_type),
                nullable:   true, // Dimension fields are nullable in aggregates
                kind:       AggregateFieldKind::Dimension {
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
                        name:       field_name,
                        field_type: Self::calendar_bucket_to_graphql(&bucket.data_type),
                        nullable:   true,
                        kind:       AggregateFieldKind::TemporalBucket {
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
                            name:       field_name,
                            field_type: "String".to_string(), /* DATE_TRUNC returns timestamp as
                                                               * string */
                            nullable:   true,
                            kind:       AggregateFieldKind::TemporalBucket {
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

    /// Generate GroupByInput
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

    /// Generate HavingInput
    fn generate_having_input(
        metadata: &FactTableMetadata,
        type_name: &str,
        include_statistical: bool,
    ) -> Result<HavingInput> {
        let mut fields = Vec::new();

        // Add HAVING fields for count
        for operator in HavingOperator::all() {
            fields.push(HavingField {
                name:       format!("count_{}", operator.field_suffix()),
                measure:    String::new(),
                function:   AggregateFunction::Count,
                operator:   *operator,
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
                        name:       format!(
                            "{}_{}_{}",
                            measure.name,
                            function.field_name(),
                            operator.field_suffix()
                        ),
                        measure:    measure.name.clone(),
                        function:   *function,
                        operator:   *operator,
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
                            name:       format!(
                                "{}_{}_{}",
                                measure.name,
                                function.field_name(),
                                operator.field_suffix()
                            ),
                            measure:    measure.name.clone(),
                            function:   *function,
                            operator:   *operator,
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
    fn sql_type_to_graphql(sql_type: &SqlType) -> String {
        match sql_type {
            SqlType::Int | SqlType::BigInt => "Int".to_string(),
            SqlType::Decimal | SqlType::Float => "Float".to_string(),
            SqlType::Text => "String".to_string(),
            SqlType::Boolean => "Boolean".to_string(),
            SqlType::Jsonb | SqlType::Json => "JSON".to_string(),
            SqlType::Uuid => "ID".to_string(),
            SqlType::Timestamp | SqlType::Date => "String".to_string(),
            SqlType::Other(_) => "String".to_string(),
        }
    }

    /// Convert dimension data type hint to GraphQL type
    fn dimension_type_to_graphql(data_type: &str) -> String {
        match data_type.to_lowercase().as_str() {
            "integer" | "int" | "number" => "Int".to_string(),
            "float" | "decimal" | "double" => "Float".to_string(),
            "boolean" | "bool" => "Boolean".to_string(),
            "date" | "timestamp" | "datetime" => "String".to_string(),
            _ => "String".to_string(), // Default to String for text and unknown types
        }
    }

    /// Convert calendar bucket data type to GraphQL type
    fn calendar_bucket_to_graphql(data_type: &str) -> String {
        match data_type.to_lowercase().as_str() {
            "integer" | "int" => "Int".to_string(),
            "date" => "String".to_string(), // Dates are returned as ISO strings
            _ => "String".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::fact_table::{DimensionColumn, MeasureColumn};

    fn create_test_metadata() -> FactTableMetadata {
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
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![],
        }
    }

    #[test]
    fn test_extract_type_name() {
        assert_eq!(AggregateTypeGenerator::extract_type_name("tf_sales").unwrap(), "Sales");
        assert_eq!(
            AggregateTypeGenerator::extract_type_name("tf_api_requests").unwrap(),
            "ApiRequests"
        );
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(AggregateTypeGenerator::to_pascal_case("sales"), "Sales");
        assert_eq!(AggregateTypeGenerator::to_pascal_case("api_requests"), "ApiRequests");
        assert_eq!(AggregateTypeGenerator::to_pascal_case("user_sessions"), "UserSessions");
    }

    #[test]
    fn test_generate_aggregate_type() {
        let metadata = create_test_metadata();
        let (aggregate_type, _, _) = AggregateTypeGenerator::generate(&metadata, false).unwrap();

        assert_eq!(aggregate_type.name, "SalesAggregate");

        // Should have: count + (revenue: sum, avg, min, max) + (quantity: sum, avg, min, max) = 9
        // fields
        assert_eq!(aggregate_type.fields.len(), 9);

        // Check count field
        assert_eq!(aggregate_type.fields[0].name, "count");
        assert_eq!(aggregate_type.fields[0].field_type, "Int");
        assert!(!aggregate_type.fields[0].nullable);

        // Check revenue aggregates
        let revenue_sum = aggregate_type.fields.iter().find(|f| f.name == "revenue_sum").unwrap();
        assert_eq!(revenue_sum.field_type, "Float");
        assert!(revenue_sum.nullable);
    }

    #[test]
    fn test_generate_with_statistical() {
        let metadata = create_test_metadata();
        let (aggregate_type, _, _) = AggregateTypeGenerator::generate(&metadata, true).unwrap();

        // Should have additional stddev and variance for each measure
        // count + (revenue: 6) + (quantity: 6) = 13 fields
        assert_eq!(aggregate_type.fields.len(), 13);

        // Check statistical functions
        assert!(aggregate_type.fields.iter().any(|f| f.name == "revenue_stddev"));
        assert!(aggregate_type.fields.iter().any(|f| f.name == "revenue_variance"));
    }

    #[test]
    fn test_generate_having_input() {
        let metadata = create_test_metadata();
        let (_, _, having_input) = AggregateTypeGenerator::generate(&metadata, false).unwrap();

        assert_eq!(having_input.name, "SalesHaving");

        // Should have: count (6 operators) + revenue (4 functions × 6 operators) + quantity (4
        // functions × 6 operators) = 6 + 24 + 24 = 54 fields
        assert_eq!(having_input.fields.len(), 54);

        // Check count HAVING fields
        assert!(having_input.fields.iter().any(|f| f.name == "count_gt"));
        assert!(having_input.fields.iter().any(|f| f.name == "count_eq"));

        // Check measure HAVING fields
        assert!(having_input.fields.iter().any(|f| f.name == "revenue_sum_gt"));
        assert!(having_input.fields.iter().any(|f| f.name == "revenue_avg_gte"));
    }

    #[test]
    fn test_sql_type_to_graphql() {
        assert_eq!(AggregateTypeGenerator::sql_type_to_graphql(&SqlType::Int), "Int");
        assert_eq!(AggregateTypeGenerator::sql_type_to_graphql(&SqlType::Decimal), "Float");
        assert_eq!(AggregateTypeGenerator::sql_type_to_graphql(&SqlType::Text), "String");
        assert_eq!(AggregateTypeGenerator::sql_type_to_graphql(&SqlType::Uuid), "ID");
    }

    // ===========================================================================
    // Dimension Fields Tests
    // ===========================================================================

    fn create_metadata_with_dimensions() -> FactTableMetadata {
        use crate::compiler::fact_table::DimensionPath;

        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![
                    DimensionPath {
                        name:      "category".to_string(),
                        json_path: "dimensions->>'category'".to_string(),
                        data_type: "string".to_string(),
                    },
                    DimensionPath {
                        name:      "region".to_string(),
                        json_path: "dimensions->>'region'".to_string(),
                        data_type: "string".to_string(),
                    },
                    DimensionPath {
                        name:      "priority".to_string(),
                        json_path: "dimensions->>'priority'".to_string(),
                        data_type: "integer".to_string(),
                    },
                ],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![],
        }
    }

    #[test]
    fn test_generate_with_dimension_fields() {
        let metadata = create_metadata_with_dimensions();
        let (aggregate_type, group_by, _) =
            AggregateTypeGenerator::generate(&metadata, false).unwrap();

        // Check aggregate type has dimension fields
        let category_field = aggregate_type.fields.iter().find(|f| f.name == "category");
        assert!(category_field.is_some());
        let category = category_field.unwrap();
        assert_eq!(category.field_type, "String");
        assert!(category.nullable);
        assert!(
            matches!(&category.kind, AggregateFieldKind::Dimension { path } if path == "dimensions->>'category'")
        );

        // Check integer dimension type
        let priority_field = aggregate_type.fields.iter().find(|f| f.name == "priority");
        assert!(priority_field.is_some());
        assert_eq!(priority_field.unwrap().field_type, "Int");

        // Check group_by has dimension fields
        assert!(group_by.fields.iter().any(|f| f.name == "category"));
        assert!(group_by.fields.iter().any(|f| f.name == "region"));
        assert!(group_by.fields.iter().any(|f| f.name == "priority"));
    }

    #[test]
    fn test_group_by_dimension_field_kind() {
        let metadata = create_metadata_with_dimensions();
        let (_, group_by, _) = AggregateTypeGenerator::generate(&metadata, false).unwrap();

        let category = group_by.fields.iter().find(|f| f.name == "category").unwrap();
        assert!(
            matches!(&category.kind, GroupByFieldKind::Dimension { path } if path == "dimensions->>'category'")
        );
    }

    // ===========================================================================
    // Calendar Dimension / Temporal Bucket Tests
    // ===========================================================================

    fn create_metadata_with_calendar_dimensions() -> FactTableMetadata {
        use crate::compiler::fact_table::{CalendarBucket, CalendarDimension, CalendarGranularity};

        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![CalendarDimension {
                source_column: "occurred_at".to_string(),
                granularities: vec![CalendarGranularity {
                    column_name: "date_info".to_string(),
                    buckets:     vec![
                        CalendarBucket {
                            json_key:    "date".to_string(),
                            bucket_type: TemporalBucket::Day,
                            data_type:   "date".to_string(),
                        },
                        CalendarBucket {
                            json_key:    "month".to_string(),
                            bucket_type: TemporalBucket::Month,
                            data_type:   "integer".to_string(),
                        },
                        CalendarBucket {
                            json_key:    "year".to_string(),
                            bucket_type: TemporalBucket::Year,
                            data_type:   "integer".to_string(),
                        },
                    ],
                }],
            }],
        }
    }

    #[test]
    fn test_generate_with_calendar_dimensions() {
        let metadata = create_metadata_with_calendar_dimensions();
        let (aggregate_type, group_by, _) =
            AggregateTypeGenerator::generate(&metadata, false).unwrap();

        // Check aggregate type has temporal bucket fields
        let day_field = aggregate_type.fields.iter().find(|f| f.name == "occurred_at_day");
        assert!(day_field.is_some());
        let day = day_field.unwrap();
        assert_eq!(day.field_type, "String"); // Date type maps to String
        assert!(day.nullable);
        assert!(matches!(&day.kind, AggregateFieldKind::TemporalBucket { column, bucket }
            if column == "date_info" && *bucket == TemporalBucket::Day));

        // Check integer bucket type
        let month_field = aggregate_type.fields.iter().find(|f| f.name == "occurred_at_month");
        assert!(month_field.is_some());
        assert_eq!(month_field.unwrap().field_type, "Int");

        // Check group_by has temporal bucket fields
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_day"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_month"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_year"));
    }

    #[test]
    fn test_group_by_temporal_bucket_field_kind() {
        let metadata = create_metadata_with_calendar_dimensions();
        let (_, group_by, _) = AggregateTypeGenerator::generate(&metadata, false).unwrap();

        let month = group_by.fields.iter().find(|f| f.name == "occurred_at_month").unwrap();
        assert!(matches!(&month.kind, GroupByFieldKind::TemporalBucket { column, bucket }
            if column == "date_info" && *bucket == TemporalBucket::Month));
    }

    // ===========================================================================
    // Fallback Temporal Buckets (from timestamp filter columns)
    // ===========================================================================

    fn create_metadata_with_timestamp_filter() -> FactTableMetadata {
        use crate::compiler::fact_table::FilterColumn;

        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![FilterColumn {
                name:     "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            }],
            calendar_dimensions:  vec![], // No calendar dimensions
        }
    }

    #[test]
    fn test_generate_fallback_temporal_buckets() {
        let metadata = create_metadata_with_timestamp_filter();
        let (aggregate_type, group_by, _) =
            AggregateTypeGenerator::generate(&metadata, false).unwrap();

        // Should have fallback temporal buckets: day, week, month, year
        assert!(aggregate_type.fields.iter().any(|f| f.name == "occurred_at_day"));
        assert!(aggregate_type.fields.iter().any(|f| f.name == "occurred_at_week"));
        assert!(aggregate_type.fields.iter().any(|f| f.name == "occurred_at_month"));
        assert!(aggregate_type.fields.iter().any(|f| f.name == "occurred_at_year"));

        // All fallback buckets should be String type (from DATE_TRUNC)
        let day = aggregate_type.fields.iter().find(|f| f.name == "occurred_at_day").unwrap();
        assert_eq!(day.field_type, "String");

        // Group by should also have temporal buckets
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_day"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_week"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_month"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_year"));
    }

    #[test]
    fn test_no_fallback_when_calendar_dimensions_exist() {
        let metadata = create_metadata_with_calendar_dimensions();
        let (aggregate_type, _, _) = AggregateTypeGenerator::generate(&metadata, false).unwrap();

        // Should only have calendar dimension buckets, not DATE_TRUNC fallbacks
        // Calendar dimensions have: day, month, year
        // Should NOT have: week (not in our calendar dimension)
        assert!(!aggregate_type.fields.iter().any(|f| f.name == "occurred_at_week"));
    }

    // ===========================================================================
    // Helper Function Tests
    // ===========================================================================

    #[test]
    fn test_dimension_type_to_graphql() {
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("string"), "String");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("integer"), "Int");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("int"), "Int");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("number"), "Int");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("float"), "Float");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("decimal"), "Float");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("boolean"), "Boolean");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("date"), "String");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("unknown"), "String");
    }

    #[test]
    fn test_calendar_bucket_to_graphql() {
        assert_eq!(AggregateTypeGenerator::calendar_bucket_to_graphql("integer"), "Int");
        assert_eq!(AggregateTypeGenerator::calendar_bucket_to_graphql("int"), "Int");
        assert_eq!(AggregateTypeGenerator::calendar_bucket_to_graphql("date"), "String");
        assert_eq!(AggregateTypeGenerator::calendar_bucket_to_graphql("unknown"), "String");
    }

    // ===========================================================================
    // Combined Dimensions and Calendar Tests
    // ===========================================================================

    fn create_metadata_with_dimensions_and_calendar() -> FactTableMetadata {
        use crate::compiler::fact_table::{
            CalendarBucket, CalendarDimension, CalendarGranularity, DimensionPath,
        };

        FactTableMetadata {
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
                    json_path: "dimensions->>'category'".to_string(),
                    data_type: "string".to_string(),
                }],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![CalendarDimension {
                source_column: "occurred_at".to_string(),
                granularities: vec![CalendarGranularity {
                    column_name: "date_info".to_string(),
                    buckets:     vec![CalendarBucket {
                        json_key:    "month".to_string(),
                        bucket_type: TemporalBucket::Month,
                        data_type:   "integer".to_string(),
                    }],
                }],
            }],
        }
    }

    #[test]
    fn test_generate_with_both_dimensions_and_calendar() {
        let metadata = create_metadata_with_dimensions_and_calendar();
        let (aggregate_type, group_by, _) =
            AggregateTypeGenerator::generate(&metadata, false).unwrap();

        // Should have both dimension fields and temporal bucket fields
        assert!(aggregate_type.fields.iter().any(|f| f.name == "category"));
        assert!(aggregate_type.fields.iter().any(|f| f.name == "occurred_at_month"));

        assert!(group_by.fields.iter().any(|f| f.name == "category"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_month"));
    }
}
