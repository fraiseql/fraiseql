//! Fact table/aggregate structs: `IntermediateFactTable`, `IntermediateMeasure`,
//! `IntermediateDimensions`, `IntermediateDimensionPath`, `IntermediateFilter`,
//! `IntermediateAggregateQuery`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// =============================================================================
// Analytics Definitions
// =============================================================================

/// Fact table definition in intermediate format (Analytics)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateFactTable {
    /// Name of the fact table
    pub table_name:               String,
    /// The type the fact table is read as: its measures, filters and dimension paths are
    /// fields of that type, gated as a read of it (accepts `type` too).
    #[serde(default, alias = "type", skip_serializing_if = "Option::is_none")]
    pub type_name:                Option<String>,
    /// Measure columns (numeric aggregates)
    pub measures:                 Vec<IntermediateMeasure>,
    /// Dimension metadata
    pub dimensions:               IntermediateDimensions,
    /// Denormalized filter columns
    pub denormalized_filters:     Vec<IntermediateFilter>,
    /// Maps JSONB measure paths to flat SQL column names for pre-aggregated views
    #[serde(default)]
    pub native_measures:          HashMap<String, String>,
    /// Maps deep JSONB dimension paths to flat SQL column names for GROUP BY
    #[serde(default)]
    pub native_dimension_mapping: HashMap<String, String>,
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
    pub name:      String,
    /// SQL data type of the filter
    pub sql_type:  String,
    /// Whether this column should be indexed
    pub indexed:   bool,
    /// For an `LTREE` path column: the `[hierarchies.<name>]` its paths belong to, so
    /// `descendant_of_id` / `ancestor_of_id` can resolve a node id (#1498).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<String>,
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
