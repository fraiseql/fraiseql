//! Field-level RBAC support for schema definition.
//!
//! Provides `Field` struct with scope metadata for field-level access control.
//! Each field can specify required scopes (permissions) for GraphQL field access.
//!
//! # Example
//! ```
//! use fraiseql_rust::Field;
//!
//! let field = Field::new("email", "String")
//!     .with_nullable(false)
//!     .with_requires_scope(Some("read:user.email".to_string()));
//! ```

use serde::Serialize;

/// pgvector configuration for a vector field.
///
/// The compiler refuses a `Vector`, `BitVector`, `HalfVector` or `SparseVector` field
/// that carries no configuration, so this is what makes those types authorable.
///
/// Which combinations of field type, metric and index exist is pgvector's business and
/// the compiler's: it holds the operator-class table — `ivfflat` has no class for a
/// sparse vector at all, and none for jaccard — and refuses a schema that asks for one
/// that does not, naming the alternative. This SDK carries no second copy of that table;
/// a copy is what drifts.
///
/// # Example
/// ```
/// use fraiseql_rust::{Field, VectorConfig, VectorIndex, VectorMetric};
///
/// let field = Field::new("embedding", "Vector")
///     .with_nullable(false)
///     .with_vector_config(Some(
///         VectorConfig::new(1536).with_index(VectorIndex::IvfFlat).with_metric(VectorMetric::L2),
///     ));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VectorConfig {
    /// Vector width: float components for `Vector`, `HalfVector` and `SparseVector`,
    /// **bits** for `BitVector`. It sizes the column, and a query vector of a different
    /// width is refused rather than silently padded.
    pub dimensions: u32,
    /// The index this column is searched through.
    pub index_type: VectorIndex,
    /// The metric a search over this column orders by.
    pub distance_metric: VectorMetric,
}

/// The index a pgvector column is searched through.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorIndex {
    /// Hierarchical Navigable Small World index — the default.
    #[default]
    Hnsw,
    /// Inverted-file index: smaller and faster to build, slower to query.
    IvfFlat,
    /// No index — exact search.
    None,
}

/// The distance metric a vector search orders by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorMetric {
    /// Cosine distance — the default, and what most text embeddings want.
    #[default]
    Cosine,
    /// Euclidean distance.
    L2,
    /// Negative inner product.
    InnerProduct,
    /// Differing bits — `BitVector` only.
    Hamming,
    /// Set overlap normalised by set size — `BitVector` only.
    Jaccard,
}

impl VectorConfig {
    /// A config of the given width, searched through an HNSW index by cosine distance.
    ///
    /// The index type and the metric are always written into the emitted schema, so it
    /// says which index and which metric the column will get rather than leaving it to a
    /// compiler default the author cannot see.
    ///
    /// # Panics
    ///
    /// Panics when `dimensions` is zero: a column with no dimensions is not a thing an
    /// author can mean, and the compiler refuses it one step later.
    #[must_use]
    pub fn new(dimensions: u32) -> Self {
        assert!(dimensions >= 1, "a vector column has at least 1 dimension");
        Self {
            dimensions,
            index_type: VectorIndex::Hnsw,
            distance_metric: VectorMetric::Cosine,
        }
    }

    /// Sets the index type (fluent API).
    #[must_use]
    pub const fn with_index(mut self, index_type: VectorIndex) -> Self {
        self.index_type = index_type;
        self
    }

    /// Sets the distance metric (fluent API).
    #[must_use]
    pub const fn with_metric(mut self, distance_metric: VectorMetric) -> Self {
        self.distance_metric = distance_metric;
        self
    }
}

/// Represents a GraphQL field definition with optional scope requirements.
///
/// Fields can have scope-based access control through either a single scope
/// or multiple scopes (all required). Scope format is `action:resource`
/// (e.g., `read:user.email`, `admin:*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Field {
    /// Field name (e.g., "email", "password")
    pub name: String,
    /// GraphQL field type (e.g., "String", "Int", "User")
    ///
    /// The wire key is `type`, not `field_type` — the intermediate format is
    /// language-agnostic and every other SDK spells it that way.
    #[serde(rename = "type")]
    pub field_type: String,
    /// Whether field is nullable in GraphQL (default: true)
    pub nullable: bool,
    /// Single required scope for field access (e.g., "read:user.email")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_scope: Option<String>,
    /// Multiple required scopes (all must be satisfied)
    ///
    /// Never serialized directly: the compiled schema and the runtime field filter
    /// represent exactly one required scope, so [`Field::normalized`] folds a singleton
    /// into `requires_scope` and refuses anything longer.
    #[serde(skip)]
    pub requires_scopes: Option<Vec<String>>,
    /// Optional field description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// pgvector configuration, on a `Vector` / `BitVector` / `HalfVector` /
    /// `SparseVector` field. The compiler refuses such a field without one.
    #[serde(rename = "vector_config", skip_serializing_if = "Option::is_none")]
    pub vector_config: Option<VectorConfig>,
    /// On a `Float` field, the vector field whose `nearest` search distance this field
    /// carries. Selecting it on a query that did not run that search is refused, not
    /// answered with null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_distance: Option<String>,
    /// Deprecation, surfacing as `isDeprecated` / `deprecationReason` through
    /// introspection so generated clients can warn. `IntermediateField.deprecated` has
    /// been readable since #1025; there was no member here to put a reason in, so a Rust
    /// author could not deprecate a field at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecation>,
}

/// Field deprecation, emitted as the `deprecated` object the compiler reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Deprecation {
    /// Why the field is deprecated. Absent means deprecated with no stated reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Field {
    /// Creates a new field with given name and type.
    ///
    /// # Arguments
    /// * `name` - Field name
    /// * `field_type` - GraphQL field type
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let field = Field::new("email", "String");
    /// assert_eq!(field.name, "email");
    /// assert!(field.nullable); // default
    /// ```
    #[must_use]
    pub fn new(name: &str, field_type: &str) -> Self {
        Self {
            name: name.to_string(),
            field_type: field_type.to_string(),
            nullable: true,
            requires_scope: None,
            requires_scopes: None,
            description: None,
            vector_config: None,
            vector_distance: None,
            deprecated: None,
        }
    }

    /// Sets nullable property (fluent API).
    ///
    /// # Arguments
    /// * `nullable` - Whether field is nullable
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let field = Field::new("id", "Int").with_nullable(false);
    /// assert!(!field.nullable);
    /// ```
    #[must_use]
    pub const fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    /// Sets single required scope (fluent API).
    ///
    /// # Arguments
    /// * `scope` - Scope in format `action:resource` (e.g., `read:user.email`)
    ///
    /// Use this when field requires a single permission scope.
    /// Cannot be used together with `with_requires_scopes()`.
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let field = Field::new("email", "String")
    ///     .with_requires_scope(Some("read:user.email".to_string()));
    /// ```
    #[must_use]
    pub fn with_requires_scope(mut self, scope: Option<String>) -> Self {
        self.requires_scope = scope;
        self
    }

    /// Sets multiple required scopes (fluent API).
    ///
    /// # Arguments
    /// * `scopes` - Vector of scopes (all must be satisfied)
    ///
    /// Use this when field requires multiple permission scopes.
    /// Cannot be used together with `with_requires_scope()`.
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let scopes = vec!["read:user.email".to_string(), "write:user.*".to_string()];
    /// let field = Field::new("email", "String")
    ///     .with_requires_scopes(Some(scopes));
    /// ```
    #[must_use]
    pub fn with_requires_scopes(mut self, scopes: Option<Vec<String>>) -> Self {
        self.requires_scopes = scopes;
        self
    }

    /// Sets field description (fluent API).
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let field = Field::new("email", "String")
    ///     .with_description(Some("User email address".to_string()));
    /// ```
    #[must_use]
    pub fn with_description(mut self, description: Option<String>) -> Self {
        self.description = description;
        self
    }

    /// Sets the pgvector configuration (fluent API).
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::{Field, VectorConfig};
    /// let field = Field::new("embedding", "Vector")
    ///     .with_vector_config(Some(VectorConfig::new(1536)));
    /// ```
    #[must_use]
    pub const fn with_vector_config(mut self, config: Option<VectorConfig>) -> Self {
        self.vector_config = config;
        self
    }

    /// Marks the field deprecated, with an optional reason (fluent API).
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let field = Field::new("name", "String")
    ///     .with_deprecated(Some("use displayName".to_string()));
    /// ```
    #[must_use]
    pub fn with_deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecated = Some(Deprecation { reason });
        self
    }

    /// Names, on a `Float` field, the vector field whose search distance it carries.
    ///
    /// # Example
    /// ```
    /// # use fraiseql_rust::Field;
    /// let field = Field::new("similarity", "Float")
    ///     .with_vector_distance(Some("embedding".to_string()));
    /// ```
    #[must_use]
    pub fn with_vector_distance(mut self, vector_field: Option<String>) -> Self {
        self.vector_distance = vector_field;
        self
    }

    /// The field as it is written to `schema.json`, with scopes normalized.
    ///
    /// # Panics
    ///
    /// Panics when the field declares more than one required scope. The compiled schema
    /// and the runtime field filter represent exactly one `requires_scope`, so a
    /// multi-scope declaration cannot be honoured; emitting it produced a field with no
    /// scope at all — silently public (#807). Failing loudly at authoring time is the
    /// only outcome that does not ship an ungated field.
    ///
    /// Also panics when the field is both an embedding and a search distance. A field is
    /// one or the other: `vector_config` declares an embedding, `vector_distance`
    /// declares the `Float` reporting how far a search's result was from the query
    /// vector.
    #[must_use]
    pub fn normalized(&self) -> Self {
        assert!(
            !(self.vector_config.is_some() && self.vector_distance.is_some()),
            "field `{}` declares both a vector config and a vector distance; a field is \
             either an embedding or the Float reporting a search's distance, not both",
            self.name
        );
        let mut field = self.clone();
        if let Some(scopes) = self.requires_scopes.as_deref() {
            match scopes {
                [] => {},
                [only] => field.requires_scope = Some(only.clone()),
                many => panic!(
                    "field `{}` declares {} required scopes; multiple required scopes are not \
                     supported — use `with_requires_scope` with a single scope",
                    self.name,
                    many.len()
                ),
            }
        }
        field.requires_scopes = None;
        field
    }

    /// Serializes field to JSON string.
    ///
    /// # Example output:
    /// ```json
    /// {
    ///   "name": "email",
    ///   "type": "String",
    ///   "nullable": false,
    ///   "requires_scope": "read:user.email"
    /// }
    /// ```
    ///
    /// Built with `serde_json`, not string concatenation. The previous implementation
    /// interpolated raw values into JSON string literals, so a `"` or `\` anywhere in a
    /// name, scope or description produced text that is not parseable JSON — a
    /// description as ordinary as `the user's "display" name` broke the export, and the
    /// CLI failed at a byte offset rather than naming a field (#855).
    ///
    /// # Panics
    ///
    /// Panics under the same condition as [`Field::normalized`].
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.normalized())
            .expect("a Field contains only strings and bools, which always serialize")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_new() {
        let field = Field::new("id", "Int");
        assert_eq!(field.name, "id");
        assert_eq!(field.field_type, "Int");
        assert!(field.nullable);
    }

    /// All four pgvector field types keep their own config through serialization.
    ///
    /// Every key is asserted, not just the object's presence: `index_type` and
    /// `distance_metric` both have compiler-side defaults, so a config that lost them
    /// would still compile — to hnsw + cosine, chosen by nobody.
    #[test]
    fn vector_config_reaches_the_emitted_json() {
        let field =
            Field::new("embedding", "Vector").with_nullable(false).with_vector_config(Some(
                VectorConfig::new(1536)
                    .with_index(VectorIndex::IvfFlat)
                    .with_metric(VectorMetric::L2),
            ));
        let json: serde_json::Value = serde_json::from_str(&field.to_json()).expect("json");
        assert_eq!(json["vector_config"]["dimensions"], 1536);
        assert_eq!(json["vector_config"]["index_type"], "ivf_flat");
        assert_eq!(json["vector_config"]["distance_metric"], "l2");
    }

    #[test]
    fn the_index_and_metric_left_to_the_default_are_written_out() {
        let field =
            Field::new("embedding", "Vector").with_vector_config(Some(VectorConfig::new(8)));
        let json: serde_json::Value = serde_json::from_str(&field.to_json()).expect("json");
        assert_eq!(json["vector_config"]["index_type"], "hnsw");
        assert_eq!(json["vector_config"]["distance_metric"], "cosine");
    }

    #[test]
    fn a_distance_field_names_the_vector_it_measures() {
        let field =
            Field::new("similarity", "Float").with_vector_distance(Some("embedding".to_string()));
        let json: serde_json::Value = serde_json::from_str(&field.to_json()).expect("json");
        assert_eq!(json["vector_distance"], "embedding");
    }

    #[test]
    fn an_ordinary_field_carries_no_vector_keys() {
        let json: serde_json::Value =
            serde_json::from_str(&Field::new("id", "ID").to_json()).expect("json");
        assert!(json.get("vector_config").is_none());
        assert!(json.get("vector_distance").is_none());
    }

    #[test]
    #[should_panic(expected = "not both")]
    fn a_field_is_an_embedding_or_a_distance_not_both() {
        let _ = Field::new("embedding", "Vector")
            .with_vector_config(Some(VectorConfig::new(8)))
            .with_vector_distance(Some("embedding".to_string()))
            .to_json();
    }

    #[test]
    #[should_panic(expected = "at least 1 dimension")]
    fn a_dimension_count_no_column_can_have_is_refused() {
        let _ = VectorConfig::new(0);
    }

    #[test]
    fn test_field_builder_chain() {
        let field = Field::new("email", "String")
            .with_nullable(false)
            .with_requires_scope(Some("read:user.email".to_string()));

        assert_eq!(field.name, "email");
        assert!(!field.nullable);
        assert_eq!(field.requires_scope, Some("read:user.email".to_string()));
    }
}
