//! SQL Projection Query Generator
//!
//! Generates database-specific SQL for field projection optimization.
//!
//! # Overview
//!
//! When a schema type has a `SqlProjectionHint`, this module generates the actual SQL
//! to project only requested fields at the database level, reducing network payload
//! and JSON deserialization overhead.
//!
//! # Supported Databases
//!
//! - PostgreSQL: Uses `jsonb_build_object()` for efficient field selection
//! - MySQL, SQLite, SQL Server: Multi-database support
//!
//! # Example
//!
//! ```rust
//! use fraiseql_db::projection_generator::PostgresProjectionGenerator;
//! # use fraiseql_error::Result;
//! # fn example() -> Result<()> {
//! let generator = PostgresProjectionGenerator::new();
//! let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];
//! let sql = generator.generate_projection_sql(&fields)?;
//! assert!(sql.contains("jsonb_build_object"));
//! # Ok(())
//! # }
//! ```

use fraiseql_error::{FraiseQLError, Result};

/// The semantic kind of a projection field, determining which JSONB extraction
/// operator to use in generated SQL.
///
/// - `Text` → `->>` (extracts as text — for String and ID scalars)
/// - `Native` → `->` (preserves native JSON type — Int, Float, Boolean, DateTime, etc.)
/// - `Composite` → `->` (preserves full JSONB structure)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Text scalar — extracted with `->>` (String, ID).
    Text,
    /// Native JSON scalar — extracted with `->` to preserve type (Int, Float, Boolean, DateTime,
    /// etc.).
    Native,
    /// Object or list — extracted with `->` to preserve JSONB structure.
    Composite,
}

/// A SQL expression a projection may emit verbatim, in place of reading a key
/// out of the JSONB column (#959).
///
/// The inner string is private and there is **no public constructor**: the only
/// way to obtain one is
/// [`vector_distance_expr`](crate::order_by::vector_distance_expr), which builds
/// it from a validated identifier, an operator drawn from a fixed set and a
/// literal whose character set it re-checks. A projection is SQL the caller does
/// not get to write, and this type is what enforces that across the crate
/// boundary — `fraiseql-core` can carry one into a `ProjectionField` but cannot
/// invent one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedExpr(String);

impl ComputedExpr {
    /// Wrap an expression assembled from validated parts.
    pub(crate) const fn from_validated_parts(sql: String) -> Self {
        Self(sql)
    }

    /// The expression as it will appear in the SELECT list.
    #[must_use]
    pub fn as_sql(&self) -> &str {
        &self.0
    }
}

/// A field in a SQL projection with type information.
///
/// Used by typed projection generators to choose the correct JSONB extraction
/// operator based on [`FieldKind`]: `->` (preserves JSONB) for composites and
/// native scalars, `->>` (text) for text scalars (String, ID).
///
/// When `sub_fields` is populated on a composite field, `generate_typed_projection_sql`
/// will recurse and emit a nested `jsonb_build_object(...)` instead of returning the full
/// composite blob.  Leave `sub_fields` as `None` to get the existing `data->'field'`
/// behaviour (full blob, no sub-selection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionField {
    /// Output (response) key — the GraphQL alias if present, otherwise the field
    /// name. Used verbatim as the key in the generated `jsonb_build_object`.
    pub name: String,

    /// Source GraphQL field name (camelCase) used to derive the JSONB column via
    /// `to_snake_case`. Equals [`name`](Self::name) for unaliased fields, but
    /// differs when the field is aliased: `myName: fullName` yields
    /// `name = "myName"`, `source = "fullName"`, so the projection reads
    /// `data->>'full_name'` while emitting it under the `myName` key (#418).
    pub source: String,

    /// Semantic kind of the field, controlling the JSONB extraction operator.
    pub kind: FieldKind,

    /// Sub-fields to project for composite (Object) types.
    ///
    /// When `Some` and non-empty, the generator recurses and produces a nested
    /// `jsonb_build_object` instead of returning the entire composite blob.
    /// Set to `None` (or `Some([])`) to fall back to `data->'field'`.
    /// List fields should always use `None` — sub-projection inside aggregated
    /// JSONB arrays is out of scope for this first iteration.
    pub sub_fields: Option<Vec<ProjectionField>>,

    /// How a localized field's stored locale map is read (#1513): as one label, or as its
    /// translations sibling's list. `None` for every other field.
    pub localized: Option<LocalizedRead>,

    /// A SQL expression this field's value comes from, instead of the JSONB
    /// column (#959) — the vector distance a `nearest` query ordered by.
    ///
    /// When set it wins over [`kind`](Self::kind) and
    /// [`source`](Self::source): the field is not stored, so there is no key to
    /// read. See [`ComputedExpr`] for why an arbitrary string cannot get here.
    pub computed: Option<ComputedExpr>,
}

impl ProjectionField {
    /// Create a text scalar projection field (uses `->>` text extraction).
    ///
    /// Use for String and ID fields only. Other scalars (Int, Float, Boolean,
    /// DateTime, etc.) should use [`Self::native`].
    #[must_use]
    pub fn scalar(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            source: name.clone(),
            name,
            kind: FieldKind::Text,
            sub_fields: None,
            localized: None,
            computed: None,
        }
    }

    /// Create a native JSON scalar projection field (uses `->` to preserve type).
    ///
    /// Use for Int, Float, Boolean, DateTime, Date, Time, Decimal, Vector, and
    /// other non-text scalars. `->>` would coerce these to strings inside
    /// `jsonb_build_object`, losing type information.
    #[must_use]
    pub fn native(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            source: name.clone(),
            name,
            kind: FieldKind::Native,
            sub_fields: None,
            localized: None,
            computed: None,
        }
    }

    /// Create a composite (object/list) projection field (uses `->` JSONB extraction).
    #[must_use]
    pub fn composite(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            source: name.clone(),
            name,
            kind: FieldKind::Composite,
            sub_fields: None,
            localized: None,
            computed: None,
        }
    }

    /// Create a composite projection field with known sub-fields.
    ///
    /// The generator will recurse into `sub_fields` and emit a nested
    /// `jsonb_build_object(...)` rather than returning the full composite blob.
    #[must_use]
    pub fn composite_with_sub_fields(name: impl Into<String>, sub_fields: Vec<Self>) -> Self {
        let name = name.into();
        Self {
            source: name.clone(),
            name,
            kind: FieldKind::Composite,
            sub_fields: Some(sub_fields),
            localized: None,
            computed: None,
        }
    }

    /// Create a field whose value is computed in SQL rather than read from the
    /// JSONB column (#959).
    ///
    /// `kind` is [`FieldKind::Native`] because the expression yields a JSON
    /// number, but the projection never consults it — a computed field has no
    /// stored key to extract.
    #[must_use]
    pub fn computed(name: impl Into<String>, expr: ComputedExpr) -> Self {
        let name = name.into();
        Self {
            source: name.clone(),
            name,
            kind: FieldKind::Native,
            sub_fields: None,
            localized: None,
            computed: Some(expr),
        }
    }

    /// Whether this field is a composite type (Object or List).
    #[must_use]
    pub const fn is_composite(&self) -> bool {
        matches!(self.kind, FieldKind::Composite)
    }
}

impl From<String> for ProjectionField {
    fn from(name: String) -> Self {
        Self::scalar(name)
    }
}

/// Validate that a GraphQL field name contains only characters that are safe
/// for use in SQL projections (alphanumeric characters and underscores only).
///
/// GraphQL field names in FraiseQL are either snake_case (schema definitions)
/// or camelCase (after the compiler's automatic conversion). Both forms are
/// subsets of `[a-zA-Z_][a-zA-Z0-9_]*`, so this function rejects any name
/// that falls outside that alphabet.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if `field` contains a character outside
/// `[a-zA-Z0-9_]`.
fn validate_field_name(field: &str) -> Result<()> {
    if field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(FraiseQLError::Validation {
            message: format!(
                "field name '{}' contains characters that cannot be safely projected; \
                 only ASCII alphanumeric characters and underscores are allowed",
                field
            ),
            path:    None,
        })
    }
}

use crate::utils::to_snake_case;

/// How a localized field's stored locale map is projected (#1513).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalizedRead {
    /// The first label of the fallback chain that is a string, `null` when none is. See
    /// [`localized_text_expr`].
    Label(Vec<String>),
    /// The translations sibling (`nameTranslations`): one object per allowed locale whose
    /// label is a string, in `allowed`'s order. See [`localized_translations_expr`].
    Translations {
        /// The schema's allowed locales, in order.
        allowed: Vec<String>,
        /// Each element's keys: the response key and what it holds.
        keys:    Vec<(String, TranslationPart)>,
    },
}

/// What one key of a translations element holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationPart {
    /// The locale tag.
    Locale,
    /// Its label.
    Value,
    /// `__typename`: `LocalizedString`.
    Typename,
}

/// `tag`, checked to be a language tag before it becomes a SQL literal.
fn checked_tag(tag: &str) -> Result<&str> {
    if tag.is_empty() || !tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err(FraiseQLError::validation(format!("locale `{tag}` is not a language tag")));
    }
    Ok(tag)
}

/// The SQL a localized field's value is read with (#1513): the first label in `chain` that is
/// a JSON string, `NULL` when none is.
///
/// `field_path` is the field's JSONB value (`"data"->'name'`), the locale map. For
/// `chain = [fr-CA, fr-FR, en-US]`:
///
/// ```text
/// CASE WHEN jsonb_typeof("data"->'name') = 'string' THEN "data"->'name' #>> '{}'
///      ELSE COALESCE(CASE WHEN jsonb_typeof("data"->'name'->'fr-CA') = 'string'
///                         THEN "data"->'name'->>'fr-CA' END, …, … 'en-US' … END) END
/// ```
///
/// A stored plain string is its own label in every locale: a field declared localized after
/// its rows were written reads them as they were until they are rewritten as maps.
///
/// Only a string counts: `->>` would render a number, a boolean or a nested object as text, and
/// the in-process evaluator (`fraiseql_core::runtime::localize`) treats them as absent, so the
/// two answer alike. The one builder of this expression: the projection, the `where` and
/// `orderBy` arms and the reported index all call it, so an index matches the query that
/// should use it.
///
/// # Errors
///
/// `FraiseQLError::Validation` for a locale outside `[A-Za-z0-9-]`: the tags come from the
/// compiled `[locale]` (BCP 47, validated at compile and at load) and are checked again here
/// before they become SQL literals.
pub fn localized_text_expr(field_path: &str, chain: &[String]) -> Result<String> {
    if chain.is_empty() {
        return Ok("NULL::text".to_string());
    }
    let arms = chain
        .iter()
        .map(|tag| {
            let tag = checked_tag(tag)?;
            Ok(format!(
                "CASE WHEN jsonb_typeof({field_path}->'{tag}') = 'string' THEN \
                 {field_path}->>'{tag}' END"
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(format!(
        "CASE WHEN jsonb_typeof({field_path}) = 'string' THEN {field_path} #>> '{{}}' ELSE \
         COALESCE({}) END",
        arms.join(", ")
    ))
}

/// The key a filter, a sort and a reported index read a localized field by (#1513): its label
/// through `chain` ([`localized_text_expr`]), under `collation` when there is one.
///
/// `path` is the field's storage path under the JSONB `column` (`["name"]`,
/// `["category", "label"]`). The one builder of this key: the `where` generator, the `ORDER
/// BY` renderer and the index report all call it, so an index matches the query that
/// should use it, `COLLATE` included (an index under one collation does not serve a
/// comparison under another).
///
/// # Errors
///
/// As [`localized_text_expr`] and [`crate::order_by::collate_suffix`].
pub fn localized_key_expr(
    column: &str,
    path: &[String],
    chain: &[String],
    collation: Option<&str>,
) -> Result<String> {
    let mut map = column.to_string();
    for segment in path {
        map.push_str("->'");
        map.push_str(&crate::path_escape::escape_postgres_jsonb_segment(segment));
        map.push('\'');
    }
    crate::order_by::collated(&localized_text_expr(&map, chain)?, collation)
}

/// An expression index a filter or sort on a localized field reads (#1513).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizedIndex {
    /// The index name: deterministic, at most 63 bytes.
    pub name: String,
    /// `CREATE INDEX CONCURRENTLY IF NOT EXISTS …`, on exactly the key queries read.
    pub ddl:  String,
}

/// The index a filter or sort on a localized field reads in one locale (#1513).
///
/// `field` is the storage key of a field of `table`. The index is one expression index on
/// [`localized_key_expr`], the key the `where` generator and the `ORDER BY` renderer build, so
/// the planner matches it.
///
/// The name is `ix_<table>_<field>_<locale>`; one longer than PostgreSQL's 63 bytes is cut
/// and ends with a hash of the whole, so it stays deterministic and distinct.
///
/// # Errors
///
/// As [`localized_key_expr`].
pub fn localized_index(
    table: &str,
    field: &str,
    locale: &str,
    chain: &[String],
    collation: Option<&str>,
) -> Result<LocalizedIndex> {
    let key = localized_key_expr("data", &[field.to_string()], chain, collation)?;
    let relation = table.rsplit('.').next().unwrap_or(table);
    let full = format!("ix_{relation}_{field}_{}", locale.to_ascii_lowercase().replace('-', "_"));
    let name = if full.len() <= 63 {
        full
    } else {
        // FNV-1a: stable across builds and platforms, unlike `DefaultHasher`.
        let hash = full.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
        let mut cut = 54;
        while !full.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}_{:08x}", &full[..cut], hash & 0xffff_ffff)
    };
    let quoted_table = crate::identifier::quote_postgres_identifier(table);
    let ddl = format!(
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS {} ON {quoted_table} (({key}));",
        crate::identifier::quote_postgres_identifier(&name)
    );
    Ok(LocalizedIndex { name, ddl })
}

/// The SQL a localized field's translations sibling is read with (#1513).
///
/// A JSON array with one object per locale of `allowed` whose label is a JSON string, in
/// `allowed`'s order, `[]` when there is none. Each object carries `keys`.
///
/// `field_path` is the field's JSONB value, the locale map. A stored key outside `allowed` is
/// not listed, and neither is a label that is not a string, so the sibling and the label
/// ([`localized_text_expr`]) agree on what a label is.
///
/// # Errors
///
/// `FraiseQLError::Validation` for a locale outside `[A-Za-z0-9-]`, as for
/// [`localized_text_expr`].
pub fn localized_translations_expr(
    field_path: &str,
    allowed: &[String],
    keys: &[(String, TranslationPart)],
) -> Result<String> {
    let tags = allowed
        .iter()
        .map(|tag| checked_tag(tag).map(|tag| format!("'{tag}'")))
        .collect::<Result<Vec<_>>>()?;
    if tags.is_empty() {
        return Ok("'[]'::jsonb".to_string());
    }
    let pairs: Vec<String> = keys
        .iter()
        .map(|(key, part)| {
            let value = match part {
                TranslationPart::Locale => "fraiseql_l.tag".to_string(),
                TranslationPart::Value => format!("{field_path}->>fraiseql_l.tag"),
                TranslationPart::Typename => "'LocalizedString'".to_string(),
            };
            format!("'{}', {value}", key.replace('\'', "''"))
        })
        .collect();
    Ok(format!(
        "COALESCE((SELECT jsonb_agg(jsonb_build_object({}) ORDER BY fraiseql_l.ord) FROM \
         unnest(ARRAY[{}]::text[]) WITH ORDINALITY AS fraiseql_l(tag, ord) WHERE \
         jsonb_typeof({field_path}->fraiseql_l.tag) = 'string'), '[]'::jsonb)",
        pairs.join(", "),
        tags.join(", ")
    ))
}

/// PostgreSQL SQL projection generator using jsonb_build_object.
///
/// Generates efficient PostgreSQL SQL that projects only requested JSONB fields,
/// reducing payload size and JSON deserialization time.
pub struct PostgresProjectionGenerator {
    /// JSONB column name (typically "data")
    jsonb_column: String,
}

impl PostgresProjectionGenerator {
    /// Create new PostgreSQL projection generator with default JSONB column name.
    ///
    /// Default JSONB column: "data"
    #[must_use]
    pub fn new() -> Self {
        Self::with_column("data")
    }

    /// Create projection generator with custom JSONB column name.
    ///
    /// # Arguments
    ///
    /// * `jsonb_column` - Name of the JSONB column in the database table
    #[must_use]
    pub fn with_column(jsonb_column: &str) -> Self {
        Self {
            jsonb_column: jsonb_column.to_string(),
        }
    }

    /// Generate PostgreSQL projection SQL for specified fields.
    ///
    /// Generates a `jsonb_build_object()` call that selects only the requested fields
    /// from the JSONB column, drastically reducing payload size.
    ///
    /// # Arguments
    ///
    /// * `fields` - GraphQL field names to project from JSONB
    ///
    /// # Returns
    ///
    /// SQL fragment that can be used in a SELECT clause, e.g.:
    /// `jsonb_build_object('id', data->>'id', 'email', data->>'email')`
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_db::projection_generator::PostgresProjectionGenerator;
    /// # use fraiseql_error::Result;
    /// # fn example() -> Result<()> {
    /// let generator = PostgresProjectionGenerator::new();
    /// let fields = vec!["id".to_string(), "email".to_string()];
    /// let sql = generator.generate_projection_sql(&fields)?;
    /// // Returns:
    /// // jsonb_build_object('id', data->>'id', 'email', data->>'email')
    /// assert!(sql.contains("jsonb_build_object"));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if any field name contains characters
    /// that cannot be safely included in a SQL projection.
    pub fn generate_projection_sql(&self, fields: &[String]) -> Result<String> {
        if fields.is_empty() {
            // No fields to project, return pass-through
            return Ok(format!("\"{}\"", self.jsonb_column));
        }

        // Validate all field names before generating any SQL.
        for field in fields {
            validate_field_name(field)?;
        }

        // Build the jsonb_build_object() call with all requested fields
        let field_pairs: Vec<String> = fields
            .iter()
            .map(|field| {
                // Response key uses the GraphQL field name (camelCase).
                // Used as a SQL *string literal* key (inside single-quotes): escape ' → ''.
                let safe_field = Self::escape_sql_string(field);
                // JSONB key uses the original schema field name (snake_case).
                let jsonb_key = to_snake_case(field);
                let safe_jsonb_key = Self::escape_sql_string(&jsonb_key);
                format!("'{}', \"{}\"->>'{}' ", safe_field, self.jsonb_column, safe_jsonb_key)
            })
            .collect();

        // Format: jsonb_build_object('field1', data->>'field1', 'field2', data->>'field2', ...)
        Ok(format!("jsonb_build_object({})", field_pairs.join(",")))
    }

    /// Generate type-aware PostgreSQL projection SQL.
    ///
    /// Uses `->` (JSONB extraction) for composite fields (objects, lists) and
    /// `->>` (text extraction) for scalar fields. This avoids the unnecessary
    /// text→JSON round-trip that occurs when `->>` is used for nested objects.
    ///
    /// When a composite field carries `sub_fields`, the generator recurses and
    /// emits a nested `jsonb_build_object(...)` that selects only the requested
    /// sub-fields rather than returning the entire blob, at every depth. `sub_fields` of
    /// `Some(vec![])` selects nothing of the object and renders `jsonb_build_object()`.
    /// There is no depth cap: the fields follow a selection, which the caller bounds
    /// (`max_query_depth`), and a cap that fell back to `data->'field'` served every
    /// stored key of the object below it. PostgreSQL 16's parser refuses a nest of 2 044
    /// `jsonb_build_object` levels (`memory exhausted`), two orders of magnitude above
    /// any bound a caller sets.
    ///
    /// # Arguments
    ///
    /// * `fields` - Projection fields with type information
    ///
    /// An empty field list projects an empty object, **not** the JSONB column.
    /// "Nothing was requested" and "everything was requested" are opposite
    /// answers, and returning the whole column for the first is how a `node(id:)`
    /// lookup whose selection resolved to nothing served every column in the view
    /// (#827).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if any field name contains characters
    /// that cannot be safely included in a SQL projection.
    pub fn generate_typed_projection_sql(&self, fields: &[ProjectionField]) -> Result<String> {
        if fields.is_empty() {
            return Ok("jsonb_build_object()".to_string());
        }

        let path = format!("\"{}\"", self.jsonb_column);
        let field_pairs = fields
            .iter()
            .map(|field| Self::render_field(field, &path))
            .collect::<Result<Vec<_>>>()?;

        Ok(format!("jsonb_build_object({})", field_pairs.join(",")))
    }

    /// Project the whole JSONB column **plus** a set of computed fields (#959).
    ///
    /// `"data" || jsonb_build_object('similarity', (…))`. Used where the row has
    /// to come back whole — a policy-gated field needs the full parent to decide
    /// on, and the stream strategy returns the stored blob — but a computed
    /// value still has to reach the response. Concatenation, so a computed key
    /// that collides with a stored one wins, which is the only reading that
    /// matches what the caller asked for.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if a field name cannot be safely
    /// projected.
    pub fn generate_merged_projection_sql(&self, computed: &[ProjectionField]) -> Result<String> {
        if computed.is_empty() {
            return Ok(format!("\"{}\"", self.jsonb_column));
        }
        let path = format!("\"{}\"", self.jsonb_column);
        let pairs = computed
            .iter()
            .map(|field| Self::render_field(field, &path))
            .collect::<Result<Vec<_>>>()?;
        Ok(format!("{path} || jsonb_build_object({})", pairs.join(",")))
    }

    /// Recursively render one projection field as a `'key', <expr>` pair for
    /// `jsonb_build_object`.
    ///
    /// * `field` — field to render
    /// * `path`  — JSONB path prefix built so far (e.g. `"data"` at depth 0, `"data"->'author'` at
    ///   depth 1)
    fn render_field(field: &ProjectionField, path: &str) -> Result<String> {
        // Output key is the (possibly aliased) response key; the JSONB column is
        // derived from the *source* field name. For unaliased fields these are
        // the same, but an aliased field (`myName: fullName`) must read
        // `data->>'full_name'`, not `data->>'my_name'` (#418).
        let resp_key = Self::escape_sql_string(&field.name);
        // A computed field has no stored key: the expression is the value, at
        // whatever depth the field appears (#959).
        if let Some(expr) = &field.computed {
            return Ok(format!("'{}', ({})", resp_key, expr.as_sql()));
        }
        let jsonb_key = to_snake_case(&field.source);
        let safe_jsonb_key = Self::escape_sql_string(&jsonb_key);

        // A localized field reads its chain's first string label, and its translations
        // sibling every allowed one (#1513).
        if let Some(read) = &field.localized {
            let field_path = format!("{path}->'{safe_jsonb_key}'");
            let expr = match read {
                LocalizedRead::Label(chain) => localized_text_expr(&field_path, chain)?,
                LocalizedRead::Translations { allowed, keys } => {
                    localized_translations_expr(&field_path, allowed, keys)?
                },
            };
            return Ok(format!("'{resp_key}', {expr}"));
        }

        // An object's sub-fields, at any depth — never the stored object in their place.
        //
        // Built only when the stored value IS an object (#1364): a JSON `null` or a missing
        // key reads every sub-field as NULL, and an unconditional `jsonb_build_object`
        // turned that into `{"id": null, ...}` — a nullable object that could never be
        // `null`, and a non-null sub-field answered with `null`. The `CASE` with no `ELSE`
        // yields NULL, and recursion applies the guard at every depth.
        if let Some(subs) = &field.sub_fields {
            let nested_path = format!("{}->'{}'", path, safe_jsonb_key);
            let inner = subs
                .iter()
                .map(|sf| Self::render_field(sf, &nested_path))
                .collect::<Result<Vec<_>>>()?;
            return Ok(format!(
                "'{}', CASE WHEN jsonb_typeof({}) = 'object' THEN jsonb_build_object({}) END",
                resp_key,
                nested_path,
                inner.join(",")
            ));
        }

        // Text: ->> (text cast, for String/ID).
        // Native / Composite: -> (preserves native JSONB type).
        let op = if field.kind == FieldKind::Text {
            "->>"
        } else {
            "->"
        };
        Ok(format!("'{}', {}{}'{}'", resp_key, path, op, safe_jsonb_key))
    }

    /// Generate complete SELECT clause with projection for a table.
    ///
    /// # Arguments
    ///
    /// * `table_alias` - Table alias or name in the FROM clause
    /// * `fields` - Fields to project
    ///
    /// # Returns
    ///
    /// Complete SELECT clause, e.g.: `SELECT jsonb_build_object(...) as data`
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_db::projection_generator::PostgresProjectionGenerator;
    ///
    /// let generator = PostgresProjectionGenerator::new();
    /// let fields = vec!["id".to_string(), "name".to_string()];
    /// let sql = generator.generate_select_clause("t", &fields).unwrap();
    /// assert!(sql.contains("SELECT"));
    /// ```
    ///
    /// # Errors
    ///
    /// Propagates any error from [`Self::generate_projection_sql`].
    pub fn generate_select_clause(&self, table_alias: &str, fields: &[String]) -> Result<String> {
        let projection = self.generate_projection_sql(fields)?;
        Ok(format!(
            "SELECT {} as \"{}\" FROM \"{}\" ",
            projection, self.jsonb_column, table_alias
        ))
    }

    /// Escape a value for use as a SQL *string literal* (inside single quotes).
    ///
    /// Doubles any embedded single-quote (`'` → `''`) to prevent SQL injection
    /// when the field name is embedded as a string literal key, e.g. in
    /// `jsonb_build_object('key', ...)` or `data->>'key'`.
    fn escape_sql_string(s: &str) -> String {
        s.replace('\'', "''")
    }

    /// Escape a SQL identifier using PostgreSQL double-quote quoting.
    ///
    /// Double-quote delimiters prevent identifier injection: any `"` within
    /// the identifier is doubled (`""`), and the whole name is wrapped in `"`.
    /// Use this when the name appears in an *identifier* position (column name,
    /// table alias) rather than as a string literal.
    #[allow(dead_code)] // Reason: available for callers embedding names as SQL identifiers
    fn escape_identifier(field: &str) -> String {
        format!("\"{}\"", field.replace('"', "\"\""))
    }
}

impl Default for PostgresProjectionGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
