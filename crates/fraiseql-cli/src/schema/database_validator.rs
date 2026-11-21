//! Compile-time database validation for schema definitions.
//!
//! Validates a compiled schema against a live database at three levels:
//! - **L1**: `sql_source` relation exists in the database
//! - **L2**: Columns and JSON column types match
//! - **L3**: JSONB keys exist in sampled rows (best-effort)
//!
//! Each finding carries a [`Severity`]: error-severity drift (a relation that
//! does not exist, a required field's key absent from every sampled row) fails
//! `compile --database` unless `--allow-drift` is passed (#384); warn-severity
//! findings are always advisory.

use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use fraiseql_core::{
    db::{
        DatabaseType,
        introspector::{DatabaseIntrospector, RelationInfo},
    },
    schema::{
        CompiledSchema, FieldType, PaginationOrder, SourceKind, SourceProbe, sql_source_probes,
    },
};
use fraiseql_db::postgres::PostgresTlsConfig;

use super::mutation_contract::Severity;

/// Report containing all database validation warnings and discovered metadata.
pub struct DatabaseValidationReport {
    /// All warnings emitted during validation.
    pub warnings:          Vec<DatabaseWarning>,
    /// Native columns discovered per query during L2 validation.
    ///
    /// Key: query name. Value: map of argument name → `PostgreSQL` type string
    /// (e.g. `"uuid"`, `"integer"`, `"text"`).
    ///
    /// Only contains entries for queries that have at least one direct argument
    /// with a matching native column on their `sql_source`.
    pub native_columns:    HashMap<String, HashMap<String, fraiseql_core::schema::NativeColumn>>,
    /// Cheaper page orderings discovered per query during L2 validation (#1303).
    ///
    /// Key: query name. Value: the ordering to replace the compiler's offline
    /// answer with.
    ///
    /// Only contains entries for queries whose `pagination_order` the compiler
    /// *derived* — an authored one is never overridden, and a query that declared
    /// `none` is never given one back. The offline derivation is already correct;
    /// this makes it cheap, which is why introspection may sharpen the answer and
    /// may never be required to produce one.
    pub pagination_orders: HashMap<String, PaginationOrder>,
}

/// A single database validation warning.
#[derive(Debug)]
pub enum DatabaseWarning {
    /// L1: `sql_source` relation does not exist.
    MissingRelation {
        /// Name of the query or mutation.
        query_name: String,
        /// The `sql_source` value that was not found.
        sql_source: String,
    },
    /// L1: a mutation's `sql_source` **function** does not exist.
    ///
    /// Distinct from [`MissingRelation`](Self::MissingRelation): a mutation's
    /// `sql_source` names a SQL *function* (the runtime calls
    /// `SELECT * FROM fn(...)`), not a view/table, so it is probed via `pg_proc`
    /// rather than the relation catalog.
    MissingFunction {
        /// Name of the mutation.
        mutation_name: String,
        /// The `sql_source` function that was not found.
        sql_source:    String,
    },
    /// L1: a `@fraiseql.type`'s declared `sql_source` view does not exist (#653).
    ///
    /// Most often an embedded value object the SDK gave a synthesized `v_<name>` source by
    /// naming convention (a view that was never authored). Advisory only — it never fails
    /// the compile; the hard `FRAISEQL_VALIDATE_SQL_SOURCES` gate deliberately does **not**
    /// probe type sources (it would drown in synthesized-source noise), so this warn-grade
    /// path under `--database` is the only place phantom type sources surface.
    MissingTypeSource {
        /// Name of the type.
        type_name:  String,
        /// The `sql_source` view that was not found.
        sql_source: String,
    },
    /// L1: `additional_view` does not exist.
    MissingAdditionalView {
        /// Name of the query.
        query_name: String,
        /// The view name that was not found.
        view_name:  String,
    },
    /// L2: `jsonb_column` does not exist on the relation.
    MissingJsonColumn {
        /// Name of the query.
        query_name:  String,
        /// The `sql_source` relation.
        sql_source:  String,
        /// The missing column name.
        column_name: String,
    },
    /// L2: `jsonb_column` exists but is not a JSON/JSONB type.
    WrongJsonColumnType {
        /// Name of the query.
        query_name:  String,
        /// The `sql_source` relation.
        sql_source:  String,
        /// The column name.
        column_name: String,
        /// The actual SQL data type.
        actual_type: String,
    },
    /// L2: an authored `pagination_order` column does not exist on the relation (#1303).
    ///
    /// The same shape as [`MissingCursorColumn`](Self::MissingCursorColumn) and
    /// error-grade for the same reason: the column is rendered into `ORDER BY`, so
    /// a name the relation does not carry is a query that fails at request time,
    /// on a route that compiled cleanly.
    MissingPaginationColumn {
        /// Name of the query.
        query_name:  String,
        /// The `sql_source` relation.
        sql_source:  String,
        /// The missing column name.
        column_name: String,
    },
    /// L2: the query's pages are ordered by the type's identity, and its relation
    /// exposes neither `pk_<type>` nor an `id` column to order them by (#1569).
    ///
    /// An error: ADR-0017 makes `id` a base column of every read table and view, and
    /// the identity is never read out of the document, so the compiled `ORDER BY`
    /// would fail at the first paged read.
    MissingIdentityColumn {
        /// Name of the query.
        query_name: String,
        /// The `sql_source` relation.
        sql_source: String,
        /// The `pk_<type>` column that would have been used had it existed.
        pk_column:  String,
    },
    /// L2: the view carries its own `ORDER BY` and the query did not declare
    /// `pagination_order = "none"` (#1303).
    ///
    /// The one case where ordering pages by the entity identity is a *regression*:
    /// a self-ordering view is the compiler's own documented remedy for
    /// `order_by = false`, and a default ordering replaces the author's order
    /// rather than adding to it. Advisory rather than error-grade because
    /// `pg_get_viewdef` cannot tell a top-level `ORDER BY` from one inside a
    /// subquery or window frame, where imposing a page order is entirely correct —
    /// so this reports a suspicion, and the author resolves it by declaring either
    /// way.
    SelfOrderingView {
        /// Name of the query.
        query_name: String,
        /// The `sql_source` view.
        sql_source: String,
    },
    /// L2: `relay_cursor_column` does not exist on the relation.
    MissingCursorColumn {
        /// Name of the query.
        query_name:  String,
        /// The `sql_source` relation.
        sql_source:  String,
        /// The missing cursor column name.
        column_name: String,
    },
    /// L3: a JSON key path is declared but not found in sampled data.
    MissingJsonKey {
        /// Name of the query.
        query_name:     String,
        /// The GraphQL type that declares the field (where to fix it).
        type_name:      String,
        /// The `sql_source` relation.
        sql_source:     String,
        /// The JSON column being sampled.
        json_column:    String,
        /// The GraphQL field name.
        field_name:     String,
        /// The snake_case key looked up in the JSON.
        json_key:       String,
        /// Whether the field is declared non-nullable. A required field's key
        /// cannot legitimately be absent from every sampled row, so this drives
        /// the finding's [`Severity`]: required → error, nullable → warning.
        field_required: bool,
    },
    /// L2: a direct query argument has no matching native column — will fall back to JSONB
    /// extraction.
    ///
    /// For best performance, consider adding a native column with the same name
    /// and an index on the `sql_source` table/view.
    NativeColumnFallback {
        /// Name of the query.
        query_name: String,
        /// The `sql_source` relation.
        sql_source: String,
        /// The argument name that has no matching native column.
        arg_name:   String,
    },
    /// A filter the query's relation cannot provide — an `inject_params` key (#1382) or a
    /// direct argument (#1394): no column of that name, and no sampled `data` row carrying
    /// it — so the predicate is false for every row and every caller gets an empty,
    /// successful answer.
    UnresolvableFilter {
        /// Name of the query.
        query_name: String,
        /// The `sql_source` relation.
        sql_source: String,
        /// What filters the relation.
        filter:     QueryFilter,
        /// The column / JSON key the runtime reads for it.
        key:        String,
        /// Why the relation cannot provide it.
        reason:     InjectMiss,
    },
    /// L2: a direct query argument resolves to a native column whose SQL type cannot
    /// cleanly drive the predicate for the argument's GraphQL scalar type (e.g. an
    /// `Int` argument filtering a `uuid` column — `WHERE col = $N` errors or never
    /// matches).
    TypeConvertibility {
        /// Name of the query.
        query_name:   String,
        /// The `sql_source` relation.
        sql_source:   String,
        /// The argument name.
        arg_name:     String,
        /// The argument's declared GraphQL type (e.g. `Int`).
        graphql_type: String,
        /// The native column's SQL type (e.g. `uuid`).
        column_type:  String,
    },
    /// L2: a query sorts or filters a `DateTime`, `Date` or `Time` field it reads out of its
    /// relation's document (#1571). The runtime compares `(data->>'key')::timestamptz`,
    /// a cast PostgreSQL will not index (it depends on the session's `TimeZone`), so every sort
    /// or filter on the key reads every row. A native column of the type, mapped by an argument
    /// of the field's name, is read instead, and an index on it serves both.
    TemporalKeyInDocument {
        /// Name of the query.
        query_name: String,
        /// The `sql_source` relation.
        sql_source: String,
        /// The field, as the schema names it.
        field:      String,
        /// The column the runtime would read for it.
        column:     String,
        /// What would let an index serve the field.
        remedy:     TemporalKeyRemedy,
    },
    /// L2: a query filters a TVIEW's document on keys no index of the TVIEW serves (#1571):
    /// its inject parameters and arguments that read `data->>'key'`, on every call. `pg_tviews`
    /// keeps an index declared with the TVIEW (its `indexes` option) over its rebuilds.
    TviewIndexMissing {
        /// Name of the query.
        query_name: String,
        /// The TVIEW's table, schema-qualified.
        tview:      String,
        /// The document keys it filters on, in the index's order.
        keys:       Vec<String>,
        /// The TVIEW's options with the index added: the whole declaration a replace must
        /// pass, since one without `indexes` drops them.
        options:    serde_json::Value,
    },
    /// L3 (#1579): a query is scoped by an `inject_params` column that is a native column of
    /// its relation, its type publishes the key as a field, and sampled rows hold a
    /// different value in the column than in the document. The root read filters on the
    /// column and an embedded document of the type on the key: they must agree.
    NativeKeyDisagreement {
        /// Name of the query.
        query_name:  String,
        /// The `sql_source` relation.
        sql_source:  String,
        /// The native column, and the document key of the same name.
        column:      String,
        /// The JSON column holding the document.
        json_column: String,
        /// Rows sampled.
        sampled:     u64,
        /// Rows whose column and key differ.
        disagreeing: u64,
    },
    /// A field declares `scoped_like_parent`, and documents embedded at it in sampled rows
    /// of a served document's view lack the scope key, or hold it as `null`. The
    /// nested level is filtered on that key, so it is refused or empty on every request.
    EmbeddedScopeKeyMissing {
        /// The read whose view was sampled.
        query_name: String,
        /// Its `sql_source` relation.
        sql_source: String,
        /// The type holding the declaring field, and the field.
        owner:      String,
        /// The declaring field.
        field:      String,
        /// The scoped type it holds.
        embedded:   String,
        /// Field names from the served document down to the field, joined by `.`.
        route:      String,
        /// The document key the nested level is filtered on.
        key:        String,
        /// Embedded documents found at the route in the sampled rows.
        documents:  u64,
        /// Those without the key.
        missing:    u64,
    },
    /// A field declares `scoped_like_parent`, and documents embedded at it in sampled rows
    /// hold another scope key than their holding row. The row is filtered on its own
    /// value at the root and each embedded document on its key, so they fall out of their
    /// own parent.
    EmbeddedScopeKeyDisagreement {
        /// The read whose view was sampled.
        query_name:     String,
        /// Its `sql_source` relation.
        sql_source:     String,
        /// The type holding the declaring field.
        owner:          String,
        /// The declaring field.
        field:          String,
        /// The scoped type it holds.
        embedded:       String,
        /// Field names from the served document down to the field, joined by `.`.
        route:          String,
        /// The embedded documents' key.
        key:            String,
        /// Where the holding row's value is read: a native column of the view.
        holding_native: bool,
        /// The holding row's column, or its document key of the same name.
        holding_column: String,
        /// Embedded documents found at the route in rows that hold a value.
        documents:      u64,
        /// Those whose key differs from the holding row's value.
        disagreeing:    u64,
    },
    /// The view of a served document that embeds a `scoped_like_parent` field holds no rows,
    /// so its embedded scope keys were not checked.
    EmbeddedScopeEmptyRelation {
        /// The read whose view was sampled.
        query_name: String,
        /// Its `sql_source` relation.
        sql_source: String,
        /// The served type.
        root:       String,
    },
    /// A served document reaches fields declaring `scoped_like_parent` by more routes than
    /// are sampled.
    EmbeddedScopeRoutesUnchecked {
        /// The served type.
        root:      String,
        /// Routes sampled.
        checked:   usize,
        /// Routes not sampled.
        unchecked: usize,
    },
    /// L2: a temporal argument filters a native column of another type than its scalar's.
    /// The value is bound with the column's type, so a `DateTime` over a `timestamp` column
    /// has its offset dropped and is compared as another instant.
    TemporalColumnType {
        /// Name of the query.
        query_name:  String,
        /// The `sql_source` relation.
        sql_source:  String,
        /// The argument name.
        arg_name:    String,
        /// The argument's temporal scalar.
        scalar:      &'static str,
        /// The column type that holds that scalar's values.
        expected:    &'static str,
        /// The native column's SQL type.
        column_type: String,
    },
    /// L3: sampled documents hold a temporal field outside its scalar's format, so a response
    /// would serve the value as stored: a `DateTime` written from a `timestamp` column has no
    /// offset.
    TemporalValueOutOfFormat {
        /// The read whose view was sampled.
        query_name: String,
        /// Its `sql_source` relation.
        sql_source: String,
        /// The type declaring the field.
        owner:      String,
        /// The field, as the schema names it.
        field:      String,
        /// The path of fields from the served document to the one holding the field, empty at
        /// the root.
        route:      String,
        /// The field's temporal scalar.
        scalar:     &'static str,
        /// The column type whose values are in that format.
        expected:   &'static str,
        /// Documents holding the field with a non-null value.
        documents:  u64,
        /// Those whose value is outside the format.
        refused:    u64,
    },
    /// L3: sampled documents hold an `Int` field outside the signed 32-bit range, which a
    /// response refuses as a field error: the column is wider than the scalar, and `BigInt`
    /// holds it.
    IntValueOutOfRange {
        /// The read whose view was sampled.
        query_name: String,
        /// Its `sql_source` relation.
        sql_source: String,
        /// The type declaring the field.
        owner:      String,
        /// The field, as the schema names it.
        field:      String,
        /// The path of fields from the served document to the one holding the field, empty at
        /// the root.
        route:      String,
        /// Documents holding the field with a non-null value.
        documents:  u64,
        /// Those whose value is outside the range.
        refused:    u64,
    },
    /// L2: an `Int` argument resolves to a `bigint` native column, so a value of the column
    /// past 32 bits cannot be passed to it: `BigInt` can.
    IntArgumentOverBigintColumn {
        /// Name of the query.
        query_name:  String,
        /// The `sql_source` relation.
        sql_source:  String,
        /// The argument name.
        arg_name:    String,
        /// The native column's SQL type.
        column_type: String,
    },
    /// L2: a view reads a document key as text cast to a temporal type, so PostgreSQL has
    /// reinterpreted the stored text before the server sees it: a value without an offset is
    /// read in the session's time zone, an offset is dropped, or a date-time is cut to its
    /// date. A sample of the view's output cannot see it: the original text is gone.
    TemporalCastFromText {
        /// The view whose definition holds the cast.
        view: String,
        /// The key read (a path, as `{a,b}`, for `#>>`).
        key:  String,
        /// The type the text is cast to, as PostgreSQL writes it.
        cast: &'static str,
    },
    /// L3: the view of a served document with temporal fields holds no rows, so their formats
    /// were not checked.
    TemporalFormatsUnchecked {
        /// The read whose view was sampled.
        query_name: String,
        /// Its `sql_source` relation.
        sql_source: String,
    },
}

/// What would let an index serve a temporal field read out of a document
/// ([`DatabaseWarning::TemporalKeyInDocument`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalKeyRemedy {
    /// The relation has the column: only the query's mapping is missing.
    MapColumn,
    /// A TVIEW without the column: its definition can project one.
    AddToTview,
    /// A table without the column: it can have one.
    AddToTable,
    /// A view without the column: it can expose its base table's, where the index lives.
    ExposeFromView,
}

impl DatabaseWarning {
    /// For [`TviewIndexMissing`](Self::TviewIndexMissing): the statement that declares the
    /// index with the TVIEW, keeping its definition and its other options. `None` for any
    /// other warning.
    #[must_use]
    pub fn statement(&self) -> Option<String> {
        let Self::TviewIndexMissing { tview, options, .. } = self else {
            return None;
        };
        let literal = |text: &str| text.replace('\'', "''");
        Some(format!(
            "SELECT tviews.pg_tviews_create_or_replace(format('%I.%I', schema, name), query, \
             '{}'::jsonb) FROM tviews.registry WHERE format('%I.%I', schema, name) = '{}';",
            literal(&options.to_string()),
            literal(tview)
        ))
    }
}

/// A query filter the runtime lowers onto the relation: what the author declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryFilter {
    /// `inject_params['<name>']`.
    Inject(String),
    /// A direct GraphQL argument (its GraphQL name).
    Argument(String),
}

impl fmt::Display for QueryFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inject(name) => write!(f, "inject_params['{name}']"),
            Self::Argument(name) => write!(f, "argument `{name}`"),
        }
    }
}

/// Why a relation cannot provide a query filter (#1382, #1394).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectMiss {
    /// No column of that name, and no JSON column to fall back on.
    NoJsonColumn,
    /// No column of that name, and none of the sampled rows' JSON carries the key.
    NotInSampledRows {
        /// The JSON column sampled.
        json_column: String,
    },
    /// No column of that name, and the relation is empty, so the JSON key cannot be checked.
    EmptyRelation {
        /// The JSON column that would have been sampled.
        json_column: String,
    },
}

impl DatabaseWarning {
    /// The severity of this finding under `compile --database` (#384).
    ///
    /// `Error`-severity drift fails the compile (unless `--allow-drift`):
    /// the declared schema names database objects that do not exist or cannot
    /// serve the declared shape, so the first query/mutation against them fails
    /// at runtime. `Warn`-severity findings are advisory:
    ///
    /// - [`MissingTypeSource`](Self::MissingTypeSource) is warn-grade by design (#653): synthesized
    ///   value-object sources would drown a hard gate.
    /// - [`MissingJsonKey`](Self::MissingJsonKey) is sampling-based; a *nullable* field may
    ///   legitimately be unset in every sampled row, so only a **required** field's absent key is
    ///   an error.
    /// - [`NativeColumnFallback`](Self::NativeColumnFallback) is a performance advisory — the JSONB
    ///   path is correct, just slower.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        match self {
            Self::MissingRelation { .. }
            | Self::MissingFunction { .. }
            | Self::MissingAdditionalView { .. }
            | Self::MissingJsonColumn { .. }
            | Self::WrongJsonColumnType { .. }
            | Self::MissingCursorColumn { .. }
            | Self::MissingPaginationColumn { .. }
            | Self::MissingIdentityColumn { .. }
            | Self::TypeConvertibility { .. }
            | Self::NativeKeyDisagreement { .. }
            | Self::EmbeddedScopeKeyMissing { .. }
            | Self::EmbeddedScopeKeyDisagreement { .. }
            | Self::TemporalColumnType { .. }
            | Self::TemporalValueOutOfFormat { .. } => Severity::Error,
            // Evidence of absence fails the compile; an empty relation proves nothing.
            Self::UnresolvableFilter { reason, .. } => match reason {
                InjectMiss::NoJsonColumn | InjectMiss::NotInSampledRows { .. } => Severity::Error,
                InjectMiss::EmptyRelation { .. } => Severity::Warn,
            },
            Self::MissingJsonKey { field_required, .. } => {
                if *field_required {
                    Severity::Error
                } else {
                    Severity::Warn
                }
            },
            Self::MissingTypeSource { .. }
            | Self::NativeColumnFallback { .. }
            | Self::SelfOrderingView { .. }
            | Self::TemporalKeyInDocument { .. }
            | Self::TviewIndexMissing { .. }
            | Self::EmbeddedScopeEmptyRelation { .. }
            | Self::EmbeddedScopeRoutesUnchecked { .. }
            | Self::TemporalCastFromText { .. }
            | Self::TemporalFormatsUnchecked { .. }
            | Self::IntValueOutOfRange { .. }
            | Self::IntArgumentOverBigintColumn { .. } => Severity::Warn,
        }
    }
}

/// The sampled documents a finding counts: `refused` of `documents`, at the root of
/// `sql_source`'s documents or embedded at `route` as `owner` documents.
fn sampled_holders(
    sql_source: &str,
    owner: &str,
    route: &str,
    documents: u64,
    refused: u64,
) -> String {
    if route.is_empty() {
        format!("{refused} of {documents} sampled `{sql_source}` documents")
    } else {
        format!(
            "{refused} of {documents} `{owner}` documents embedded at `{route}` in sampled \
             `{sql_source}` rows"
        )
    }
}

impl fmt::Display for DatabaseWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRelation {
                query_name,
                sql_source,
            } => {
                write!(
                    f,
                    "query `{query_name}`: sql_source `{sql_source}` does not exist in database"
                )
            },
            Self::MissingFunction {
                mutation_name,
                sql_source,
            } => {
                write!(
                    f,
                    "mutation `{mutation_name}`: sql_source function `{sql_source}` does not exist in database"
                )
            },
            Self::MissingTypeSource {
                type_name,
                sql_source,
            } => {
                write!(
                    f,
                    "type `{type_name}`: sql_source `{sql_source}` does not exist in the \
                     database — if `{type_name}` is an embedded value object it should declare \
                     no source (a value object has no independent identity; it is fetched via \
                     its parent)"
                )
            },
            Self::MissingAdditionalView {
                query_name,
                view_name,
            } => {
                write!(
                    f,
                    "query `{query_name}`: additional_view `{view_name}` does not exist in database"
                )
            },
            Self::MissingJsonColumn {
                query_name,
                sql_source,
                column_name,
            } => {
                write!(
                    f,
                    "query `{query_name}`: column `{column_name}` not found on `{sql_source}`"
                )
            },
            Self::WrongJsonColumnType {
                query_name,
                sql_source,
                column_name,
                actual_type,
            } => {
                write!(
                    f,
                    "query `{query_name}`: column `{column_name}` on `{sql_source}` is `{actual_type}`, expected json/jsonb"
                )
            },
            Self::MissingCursorColumn {
                query_name,
                sql_source,
                column_name,
            } => {
                write!(
                    f,
                    "query `{query_name}`: relay cursor column `{column_name}` not found on `{sql_source}`"
                )
            },
            Self::MissingPaginationColumn {
                query_name,
                sql_source,
                column_name,
            } => {
                write!(
                    f,
                    "query `{query_name}`: pagination_order column `{column_name}` not found on \
                     `{sql_source}` — it is rendered into ORDER BY, so this query fails at \
                     request time"
                )
            },
            Self::MissingIdentityColumn {
                query_name,
                sql_source,
                pk_column,
            } => {
                write!(
                    f,
                    "query `{query_name}`: offset pages are ordered by the type's identity, and \
                     `{sql_source}` exposes neither `{pk_column}` nor an `id` column (ADR-0017 \
                     makes `id` a base column of every read relation). Expose one of them, or \
                     declare pagination_order"
                )
            },
            Self::SelfOrderingView {
                query_name,
                sql_source,
            } => {
                write!(
                    f,
                    "query `{query_name}`: view `{sql_source}` carries its own ORDER BY, and \
                     this query does not declare pagination_order. An offset page will now be \
                     ordered by the entity identity instead, replacing the view's order. \
                     Declare pagination_order = \"none\" to keep it, or a column to confirm \
                     the new one"
                )
            },
            Self::MissingJsonKey {
                query_name,
                type_name,
                sql_source,
                json_column,
                field_name,
                json_key,
                field_required,
            } => {
                let requiredness = if *field_required {
                    "required"
                } else {
                    "nullable"
                };
                write!(
                    f,
                    "query `{query_name}`: {requiredness} field `{type_name}.{field_name}` (key `{json_key}`) not found in `{sql_source}.{json_column}` sample data"
                )
            },
            Self::NativeColumnFallback {
                query_name,
                sql_source,
                arg_name,
            } => {
                let key = fraiseql_core::utils::to_snake_case(arg_name);
                write!(
                    f,
                    "query `{query_name}`: argument `{arg_name}` will use JSONB extraction \
                     (`{sql_source}.data->>'{key}'`) — no native column `{key}` found on \
                     `{sql_source}`. Add a native column with an index for O(log n) lookup."
                )
            },
            Self::UnresolvableFilter {
                query_name,
                sql_source,
                filter,
                key,
                reason,
            } => {
                let why = match reason {
                    InjectMiss::NoJsonColumn => "has no JSON column to fall back on".to_string(),
                    InjectMiss::NotInSampledRows { json_column } => {
                        format!("and none of the sampled `{json_column}` rows carries the key")
                    },
                    InjectMiss::EmptyRelation { json_column } => format!(
                        "and is empty, so whether `{json_column}` carries the key cannot be checked"
                    ),
                };
                let way_out = match filter {
                    QueryFilter::Inject(_) => {
                        "Add the column to the relation, or, if this data is not scoped by it, drop \
                         the parameter from this query (`exclude_inject_defaults` when it comes from \
                         [inject_defaults])"
                    },
                    QueryFilter::Argument(_) => {
                        "Add the column to the relation (or the key to its `data`), or rename the \
                         argument after the column it filters"
                    },
                };
                write!(
                    f,
                    "query `{query_name}`: {filter} filters `{sql_source}` on `{key}`, which has no \
                     `{key}` column {why} — the predicate would match no row and every caller would \
                     get an empty answer. {way_out}"
                )
            },
            Self::TypeConvertibility {
                query_name,
                sql_source,
                arg_name,
                graphql_type,
                column_type,
            } => {
                write!(
                    f,
                    "query `{query_name}`: argument `{arg_name}` is `{graphql_type}` but the native column \
                     `{sql_source}.{arg_name}` is `{column_type}` — the predicate may error or never match. \
                     Align the argument type with the column, or store the field in `data` for JSONB extraction."
                )
            },
            Self::TemporalKeyInDocument {
                query_name,
                sql_source,
                field,
                column,
                remedy,
            } => {
                let step = match remedy {
                    TemporalKeyRemedy::MapColumn => format!(
                        "`{sql_source}` already has `{column}`: give the query an argument named \
                         `{field}` so this compile maps it to the column"
                    ),
                    TemporalKeyRemedy::AddToTview => format!(
                        "Add `{column}` to the TVIEW's definition (with its own type, e.g. \
                         `timestamptz`, and an index), then give the query an argument named \
                         `{field}` so this compile maps it to the column"
                    ),
                    TemporalKeyRemedy::AddToTable => format!(
                        "Add a `{column}` column to `{sql_source}` (with its own type, e.g. \
                         `timestamptz`, and an index), then give the query an argument named \
                         `{field}` so this compile maps it to the column"
                    ),
                    TemporalKeyRemedy::ExposeFromView => format!(
                        "Expose `{column}` from the view's base table as a column of its own \
                         type (e.g. `timestamptz`), index it on the base table with the page's \
                         tie-breaker (`({column} DESC, id)`), then give the query an argument \
                         named `{field}` so this compile maps it to the column"
                    ),
                };
                write!(
                    f,
                    "query `{query_name}`: `{field}` is read out of `{sql_source}`'s document, so \
                     every sort or filter on it casts each row's value and no index can serve it. \
                     {step} (docs/performance.md, \"Use a Native Column\")."
                )
            },
            Self::NativeKeyDisagreement {
                query_name,
                sql_source,
                column,
                json_column,
                sampled,
                disagreeing,
            } => {
                write!(
                    f,
                    "query `{query_name}`: is scoped by `{column}`, read from the native column \
                     at the root and from the `{json_column}` key `{column}` wherever the \
                     document is embedded, and {disagreeing} of {sampled} sampled \
                     `{sql_source}` rows hold a different value in each — an embedded document \
                     would be filtered on another value than its row. Build the key from the \
                     column (`jsonb_build_object('{column}', {column}, …)`)."
                )
            },
            Self::EmbeddedScopeKeyMissing {
                query_name,
                sql_source,
                owner,
                field,
                embedded,
                route,
                key,
                documents,
                missing,
            } => {
                write!(
                    f,
                    "query `{query_name}`: `{owner}.{field}` declares `scoped_like_parent`, and \
                     {missing} of {documents} `{embedded}` documents embedded at `{route}` in \
                     sampled `{sql_source}` rows do not carry the key `{key}` the nested level \
                     is filtered on, so that level is refused or empty on every request. Build \
                     the embedded object with `{key}`."
                )
            },
            Self::EmbeddedScopeKeyDisagreement {
                query_name,
                sql_source,
                owner,
                field,
                embedded,
                route,
                key,
                holding_native,
                holding_column,
                documents,
                disagreeing,
            } => {
                let holding = if *holding_native {
                    format!("column `{holding_column}`")
                } else {
                    format!("document key `{holding_column}`")
                };
                write!(
                    f,
                    "query `{query_name}`: `{owner}.{field}` declares `scoped_like_parent`, and \
                     {disagreeing} of {documents} `{embedded}` documents embedded at `{route}` \
                     in sampled `{sql_source}` rows hold another `{key}` than their row's \
                     {holding}. The row is filtered on its {holding} and each embedded document \
                     on its key `{key}`, compared as JSON (`\"5\"` is not `5`), so those \
                     documents are filtered out of their own parent. Build the embedded key \
                     from the row's {holding}."
                )
            },
            Self::EmbeddedScopeEmptyRelation {
                query_name,
                sql_source,
                root,
            } => {
                write!(
                    f,
                    "query `{query_name}`: `{sql_source}` holds no rows, so the scope keys of the \
                     documents `{root}` embeds under `scoped_like_parent` fields were not checked"
                )
            },
            Self::EmbeddedScopeRoutesUnchecked {
                root,
                checked,
                unchecked,
            } => {
                write!(
                    f,
                    "`{root}` reaches fields declaring `scoped_like_parent` by more routes than \
                     are sampled: {checked} checked, {unchecked} not checked"
                )
            },
            Self::TemporalColumnType {
                query_name,
                sql_source,
                arg_name,
                scalar,
                expected,
                column_type,
            } => {
                write!(
                    f,
                    "query `{query_name}`: argument `{arg_name}` is a {scalar}, which a \
                     `{expected}` column holds, but `{sql_source}.{arg_name}` is `{column_type}`: \
                     the value is cast to the column's type and compared as another value. Give \
                     the column the type `{expected}`, or the argument the column's scalar"
                )
            },
            Self::IntValueOutOfRange {
                query_name,
                sql_source,
                owner,
                field,
                route,
                documents,
                refused,
            } => {
                let held = sampled_holders(sql_source, owner, route, *documents, *refused);
                write!(
                    f,
                    "query `{query_name}`: {held} hold `{owner}.{field}` outside the signed \
                     32-bit range of an `Int`, and a response refuses each as a field error. \
                     Declare it `BigInt`, a signed 64-bit integer"
                )
            },
            Self::IntArgumentOverBigintColumn {
                query_name,
                sql_source,
                arg_name,
                column_type,
            } => {
                write!(
                    f,
                    "query `{query_name}`: argument `{arg_name}` is an `Int`, a signed 32-bit \
                     integer, but `{sql_source}.{arg_name}` is `{column_type}`: a value of the \
                     column past 32 bits cannot be passed. Declare it `BigInt`, a signed \
                     64-bit integer"
                )
            },
            Self::TemporalValueOutOfFormat {
                query_name,
                sql_source,
                owner,
                field,
                route,
                scalar,
                expected,
                documents,
                refused,
            } => {
                let held = sampled_holders(sql_source, owner, route, *documents, *refused);
                write!(
                    f,
                    "query `{query_name}`: {held} hold `{owner}.{field}` outside the {scalar} \
                     format, and a response would serve them as stored. Write the key from a \
                     `{expected}` value (see docs/features/temporal-values.md)"
                )
            },
            Self::TemporalCastFromText { view, key, cast } => {
                let why = match *cast {
                    "timestamp with time zone" | "time with time zone" => {
                        "a value without an offset is read in the session's time zone"
                    },
                    "date" => "a date-time is cut to its date",
                    _ => "an offset the value carries is dropped",
                };
                write!(
                    f,
                    "`{view}` reads the key `{key}` as text cast to `{cast}`: {why}. Store the \
                     value in a column of that type, or serve the text as stored and let the \
                     server hold it to its scalar's format"
                )
            },
            Self::TemporalFormatsUnchecked {
                query_name,
                sql_source,
            } => {
                write!(
                    f,
                    "query `{query_name}`: `{sql_source}` holds no rows, so the formats of its \
                     temporal fields are unchecked"
                )
            },
            Self::TviewIndexMissing {
                query_name,
                tview,
                keys,
                ..
            } => {
                write!(
                    f,
                    "query `{query_name}`: filters `{tview}`'s document on `{}` on every call, \
                     and no index of the TVIEW serves it. Declare one with the TVIEW, keeping its \
                     other options (a replace's options are its whole declaration: one without \
                     `indexes` drops them): {}",
                    keys.join("`, `"),
                    self.statement().unwrap_or_default()
                )
            },
        }
    }
}

/// Check if a SQL data type represents a JSON column for the given database.
pub(crate) fn is_json_type(data_type: &str, db_type: DatabaseType) -> bool {
    let lower = data_type.to_lowercase();
    match db_type {
        DatabaseType::PostgreSQL => lower == "jsonb" || lower == "json",
    }
}

/// Coarse SQL type families, used to flag argument↔column type mismatches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SqlFamily {
    /// Integer/decimal/float numeric types.
    Numeric,
    /// Boolean.
    Boolean,
    /// UUID / `uniqueidentifier`.
    Uuid,
    /// Character/string types.
    Text,
    /// Date/time/interval types.
    Temporal,
    /// `json` / `jsonb`.
    Json,
    /// Anything else (custom domains, enums, arrays, geometry, …) — never flagged.
    Other,
}

/// Classify a (any-dialect) SQL type string into a coarse [`SqlFamily`].
///
/// Strips length/precision and multi-word suffixes (`character varying`,
/// `timestamp with time zone`, `double precision`) down to the leading base word.
fn sql_type_family(sql_type: &str) -> SqlFamily {
    let lower = sql_type.trim().to_lowercase();
    let base = lower.split(['(', ' ', '[']).next().unwrap_or(lower.as_str());
    match base {
        "smallint" | "integer" | "int" | "int2" | "int4" | "int8" | "bigint" | "serial"
        | "bigserial" | "smallserial" | "numeric" | "decimal" | "real" | "double" | "float"
        | "float4" | "float8" | "money" | "mediumint" | "tinyint" => SqlFamily::Numeric,
        "boolean" | "bool" | "bit" => SqlFamily::Boolean,
        "uuid" | "uniqueidentifier" => SqlFamily::Uuid,
        "text" | "varchar" | "char" | "bpchar" | "name" | "citext" | "nvarchar" | "nchar"
        | "character" | "clob" | "string" => SqlFamily::Text,
        "date" | "time" | "timestamp" | "timestamptz" | "timetz" | "datetime" | "datetime2"
        | "interval" | "smalldatetime" => SqlFamily::Temporal,
        "json" | "jsonb" => SqlFamily::Json,
        _ => SqlFamily::Other,
    }
}

/// Whether a query argument's GraphQL type can cleanly drive an equality predicate
/// against a native column of `sql_type`.
///
/// Conservative by design: only the strongly-typed scalars (`Int`, `BigInt`, `Float`,
/// `Decimal`, `Boolean`, `UUID`) can flag a mismatch, and only against a *known*
/// incompatible column family. Permissive scalars (`String`, `ID`, date/time, `JSON`,
/// custom scalars) and non-scalar references never warn — the runtime binds them as
/// text-coercible parameters, and `ID` intentionally spans uuid / integer / text keys.
fn arg_type_convertible(field_type: &FieldType, sql_type: &str) -> bool {
    // A list filter (`[ID!]`) drives the predicate with its element type.
    let base = match field_type {
        FieldType::List { item: inner, .. } => inner.as_ref(),
        other => other,
    };
    let family = sql_type_family(sql_type);
    // Unknown column family → don't second-guess it.
    if family == SqlFamily::Other {
        return true;
    }
    match base {
        FieldType::Int | FieldType::BigInt | FieldType::Float | FieldType::Decimal => {
            family == SqlFamily::Numeric
        },
        FieldType::Boolean => family == SqlFamily::Boolean,
        FieldType::Uuid => matches!(family, SqlFamily::Uuid | SqlFamily::Text),
        _ => true,
    }
}

/// Split a potentially schema-qualified name into (optional_schema, name).
fn split_schema_qualified(sql_source: &str) -> (Option<&str>, &str) {
    match sql_source.split_once('.') {
        Some((schema, table)) => (Some(schema), table),
        None => (None, sql_source),
    }
}

/// Check if a relation exists in the relation lookup maps.
fn relation_exists(
    schema_qualified: &HashMap<(String, String), RelationInfo>,
    unqualified: &HashMap<String, Vec<String>>,
    sql_source: &str,
) -> bool {
    let (schema, name) = split_schema_qualified(sql_source);
    if let Some(s) = schema {
        schema_qualified.contains_key(&(s.to_string(), name.to_string()))
    } else {
        unqualified.contains_key(name)
    }
}

/// L1 relation existence with runtime-faithful resolution.
///
/// For a **schema-qualified** source, probes via `to_regclass` on the
/// case-sensitively-quoted identifier (search_path-independent — exactly how the
/// runtime resolves it), so a mixed-case relation in an off-`search_path` schema
/// is not falsely flagged. Falls back to the relation map for **bare** names
/// (search_path-scoped, also matching the runtime) or when the connector cannot
/// probe directly (non-Postgres).
async fn relation_source_exists(
    introspector: &impl DatabaseIntrospector,
    schema_qualified: &HashMap<(String, String), RelationInfo>,
    unqualified: &HashMap<String, Vec<String>>,
    sql_source: &str,
) -> fraiseql_core::Result<bool> {
    let (schema, name) = split_schema_qualified(sql_source);
    if let Some(s) = schema {
        if let Some(exists) = introspector.qualified_relation_exists(s, name).await? {
            return Ok(exists);
        }
    }
    // Bare name, or a connector that can't probe → relation-map lookup.
    Ok(relation_exists(schema_qualified, unqualified, sql_source))
}

/// L1 function existence for a mutation's `sql_source`.
///
/// A mutation's `sql_source` names a SQL *function*, so it is probed via `pg_proc`
/// (not the relation catalog). When the connector cannot probe functions
/// (non-Postgres), returns `true` — the check is skipped rather than false-failing.
async fn function_source_exists(
    introspector: &impl DatabaseIntrospector,
    sql_source: &str,
) -> fraiseql_core::Result<bool> {
    let (schema, name) = split_schema_qualified(sql_source);
    Ok(introspector.function_exists(schema, name).await?.unwrap_or(true))
}

/// Validate a compiled schema against a live database.
///
/// Performs three levels of validation:
/// - **L1**: Checks that `sql_source` relations exist
/// - **L2**: Checks column existence and JSON column types
/// - **L3**: Checks JSONB key existence via sampling
///
/// All diagnostics are warnings — the report never causes compilation to fail.
///
/// # Errors
///
/// Returns `FraiseQLError` if database introspection queries fail.
pub async fn validate_schema_against_database(
    schema: &CompiledSchema,
    introspector: &impl DatabaseIntrospector,
) -> fraiseql_core::Result<DatabaseValidationReport> {
    // Auto-wired argument names excluded from direct-arg native column detection.
    // Must stay in sync with AUTO_PARAM_NAMES in fraiseql-core/runtime/executor/query.rs.
    const AUTO_PARAM_NAMES: &[&str] = &[
        "where", "limit", "offset", "orderBy", "first", "last", "after", "before",
    ];

    let mut warnings = Vec::new();
    let mut native_columns: HashMap<String, HashMap<String, fraiseql_core::schema::NativeColumn>> =
        HashMap::new();
    let mut pagination_orders: HashMap<String, PaginationOrder> = HashMap::new();
    let db_type = introspector.database_type();

    // Each served document that embeds a `scoped_like_parent` field is sampled once
    // per view, through the first read of it below.
    let scoped_roots = schema.scoped_embedding_routes();
    for root in scoped_roots.iter().filter(|r| r.unchecked > 0) {
        warnings.push(DatabaseWarning::EmbeddedScopeRoutesUnchecked {
            root:      root.root.to_string(),
            checked:   root.routes.len(),
            unchecked: root.unchecked,
        });
    }
    let mut sampled_roots: HashSet<(&str, String)> = HashSet::new();

    // L1: Build relation lookup maps
    let relations = introspector.list_relations().await?;
    let (schema_qualified, unqualified) = build_relation_maps(&relations);

    // Validate queries. A function root reads no relation: its function is checked
    // by `function_root_contract`.
    for query in schema.queries.iter().filter(|q| q.sql_source_kind.is_relation()) {
        if let Some(ref source) = query.sql_source {
            // L1: Check relation exists (qualified → to_regclass verbatim, bare → map).
            if !relation_source_exists(introspector, &schema_qualified, &unqualified, source)
                .await?
            {
                warnings.push(DatabaseWarning::MissingRelation {
                    query_name: query.name.clone(),
                    sql_source: source.clone(),
                });
                continue; // Skip L2/L3 if relation doesn't exist
            }

            // L2: Get columns for the relation.
            // Pass the full source (possibly schema-qualified like "benchmark.tv_post") so
            // the introspector can use the explicit schema when present.
            let columns = introspector.get_columns(source).await?;
            // The columns PostgreSQL proves `NOT NULL` (#1533). A view's are none: the
            // catalog reports every view column nullable, so only a base relation proves it.
            let not_null: std::collections::HashSet<String> = columns
                .iter()
                .filter(|(_, _, nullable)| !nullable)
                .map(|(name, _, _)| name.clone())
                .collect();
            let column_map: HashMap<String, String> =
                columns.into_iter().map(|(name, dtype, _)| (name, dtype)).collect();

            // L2: Check jsonb_column
            let jsonb_col = &query.jsonb_column;
            if !jsonb_col.is_empty() {
                if let Some(actual_type) = column_map.get(jsonb_col) {
                    if !is_json_type(actual_type, db_type) {
                        warnings.push(DatabaseWarning::WrongJsonColumnType {
                            query_name:  query.name.clone(),
                            sql_source:  source.clone(),
                            column_name: jsonb_col.clone(),
                            actual_type: actual_type.clone(),
                        });
                    }
                } else {
                    warnings.push(DatabaseWarning::MissingJsonColumn {
                        query_name:  query.name.clone(),
                        sql_source:  source.clone(),
                        column_name: jsonb_col.clone(),
                    });
                }
            }

            // L3: the temporal keys of the served documents, in their scalars' formats.
            if column_map.get(jsonb_col).is_some_and(|t| is_json_type(t, db_type)) {
                check_document_values(schema, query, source, introspector, &mut warnings).await?;
            }

            // L2: Check relay_cursor_column
            if query.relay {
                if let Some(ref cursor_col) = query.relay_cursor_column {
                    if !column_map.contains_key(cursor_col) {
                        warnings.push(DatabaseWarning::MissingCursorColumn {
                            query_name:  query.name.clone(),
                            sql_source:  source.clone(),
                            column_name: cursor_col.clone(),
                        });
                    }
                }
            }

            // L2: pagination_order (#1303). Three questions, in the order the
            // compiled value answers them: a declared column must exist; a derived
            // identity is sharpened to `pk_<type>` (1.7x the unordered plan at a deep
            // offset, against 4.2x for `id`) and needs one of the two (#1569); and
            // either way, a view that orders itself is about to have that order replaced.
            match &query.pagination_order {
                Some(PaginationOrder::Column(col)) if !column_map.contains_key(col) => {
                    warnings.push(DatabaseWarning::MissingPaginationColumn {
                        query_name:  query.name.clone(),
                        sql_source:  source.clone(),
                        column_name: col.clone(),
                    });
                },
                Some(PaginationOrder::Column(_)) => {},
                Some(PaginationOrder::Identity) => {
                    let pk = pk_column_name(&query.return_type);
                    if column_map.contains_key(&pk) {
                        pagination_orders.insert(query.name.clone(), PaginationOrder::Column(pk));
                    } else if !column_map.contains_key("id") {
                        warnings.push(DatabaseWarning::MissingIdentityColumn {
                            query_name: query.name.clone(),
                            sql_source: source.clone(),
                            pk_column:  pk,
                        });
                    }
                },
                None => {},
            }

            // A view that orders itself is the one shape a default page ordering
            // makes worse, and the author is the only one who can adjudicate it —
            // so it is reported wherever a default is about to apply, whether the
            // ordering came from the author or from the compiler. A query that
            // declared `none` has already answered and is silent.
            if query.pagination_order.is_some()
                && introspector
                    .get_view_definition(source)
                    .await?
                    .is_some_and(|body| orders_itself(&body))
            {
                warnings.push(DatabaseWarning::SelfOrderingView {
                    query_name: query.name.clone(),
                    sql_source: source.clone(),
                });
            }

            // L3: Sample JSON keys if jsonb_column is valid JSON type
            if !jsonb_col.is_empty() {
                let json_type_ok =
                    column_map.get(jsonb_col).is_some_and(|t| is_json_type(t, db_type));

                if json_type_ok {
                    validate_json_keys(
                        schema,
                        query,
                        source,
                        jsonb_col,
                        introspector,
                        source, // pass full schema-qualified source for sample queries
                        &mut warnings,
                    )
                    .await?;
                }
            }

            // L2: Detect native columns for direct (non-auto-param) arguments AND
            // inject params. Both are filtered against the view at runtime, so a name
            // that matches a real column must use the native-column path
            // (`WHERE col = $N`) rather than the JSONB fallback (`data->>'name'`).
            let direct_args: Vec<&str> = query
                .arguments
                .iter()
                .filter(|a| !AUTO_PARAM_NAMES.contains(&a.name.as_str()))
                .map(|a| a.name.as_str())
                .collect();

            let (query_native, arg_fallbacks) = detect_query_native_columns(
                &direct_args,
                query.inject_params.keys().map(String::as_str),
                &column_map,
                &not_null,
            );
            // L3 (#1579): an inject column the type publishes is read from the column at the
            // root and from the document key wherever the document is embedded.
            if !jsonb_col.is_empty()
                && column_map.get(jsonb_col).is_some_and(|t| is_json_type(t, db_type))
            {
                check_native_keys_agree(
                    schema,
                    query,
                    source,
                    &query_native,
                    introspector,
                    &mut warnings,
                )
                .await?;
                for root in &scoped_roots {
                    if root.reads.iter().any(|read| read.name == query.name)
                        && sampled_roots.insert((root.root, source.clone()))
                    {
                        check_embedded_scope_keys(
                            root,
                            query,
                            source,
                            &column_map,
                            introspector,
                            &mut warnings,
                        )
                        .await?;
                    }
                }
            }

            // An inject key or argument with no column falls back to `data->>'key'` at
            // runtime. If the JSON does not carry it either, the predicate is false for every
            // row (#1382, #1394); if it does, the fallback is only slower.
            let mut unresolved: Vec<(QueryFilter, String)> = query
                .inject_params
                .keys()
                .filter(|param| !column_map.contains_key(*param))
                .map(|param| (QueryFilter::Inject(param.clone()), param.clone()))
                .collect();
            unresolved.extend(arg_fallbacks.into_iter().map(|arg| {
                let key = fraiseql_core::utils::to_snake_case(&arg);
                (QueryFilter::Argument(arg), key)
            }));
            // L2 (#1571): on a TVIEW, an index declared with it serves those keys.
            if !unresolved.is_empty() {
                if let Some(declaration) = introspector.tview_declaration(source).await? {
                    warnings.extend(tview_index_advice(query, &declaration, &unresolved));
                }
            }
            check_filters_resolvable(
                query,
                source,
                unresolved,
                &column_map,
                db_type,
                introspector,
                &mut warnings,
            )
            .await?;

            // L2 (type-convertibility): a direct argument that resolved to a native
            // column whose SQL type cannot cleanly drive the predicate is a likely
            // authoring bug (e.g. an `Int` argument filtering a `uuid` column).
            for arg in &query.arguments {
                if let Some(native) = query_native.get(&arg.name) {
                    let col_type = &native.pg_type;
                    if arg.arg_type == FieldType::Int
                        && matches!(col_type.trim().to_lowercase().as_str(), "bigint" | "int8")
                    {
                        warnings.push(DatabaseWarning::IntArgumentOverBigintColumn {
                            query_name:  query.name.clone(),
                            sql_source:  source.clone(),
                            arg_name:    arg.name.clone(),
                            column_type: col_type.clone(),
                        });
                    } else if let Some((scalar, expected, names)) = temporal_column(&arg.arg_type) {
                        if !names.contains(&col_type.trim().to_lowercase().as_str()) {
                            warnings.push(DatabaseWarning::TemporalColumnType {
                                query_name: query.name.clone(),
                                sql_source: source.clone(),
                                arg_name: arg.name.clone(),
                                scalar,
                                expected,
                                column_type: col_type.clone(),
                            });
                        }
                    } else if !arg_type_convertible(&arg.arg_type, col_type) {
                        warnings.push(DatabaseWarning::TypeConvertibility {
                            query_name:   query.name.clone(),
                            sql_source:   source.clone(),
                            arg_name:     arg.name.clone(),
                            graphql_type: arg.arg_type.to_graphql_string(),
                            column_type:  col_type.clone(),
                        });
                    }
                }
            }

            // L2 (#1571): a sorted or filtered temporal key read out of a document.
            if !jsonb_col.is_empty() && compares_document_keys(query) {
                let backing = if introspector.is_tview(source).await? {
                    TemporalBacking::Tview
                } else if relation_kind(&schema_qualified, &unqualified, source)
                    == Some(fraiseql_core::db::RelationKind::View)
                    || introspector.get_view_definition(source).await?.is_some()
                {
                    TemporalBacking::View
                } else {
                    TemporalBacking::Table
                };
                warnings.extend(temporal_keys_in_document(
                    schema,
                    query,
                    source,
                    &query_native,
                    &column_map,
                    backing,
                ));
            }

            if !query_native.is_empty() {
                native_columns.insert(query.name.clone(), query_native);
            }

            // L1: Check additional_views (relations, same resolution as sql_source).
            for view in &query.additional_views {
                if !relation_source_exists(introspector, &schema_qualified, &unqualified, view)
                    .await?
                {
                    warnings.push(DatabaseWarning::MissingAdditionalView {
                        query_name: query.name.clone(),
                        view_name:  view.clone(),
                    });
                }
            }
        }
    }

    // Validate mutations (L1 only). A mutation's `sql_source` is a FUNCTION, not a
    // relation — probe `pg_proc`, not the relation catalog (#485). Mirrors the
    // shared `fraiseql_core::schema::sql_source_probes` Relation/Function split.
    for mutation in &schema.mutations {
        if let Some(ref source) = mutation.sql_source {
            if !function_source_exists(introspector, source).await? {
                warnings.push(DatabaseWarning::MissingFunction {
                    mutation_name: mutation.name.clone(),
                    sql_source:    source.clone(),
                });
            }
        }
    }

    // Validate type sources (L1 only, warn-grade). A `@fraiseql.type`'s `sql_source` is a
    // view/table; a declared source that does not exist is usually an embedded value object
    // the SDK synthesized a source for (#653). Surface it at its origin — never failing —
    // instead of letting it resurface later as an unrelated cascade `id` error. Unlike
    // queries/mutations, type sources are not on the shared `sql_source_probes` work-list,
    // so the hard `FRAISEQL_VALIDATE_SQL_SOURCES` gate does not flag them; this is the only
    // check that does. Synthetic types (empty source) and error types are skipped.
    //
    // This set INTENTIONALLY DIVERGES from `is_queryable_entity` (the cascade classifier).
    // After #665 that predicate ALSO excludes framework-`internal` projections, but this
    // existence check must NOT: a missing `v_entity_change_log` / `v_transport_checkpoint`
    // is exactly #569's failure mode ("no install path; every mutation fails on a fresh
    // stack"), so validating the change-log views is precisely the point. Do NOT DRY the
    // two predicates together — the `internal` exemption belongs to cascade classification
    // only. The `validator_checks_internal_projection_view_existence` tripwire test pins
    // this divergence so a future "unification" cannot silently delete the #569 signal.
    for ty in &schema.types {
        let source = ty.sql_source.as_str();
        if ty.is_error || source.is_empty() {
            continue;
        }
        if !relation_source_exists(introspector, &schema_qualified, &unqualified, source).await? {
            warnings.push(DatabaseWarning::MissingTypeSource {
                type_name:  ty.name.to_string(),
                sql_source: source.to_string(),
            });
        }
    }

    check_temporal_casts(schema, introspector, &mut warnings).await?;
    unify_tview_index_advice(&mut warnings);

    Ok(DatabaseValidationReport {
        warnings,
        native_columns,
        pagination_orders,
    })
}

/// The views a read serves from: each query's relation and, with gRPC enabled, the row
/// view (`vr_<sql_source>`) of each type a query returns, which a gRPC read selects from.
fn served_views(schema: &CompiledSchema) -> std::collections::BTreeSet<String> {
    let grpc = schema.grpc_config.as_ref().is_some_and(|g| g.enabled);
    let mut views = std::collections::BTreeSet::new();
    for query in schema.queries.iter().filter(|q| q.sql_source_kind.is_relation()) {
        if let Some(source) = &query.sql_source {
            views.insert(source.clone());
        }
        if grpc {
            if let Some(ty) = schema.find_type(&query.return_type) {
                if !ty.sql_source.is_empty() {
                    views.insert(format!("vr_{}", ty.sql_source));
                }
            }
        }
    }
    views
}

/// Warn for each document key a served view reads as text cast to a temporal type
/// ([`DatabaseWarning::TemporalCastFromText`]).
///
/// The check reads each view's definition; a table, or a relation that does not exist, has
/// none.
async fn check_temporal_casts(
    schema: &CompiledSchema,
    introspector: &impl DatabaseIntrospector,
    warnings: &mut Vec<DatabaseWarning>,
) -> fraiseql_core::Result<()> {
    for view in served_views(schema) {
        let Some(body) = introspector.get_view_definition(&view).await? else {
            continue;
        };
        for (key, cast) in text_casts_to_temporal(&body) {
            warnings.push(DatabaseWarning::TemporalCastFromText {
                view: view.clone(),
                key,
                cast,
            });
        }
    }
    Ok(())
}

/// The document keys `view_body` reads as text and casts to a temporal type, each once,
/// with the type.
///
/// Deliberately lexical, over `pg_get_viewdef`'s deparsed form, which writes every such
/// cast one way: `(… ->> 'k'::text)::timestamp with time zone`, or `#>> '{a,k}'::text[]` for
/// a path. `CAST(… AS timestamptz)` and `(… ->> 'k')::text::timestamptz` deparse to it too.
fn text_casts_to_temporal(view_body: &str) -> Vec<(String, &'static str)> {
    static CAST: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"(?:->>|#>>) '((?:[^']|'')*)'::text(?:\[\])?\)::(timestamp with time zone|timestamp without time zone|time with time zone|time without time zone|date)\b",
        )
        .expect("a valid pattern")
    });
    let mut found: Vec<(String, &'static str)> = Vec::new();
    for capture in CAST.captures_iter(view_body) {
        let key = capture[1].replace("''", "'");
        let cast = match &capture[2] {
            "timestamp with time zone" => "timestamp with time zone",
            "timestamp without time zone" => "timestamp without time zone",
            "time with time zone" => "time with time zone",
            "time without time zone" => "time without time zone",
            _ => "date",
        };
        if !found.iter().any(|(k, c)| *k == key && *c == cast) {
            found.push((key, cast));
        }
    }
    found
}

/// Give every [`TviewIndexMissing`](DatabaseWarning::TviewIndexMissing) of one TVIEW the
/// options with every index advised for it (#1571): a replace's options are its whole
/// declaration, so applying one query's statement after another's would drop the first's
/// index. Each statement is then the same declaration, whichever is applied, in any order.
fn unify_tview_index_advice(warnings: &mut [DatabaseWarning]) {
    let mut by_tview: HashMap<String, serde_json::Value> = HashMap::new();
    for warning in warnings.iter() {
        let DatabaseWarning::TviewIndexMissing { tview, options, .. } = warning else {
            continue;
        };
        let advised = options.get("indexes").and_then(serde_json::Value::as_array);
        match by_tview.get_mut(tview) {
            None => {
                by_tview.insert(tview.clone(), options.clone());
            },
            Some(merged) => {
                let Some(indexes) = merged.get_mut("indexes").and_then(|i| i.as_array_mut()) else {
                    continue;
                };
                for index in advised.into_iter().flatten() {
                    if !indexes.contains(index) {
                        indexes.push(index.clone());
                    }
                }
            },
        }
    }
    for warning in warnings.iter_mut() {
        if let DatabaseWarning::TviewIndexMissing { tview, options, .. } = warning {
            if let Some(merged) = by_tview.get(tview) {
                options.clone_from(merged);
            }
        }
    }
}

/// The `DateTime`, `Date` and `Time` fields `query` sorts or filters while reading them out of
/// `source`'s document, with no native column mapped for them (#1571).
///
/// Every relation: the cast is unindexable whatever reads it. The caller asks only for a query
/// that [`compares_document_keys`]: one that does not never compares the key. The remedy
/// depends on what
/// `source` is: a TVIEW (`is_tview`, from `pg_tviews`' catalog, never from a name), a view or a
/// table.
fn temporal_keys_in_document(
    schema: &CompiledSchema,
    query: &fraiseql_core::schema::QueryDefinition,
    source: &str,
    query_native: &HashMap<String, fraiseql_core::schema::NativeColumn>,
    column_map: &HashMap<String, String>,
    backing: TemporalBacking,
) -> Vec<DatabaseWarning> {
    let Some(ty) = schema.find_type(&query.return_type) else {
        return Vec::new();
    };
    ty.fields
        .iter()
        .filter(|f| {
            matches!(
                f.field_type,
                FieldType::DateTime | FieldType::LocalDateTime | FieldType::Date | FieldType::Time
            )
        })
        .filter(|f| !query_native.contains_key(f.name.as_str()))
        .map(|f| {
            let column = fraiseql_core::utils::to_snake_case(f.name.as_str());
            let remedy = match backing {
                _ if column_map.contains_key(&column) => TemporalKeyRemedy::MapColumn,
                TemporalBacking::Tview => TemporalKeyRemedy::AddToTview,
                TemporalBacking::View => TemporalKeyRemedy::ExposeFromView,
                TemporalBacking::Table => TemporalKeyRemedy::AddToTable,
            };
            DatabaseWarning::TemporalKeyInDocument {
                query_name: query.name.clone(),
                sql_source: source.to_string(),
                field: f.name.to_string(),
                column,
                remedy,
            }
        })
        .collect()
}

/// Whether `query` compares its document's keys: it exposes `orderBy` or `where`.
const fn compares_document_keys(query: &fraiseql_core::schema::QueryDefinition) -> bool {
    query.auto_params.has_order_by || query.auto_params.has_where
}

/// What a query's `sql_source` is, for [`temporal_keys_in_document`]'s advice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemporalBacking {
    /// A table `pg_tviews` maintains.
    Tview,
    /// A view.
    View,
    /// Any other table.
    Table,
}

/// The kind of `source` among `relations`: by schema when it is qualified, else by the
/// first schema the search path lists it in.
fn relation_kind(
    schema_qualified: &HashMap<(String, String), RelationInfo>,
    unqualified: &HashMap<String, Vec<String>>,
    source: &str,
) -> Option<fraiseql_core::db::RelationKind> {
    let (schema, name) = match source.split_once('.') {
        Some((schema, name)) => (schema.to_string(), name),
        None => (unqualified.get(source)?.first()?.clone(), source),
    };
    schema_qualified.get(&(schema, name.to_string())).map(|r| r.kind.clone())
}

/// The index of `declaration`'s TVIEW that `filters` (the document keys `query` reads with
/// `data->>'key'` on every call) need, unless one it declares leads with them (#1571).
///
/// Each key as FraiseQL compares it, which is how `pg_tviews` writes the index: an argument's
/// JSON number is compared as `numeric`, a boolean as `boolean`, anything else (an inject
/// parameter included) as text.
fn tview_index_advice(
    query: &fraiseql_core::schema::QueryDefinition,
    declaration: &fraiseql_db::TviewDeclaration,
    filters: &[(QueryFilter, String)],
) -> Option<DatabaseWarning> {
    if filters.is_empty() {
        return None;
    }
    let keys: Vec<serde_json::Value> = filters
        .iter()
        .map(|(filter, key)| {
            let arg_type = match filter {
                QueryFilter::Argument(name) => {
                    query.arguments.iter().find(|a| &a.name == name).map(|a| &a.arg_type)
                },
                QueryFilter::Inject(_) => None,
            };
            match arg_type {
                Some(FieldType::Int | FieldType::BigInt | FieldType::Float) => {
                    serde_json::json!({ "path": key, "type": "numeric" })
                },
                Some(FieldType::Boolean) => serde_json::json!({ "path": key, "type": "boolean" }),
                _ => serde_json::json!(key),
            }
        })
        .collect();
    let declared: Vec<serde_json::Value> = declaration
        .options
        .get("indexes")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if declared.iter().any(|index| index_leads_with(index, &keys)) {
        return None;
    }
    let mut options = declaration.options.clone();
    let mut indexes = declared;
    indexes.push(serde_json::Value::Array(keys));
    options
        .as_object_mut()?
        .insert("indexes".to_string(), serde_json::Value::Array(indexes));
    Some(DatabaseWarning::TviewIndexMissing {
        query_name: query.name.clone(),
        tview: declaration.tview.clone(),
        keys: filters.iter().map(|(_, key)| key.clone()).collect(),
        options,
    })
}

/// Whether the `pg_tviews` index `index` (an array of keys) opens with `keys`: a key is a
/// path, or `{"path", "type"}` with `text` the default type; its direction does not matter
/// to an equality.
fn index_leads_with(index: &serde_json::Value, keys: &[serde_json::Value]) -> bool {
    fn key(value: &serde_json::Value) -> Option<(&str, &str)> {
        match value {
            serde_json::Value::String(path) => Some((path, "text")),
            serde_json::Value::Object(key) => Some((
                key.get("path")?.as_str()?,
                key.get("type").and_then(serde_json::Value::as_str).unwrap_or("text"),
            )),
            _ => None,
        }
    }
    let Some(index) = index.as_array() else {
        return false;
    };
    index.len() >= keys.len()
        && index.iter().zip(keys).all(|(a, b)| key(a).is_some_and(|a| Some(a) == key(b)))
}

/// The Trinity primary-key column a type's relation is expected to expose.
///
/// The same derivation `relay_cursor_column` uses (`User` → `pk_user`), and
/// deliberately so: when a relation carries it, both pagination families cut
/// their pages on the same column, which is ADR-0017's "one identity, consumed
/// uniformly, not re-derived per subsystem".
fn pk_column_name(return_type: &str) -> String {
    format!("pk_{}", fraiseql_core::utils::to_snake_case(return_type))
}

/// Does this view body carry its own `ORDER BY`?
///
/// Deliberately lexical, and deliberately not exact. `pg_get_viewdef` returns
/// normalised SQL, so `ORDER BY` appears upper-cased and space-separated when it
/// appears at all — but it appears for a window frame and a subquery too, where a
/// page ordering on the outer read is entirely correct. The finding this drives is
/// therefore advisory and says what it saw; a stricter parser would trade a
/// question the author can answer in a second for a dependency on parsing SQL.
fn orders_itself(view_body: &str) -> bool {
    view_body.contains("ORDER BY")
}

/// Build lookup maps from the list of relations.
fn build_relation_maps(
    relations: &[RelationInfo],
) -> (HashMap<(String, String), RelationInfo>, HashMap<String, Vec<String>>) {
    let mut schema_qualified = HashMap::new();
    let mut unqualified: HashMap<String, Vec<String>> = HashMap::new();

    for rel in relations {
        schema_qualified.insert((rel.schema.clone(), rel.name.clone()), rel.clone());
        unqualified.entry(rel.name.clone()).or_default().push(rel.schema.clone());
    }

    (schema_qualified, unqualified)
}

/// Find every declared `sql_source` that is **not backed** by a live database
/// object, over the shared [`sql_source_probes`] work-list (#487).
///
/// Resolution mirrors the runtime exactly: a query relation by `to_regclass`
/// (qualified, verbatim) or the search_path-scoped relation map (bare); a mutation
/// function via `pg_proc`. Returns the unbacked probes in declaration order — an
/// empty vec means every source is backed. This is the existence gate the
/// `validate --against-db` CLI and the opt-in server boot check share, so the two
/// agree on "backed" by construction.
///
/// When the connector cannot probe a kind (non-Postgres), that source is treated
/// as backed (skipped) rather than false-failed.
///
/// # Errors
///
/// Returns an error if database introspection fails.
pub async fn find_unbacked_sources(
    schema: &CompiledSchema,
    introspector: &impl DatabaseIntrospector,
) -> fraiseql_core::Result<Vec<SourceProbe>> {
    let relations = introspector.list_relations().await?;
    let (schema_qualified, unqualified) = build_relation_maps(&relations);

    let mut unbacked = Vec::new();
    for probe in sql_source_probes(schema) {
        let exists = match probe.kind {
            SourceKind::Function => introspector
                .function_exists(probe.schema.as_deref(), &probe.name)
                .await?
                .unwrap_or(true),
            SourceKind::Relation => {
                if let Some(s) = &probe.schema {
                    match introspector.qualified_relation_exists(s, &probe.name).await? {
                        Some(exists) => exists,
                        None => schema_qualified.contains_key(&(s.clone(), probe.name.clone())),
                    }
                } else {
                    unqualified.contains_key(&probe.name)
                }
            },
        };
        if !exists {
            unbacked.push(probe);
        }
    }
    Ok(unbacked)
}

/// Validate JSON keys in sampled data for L3 checking.
async fn validate_json_keys(
    schema: &CompiledSchema,
    query: &fraiseql_core::schema::QueryDefinition,
    source: &str,
    jsonb_col: &str,
    introspector: &impl DatabaseIntrospector,
    table_name: &str,
    warnings: &mut Vec<DatabaseWarning>,
) -> fraiseql_core::Result<()> {
    let samples = introspector.get_sample_json_rows(table_name, jsonb_col, 5).await?;

    if samples.is_empty() {
        return Ok(());
    }

    // Merge all top-level keys from sampled rows
    let mut all_keys = HashSet::new();
    for sample in &samples {
        if let serde_json::Value::Object(map) = sample {
            for key in map.keys() {
                all_keys.insert(key.clone());
            }
        }
    }

    if all_keys.is_empty() {
        return Ok(());
    }

    // Find the type definition for this query's return type
    let type_def = schema.types.iter().find(|t| t.name.as_str() == query.return_type);

    if let Some(type_def) = type_def {
        for field in &type_def.fields {
            let field_str = field.name.as_str();
            // Use the canonical acronym/digit-aware caser the runtime queries with
            // (`httpResponse` → `http_response`, not `h_t_t_p_response`), so the
            // L3 JSON-key check never cries wolf on acronym/digit fields (#485).
            let json_key = fraiseql_core::utils::to_snake_case(field_str);
            // Skip fields that are top-level columns (not from JSONB)
            // Convention: fields like "id", "pk_*", "fk_*" are columns, not JSON keys
            if field_str == "id" || field_str.starts_with("pk_") || field_str.starts_with("fk_") {
                continue;
            }
            if !all_keys.contains(&json_key) && !all_keys.contains(field_str) {
                warnings.push(DatabaseWarning::MissingJsonKey {
                    query_name: query.name.clone(),
                    type_name: type_def.name.as_str().to_string(),
                    sql_source: source.to_string(),
                    json_column: jsonb_col.to_string(),
                    field_name: field_str.to_string(),
                    json_key,
                    field_required: !field.nullable,
                });
            }
        }
    }

    Ok(())
}

/// Enum dispatch for database introspectors.
///
/// Uses enum dispatch instead of `Box<dyn DatabaseIntrospector>` because the
/// trait uses `async_fn_in_trait` and cannot be object-safe.
pub enum AnyIntrospector {
    /// `PostgreSQL` introspector.
    Postgres(fraiseql_core::db::PostgresIntrospector),
}

impl DatabaseIntrospector for AnyIntrospector {
    async fn list_fact_tables(&self) -> fraiseql_core::Result<Vec<String>> {
        match self {
            Self::Postgres(i) => i.list_fact_tables().await,
        }
    }

    async fn get_columns(
        &self,
        table_name: &str,
    ) -> fraiseql_core::Result<Vec<(String, String, bool)>> {
        match self {
            Self::Postgres(i) => i.get_columns(table_name).await,
        }
    }

    async fn get_indexed_columns(&self, table_name: &str) -> fraiseql_core::Result<Vec<String>> {
        match self {
            Self::Postgres(i) => i.get_indexed_columns(table_name).await,
        }
    }

    fn database_type(&self) -> DatabaseType {
        match self {
            Self::Postgres(i) => i.database_type(),
        }
    }

    async fn get_sample_jsonb(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> fraiseql_core::Result<Option<serde_json::Value>> {
        match self {
            Self::Postgres(i) => i.get_sample_jsonb(table_name, column_name).await,
        }
    }

    async fn list_relations(&self) -> fraiseql_core::Result<Vec<fraiseql_core::db::RelationInfo>> {
        match self {
            Self::Postgres(i) => i.list_relations().await,
        }
    }

    async fn get_sample_json_rows(
        &self,
        table_name: &str,
        column_name: &str,
        limit: usize,
    ) -> fraiseql_core::Result<Vec<serde_json::Value>> {
        match self {
            Self::Postgres(i) => i.get_sample_json_rows(table_name, column_name, limit).await,
        }
    }

    async fn column_key_disagreements(
        &self,
        relation: &str,
        column: &str,
        json_column: &str,
        key: &str,
        limit: usize,
    ) -> fraiseql_core::Result<Option<(u64, u64)>> {
        match self {
            Self::Postgres(i) => {
                i.column_key_disagreements(relation, column, json_column, key, limit).await
            },
        }
    }

    async fn sample_documents(
        &self,
        relation: &str,
        json_column: &str,
        columns: &[&str],
        limit: usize,
    ) -> fraiseql_core::Result<Option<Vec<(serde_json::Value, Vec<serde_json::Value>)>>> {
        match self {
            Self::Postgres(i) => i.sample_documents(relation, json_column, columns, limit).await,
        }
    }

    async fn function_exists(
        &self,
        schema: Option<&str>,
        name: &str,
    ) -> fraiseql_core::Result<Option<bool>> {
        // Must delegate (not inherit the trait default) so the Postgres impl is
        // actually used — otherwise every variant would return `None`.
        match self {
            Self::Postgres(i) => i.function_exists(schema, name).await,
        }
    }

    async fn qualified_relation_exists(
        &self,
        schema: &str,
        name: &str,
    ) -> fraiseql_core::Result<Option<bool>> {
        match self {
            Self::Postgres(i) => i.qualified_relation_exists(schema, name).await,
        }
    }

    async fn get_view_definition(&self, relation: &str) -> fraiseql_core::Result<Option<String>> {
        match self {
            Self::Postgres(i) => i.get_view_definition(relation).await,
        }
    }

    async fn tview_declaration(
        &self,
        relation: &str,
    ) -> fraiseql_core::Result<Option<fraiseql_db::TviewDeclaration>> {
        match self {
            Self::Postgres(i) => i.tview_declaration(relation).await,
        }
    }

    async fn is_tview(&self, relation: &str) -> fraiseql_core::Result<bool> {
        match self {
            Self::Postgres(i) => i.is_tview(relation).await,
        }
    }
}

/// Refuse a database URL that targets an engine whose support was removed (#374).
///
/// The first half of [`crate::connection::require_postgres`], the one rule for every
/// command that takes a database URL, run before anything tries to connect. Without it
/// `compile --database sqlite://…` failed inside the connection pool with "invalid
/// connection string".
///
/// # Errors
///
/// Returns an error naming the PostgreSQL-only rule for a MySQL / MariaDB / SQLite /
/// SQL Server URL or a SQLite database file path.
pub fn refuse_removed_engine_url(db_url: &str) -> anyhow::Result<()> {
    let removed_engine = db_url.starts_with("mysql")
        || db_url.starts_with("mariadb")
        || db_url.starts_with("sqlite")
        || db_url.starts_with("mssql")
        || db_url.starts_with("server=")
        || std::path::Path::new(db_url).extension().is_some_and(|ext| {
            ext.eq_ignore_ascii_case("db") || ext.eq_ignore_ascii_case("sqlite")
        });
    if removed_engine {
        anyhow::bail!(
            "This database URL targets an engine whose support was removed: FraiseQL is \
             PostgreSQL-only (G2 decision on #374; the non-PostgreSQL dialects were never \
             production-correct, see #721/#799). Use a postgresql:// URL."
        );
    }
    Ok(())
}

/// Create an introspector from a database URL.
///
/// # Errors
///
/// Returns error if the URL is not a PostgreSQL connection string (see
/// [`crate::connection::require_postgres`]), the server cannot be reached, or it is
/// older than PostgreSQL 18.
pub async fn create_introspector(
    db_url: &str,
    tls: &PostgresTlsConfig,
) -> anyhow::Result<AnyIntrospector> {
    let pool = crate::connection::postgres_pool(db_url, "schema introspection", tls).await?;
    Ok(AnyIntrospector::Postgres(fraiseql_core::db::PostgresIntrospector::new(pool)))
}

/// Report every query filter the relation cannot provide (#1382, #1394).
///
/// `unresolved` holds the filters that named no column, each with the JSON key the runtime
/// reads for it. That key is only sound if the JSON carries it: sampled rows that all lack
/// it are evidence it never does, and the filter is an error. One the JSON does carry is
/// only a slower path — an argument there keeps its `NativeColumnFallback` advisory. An
/// empty relation proves nothing and is reported as advisory.
async fn check_filters_resolvable(
    query: &fraiseql_core::schema::QueryDefinition,
    source: &str,
    unresolved: Vec<(QueryFilter, String)>,
    column_map: &HashMap<String, String>,
    db_type: DatabaseType,
    introspector: &impl DatabaseIntrospector,
    warnings: &mut Vec<DatabaseWarning>,
) -> fraiseql_core::Result<()> {
    if unresolved.is_empty() {
        return Ok(());
    }
    let jsonb_col = &query.jsonb_column;
    let json_ok = !jsonb_col.is_empty()
        && column_map.get(jsonb_col).is_some_and(|t| is_json_type(t, db_type));
    let carried: Option<HashSet<String>> = if json_ok {
        let samples =
            introspector.get_sample_json_rows(source, jsonb_col, INJECT_SAMPLE_ROWS).await?;
        if samples.is_empty() {
            None
        } else {
            Some(
                samples
                    .iter()
                    .filter_map(serde_json::Value::as_object)
                    .flat_map(|row| row.keys().cloned())
                    .collect(),
            )
        }
    } else {
        None
    };
    for (filter, key) in unresolved {
        let reason = match (&carried, json_ok) {
            (_, false) => InjectMiss::NoJsonColumn,
            (None, true) => InjectMiss::EmptyRelation {
                json_column: jsonb_col.clone(),
            },
            (Some(keys), true) if !keys.contains(&key) => InjectMiss::NotInSampledRows {
                json_column: jsonb_col.clone(),
            },
            (Some(_), true) => {
                if let QueryFilter::Argument(arg_name) = filter {
                    warnings.push(DatabaseWarning::NativeColumnFallback {
                        query_name: query.name.clone(),
                        sql_source: source.to_string(),
                        arg_name,
                    });
                }
                continue;
            },
        };
        warnings.push(DatabaseWarning::UnresolvableFilter {
            query_name: query.name.clone(),
            sql_source: source.to_string(),
            filter,
            key,
            reason,
        });
    }
    Ok(())
}

/// Report every `inject_params` column of `query` that is a native column of `source`, is a
/// key its type publishes, and holds another value than the document's key in a sampled row
/// (#1579). The nested row gates read such a key off an embedded document; the root read
/// reads the column.
async fn check_native_keys_agree(
    schema: &fraiseql_core::schema::CompiledSchema,
    query: &fraiseql_core::schema::QueryDefinition,
    source: &str,
    query_native: &HashMap<String, fraiseql_core::schema::NativeColumn>,
    introspector: &impl DatabaseIntrospector,
    warnings: &mut Vec<DatabaseWarning>,
) -> fraiseql_core::Result<()> {
    let Some(declared) = schema.find_type(&query.return_type) else {
        return Ok(());
    };
    for column in query.inject_params.keys().filter(|c| query_native.contains_key(*c)) {
        let key = fraiseql_core::utils::to_snake_case(column);
        let published = declared
            .fields
            .iter()
            .any(|f| fraiseql_core::utils::to_snake_case(f.name.as_str()) == key);
        if !published {
            continue;
        }
        let compared = introspector
            .column_key_disagreements(
                source,
                column,
                &query.jsonb_column,
                &key,
                AGREEMENT_SAMPLE_ROWS,
            )
            .await?;
        if let Some((sampled, disagreeing)) = compared.filter(|(_, d)| *d > 0) {
            warnings.push(DatabaseWarning::NativeKeyDisagreement {
                query_name: query.name.clone(),
                sql_source: source.to_string(),
                column: column.clone(),
                json_column: query.jsonb_column.clone(),
                sampled,
                disagreeing,
            });
        }
    }
    Ok(())
}

/// Sample `source`, a view of `root`'s documents, and report each `scoped_like_parent` route
/// whose embedded documents lack the scope key or hold another value than their row.
///
/// The nested level is filtered as the runtime filters it: on the key `to_snake_case(column)`
/// of each embedded document, compared as JSON with the caller's value. The holding row is
/// filtered at the root on its native column when the view has one, otherwise on its own
/// document's key; a row with no value there is served to nobody and is skipped.
async fn check_embedded_scope_keys(
    root: &fraiseql_core::schema::ScopedEmbeddingRoot<'_>,
    query: &fraiseql_core::schema::QueryDefinition,
    source: &str,
    column_map: &HashMap<String, String>,
    introspector: &impl DatabaseIntrospector,
    warnings: &mut Vec<DatabaseWarning>,
) -> fraiseql_core::Result<()> {
    let mut native: Vec<&str> = Vec::new();
    for route in &root.routes {
        for column in &route.columns {
            if column_map.contains_key(*column) && !native.contains(column) {
                native.push(column);
            }
        }
    }
    let Some(rows) = introspector
        .sample_documents(source, &query.jsonb_column, &native, AGREEMENT_SAMPLE_ROWS)
        .await?
    else {
        return Ok(());
    };
    if rows.is_empty() {
        warnings.push(DatabaseWarning::EmbeddedScopeEmptyRelation {
            query_name: query.name.clone(),
            sql_source: source.to_string(),
            root:       root.root.to_string(),
        });
        return Ok(());
    }

    for route in &root.routes {
        for column in &route.columns {
            let key = fraiseql_core::utils::to_snake_case(column);
            let position = native.iter().position(|n| n == column);
            let tally =
                tally_embedded_keys(
                    &rows,
                    &route.path,
                    &key,
                    |(document, values)| match position {
                        Some(at) => values.get(at),
                        None => document.get(&key),
                    },
                );
            let route_name = route.path.join(".");
            if tally.missing > 0 {
                warnings.push(DatabaseWarning::EmbeddedScopeKeyMissing {
                    query_name: query.name.clone(),
                    sql_source: source.to_string(),
                    owner:      route.owner.to_string(),
                    field:      route.field.to_string(),
                    embedded:   route.embedded.to_string(),
                    route:      route_name.clone(),
                    key:        key.clone(),
                    documents:  tally.documents,
                    missing:    tally.missing,
                });
            }
            if tally.disagreeing > 0 {
                warnings.push(DatabaseWarning::EmbeddedScopeKeyDisagreement {
                    query_name:     query.name.clone(),
                    sql_source:     source.to_string(),
                    owner:          route.owner.to_string(),
                    field:          route.field.to_string(),
                    embedded:       route.embedded.to_string(),
                    route:          route_name,
                    key:            key.clone(),
                    holding_native: position.is_some(),
                    holding_column: if position.is_some() {
                        (*column).to_string()
                    } else {
                        key.clone()
                    },
                    documents:      tally.documents,
                    disagreeing:    tally.disagreeing,
                });
            }
        }
    }
    Ok(())
}

/// What the documents embedded at one route hold against their rows.
#[derive(Debug, Default, PartialEq, Eq)]
struct EmbeddedKeyTally {
    /// Embedded documents found in rows that hold a value.
    documents:   u64,
    /// Those without `key`, or with `key` `null`: `null` meets nothing at run time.
    missing:     u64,
    /// Those whose `key` is another JSON value than the row's.
    disagreeing: u64,
}

/// Count, over `rows`, the documents embedded at `path` whose `key` is missing or differs
/// from the holding row's value, which `holding` reads off a row. A row without a value is
/// served to nobody and is skipped.
fn tally_embedded_keys<'r>(
    rows: &'r [(serde_json::Value, Vec<serde_json::Value>)],
    path: &[&str],
    key: &str,
    holding: impl Fn(&'r (serde_json::Value, Vec<serde_json::Value>)) -> Option<&'r serde_json::Value>,
) -> EmbeddedKeyTally {
    let mut tally = EmbeddedKeyTally::default();
    for row in rows {
        let Some(expected) = holding(row).filter(|v| !v.is_null()) else {
            continue;
        };
        for embedded in embedded_documents(&row.0, path) {
            tally.documents += 1;
            match embedded.get(key) {
                None | Some(serde_json::Value::Null) => tally.missing += 1,
                Some(actual) if actual != expected => tally.disagreeing += 1,
                Some(_) => {},
            }
        }
    }
    tally
}

/// The objects at `path` in `document`: each step read under the field's stored key
/// (`snake_case`, then `camelCase`) as the runtime reads an embedded level, through a list
/// at any step.
fn embedded_documents<'d>(
    document: &'d serde_json::Value,
    path: &[&str],
) -> Vec<&'d serde_json::Value> {
    let mut level = vec![document];
    for step in path {
        let (stored, fallback) = fraiseql_core::runtime::stored_key_candidates(step);
        level = level
            .into_iter()
            .flat_map(|at| match at {
                serde_json::Value::Array(items) => items.iter().collect(),
                other => vec![other],
            })
            .filter_map(|at| at.get(&stored).or_else(|| fallback.as_ref().and_then(|f| at.get(f))))
            .collect();
    }
    level
        .into_iter()
        .flat_map(|at| match at {
            serde_json::Value::Array(items) => items.iter().collect(),
            other => vec![other],
        })
        .filter(|at| at.is_object())
        .collect()
}

/// A temporal type's scalar name, the column type holding its values, and the names the
/// catalog gives that type.
fn temporal_column(
    field_type: &FieldType,
) -> Option<(&'static str, &'static str, &'static [&'static str])> {
    let base = match field_type {
        FieldType::List { item, .. } => item.as_ref(),
        other => other,
    };
    match base {
        FieldType::DateTime => {
            Some(("DateTime", "timestamptz", &["timestamp with time zone", "timestamptz"]))
        },
        FieldType::LocalDateTime => {
            Some(("LocalDateTime", "timestamp", &["timestamp without time zone", "timestamp"]))
        },
        FieldType::Date => Some(("Date", "date", &["date"])),
        FieldType::Time => Some(("Time", "time", &["time without time zone", "time"])),
        _ => None,
    }
}

/// A field reached from a served document whose stored values are checked: where its
/// documents are, and what its values are held to.
struct DocumentLeaf {
    route: Vec<String>,
    owner: String,
    field: String,
    check: ValueCheck,
}

/// What a sampled leaf's values are held to.
enum ValueCheck {
    /// A temporal scalar's format, which a `expected` column writes.
    Temporal {
        format:   fraiseql_db::types::temporal::Temporal,
        scalar:   &'static str,
        expected: &'static str,
    },
    /// An `Int`'s signed 32-bit range.
    Int,
}

impl ValueCheck {
    /// The check a field of type `base`, its list item type unwrapped, is held to.
    fn of(base: &FieldType) -> Option<Self> {
        if let (Some(format), Some((scalar, expected, _))) =
            (base.temporal(), temporal_column(base))
        {
            return Some(Self::Temporal {
                format,
                scalar,
                expected,
            });
        }
        (*base == FieldType::Int).then_some(Self::Int)
    }

    /// Whether `value`, a non-null stored value, is outside what the check holds it to. A
    /// value of another kind is the completion's concern, not this range's: an `Int` holding
    /// a string is not counted.
    fn refuses(&self, value: &serde_json::Value) -> bool {
        match (self, value) {
            (Self::Temporal { format, .. }, serde_json::Value::String(text)) => {
                format.refusal(text).is_some()
            },
            (Self::Temporal { .. }, _) => true,
            (Self::Int, serde_json::Value::Number(number)) => number
                .as_f64()
                .is_some_and(|n| n < f64::from(i32::MIN) || n > f64::from(i32::MAX)),
            (Self::Int, _) => false,
        }
    }
}

/// The checked fields of `type_name` and of the object types its document embeds, each with
/// the route to the documents holding it. A type already on the route is not entered again,
/// and a field the document does not hold as written (encrypted or localized) is skipped.
fn document_leaves(
    schema: &fraiseql_core::schema::CompiledSchema,
    type_name: &str,
    route: &mut Vec<String>,
    entered: &mut Vec<String>,
    leaves: &mut Vec<DocumentLeaf>,
) {
    let Some(declared) = schema.find_type(type_name) else {
        return;
    };
    entered.push(type_name.to_string());
    for field in &declared.fields {
        if field.encryption.is_some() || field.localized {
            continue;
        }
        let base = match &field.field_type {
            FieldType::List { item, .. } => item.as_ref(),
            other => other,
        };
        if let Some(check) = ValueCheck::of(base) {
            leaves.push(DocumentLeaf {
                route: route.clone(),
                owner: type_name.to_string(),
                field: field.name.to_string(),
                check,
            });
        } else if let FieldType::Object(embedded) = base {
            if !entered.contains(embedded) {
                route.push(field.name.to_string());
                document_leaves(schema, embedded, route, entered, leaves);
                route.pop();
            }
        }
    }
    entered.pop();
}

/// Sample `source`, the view of `query`'s documents, and report each checked field, at the
/// root or embedded, holding a value outside what it is held to: a temporal field outside
/// its scalar's format, an `Int` outside the signed 32-bit range. A finding counts documents
/// and never quotes a value.
async fn check_document_values(
    schema: &fraiseql_core::schema::CompiledSchema,
    query: &fraiseql_core::schema::QueryDefinition,
    source: &str,
    introspector: &impl DatabaseIntrospector,
    warnings: &mut Vec<DatabaseWarning>,
) -> fraiseql_core::Result<()> {
    let mut leaves = Vec::new();
    document_leaves(schema, &query.return_type, &mut Vec::new(), &mut Vec::new(), &mut leaves);
    if leaves.is_empty() {
        return Ok(());
    }
    let Some(rows) = introspector
        .sample_documents(source, &query.jsonb_column, &[], AGREEMENT_SAMPLE_ROWS)
        .await?
    else {
        return Ok(());
    };
    if rows.is_empty() {
        // An empty view proves nothing; only a temporal format is named as unchecked.
        if leaves.iter().any(|leaf| matches!(leaf.check, ValueCheck::Temporal { .. })) {
            warnings.push(DatabaseWarning::TemporalFormatsUnchecked {
                query_name: query.name.clone(),
                sql_source: source.to_string(),
            });
        }
        return Ok(());
    }
    for leaf in leaves {
        let path: Vec<&str> = leaf.route.iter().map(String::as_str).collect();
        let (stored, fallback) = fraiseql_core::runtime::stored_key_candidates(&leaf.field);
        let (mut documents, mut refused) = (0_u64, 0_u64);
        for (document, _) in &rows {
            for holder in embedded_documents(document, &path) {
                let value =
                    holder.get(&stored).or_else(|| fallback.as_ref().and_then(|f| holder.get(f)));
                let values: Vec<&serde_json::Value> = match value {
                    None | Some(serde_json::Value::Null) => continue,
                    Some(serde_json::Value::Array(items)) => items.iter().collect(),
                    Some(other) => vec![other],
                };
                documents += 1;
                if values.iter().any(|value| !value.is_null() && leaf.check.refuses(value)) {
                    refused += 1;
                }
            }
        }
        if refused == 0 {
            continue;
        }
        let (query_name, sql_source) = (query.name.clone(), source.to_string());
        let (owner, field, route) = (leaf.owner, leaf.field, leaf.route.join("."));
        warnings.push(match leaf.check {
            ValueCheck::Temporal {
                scalar, expected, ..
            } => DatabaseWarning::TemporalValueOutOfFormat {
                query_name,
                sql_source,
                owner,
                field,
                route,
                scalar,
                expected,
                documents,
                refused,
            },
            ValueCheck::Int => DatabaseWarning::IntValueOutOfRange {
                query_name,
                sql_source,
                owner,
                field,
                route,
                documents,
                refused,
            },
        });
    }
    Ok(())
}

/// Rows sampled to check a native column against its document key (#1579).
const AGREEMENT_SAMPLE_ROWS: usize = 1000;

/// Rows sampled to decide whether a filter key lives in the JSON column.
const INJECT_SAMPLE_ROWS: usize = 20;

/// Build a query's native-column map from its explicit (non-auto-param) arguments
/// and its inject-param names, consulting the introspected `column_map`.
///
/// A name that matches a real view column is rendered as a native-column predicate
/// (`WHERE col = $N`) at runtime; a name with no matching column falls back to the
/// JSONB path (`data->>'name'`). inject params were previously omitted here, so an
/// inject-scoped list query against a view that keeps e.g. `tenant_id` as a real
/// column (and not inside `data`) rendered `data->>'tenant_id'` and matched no rows.
///
/// Returns the native-column map plus the explicit-arg names that did NOT resolve to
/// a column — the caller emits a `NativeColumnFallback` warning for each. Inject-param
/// misses are intentionally silent: a claim may legitimately live in the `data` JSONB.
fn detect_query_native_columns<'a>(
    direct_arg_names: &[&str],
    inject_param_names: impl Iterator<Item = &'a str>,
    column_map: &HashMap<String, String>,
    not_null: &std::collections::HashSet<String>,
) -> (HashMap<String, fraiseql_core::schema::NativeColumn>, Vec<String>) {
    let native_column = |column: &str, col_type: &String| fraiseql_core::schema::NativeColumn {
        pg_type:  col_type.clone(),
        not_null: not_null.contains(column),
    };
    let mut native: HashMap<String, fraiseql_core::schema::NativeColumn> = HashMap::new();
    let mut arg_fallbacks: Vec<String> = Vec::new();

    for arg in direct_arg_names {
        // The runtime filters on the snake_case form of the argument's GraphQL name
        // (`combine_explicit_arg_where`, #486), so that is the column to look up (#1394):
        // `customerId` must find `customer_id`, never miss it and fall back to a key that
        // the view's `data` does not carry.
        let column = fraiseql_core::utils::to_snake_case(arg);
        if let Some(col_type) = column_map.get(&column) {
            native.insert((*arg).to_string(), native_column(&column, col_type));
        } else {
            arg_fallbacks.push((*arg).to_string());
        }
    }

    for name in inject_param_names {
        if let Some(col_type) = column_map.get(name) {
            native.insert(name.to_string(), native_column(name, col_type));
        }
    }

    (native, arg_fallbacks)
}

#[cfg(test)]
#[path = "database_validator_tests.rs"]
mod database_validator_tests;
