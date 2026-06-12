//! Field type definitions for compiled schemas.
//!
//! These types represent GraphQL field types in a Rust-native format.
//! All types are serializable to/from JSON for cross-language compatibility.

use serde::{Deserialize, Serialize};

use super::{domain_types::FieldName, scalar_types};

// ============================================================================
// Vector Types - pgvector support
// ============================================================================

/// Configuration for a vector field (pgvector).
///
/// This represents the configuration for a vector embedding field,
/// including dimensions, index type, and distance metric.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{VectorConfig, VectorIndexType, DistanceMetric};
///
/// let config = VectorConfig {
///     dimensions: 1536,
///     index_type: VectorIndexType::Hnsw,
///     distance_metric: DistanceMetric::Cosine,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorConfig {
    /// Number of dimensions in the vector (e.g., 1536 for `OpenAI` embeddings).
    pub dimensions: u32,

    /// Type of index to use for similarity search.
    #[serde(default)]
    pub index_type: VectorIndexType,

    /// Distance metric for similarity calculations.
    #[serde(default)]
    pub distance_metric: DistanceMetric,
}

impl VectorConfig {
    /// Create a new vector config with default index and distance metric.
    #[must_use]
    pub fn new(dimensions: u32) -> Self {
        Self {
            dimensions,
            index_type: VectorIndexType::default(),
            distance_metric: DistanceMetric::default(),
        }
    }

    /// Create a vector config for `OpenAI` embeddings (1536 dimensions, cosine).
    #[must_use]
    pub const fn openai() -> Self {
        Self {
            dimensions:      1536,
            index_type:      VectorIndexType::Hnsw,
            distance_metric: DistanceMetric::Cosine,
        }
    }

    /// Create a vector config for `OpenAI` small embeddings (512 dimensions, cosine).
    #[must_use]
    pub const fn openai_small() -> Self {
        Self {
            dimensions:      512,
            index_type:      VectorIndexType::Hnsw,
            distance_metric: DistanceMetric::Cosine,
        }
    }

    /// Set the index type.
    #[must_use]
    pub const fn with_index(mut self, index_type: VectorIndexType) -> Self {
        self.index_type = index_type;
        self
    }

    /// Set the distance metric.
    #[must_use]
    pub const fn with_distance(mut self, distance_metric: DistanceMetric) -> Self {
        self.distance_metric = distance_metric;
        self
    }
}

impl Default for VectorConfig {
    fn default() -> Self {
        Self::openai()
    }
}

/// Index type for vector similarity search.
///
/// pgvector supports two main index types:
/// - HNSW: Hierarchical Navigable Small World (faster queries, more memory)
/// - `IVFFlat`: Inverted File with Flat compression (slower queries, less memory)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VectorIndexType {
    /// HNSW index - best for most use cases.
    /// Pros: Fast queries, good recall
    /// Cons: More memory, slower index builds
    #[default]
    Hnsw,

    /// IVF Flat index - good for memory-constrained environments.
    /// Pros: Less memory, faster index builds
    /// Cons: Slower queries, requires training
    IvfFlat,

    /// No index - exact nearest neighbor search.
    /// Only suitable for small datasets (<10K vectors).
    None,
}

impl VectorIndexType {
    /// Get the pgvector index creation SQL.
    #[must_use]
    pub fn index_sql(
        &self,
        table: &str,
        column: &str,
        distance_metric: DistanceMetric,
    ) -> Option<String> {
        match self {
            Self::Hnsw => {
                let ops = distance_metric.hnsw_ops_class();
                Some(format!("CREATE INDEX ON {table} USING hnsw ({column} {ops})"))
            },
            Self::IvfFlat => {
                let ops = distance_metric.ivfflat_ops_class();
                Some(format!("CREATE INDEX ON {table} USING ivfflat ({column} {ops})"))
            },
            Self::None => None,
        }
    }
}

/// Distance metric for vector similarity calculations.
///
/// Each metric has a corresponding pgvector operator:
/// - Cosine: `<=>` (most common for text embeddings)
/// - L2: `<->` (Euclidean distance)
/// - `InnerProduct`: `<#>` (dot product, negate for similarity)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DistanceMetric {
    /// Cosine distance (1 - cosine similarity).
    /// Best for normalized embeddings (`OpenAI`, most text embeddings).
    /// Operator: `<=>`
    #[default]
    Cosine,

    /// L2 (Euclidean) distance.
    /// Good for image embeddings and when magnitude matters.
    /// Operator: `<->`
    L2,

    /// Inner product (negative for similarity).
    /// Use when embeddings are already normalized and you want max inner product.
    /// Operator: `<#>`
    InnerProduct,
}

impl DistanceMetric {
    /// Get the pgvector operator for this distance metric.
    #[must_use]
    pub const fn operator(&self) -> &'static str {
        match self {
            Self::Cosine => "<=>",
            Self::L2 => "<->",
            Self::InnerProduct => "<#>",
        }
    }

    /// Get the HNSW index operator class.
    #[must_use]
    pub const fn hnsw_ops_class(&self) -> &'static str {
        match self {
            Self::Cosine => "vector_cosine_ops",
            Self::L2 => "vector_l2_ops",
            Self::InnerProduct => "vector_ip_ops",
        }
    }

    /// Get the `IVFFlat` index operator class.
    #[must_use]
    pub const fn ivfflat_ops_class(&self) -> &'static str {
        match self {
            Self::Cosine => "vector_cosine_ops",
            Self::L2 => "vector_l2_ops",
            Self::InnerProduct => "vector_ip_ops",
        }
    }
}

// ============================================================================
// Field Deny Policy
// ============================================================================

/// Policy applied when a user lacks the required scope for a field.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FieldDenyPolicy {
    /// Reject the entire query with a `FORBIDDEN` error (default).
    #[default]
    Reject,
    /// Return `null` for this field — the query succeeds.
    Mask,
}

// ============================================================================
// Field Definition
// ============================================================================

/// A field within a GraphQL type.
///
/// This represents a single field definition after compilation from
/// authoring-language decorators. All data is Rust-owned.
///
/// # JSONB Architecture Note
///
/// FraiseQL stores all field data in a JSONB column (typically `data`).
/// The `name` field corresponds to the key in the JSONB object.
/// SQL columns are only used for WHERE clause filtering, not data retrieval.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::{FieldDefinition, FieldDenyPolicy, FieldType};
///
/// let field = FieldDefinition {
///     name: "email".into(),
///     field_type: FieldType::String,
///     nullable: true,
///     description: Some("User's email address".to_string()),
///     default_value: None,
///     vector_config: None,
///     alias: None,
///     deprecation: None,
///     requires_scope: None,
///     on_deny: FieldDenyPolicy::default(),
///     authorize: false,
///     encryption: None,
///     hierarchy: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDefinition {
    /// Field name - the key in the JSONB `data` column (e.g., "email").
    pub name: FieldName,

    /// Field type.
    pub field_type: FieldType,

    /// Is this field nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Optional description (from docstring).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<super::graphql_value::GraphQLValue>,

    /// Vector configuration (for pgvector fields).
    /// Only present when `field_type` is Vector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector_config: Option<VectorConfig>,

    /// GraphQL alias for this field (output key name in response).
    /// When set, the field value from JSONB key `name` is output under this alias.
    /// Example: `{ writer: author { name } }` - reads JSONB key "author", outputs as "writer"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Deprecation information (from @deprecated directive).
    /// When set, the field is marked as deprecated in the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<DeprecationInfo>,

    /// Scope required to access this field (field-level access control).
    ///
    /// When set, users must have this scope in their JWT to query this field.
    /// The runtime `FieldFilter` validates these requirements.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::{FieldDefinition, FieldDenyPolicy, FieldType};
    ///
    /// let field = FieldDefinition {
    ///     name: "salary".into(),
    ///     field_type: FieldType::Int,
    ///     nullable: false,
    ///     description: None,
    ///     default_value: None,
    ///     vector_config: None,
    ///     alias: None,
    ///     deprecation: None,
    ///     requires_scope: Some("read:Employee.salary".to_string()),
    ///     on_deny: FieldDenyPolicy::Reject,
    ///     authorize: false,
    ///     encryption: None,
    ///     hierarchy: None,
    /// };
    /// ```
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_scope: Option<String>,

    /// Policy when a user lacks the required scope for this field.
    ///
    /// - `Reject` (default): the entire query fails with a `FORBIDDEN` error.
    /// - `Mask`: the query succeeds but this field returns `null`.
    #[serde(default)]
    pub on_deny: FieldDenyPolicy,

    /// Whether this field is gated by the dynamic
    /// [`FieldAuthorizer`](crate::security::FieldAuthorizer).
    ///
    /// When `true` and a `FieldAuthorizer` is configured on the runtime, the
    /// authorizer is consulted per row for this field (composing as a logical AND
    /// with any static `requires_scope`). The decision can vary by principal, parent
    /// row, and field arguments — expressing rules `requires_scope` cannot.
    ///
    /// Defaults to `false` (no dynamic gate). `false` is not serialized, so compiled
    /// schemas that predate this field deserialize unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub authorize: bool,

    /// Encryption configuration for this field.
    ///
    /// **Not supported in this release.** End-to-end field-level at-rest encryption is
    /// not implemented: the mutation/write path does not encrypt field values, so a field
    /// marked for encryption would be stored in plaintext and then fail to decrypt on read.
    /// To avoid silently storing sensitive data unencrypted, the server **refuses to start**
    /// when any field declares this. The field is retained so existing compiled schemas
    /// still deserialize and produce that boot-time error rather than a silent plaintext write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption: Option<FieldEncryptionConfig>,

    /// Named hierarchy reference for ID-based ltree operators.
    ///
    /// When set, this field can use `descendantOfId` / `ancestorOfId` filter
    /// operators. The value references a key in the `hierarchies` config map,
    /// which provides the table and ltree path column for subquery generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<String>,
}

/// Encryption configuration for a field in the compiled schema.
///
/// **Not supported in this release** — see the [`FieldDefinition`] `encryption` field. The
/// write path does not encrypt, so a field carrying this config would be stored in plaintext;
/// the server refuses to boot rather than do so. This type describes the *intended* shape
/// (key reference + algorithm) for when end-to-end field encryption is implemented.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::FieldEncryptionConfig;
///
/// let config = FieldEncryptionConfig {
///     key_reference: "keys/user-email".to_string(),
///     algorithm: "AES-256-GCM".to_string(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldEncryptionConfig {
    /// Path or name for fetching the encryption key from the secrets backend.
    pub key_reference: String,
    /// Encryption algorithm identifier.
    #[serde(default = "default_encryption_algorithm")]
    pub algorithm:     String,
}

fn default_encryption_algorithm() -> String {
    "AES-256-GCM".to_string()
}

/// Deprecation information for a field or type.
///
/// Per GraphQL spec §4.4, deprecated fields should include a reason
/// explaining why the field is deprecated and what to use instead.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::DeprecationInfo;
///
/// let deprecation = DeprecationInfo {
///     reason: Some("Use 'userId' instead".to_string()),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeprecationInfo {
    /// Deprecation reason (what to use instead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl FieldDefinition {
    /// Create a new required field.
    #[must_use]
    pub fn new(name: impl Into<String>, field_type: FieldType) -> Self {
        Self {
            name: FieldName::new(name),
            field_type,
            nullable: false,
            description: None,
            default_value: None,
            vector_config: None,
            alias: None,
            deprecation: None,
            requires_scope: None,
            on_deny: FieldDenyPolicy::default(),
            authorize: false,
            encryption: None,
            hierarchy: None,
        }
    }

    /// Create a new nullable field.
    #[must_use]
    pub fn nullable(name: impl Into<String>, field_type: FieldType) -> Self {
        Self {
            name: FieldName::new(name),
            field_type,
            nullable: true,
            description: None,
            default_value: None,
            vector_config: None,
            alias: None,
            deprecation: None,
            requires_scope: None,
            on_deny: FieldDenyPolicy::default(),
            authorize: false,
            encryption: None,
            hierarchy: None,
        }
    }

    /// Create a new vector field.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::{FieldDefinition, VectorConfig};
    ///
    /// let embedding = FieldDefinition::vector("embedding", VectorConfig::openai());
    /// ```
    #[must_use]
    pub fn vector(name: impl Into<String>, config: VectorConfig) -> Self {
        Self {
            name:           FieldName::new(name),
            field_type:     FieldType::Vector,
            nullable:       false,
            description:    None,
            default_value:  None,
            vector_config:  Some(config),
            alias:          None,
            deprecation:    None,
            requires_scope: None,
            on_deny:        FieldDenyPolicy::default(),
            authorize:      false,
            encryption:     None,
            hierarchy:      None,
        }
    }

    /// Add a scope requirement to the field (field-level access control).
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::{FieldDefinition, FieldType};
    ///
    /// let salary = FieldDefinition::new("salary", FieldType::Int)
    ///     .with_requires_scope("read:Employee.salary");
    /// ```
    #[must_use]
    pub fn with_requires_scope(mut self, scope: impl Into<String>) -> Self {
        self.requires_scope = Some(scope.into());
        self
    }

    /// Set the deny policy for when a user lacks the required scope.
    #[must_use]
    pub const fn with_on_deny(mut self, policy: FieldDenyPolicy) -> Self {
        self.on_deny = policy;
        self
    }

    /// Mark this field as gated by the dynamic
    /// [`FieldAuthorizer`](crate::security::FieldAuthorizer).
    ///
    /// When `true` and a `FieldAuthorizer` is configured on the runtime, the
    /// authorizer is consulted per row for this field.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::{FieldDefinition, FieldType};
    ///
    /// let email = FieldDefinition::new("email", FieldType::String).with_authorize(true);
    /// assert!(email.authorize);
    /// ```
    #[must_use]
    pub const fn with_authorize(mut self, authorize: bool) -> Self {
        self.authorize = authorize;
        self
    }

    /// Add description to field.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add default value to field.
    #[must_use]
    pub fn with_default(mut self, value: super::graphql_value::GraphQLValue) -> Self {
        self.default_value = Some(value);
        self
    }

    /// Add vector configuration to field.
    #[must_use]
    pub const fn with_vector_config(mut self, config: VectorConfig) -> Self {
        self.vector_config = Some(config);
        self
    }

    /// Set a GraphQL alias for this field (output key name in response).
    ///
    /// The alias determines the key name in the JSON response, while `name`
    /// remains the JSONB key where data is read from.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::{FieldDefinition, FieldType};
    ///
    /// // JSONB key "author" will be output as "writer" in the response
    /// let field = FieldDefinition::new("author", FieldType::Object("User".to_string()))
    ///     .with_alias("writer");
    /// assert_eq!(field.output_name(), "writer");
    /// assert_eq!(field.name, "author"); // JSONB key unchanged
    /// ```
    #[must_use]
    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    /// Get the output name for this field (alias if set, otherwise name).
    ///
    /// This is the key name that appears in the GraphQL JSON response.
    #[must_use]
    pub fn output_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(self.name.as_str())
    }

    /// Get the JSONB key name for this field.
    ///
    /// This is always `name`, regardless of alias. Used for:
    /// - Reading data from JSONB column
    /// - Building WHERE clause paths
    #[must_use]
    pub fn jsonb_key(&self) -> &str {
        self.name.as_str()
    }

    /// Check if this field has an alias.
    #[must_use]
    pub const fn has_alias(&self) -> bool {
        self.alias.is_some()
    }

    /// Check if this is a vector field.
    #[must_use]
    pub const fn is_vector(&self) -> bool {
        matches!(self.field_type, FieldType::Vector)
    }

    /// Mark this field as deprecated.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::{FieldDefinition, FieldType};
    ///
    /// let field = FieldDefinition::new("oldId", FieldType::Int)
    ///     .deprecated(Some("Use 'id' instead".to_string()));
    /// assert!(field.is_deprecated());
    /// ```
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(DeprecationInfo { reason });
        self
    }

    /// Check if this field is deprecated.
    #[must_use]
    pub const fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Get the deprecation reason if deprecated.
    #[must_use]
    pub fn deprecation_reason(&self) -> Option<&str> {
        self.deprecation.as_ref().and_then(|d| d.reason.as_deref())
    }

    /// Add encryption configuration to this field.
    #[must_use]
    pub fn with_encryption(mut self, config: FieldEncryptionConfig) -> Self {
        self.encryption = Some(config);
        self
    }

    /// Check if this field is encrypted.
    #[must_use]
    pub const fn is_encrypted(&self) -> bool {
        self.encryption.is_some()
    }

    /// Whether this field is a primary key (name starts with "pk_" or equals "id").
    #[must_use]
    pub fn is_primary_key(&self) -> bool {
        self.name.as_str() == "id" || self.name.as_str().starts_with("pk_")
    }
}

/// Supported field types in GraphQL schema.
///
/// This enum represents all field types that can appear in a compiled schema.
/// It uses serde's adjacently-tagged representation for clean JSON serialization.
///
/// # JSON Representation
///
/// Scalar types serialize as: `{"String": null}`, `{"Int": null}`, etc.
/// Complex types serialize as: `{"List": {"String": null}}`, `{"Object": "User"}`, etc.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::FieldType;
///
/// let string_type = FieldType::String;
/// let list_type = FieldType::List(Box::new(FieldType::String));
/// let object_type = FieldType::Object("User".to_string());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[non_exhaustive]
pub enum FieldType {
    // ===== Core Scalar Types (GraphQL built-ins) =====
    /// GraphQL String type.
    #[default]
    String,

    /// GraphQL Int type (32-bit signed integer).
    Int,

    /// GraphQL Float type (double precision).
    Float,

    /// GraphQL Boolean type.
    Boolean,

    /// GraphQL ID type (serialized as string, UUID v4 in FraiseQL).
    #[serde(rename = "ID")]
    Id,

    // ===== Date/Time Types =====
    /// ISO 8601 `DateTime` (e.g., "2025-01-10T12:00:00Z").
    DateTime,

    /// ISO 8601 `Date` (e.g., "2025-01-10").
    Date,

    /// ISO 8601 `Time` (e.g., "12:00:00").
    Time,

    // ===== Complex Types =====
    /// Arbitrary JSON value.
    Json,

    /// UUID type (serialized as string).
    #[serde(rename = "UUID")]
    Uuid,

    /// Decimal/BigDecimal type (serialized as string for precision).
    Decimal,

    // ===== Vector Types (pgvector) =====
    /// Vector type for pgvector embeddings.
    /// Serialized as `[Float!]!` in GraphQL, stored as `vector(N)` in `PostgreSQL`.
    Vector,

    // ===== Rich/Custom Scalar Types =====
    /// Named scalar type (rich scalars like Email, URL, IBAN, or custom user-defined).
    ///
    /// This variant handles:
    /// - Built-in rich scalars: Email, URL, `PhoneNumber`, IBAN, etc.
    /// - User-defined custom scalars
    ///
    /// The string contains the scalar name exactly as defined (e.g., "Email", "IBAN").
    /// Validation rules are applied at runtime based on the scalar name.
    Scalar(String),

    // ===== Container Types =====
    /// List of another type.
    List(Box<FieldType>),

    /// Reference to another GraphQL object type.
    Object(String),

    /// Reference to an enum type.
    Enum(String),

    /// Reference to an input type.
    Input(String),

    /// Reference to an interface type.
    Interface(String),

    /// Reference to a union type.
    Union(String),
}

impl FieldType {
    /// Check if this is a scalar type (including rich/custom scalars).
    #[must_use]
    pub const fn is_scalar(&self) -> bool {
        matches!(
            self,
            Self::String
                | Self::Int
                | Self::Float
                | Self::Boolean
                | Self::Id
                | Self::DateTime
                | Self::Date
                | Self::Time
                | Self::Json
                | Self::Uuid
                | Self::Decimal
                | Self::Vector
                | Self::Scalar(_)
        )
    }

    /// Check if this is a vector type.
    #[must_use]
    pub const fn is_vector(&self) -> bool {
        matches!(self, Self::Vector)
    }

    /// Check if this is a list type.
    #[must_use]
    pub const fn is_list(&self) -> bool {
        matches!(self, Self::List(_))
    }

    /// Check if this is an object reference.
    #[must_use]
    pub const fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    /// Get the inner type if this is a list.
    #[must_use]
    pub fn inner_type(&self) -> Option<&FieldType> {
        match self {
            Self::List(inner) => Some(inner),
            _ => None,
        }
    }

    /// Get the type name if this is an object/enum/input reference.
    #[must_use]
    pub fn type_name(&self) -> Option<&str> {
        match self {
            Self::Object(name)
            | Self::Enum(name)
            | Self::Input(name)
            | Self::Interface(name)
            | Self::Union(name) => Some(name),
            _ => None,
        }
    }

    /// Convert to GraphQL SDL type string.
    #[must_use]
    pub fn to_graphql_string(&self) -> String {
        match self {
            Self::String => "String".to_string(),
            Self::Int => "Int".to_string(),
            Self::Float => "Float".to_string(),
            Self::Boolean => "Boolean".to_string(),
            Self::Id => "ID".to_string(),
            Self::DateTime => "DateTime".to_string(),
            Self::Date => "Date".to_string(),
            Self::Time => "Time".to_string(),
            Self::Json => "JSON".to_string(),
            Self::Uuid => "UUID".to_string(),
            Self::Decimal => "Decimal".to_string(),
            Self::Vector => "[Float!]!".to_string(), // Vectors are arrays of floats
            Self::List(inner) => format!("[{}]", inner.to_graphql_string()),
            // Named types: scalars, objects, enums, inputs, interfaces, unions all use their name
            Self::Scalar(name)
            | Self::Object(name)
            | Self::Enum(name)
            | Self::Input(name)
            | Self::Interface(name)
            | Self::Union(name) => name.clone(),
        }
    }

    /// Convert to SQL type string for `PostgreSQL`.
    #[must_use]
    pub fn to_sql_type(&self, vector_config: Option<&VectorConfig>) -> String {
        match self {
            // String, ID, and custom scalars are all stored as TEXT
            Self::String | Self::Id | Self::Scalar(_) => "TEXT".to_string(),
            Self::Int => "INTEGER".to_string(),
            Self::Float => "DOUBLE PRECISION".to_string(),
            Self::Boolean => "BOOLEAN".to_string(),
            Self::DateTime => "TIMESTAMPTZ".to_string(),
            Self::Date => "DATE".to_string(),
            Self::Time => "TIME".to_string(),
            Self::Uuid => "UUID".to_string(),
            Self::Decimal => "NUMERIC".to_string(),
            Self::Vector => {
                if let Some(config) = vector_config {
                    format!("vector({})", config.dimensions)
                } else {
                    "vector".to_string()
                }
            },
            // Lists and complex types stored as JSONB
            Self::Json
            | Self::List(_)
            | Self::Object(_)
            | Self::Enum(_)
            | Self::Input(_)
            | Self::Interface(_)
            | Self::Union(_) => "JSONB".to_string(),
        }
    }
}

impl std::fmt::Display for FieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_graphql_string())
    }
}

impl FieldType {
    /// Parse a GraphQL type string into a `FieldType`.
    ///
    /// Supports formats like:
    /// - `"String"`, `"Int"`, `"Boolean"`, etc. (scalar types)
    /// - `"String!"`, `"Int!"` (non-null scalars - the `!` is ignored, nullability is separate)
    /// - `"[String]"`, `"[User]"` (list types)
    /// - `"[String!]!"`, `"[User!]!"` (non-null list of non-null items)
    /// - `"User"`, `"Post"` (object types - anything not a known scalar)
    ///
    /// # Arguments
    ///
    /// * `type_str` - GraphQL type string
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::FieldType;
    ///
    /// assert_eq!(FieldType::parse("String"), FieldType::String);
    /// assert_eq!(FieldType::parse("Int!"), FieldType::Int);
    /// assert_eq!(FieldType::parse("[String]"), FieldType::List(Box::new(FieldType::String)));
    /// assert_eq!(FieldType::parse("User"), FieldType::Object("User".to_string()));
    /// ```
    #[must_use]
    pub fn parse(type_str: &str) -> Self {
        Self::parse_type_string(type_str.trim())
    }

    /// Try to match a type string against known rich scalars (case-insensitive).
    ///
    /// Returns the canonical scalar name if found, or None if not a rich scalar.
    fn try_match_rich_scalar(s: &str) -> Option<String> {
        let lower = s.to_lowercase();
        scalar_types::RICH_SCALARS
            .iter()
            .find(|&&rich_scalar| lower == rich_scalar.to_lowercase())
            .map(|&name| name.to_string())
    }

    /// Internal parser for type strings.
    fn parse_type_string(s: &str) -> Self {
        // Strip non-null marker (we handle nullability separately)
        let s = s.trim_end_matches('!');

        // Handle list types: [Type] or [Type!]
        if s.starts_with('[') && s.ends_with(']') {
            let inner = &s[1..s.len() - 1];
            let inner_type = Self::parse_type_string(inner);
            return Self::List(Box::new(inner_type));
        }

        // Handle core scalar types (case-insensitive matching)
        match s.to_lowercase().as_str() {
            "string" => Self::String,
            "int" | "integer" => Self::Int,
            "float" | "double" => Self::Float,
            "boolean" | "bool" => Self::Boolean,
            "id" => Self::Id,
            "datetime" | "timestamp" => Self::DateTime,
            "date" => Self::Date,
            "time" => Self::Time,
            "json" | "jsonb" => Self::Json,
            "uuid" => Self::Uuid,
            "decimal" | "numeric" | "bigdecimal" => Self::Decimal,
            "vector" => Self::Vector,
            _ => {
                // Check if it's a known rich scalar (case-insensitive)
                if let Some(canonical_name) = Self::try_match_rich_scalar(s) {
                    return Self::Scalar(canonical_name);
                }

                // Unknown type - default to Object for backwards compatibility
                // Custom scalars must be explicitly defined in scalar_types::RICH_SCALARS or
                // handled at a higher level (e.g., schema validation)
                Self::Object(s.to_string())
            },
        }
    }

    /// Parse a type string, treating unknown types as custom scalars.
    ///
    /// Unlike `parse()`, this method treats any unknown type as a `Scalar`
    /// rather than an `Object`. Use this when parsing user-defined scalar types.
    #[must_use]
    pub fn parse_as_scalar_if_unknown(
        type_str: &str,
        known_types: &std::collections::HashSet<String>,
    ) -> Self {
        let result = Self::parse(type_str);
        match result {
            Self::Object(name) if !known_types.contains(&name) => Self::Scalar(name),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests;
