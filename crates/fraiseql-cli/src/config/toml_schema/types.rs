//! Type and field definitions for TOML schema configuration.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Type definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypeDefinition {
    /// SQL source table or view
    pub sql_source:    String,
    /// Human-readable type description
    pub description:   Option<String>,
    /// Field definitions
    pub fields:        BTreeMap<String, FieldDefinition>,
    /// Relationships to other types, followed by REST resource embedding (#1266).
    ///
    /// Keyed by relationship name, like [`fields`](Self::fields), so the name is the
    /// table header rather than a key inside it:
    ///
    /// ```toml
    /// [types.Author.relationships.posts]
    /// target_type = "Post"
    /// cardinality = "OneToMany"
    /// foreign_key = "fk_author"     # column on the child table
    /// referenced_key = "id"         # column on the parent table
    /// ```
    ///
    /// The name is what a client writes in `?select=posts(id,title)`. Both the code
    /// comment on `embedding::executor::declared_key` and CHANGELOG 2.14.0 used to cite
    /// a top-level `[[relationships]]` block instead; no such block ever parsed, and no
    /// authoring surface of any kind existed until this one.
    #[serde(default)]
    pub relationships: BTreeMap<String, RelationshipDefinition>,
}

impl Default for TypeDefinition {
    fn default() -> Self {
        Self {
            sql_source:    "v_entity".to_string(),
            description:   None,
            fields:        BTreeMap::new(),
            relationships: BTreeMap::new(),
        }
    }
}

impl TypeDefinition {
    /// Render this type in the intermediate (IR) JSON shape the converter reads.
    ///
    /// One emitter for every TOML path. The type shape was hand-built in three places —
    /// `TomlSchema::to_intermediate_schema`, `SchemaMerger::merge_values`' TOML-only
    /// branch, and `SchemaMerger::enrich_type_from_toml` — which is the arrangement #959
    /// found already drifted at the *field* level, where a TOML key reaching the compiler
    /// on one path and vanishing on another is indistinguishable, from the author's side,
    /// from a key that does nothing.
    ///
    /// `enrich_type_from_toml` still merges rather than replaces, because there the type
    /// already exists from an SDK's `schema.json` and only the TOML's contribution may be
    /// written; it consumes [`Self::intermediate_overlay`], which is this function minus
    /// the identity keys.
    #[must_use]
    pub fn to_intermediate_json(&self, name: &str) -> serde_json::Value {
        let mut out = serde_json::json!({
            "name": name,
            "fields": self.fields.iter()
                .map(|(fname, fdef)| fdef.to_intermediate_json(fname))
                .collect::<Vec<_>>(),
        });
        for (key, value) in self.intermediate_overlay() {
            out[key] = value;
        }
        out
    }

    /// The keys TOML contributes to a type that may already exist from a `schema.json`.
    ///
    /// Identity (`name`) and `fields` are excluded: enriching a type an SDK authored must
    /// not overwrite the fields the SDK declared.
    #[must_use]
    pub fn intermediate_overlay(&self) -> Vec<(&'static str, serde_json::Value)> {
        let mut out: Vec<(&'static str, serde_json::Value)> =
            vec![("sql_source", serde_json::json!(self.sql_source))];
        if let Some(desc) = &self.description {
            out.push(("description", serde_json::json!(desc)));
        }
        if !self.relationships.is_empty() {
            out.push((
                "relationships",
                serde_json::Value::Array(
                    self.relationships
                        .iter()
                        .map(|(rname, rdef)| rdef.to_intermediate_json(rname))
                        .collect(),
                ),
            ));
        }
        out
    }
}

/// Relationship definition in TOML (#1266).
///
/// Deserialized straight into the intermediate shape; see
/// [`IntermediateRelationship`](crate::schema::intermediate::IntermediateRelationship) for
/// which key is read off which side per cardinality.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipDefinition {
    /// Target GraphQL type name, e.g. `"Post"`.
    pub target_type:    String,
    /// `"OneToMany"`, `"ManyToOne"` or `"OneToOne"`.
    pub cardinality:    fraiseql_core::schema::Cardinality,
    /// Foreign key column on the child table, e.g. `fk_author`.
    pub foreign_key:    String,
    /// Referenced key column on the parent table, e.g. `id`.
    pub referenced_key: String,
}

impl RelationshipDefinition {
    /// Render this relationship in the intermediate (IR) JSON shape the converter reads.
    ///
    /// # Panics
    ///
    /// Never in practice: the only fallible step is serializing `Cardinality`, a plain
    /// three-variant enum.
    #[must_use]
    pub fn to_intermediate_json(&self, name: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "target_type": self.target_type,
            "cardinality": serde_json::to_value(self.cardinality)
                .expect("Cardinality is a plain unit-variant enum, so serializing it cannot fail"),
            "foreign_key": self.foreign_key,
            "referenced_key": self.referenced_key,
        })
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
