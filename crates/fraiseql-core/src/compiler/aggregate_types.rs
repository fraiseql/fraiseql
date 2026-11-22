//! Aggregate functions, temporal buckets and HAVING operators.
//!
//! The vocabulary an aggregation query is planned and executed with.

use serde::{Deserialize, Serialize};

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
    /// Half-month grouping: periods start on the 1st and on the 16th
    HalfMonth,
    /// Month-level grouping
    Month,
    /// Quarter-level grouping
    Quarter,
    /// Semester grouping: periods start on 1 January and on 1 July
    Semester,
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
            Self::HalfMonth,
            Self::Month,
            Self::Quarter,
            Self::Semester,
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
            Self::HalfMonth => "half_month",
            Self::Month => "month",
            Self::Quarter => "quarter",
            Self::Semester => "semester",
            Self::Year => "year",
        }
    }

    /// The PostgreSQL expression for the start of the bucket `value` falls in, of `value`'s
    /// own type (`timestamp` or `timestamptz`).
    ///
    /// `DATE_TRUNC` has a unit for every bucket but the half-month and the semester. Those
    /// are the month and the year truncated, moved forward 15 days or 6 months when `value`
    /// lies in the second half. `EXTRACT` reads `value` in the session's time zone, as
    /// `DATE_TRUNC` does, so the two agree on which half a `timestamptz` instant is in.
    #[must_use]
    pub fn postgres_start(&self, value: &str) -> String {
        match self {
            Self::HalfMonth => format!(
                "(DATE_TRUNC('month', {value}) + CASE WHEN EXTRACT(DAY FROM {value}) <= 15 THEN \
                 INTERVAL '0 days' ELSE INTERVAL '15 days' END)"
            ),
            Self::Semester => format!(
                "(DATE_TRUNC('year', {value}) + CASE WHEN EXTRACT(MONTH FROM {value}) <= 6 THEN \
                 INTERVAL '0 months' ELSE INTERVAL '6 months' END)"
            ),
            Self::Second
            | Self::Minute
            | Self::Hour
            | Self::Day
            | Self::Week
            | Self::Month
            | Self::Quarter
            | Self::Year => format!("DATE_TRUNC('{}', {value})", self.field_suffix()),
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
            "half_month" => Ok(Self::HalfMonth),
            "month" => Ok(Self::Month),
            "quarter" => Ok(Self::Quarter),
            "semester" => Ok(Self::Semester),
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
