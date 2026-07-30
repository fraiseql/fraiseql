//! Core type structs: `IntermediateType`, `IntermediateField`, `IntermediateEnum`,
//! `IntermediateEnumValue`, `IntermediateScalar`, `IntermediateDeprecation`.

use fraiseql_core::validation::ValidationRule;
use serde::{Deserialize, Serialize};

use super::fragments::IntermediateAppliedDirective;

/// Type definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateType {
    /// Type name (e.g., "User")
    pub name: String,

    /// Type fields
    pub fields: Vec<IntermediateField>,

    /// Type description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Type-level SQL source (backing table/view), e.g. `v_organization`.
    ///
    /// Owned types bind their relation on the *query* that returns them, so this
    /// is normally absent. It is emitted by the authoring SDK only for an
    /// owner-split `extend type … @key` federation entity: a subgraph that does
    /// not own the entity exposes no root query returning it, so the federation
    /// `_entities` resolver has no query to source the backing relation from and
    /// would otherwise guess `lower(typename)` and resolve to null (#507). When
    /// present it flows through to `TypeDefinition.sql_source` and is used as the
    /// `_entities` fallback relation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Interfaces this type implements (GraphQL spec §3.6)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,

    /// Role required to execute any operation returning this type.
    ///
    /// Lowered onto every query and mutation whose `return_type` is this type when the
    /// compiled schema loads, so the runtime's operation-level role gate enforces it —
    /// there is no separate type-level check to keep in step (#677). A gated type
    /// reachable as a *field* of a type that is not gated the same way is refused at
    /// load: operations returning the container carry no role, so the gated type would
    /// travel out ungated.
    ///
    /// It also filters the REST `/introspection` route. It does **not** filter GraphQL
    /// `__schema` introspection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_role: Option<String>,

    /// Whether this type is a mutation error type (tagged with `@fraiseql.error`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,

    /// Whether the author declared this a GraphQL **input** object rather than an output
    /// type (`is_input: true`).
    ///
    /// Four SDKs advertise this in their READMEs and emit it — Elixir
    /// (`fraiseql_type "X", is_input: true`), F# (`[<GraphQLType(IsInput=…)>]`), PHP
    /// (`#[GraphQLType(isInput: true)]`) and C# (`GraphQLTypeAttribute.IsInput`). There was
    /// no field here to receive it, so such a type compiled as an *object* type: a mutation
    /// argument referencing it produced a schema violating GraphQL §3.10 ("arguments must
    /// be input types"), which introspection-driven clients reject and federation
    /// composition fails on. `fraiseql compile` and `fraiseql lint` both exited 0 (#848).
    ///
    /// The converter routes a type carrying this into `input_types` and omits it from
    /// `types`. That is what makes Elixir's surface work at all: its exporter emits no
    /// `input_types` key whatsoever, so this flag is its *only* route to an input object.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_input: bool,

    /// Whether this type implements the Relay Node interface.
    /// When true, the compiler generates global node IDs (`base64("TypeName:uuid")`)
    /// and validates that `pk_{entity}` (BIGINT) is present in the view's data JSONB.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub relay: bool,

    /// Whether the author declared this an embedded value object
    /// (`@fraiseql.type(embedded=True)`, #687).
    ///
    /// A value object has no independent identity and is always nested under a parent
    /// entity, so the compiler exempts it from cascade entity classification: no
    /// `id: ID!` enforcement, no auto-`implements CascadeNode`. Threaded verbatim to
    /// `TypeDefinition.embedded` — it is the author's declaration, not something the
    /// compiler infers.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub embedded: bool,

    /// `@subscribable(tables=[...])` — the underlying base table(s) whose external
    /// writes should be captured onto the Change Spine (#366).
    ///
    /// The compiler aggregates the set of `(name, tables)` for every type carrying
    /// this into `CompiledSchema.subscribable`, which the
    /// `generate_capture_trigger_ddl` generator turns into per-table capture
    /// triggers. `None`/absent (the default) when the type is not subscribable.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "name": "Post",
    ///   "fields": [],
    ///   "subscribable_tables": ["tb_post"]
    /// }
    /// ```
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscribable_tables: Option<Vec<String>>,

    /// `@subscribable(..., pre_image=True)` — whether the capture triggers on this
    /// type's tables also record the changed entity's pre-image (OLD) into
    /// `object_data_before`, the out-of-band parity for a mutation's
    /// `changelog_pre_image`. Only meaningful alongside `subscribable_tables`;
    /// `false`/absent (the default) captures the after-image only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub subscribable_pre_image: bool,
}

/// Field definition in intermediate format
///
/// **NOTE**: Uses `type` field (not `field_type`)
/// This is the language-agnostic format. Rust conversion happens in converter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateField {
    /// Field name (e.g., "id")
    pub name: String,

    /// Field type name (e.g., "Int", "String", "User")
    ///
    /// **Language-agnostic**: All languages use "type", not "`field_type`"
    #[serde(rename = "type")]
    pub field_type: String,

    /// Is field nullable?
    pub nullable: bool,

    /// Field description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Applied directives (e.g., @deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directives: Option<Vec<IntermediateAppliedDirective>>,

    /// Scope required to access this field (field-level access control)
    ///
    /// When set, users must have this scope in their JWT to query this field.
    /// Supports patterns like "read:Type.field" or custom scopes like "hr:view_pii".
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "name": "salary",
    ///   "type": "Int",
    ///   "nullable": false,
    ///   "requires_scope": "read:Employee.salary"
    /// }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_scope: Option<String>,

    /// Policy when the user lacks `requires_scope`: `"reject"` (default) or `"mask"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_deny: Option<String>,

    /// Whether this field is gated by the dynamic field authorizer at runtime.
    ///
    /// When `true`, the compiled field is marked policy-gated
    /// (`FieldDefinition.authorize`) and a configured `FieldAuthorizer` is consulted
    /// per row. Defaults to `false` when absent.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "name": "email",
    ///   "type": "String",
    ///   "nullable": true,
    ///   "authorize": true
    /// }
    /// ```
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorize: Option<bool>,

    /// Named hierarchy reference for ID-based ltree operators.
    /// References a key in the `hierarchies` config map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<String>,
}

// =============================================================================
// Enum Definitions
// =============================================================================

/// GraphQL enum type definition in intermediate format.
///
/// Enums represent a finite set of possible values.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "OrderStatus",
///   "values": [
///     {"name": "PENDING"},
///     {"name": "PROCESSING"},
///     {"name": "SHIPPED", "description": "Package has been shipped"},
///     {"name": "DELIVERED"}
///   ],
///   "description": "Possible states of an order"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateEnum {
    /// Enum type name (e.g., "OrderStatus")
    pub name: String,

    /// Possible values for this enum
    pub values: Vec<IntermediateEnumValue>,

    /// Enum description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A single value within an enum type.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "ACTIVE",
///   "description": "The item is currently active",
///   "deprecated": {"reason": "Use ENABLED instead"}
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateEnumValue {
    /// Value name (e.g., "PENDING")
    pub name: String,

    /// Value description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Deprecation info (if value is deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}

/// Deprecation information for enum values or input fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateDeprecation {
    /// Deprecation reason (what to use instead)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// =============================================================================
// Custom Scalar Definitions
// =============================================================================

/// Custom scalar type definition in intermediate format.
///
/// Custom scalars allow applications to define domain-specific types with validation.
/// Scalars are defined in language SDKs (Python, `TypeScript`, Java, Go, Rust)
/// and compiled into the schema.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "Email",
///   "description": "Valid email address",
///   "specified_by_url": "https://tools.ietf.org/html/rfc5322",
///   "base_type": "String",
///   "validation_rules": [
///     {
///       "type": "pattern",
///       "value": {
///         "pattern": "^[a-z0-9._%+-]+@[a-z0-9.-]+\\.[a-z]{2,}$"
///       }
///     }
///   ]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateScalar {
    /// Scalar name (e.g., "Email", "Phone", "ISBN")
    pub name: String,

    /// Scalar description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// URL to specification/RFC (GraphQL spec §3.5.1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub specified_by_url: Option<String>,

    /// Built-in validation rules
    #[serde(default)]
    pub validation_rules: Vec<ValidationRule>,

    /// Base type for type aliases (e.g., "String" for Email scalar)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_type: Option<String>,
}
