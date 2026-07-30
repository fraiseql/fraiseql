//! Query/mutation structs: `IntermediateQuery`, `IntermediateMutation`,
//! `IntermediateArgument`, `IntermediateAutoParams`, `IntermediateQueryDefaults`.

use fraiseql_core::schema::InputStyle;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::types::IntermediateDeprecation;

/// Serde representation of the server-injected parameter map.
///
/// **The wire key is `inject_params` and it matches the compiled schema's key.** That is
/// the fix for #806, not a cosmetic choice: the intermediate format used to call this
/// `inject` while the compiled artifact called it `inject_params`, so every SDK author
/// who looked at a compiled schema to learn the name got it wrong. `TypeScript`, Go and
/// Java all did, independently, and `#[serde(default)]` with no `deny_unknown_fields`
/// turned each of their injection maps into an empty default — compiling a query with no
/// tenant predicate and printing `✓ Schema compiled successfully`. Two names for one
/// concept is what made that mistake available; one name retires it.
///
/// Both value shapes are accepted, because both are unambiguous and both are already in
/// use:
///
/// * nested — `{"tenant_id": {"source": "jwt", "claim": "org_id"}}`, what every SDK emits
/// * flat — `{"tenant_id": "jwt:org_id"}`, which is far pleasanter to hand-author
///
/// Serialization always emits the nested form, so a round-tripped intermediate schema is
/// byte-comparable with SDK output.
mod inject_params_serde {
    use indexmap::IndexMap;
    use serde::{
        Deserialize, Deserializer, Serializer,
        de::Error as _,
        ser::{Error as _, SerializeMap as _},
    };

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Spec {
        /// `"jwt:org_id"`
        Flat(String),
        /// `{"source": "jwt", "claim": "org_id"}`
        Nested { source: String, claim: String },
    }

    /// Deserialize either accepted shape into the internal `"<source>:<claim>"` form.
    ///
    /// # Errors
    ///
    /// Returns an error for a flat value with no `<source>:` prefix. A malformed source
    /// must not become a silently-unfiltered query — the same reasoning as the key.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<IndexMap<String, String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        IndexMap::<String, Spec>::deserialize(deserializer)?
            .into_iter()
            .map(|(param, spec)| match spec {
                Spec::Flat(source) if source.contains(':') => Ok((param, source)),
                Spec::Flat(source) => Err(D::Error::custom(format!(
                    "inject_params['{param}'] must be \"<source>:<claim>\" (for example \
                     \"jwt:org_id\") or {{\"source\": \"jwt\", \"claim\": \"org_id\"}}, got \
                     \"{source}\""
                ))),
                Spec::Nested { source, claim } => Ok((param, format!("{source}:{claim}"))),
            })
            .collect()
    }

    /// Serialize as the nested form every SDK emits.
    ///
    /// # Errors
    ///
    /// Returns an error if an entry is not in `"<source>:<claim>"` form, which
    /// [`deserialize`] guarantees but a direct struct construction does not.
    pub fn serialize<S>(map: &IndexMap<String, String>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut out = serializer.serialize_map(Some(map.len()))?;
        for (param, spec) in map {
            let (source, claim) = spec.split_once(':').ok_or_else(|| {
                S::Error::custom(format!(
                    "inject_params['{param}'] is not in \"<source>:<claim>\" form: \"{spec}\""
                ))
            })?;
            out.serialize_entry(param, &serde_json::json!({"source": source, "claim": claim}))?;
        }
        out.end()
    }
}

/// Per-operation REST route override, in the shape every SDK emits.
///
/// The wire form is `"rest": {"path": "/api/v1/orders", "method": "GET"}` — verified
/// identical across the Python, Go, PHP, C#, Java, Elixir, F# and Rust SDKs. It had no
/// consumer: neither [`IntermediateQuery`] nor [`IntermediateMutation`] declared the
/// field and the intermediate schema has no `deny_unknown_fields`, so serde discarded
/// the block and the converter hardcoded `rest_path`/`rest_method` to `None`. An author
/// setting `rest_path` got a clean compile and a route that 404s — and `detect_conflicts`
/// told operators to "use `rest_path` override to resolve" a route collision, advice that
/// could not work (#846).
///
/// `deny_unknown_fields` here is deliberate. This block exists precisely because a
/// silently-dropped key shipped for two releases; a typo inside it must fail the compile
/// rather than quietly degrade to a path-only override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateRest {
    /// Route path, replacing the derived one (e.g. `/api/v1/orders`).
    pub path: String,

    /// HTTP method. Absent means the derived default for the operation kind —
    /// `GET` for a query, `POST` for a mutation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

/// Project-wide default injected parameters, applied to operations that do not name them.
///
/// Seven SDKs — Python, Go, Java, PHP, C#, Elixir and F# — ship a `ConfigLoader` that
/// parses `[inject_defaults]` from `fraiseql.toml` and emits this block as a top-level key.
/// The compiler had never heard of it: `grep -rn inject_defaults crates/` returned nothing,
/// so `fraiseql compile` accepted the key, exited 0, printed `✓ Schema compiled
/// successfully`, and produced operations carrying no injection at all. An operator who
/// wrote `[inject_defaults] tenant_id = "jwt:tenant_id"` to stamp every operation with the
/// caller's tenant got a clean compile and **not one compiled operation with a tenant
/// predicate** (#847). Seven separate implementations of a feature that had never done
/// anything.
///
/// ## Ordering, and why it is not the one the issue suggested
///
/// `#847` proposed merging these in *before* `[fraiseql.tenancy]` validation. That is
/// wrong, and the two features would have become mutually exclusive: tenancy auto-injects
/// the annotated field when an operation's `inject_params` is empty and **fails the
/// compile** when it is non-empty but lacks the field. A default supplying any unrelated
/// key (`read_scope`, the example in the Python SDK's own docstring) would therefore have
/// turned every tenancy-annotated query into a compile error.
///
/// Defaults are applied by the converter, which runs after tenancy validation. Precedence,
/// most specific first:
///
/// 1. the operation's own `inject_params`
/// 2. `[fraiseql.tenancy]` auto-injection
/// 3. `[inject_defaults].queries` / `.mutations`
/// 4. `[inject_defaults].base`
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateInjectDefaults {
    /// Applied to queries **and** mutations.
    #[serde(
        default,
        with = "inject_params_serde",
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub base: IndexMap<String, String>,

    /// Applied to queries only, overriding a `base` entry of the same name.
    #[serde(
        default,
        with = "inject_params_serde",
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub queries: IndexMap<String, String>,

    /// Applied to mutations only, overriding a `base` entry of the same name.
    #[serde(
        default,
        with = "inject_params_serde",
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub mutations: IndexMap<String, String>,
}

impl IntermediateInjectDefaults {
    /// The defaults that apply to a query: `base`, then `queries` overriding it.
    pub fn for_queries(&self) -> IndexMap<String, String> {
        Self::layer(&self.base, &self.queries)
    }

    /// The defaults that apply to a mutation: `base`, then `mutations` overriding it.
    pub fn for_mutations(&self) -> IndexMap<String, String> {
        Self::layer(&self.base, &self.mutations)
    }

    /// `base` with `specific` layered on top.
    fn layer(
        base: &IndexMap<String, String>,
        specific: &IndexMap<String, String>,
    ) -> IndexMap<String, String> {
        let mut merged = base.clone();
        for (key, source) in specific {
            merged.insert(key.clone(), source.clone());
        }
        merged
    }

    /// Fill in any default the operation does not already declare.
    ///
    /// Absent-only, never overwriting: an operation that names a parameter has made a more
    /// specific decision than a project-wide default, and silently replacing it would be
    /// the same class of defect one level up.
    pub fn apply_to(defaults: &IndexMap<String, String>, inject: &mut IndexMap<String, String>) {
        for (key, source) in defaults {
            if !inject.contains_key(key) {
                inject.insert(key.clone(), source.clone());
            }
        }
    }
}

/// Argument definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateArgument {
    /// Argument name
    pub name: String,

    /// Argument type name
    ///
    /// **Language-agnostic**: Uses "type", not "`arg_type`"
    #[serde(rename = "type")]
    pub arg_type: String,

    /// Is argument optional?
    pub nullable: bool,

    /// Default value (JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    /// Argument description (from docstring or TOML `description`).
    ///
    /// The compiled `ArgumentDefinition` has always had a `description`, and the TOML
    /// authoring surface has always accepted one — but there was no field here to carry it
    /// between them, so the converter hard-coded `description: None` and every authored
    /// argument description was dropped. Found while fixing `#756`; same seam, same shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Deprecation info (from @deprecated directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}

impl From<&crate::config::toml_schema::ArgumentDefinition> for IntermediateArgument {
    /// Lower a TOML-declared argument into the intermediate form.
    ///
    /// **This conversion exists so the key names cannot drift.** `#756` was two
    /// hand-written `json!` literals emitting `"args"` with a `"required"` flag while this
    /// struct deserialized `"arguments"` with a `"nullable"` flag. Both sides carried
    /// `#[serde(default)]`, so the mismatch bound to an empty vector and every
    /// TOML-declared argument on every operation silently disappeared — compiling a query
    /// whose declared `id` filter was never applied, and returning rows the author had
    /// excluded.
    ///
    /// Producing the struct and serializing *it* means the wire keys come from the field
    /// definitions above. There is no second spelling to keep in sync.
    fn from(arg: &crate::config::toml_schema::ArgumentDefinition) -> Self {
        Self {
            name:        arg.name.clone(),
            arg_type:    arg.arg_type.clone(),
            // TOML says `required`; the intermediate and compiled schemas say `nullable`.
            // The inversion happens here, once.
            nullable:    !arg.required,
            default:     arg.default.clone(),
            description: arg.description.clone(),
            deprecated:  None,
        }
    }
}

/// Query definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateQuery {
    /// Query name (e.g., "users")
    pub name: String,

    /// Return type name (e.g., "User")
    pub return_type: String,

    /// Returns a list?
    #[serde(default)]
    pub returns_list: bool,

    /// Result is nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Query arguments
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Query description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SQL source (table/view name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Auto-generated parameters config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_params: Option<IntermediateAutoParams>,

    /// Deprecation info (from @deprecated directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,

    /// JSONB column name for extracting data (e.g., "data")
    /// Used for tv_* (denormalized JSONB tables) pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsonb_column: Option<String>,

    /// Whether this is a Relay connection query.
    /// When true, the compiler wraps results in `{ edges: [{ node, cursor }], pageInfo }`
    /// and generates `first`/`after`/`last`/`before` arguments instead of `limit`/`offset`.
    /// Requires `returns_list = true` and `sql_source` to be set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub relay: bool,

    /// Server-injected parameters: SQL column name → source expression (e.g. `"jwt:org_id"`).
    /// Not exposed as GraphQL arguments.
    ///
    /// Wire key is `inject_params`, matching the compiled schema. The two names were
    /// unified because the split is what let three SDKs emit a key that bound to
    /// nothing (#806); see the `inject_params_serde` module for the full reasoning.
    #[serde(
        default,
        rename = "inject_params",
        with = "inject_params_serde",
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub inject: IndexMap<String, String>,

    /// Per-query result cache TTL in seconds. Overrides the global cache TTL for this query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_ttl_seconds: Option<u64>,

    /// Additional database views this query reads beyond the primary `sql_source`.
    ///
    /// Used for correct cache invalidation when a query JOINs or reads multiple views.
    /// Each entry is validated as a safe SQL identifier at schema compile time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_views: Vec<String>,

    /// Role required to execute this query and see it in introspection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_role: Option<String>,

    /// Relay cursor column type: `"uuid"` for UUID PKs, `"int64"` (or absent) for bigint PKs.
    /// Only meaningful when `relay = true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_cursor_type: Option<String>,

    /// REST route override for this query. See [`IntermediateRest`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest: Option<IntermediateRest>,
}

/// Mutation definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateMutation {
    /// Mutation name (e.g., "createUser")
    pub name: String,

    /// Return type name (e.g., "User")
    pub return_type: String,

    /// Returns a list?
    #[serde(default)]
    pub returns_list: bool,

    /// Result is nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Mutation arguments
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Mutation description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SQL source (function name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Operation type (CREATE, UPDATE, DELETE, CUSTOM)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,

    /// Deprecation info (from @deprecated directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,

    /// Server-injected parameters: SQL parameter name → source expression (e.g. `"jwt:org_id"`).
    /// Not exposed as GraphQL arguments.
    ///
    /// Wire key is `inject_params`, matching the compiled schema. The two names were
    /// unified because the split is what let three SDKs emit a key that bound to
    /// nothing (#806); see the `inject_params_serde` module for the full reasoning.
    #[serde(
        default,
        rename = "inject_params",
        with = "inject_params_serde",
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub inject: IndexMap<String, String>,

    /// Role required to execute this mutation and see it in introspection.
    ///
    /// Mirrors [`IntermediateQuery::requires_role`]. Enforced at runtime with the same
    /// enumeration-hiding "not found" response as the query gate — a principal lacking
    /// the role cannot distinguish a forbidden mutation from a nonexistent one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_role: Option<String>,

    /// Fact tables whose version counter should be bumped after this mutation succeeds.
    ///
    /// Used for correct invalidation of analytic/aggregate cache entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates_fact_tables: Vec<String>,

    /// View names whose cached query results should be invalidated after this
    /// mutation succeeds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates_views: Vec<String>,

    /// Whether this mutation writes a Change-Spine change-log row (default `true`).
    ///
    /// Set `false` (via the authoring SDK's `@fraiseql.mutation(changelog=False)`)
    /// to opt a single mutation out of the in-transaction change-log outbox while
    /// the rest of the schema keeps logging. Defaults to `true` so a schema
    /// authored before this field existed keeps logging.
    #[serde(default = "default_changelog")]
    pub changelog: bool,

    /// How the GraphQL `input` argument is passed to the SQL function:
    /// `"flatten"` (positional columns, the default) or `"jsonb"` (the whole
    /// input as one `jsonb` arg). See [`InputStyle`].
    ///
    /// Orthogonal to [`operation`](IntermediateMutation::operation): set
    /// `"jsonb"` (via the authoring SDK's
    /// `@fraiseql.mutation(input_style="jsonb")`) so a backend using the
    /// single-`jsonb`-wrapper convention can register the real DML verb and
    /// still receive the whole input as one argument. Defaults to `"flatten"`,
    /// byte-identical to a schema authored before this field existed.
    #[serde(default, skip_serializing_if = "InputStyle::is_flatten")]
    pub input_style: InputStyle,

    /// Whether a successful, state-changing run of this mutation also records the
    /// changed entity's pre-image (before-state) into the Change-Spine
    /// `object_data_before` column, sourced from an optional `entity_before` on the
    /// mutation's `app.mutation_response`.
    ///
    /// Set `true` (via the authoring SDK's
    /// `@fraiseql.mutation(changelog_pre_image=True)` or TOML
    /// `changelog_pre_image = true`) to opt an audit-sensitive mutation into an
    /// inline Debezium-style `{before, after}`. Defaults to `false`,
    /// byte-identical to a schema authored before this field existed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub changelog_pre_image: bool,

    /// Whether this mutation exposes the typed graphql-cascade `cascade` field
    /// on its success payload.
    ///
    /// Set `true` via the authoring SDK's `@fraiseql.type(crud=True,
    /// cascade=True)` / `@fraiseql.mutation(cascade=True)`, or TOML
    /// `cascade = true`. Defaults to `false`, byte-identical to a schema
    /// authored before this field existed. Before this field, the compiler
    /// silently dropped the SDK flag.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cascade: bool,

    /// REST route override for this mutation. See [`IntermediateRest`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest: Option<IntermediateRest>,
}

impl Default for IntermediateMutation {
    /// Hand-written so `changelog` defaults to `true` (matching the serde default),
    /// not `bool`'s `false`. A `#[derive(Default)]` would silently opt
    /// `..Default::default()`-built mutations out of the change log.
    fn default() -> Self {
        Self {
            name:                    String::new(),
            return_type:             String::new(),
            returns_list:            false,
            nullable:                false,
            arguments:               Vec::new(),
            description:             None,
            sql_source:              None,
            operation:               None,
            deprecated:              None,
            inject:                  IndexMap::new(),
            requires_role:           None,
            invalidates_fact_tables: Vec::new(),
            invalidates_views:       Vec::new(),
            changelog:               default_changelog(),
            input_style:             InputStyle::Flatten,
            changelog_pre_image:     false,
            cascade:                 false,
            rest:                    None,
        }
    }
}

/// Serde default for [`IntermediateMutation::changelog`]: log by default (opt-out).
const fn default_changelog() -> bool {
    true
}

/// Auto-params configuration in intermediate format.
///
/// Each field is `Option<bool>`: `None` means "not specified — inherit from
/// `[query_defaults]`"; `Some(v)` means explicitly set by the authoring-language decorator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateAutoParams {
    /// Enable automatic limit parameter (None = inherit from query_defaults)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit:        Option<bool>,
    /// Enable automatic offset parameter (None = inherit from query_defaults)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset:       Option<bool>,
    /// Enable automatic where clause parameter (None = inherit from query_defaults)
    #[serde(rename = "where", default, skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<bool>,
    /// Enable automatic order_by parameter (None = inherit from query_defaults)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_by:     Option<bool>,
}

/// Global auto-param defaults for list queries (injected from TOML by the merger).
///
/// Never present in `schema.json` — set only at compile time via `[query_defaults]`
/// in `fraiseql.toml`.
///
/// The `Default` implementation returns all-`true`, matching the historical behaviour
/// when no `[query_defaults]` section is present in TOML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntermediateQueryDefaults {
    /// Default for `where` parameter
    pub where_clause: bool,
    /// Default for `order_by` parameter
    pub order_by:     bool,
    /// Default for `limit` parameter
    pub limit:        bool,
    /// Default for `offset` parameter
    pub offset:       bool,
}

impl Default for IntermediateQueryDefaults {
    fn default() -> Self {
        Self {
            where_clause: true,
            order_by:     true,
            limit:        true,
            offset:       true,
        }
    }
}
