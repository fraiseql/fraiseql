//! Typed hierarchy definitions for ID-based ltree operators.
//!
//! Replaces raw `serde_json::Value` in `CompiledSchema.hierarchies_config` with
//! a strongly-typed [`HierarchiesConfig`] map, giving compile-time field access
//! and eliminating manual JSON traversal at runtime.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A single hierarchy definition mapping a name to its database table and ltree
/// path column.
///
/// Compiled from the `[hierarchies.<name>]` TOML section. The `id` column is
/// always `id` (UUID) per the trinity pattern and is not configurable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HierarchyDefinition {
    /// Database table containing the ltree column (e.g., `"tb_category"`).
    pub table: String,

    /// Name of the ltree column in the table (e.g., `"category_path"`).
    pub path_column: String,
}

/// Maps hierarchy names to their [`HierarchyDefinition`].
pub type HierarchiesConfig = HashMap<String, HierarchyDefinition>;
