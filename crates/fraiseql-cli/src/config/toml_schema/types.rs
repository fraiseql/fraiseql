//! Type and field definitions for TOML schema configuration.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Type definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypeDefinition {
    /// SQL source table or view
    pub sql_source:  String,
    /// Human-readable type description
    pub description: Option<String>,
    /// Field definitions
    pub fields:      BTreeMap<String, FieldDefinition>,
}

impl Default for TypeDefinition {
    fn default() -> Self {
        Self {
            sql_source:  "v_entity".to_string(),
            description: None,
            fields:      BTreeMap::new(),
        }
    }
}

/// Field definition
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDefinition {
    /// GraphQL field type (ID, String, Int, Boolean, DateTime, etc.)
    #[serde(rename = "type")]
    pub field_type:      String,
    /// Whether field can be null
    #[serde(default)]
    pub nullable:        bool,
    /// Field description
    pub description:     Option<String>,
    /// Named hierarchy reference for ID-based ltree operators.
    /// References a key in `[hierarchies.<name>]` config.
    #[serde(default)]
    pub hierarchy:       Option<String>,
    /// pgvector configuration for `Vector` and `BitVector` fields (#386, #959):
    /// dimensions (required — bits, for a `BitVector`), index type and distance
    /// metric. Authored as
    /// `vector = { dimensions = 1536, index_type = "hnsw", distance_metric = "cosine" }`,
    /// or `vector = { dimensions = 768, distance_metric = "hamming" }` for a
    /// binary one. Reuses the compiled-schema type directly so the authored and
    /// compiled shapes cannot drift.
    #[serde(default)]
    pub vector:          Option<fraiseql_core::schema::VectorConfig>,
    /// Names the vector field whose search distance this field carries (#959).
    /// Authored as `vector_distance = "embedding"` on a `Float` field; the value
    /// is the distance the `nearest` search ordered by.
    #[serde(default)]
    pub vector_distance: Option<String>,
}

impl FieldDefinition {
    /// Render this field in the intermediate (IR) JSON shape the converter reads.
    ///
    /// One emitter for both TOML paths. There were two hand-built copies —
    /// `TomlSchema::to_intermediate_schema` and `SchemaMerger::merge_values` —
    /// under a comment asking whoever touched one to remember the other; they
    /// had already drifted on `hierarchy`, and a TOML key that reaches the
    /// compiler on one path and vanishes on the other is indistinguishable, from
    /// the author's side, from a key that does nothing (#959).
    ///
    /// # Panics
    ///
    /// Never in practice: the only fallible step is serializing `VectorConfig`,
    /// which holds an integer and two plain enums.
    #[must_use]
    pub fn to_intermediate_json(&self, name: &str) -> serde_json::Value {
        let mut field = serde_json::json!({
            "name": name,
            "type": self.field_type,
            "nullable": self.nullable,
            "description": self.description,
        });
        if let Some(ref hierarchy) = self.hierarchy {
            field["hierarchy"] = serde_json::Value::String(hierarchy.clone());
        }
        if let Some(ref vector) = self.vector {
            field["vector_config"] = serde_json::to_value(vector)
                .expect("VectorConfig holds only plain enums and integers");
        }
        if let Some(ref measures) = self.vector_distance {
            field["vector_distance"] = serde_json::Value::String(measures.clone());
        }
        field
    }
}

/// Argument definition
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentDefinition {
    /// Argument name
    pub name:        String,
    /// Argument type
    #[serde(rename = "type")]
    pub arg_type:    String,
    /// Whether argument is required
    #[serde(default)]
    pub required:    bool,
    /// Default value if not provided
    pub default:     Option<serde_json::Value>,
    /// Argument description
    pub description: Option<String>,
}
