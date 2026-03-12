//! Fact table/aggregate structs: `IntermediateFactTable`, `IntermediateMeasure`,
//! `IntermediateDimensions`, `IntermediateDimensionPath`, `IntermediateFilter`,
//! `IntermediateAggregateQuery`.

use serde::{Deserialize, Serialize};

// =============================================================================
// Analytics Definitions
// =============================================================================

/// Fact table definition in intermediate format (Analytics)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateFactTable {
    /// Name of the fact table
    pub table_name:           String,
    /// Measure columns (numeric aggregates)
    pub measures:             Vec<IntermediateMeasure>,
    /// Dimension metadata
    pub dimensions:           IntermediateDimensions,
    /// Denormalized filter columns
    pub denormalized_filters: Vec<IntermediateFilter>,
}

/// Measure column definition
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateMeasure {
    /// Measure column name
    pub name:     String,
    /// SQL data type of the measure
    pub sql_type: String,
    /// Whether the column can be NULL
    pub nullable: bool,
}

/// Dimensions metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateDimensions {
    /// Dimension name
    pub name:  String,
    /// Paths to dimension fields within JSONB
    pub paths: Vec<IntermediateDimensionPath>,
}

/// Dimension path within JSONB
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateDimensionPath {
    /// Path name identifier
    pub name:      String,
    /// JSON path (accepts both "`json_path`" and "path" for cross-language compat)
    #[serde(alias = "path")]
    pub json_path: String,
    /// Data type (accepts both "`data_type`" and "type" for cross-language compat)
    #[serde(alias = "type")]
    pub data_type: String,
}

/// Denormalized filter column
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateFilter {
    /// Filter column name
    pub name:     String,
    /// SQL data type of the filter
    pub sql_type: String,
    /// Whether this column should be indexed
    pub indexed:  bool,
}

/// Aggregate query definition (Analytics)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateAggregateQuery {
    /// Aggregate query name
    pub name:            String,
    /// Fact table to aggregate from
    pub fact_table:      String,
    /// Automatically generate GROUP BY clauses
    pub auto_group_by:   bool,
    /// Automatically generate aggregate functions
    pub auto_aggregates: bool,
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description:     Option<String>,
}
