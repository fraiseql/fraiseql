//! The single, cross-crate definition of "what counts as a backed `sql_source`".
//!
//! A compiled schema declares, per operation, a `sql_source`: a **relation** (the
//! view/table a query reads) or a **function** (the SQL function a mutation calls).
//! Two separate consumers must agree on which database objects have to exist for a
//! schema to be servable:
//!
//! - `fraiseql-cli` — `compile --database` / `doctor` / `validate --against-db` (executes each
//!   probe through `pg_catalog`/the introspector).
//! - `fraiseql-server` — the opt-in fail-fast boot check (executes each probe through the live
//!   `DatabaseAdapter`).
//!
//! [`sql_source_probes`] turns a [`CompiledSchema`] into the work-list once, so the
//! CLI gate and the server boot check cannot drift on the definition of "backed".
//! Each side runs the list with its own connector.

use crate::{
    db::{DatabaseAdapter, quote_postgres_identifier},
    error::{FraiseQLError, Result},
    schema::{CompiledSchema, MutationOperation},
};

/// Whether a `sql_source` names a relation or a function. They are resolved
/// differently: a relation via `to_regclass` / the relation catalog, a function via
/// `pg_proc`.
///
/// A mutation's source is always a function. A query's is a relation unless it declares
/// `sql_source_kind: "function"`: a `STABLE` set-returning function the query's
/// arguments bind to, read like the relation it returns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// A table / view / materialized view a query reads from.
    #[default]
    Relation,
    /// A SQL function a mutation calls, or a function-backed query reads from.
    Function,
}

impl SourceKind {
    /// Whether this is [`SourceKind::Relation`], the default a compiled query omits.
    #[must_use]
    pub const fn is_relation(&self) -> bool {
        matches!(self, Self::Relation)
    }
}

/// One database object a schema declares it depends on, parsed from a `sql_source`.
///
/// The identifier is kept **verbatim** (case-sensitive): the runtime resolves it
/// through `quote_postgres_identifier`, so a probe must too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProbe {
    /// Explicit schema qualifier (`events` in `events.v_log`), or `None` for a
    /// bare name resolved against the connection `search_path`.
    pub schema: Option<String>,
    /// The relation or function name, verbatim (no case-folding).
    pub name:   String,
    /// Whether to resolve `name` as a relation or a function.
    pub kind:   SourceKind,
}

impl SourceProbe {
    /// Render the probe as a (possibly schema-qualified) identifier — the form a
    /// human-facing diagnostic shows (e.g. `events.v_log`, `app.create_order`).
    #[must_use]
    pub fn display_name(&self) -> String {
        match &self.schema {
            Some(s) => format!("{s}.{}", self.name),
            None => self.name.clone(),
        }
    }
}

/// Split a possibly schema-qualified `sql_source` into `(schema?, name)`.
///
/// Naive `split_once('.')`, matching the two existing splitters in `fraiseql-cli`
/// (`database_validator::split_schema_qualified` and `pg_catalog::split_qualified`)
/// so all three agree. A quoted identifier containing a literal dot is out of
/// scope — bare/dotted names only, kept verbatim.
fn split_source(sql_source: &str) -> (Option<String>, String) {
    match sql_source.split_once('.') {
        Some((schema, name)) => (Some(schema.to_string()), name.to_string()),
        None => (None, sql_source.to_string()),
    }
}

/// A `sql_source` relation name as the body of a SQL string literal for `to_regclass`:
/// each part quoted verbatim (no case-folding, as the runtime resolves it), then the
/// literal's own quotes doubled. `to_regclass('{literal}')` resolves it against the
/// connection's `search_path` when it is unqualified.
#[must_use]
pub fn regclass_literal(sql_source: &str) -> String {
    let ident = match split_source(sql_source) {
        (Some(schema), name) => {
            format!("{}.{}", quote_postgres_identifier(&schema), quote_postgres_identifier(&name))
        },
        (None, name) => quote_postgres_identifier(&name),
    };
    ident.replace('\'', "''")
}

/// Resolve a mutation's backing function name: its explicit `sql_source`, else the
/// operation's non-empty table. `None` ⇒ not SQL-backed (federation / `Custom`
/// without a table) and therefore not probed. Mirrors the `#397` mutation-contract
/// `resolve_sql_source`.
const fn mutation_source(mutation: &crate::schema::MutationDefinition) -> Option<&str> {
    if let Some(src) = &mutation.sql_source {
        return Some(src.as_str());
    }
    match &mutation.operation {
        MutationOperation::Insert { table }
        | MutationOperation::Update { table }
        | MutationOperation::Delete { table }
            if !table.is_empty() =>
        {
            Some(table.as_str())
        },
        _ => None,
    }
}

/// Build the work-list of database objects a compiled schema must be backed by.
///
/// Queries contribute a [`SourceKind::Relation`] probe (their `sql_source` view) or, for a
/// function root, a [`SourceKind::Function`] one; mutations a [`SourceKind::Function`]
/// probe (their `sql_source`, or operation
/// table). Operations with no SQL source (federation / non-SQL) are skipped — they
/// have nothing to probe. This is the single definition both the CLI existence
/// gate (#485) and the server fail-fast boot check (#487) consume.
#[must_use]
pub fn sql_source_probes(schema: &CompiledSchema) -> Vec<SourceProbe> {
    let mut probes = Vec::with_capacity(schema.queries.len() + schema.mutations.len());

    for query in &schema.queries {
        if let Some(source) = &query.sql_source {
            let (schema_part, name) = split_source(source);
            probes.push(SourceProbe {
                schema: schema_part,
                name,
                kind: query.sql_source_kind,
            });
        }
    }

    for mutation in &schema.mutations {
        if let Some(source) = mutation_source(mutation) {
            let (schema_part, name) = split_source(source);
            probes.push(SourceProbe {
                schema: schema_part,
                name,
                kind: SourceKind::Function,
            });
        }
    }

    probes
}

/// Refuse a schema whose sources a hot standby cannot read, when reads go to replicas
/// (#1390).
///
/// PostgreSQL refuses to read an UNLOGGED or temporary relation during recovery
/// ("cannot access temporary or unlogged relations during recovery"), so with read
/// replicas configured every query over such a source fails on every replica. A `pg_tviews`
/// TVIEW is UNLOGGED when its `logged` option is false. Views are followed to the relations
/// they read (a logged view over an unlogged table fails the same way); materialized views
/// are not, because they store their own rows.
///
/// # Errors
///
/// `FraiseQLError::Configuration` naming each source and the unlogged relation it
/// depends on, and the statement that makes each UNLOGGED table LOGGED.
pub async fn refuse_standby_unreadable_sources<A: DatabaseAdapter + ?Sized>(
    adapter: &A,
    schema: &CompiledSchema,
) -> Result<()> {
    if !adapter.serves_reads_from_standbys() {
        return Ok(());
    }
    let unreadable = unreadable_dependencies(adapter, schema).await?;
    if unreadable.is_empty() {
        return Ok(());
    }
    let statements = logged_statements(&unreadable);
    Err(FraiseQLError::Configuration {
        message: format!(
            "Read replicas are configured, but {} source relation(s) of this schema depend \
             on an UNLOGGED or temporary table, which a hot standby cannot read: every query \
             over them would fail on every replica.\n  - {}\nMake those tables LOGGED, at the \
             cost of WAL for every write to them, or remove `read_replica_urls`:\n  {}\nA \
             pg_tviews TVIEW is LOGGED unless its `logged` option is false; `SET LOGGED` also \
             sets the option. A temporary table cannot be read from a replica at all.",
            unreadable.len(),
            unreadable.iter().map(Unreadable::described).collect::<Vec<_>>().join("\n  - "),
            statements.join("\n  ")
        ),
    })
}

/// A source and an UNLOGGED or temporary relation it depends on.
struct Unreadable {
    /// The source, as the schema names it.
    source:    String,
    /// The relation, as the server's `search_path` would name it.
    relation:  String,
    /// The relation, schema-qualified and quoted.
    qualified: String,
    /// Temporary rather than UNLOGGED.
    temporary: bool,
}

impl Unreadable {
    fn described(&self) -> String {
        format!("{} — reads {}", self.source, self.relation)
    }
}

/// The statement that makes each UNLOGGED table of `unreadable` LOGGED, once per table,
/// by name. A temporary table has none: no statement makes a replica read it.
fn logged_statements(unreadable: &[Unreadable]) -> Vec<String> {
    let mut statements: Vec<String> = unreadable
        .iter()
        .filter(|d| !d.temporary)
        .map(|d| format!("ALTER TABLE {} SET LOGGED;", d.qualified))
        .collect();
    // Several sources can read one table.
    statements.sort();
    statements.dedup();
    statements
}

/// The relation sources of `schema` a hot standby cannot read (#1390).
///
/// Each is reported as `<source> — reads <table>`, for every UNLOGGED or temporary table
/// it depends on. Read from the primary's catalog, so it answers whether or not replicas
/// are configured.
///
/// # Errors
///
/// `FraiseQLError::Database` if the catalog query fails.
pub async fn standby_unreadable_sources<A: DatabaseAdapter + ?Sized>(
    adapter: &A,
    schema: &CompiledSchema,
) -> Result<Vec<String>> {
    Ok(unreadable_dependencies(adapter, schema)
        .await?
        .iter()
        .map(Unreadable::described)
        .collect())
}

/// Every `(source, UNLOGGED or temporary relation)` pair of `schema`, sources in schema
/// order, relations by name.
async fn unreadable_dependencies<A: DatabaseAdapter + ?Sized>(
    adapter: &A,
    schema: &CompiledSchema,
) -> Result<Vec<Unreadable>> {
    let mut unreadable = Vec::new();
    for probe in sql_source_probes(schema) {
        if probe.kind != SourceKind::Relation {
            continue;
        }
        let literal = regclass_literal(&probe.display_name());
        let sql = format!(
            "WITH RECURSIVE rel(oid, kind) AS ( \
               SELECT c.oid, c.relkind FROM pg_class c WHERE c.oid = to_regclass('{literal}') \
               UNION \
               SELECT base.oid, base.relkind FROM rel \
               JOIN pg_rewrite r ON r.ev_class = rel.oid \
               JOIN pg_depend d ON d.classid = 'pg_rewrite'::regclass AND d.objid = r.oid \
                                AND d.refclassid = 'pg_class'::regclass \
               JOIN pg_class base ON base.oid = d.refobjid AND base.oid <> rel.oid \
               WHERE rel.kind = 'v' \
             ) \
             SELECT c.oid::regclass::text AS relation, \
                    format('%I.%I', n.nspname, c.relname) AS qualified, \
                    c.relpersistence = 't' AS temporary \
             FROM rel JOIN pg_class c ON c.oid = rel.oid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relpersistence IN ('u', 't') \
             ORDER BY 1"
        );
        for row in adapter.execute_raw_query(&sql).await? {
            let text = |key: &str| row.get(key).and_then(serde_json::Value::as_str);
            if let (Some(relation), Some(qualified)) = (text("relation"), text("qualified")) {
                unreadable.push(Unreadable {
                    source:    probe.display_name(),
                    relation:  relation.to_string(),
                    qualified: qualified.to_string(),
                    temporary: row.get("temporary").and_then(serde_json::Value::as_bool)
                        == Some(true),
                });
            }
        }
    }
    Ok(unreadable)
}

#[cfg(test)]
#[path = "source_probe_tests.rs"]
mod source_probe_tests;

/// Refuse a schema whose proven native joins (#1587) the database does not have.
///
/// `fraiseql compile --database` records the SQL columns a relationship joins on
/// (`native_join`), and composed reads compare them directly. Deployed against a database
/// whose views lack them (a TVIEW renamed or rebuilt, another database), every such read
/// would fail with `column … does not exist`: this says so at boot instead.
///
/// # Errors
///
/// `FraiseQLError::Configuration` naming each relationship and its missing column;
/// `FraiseQLError::Database` if the catalog query fails.
pub async fn refuse_unproven_native_joins<A: DatabaseAdapter + ?Sized>(
    adapter: &A,
    schema: &CompiledSchema,
) -> Result<()> {
    let columns: Vec<(String, &str, &str)> = schema
        .types
        .iter()
        .flat_map(|t| t.relationships.iter().map(move |r| (t, r)))
        .filter_map(|(t, r)| r.native_join.as_ref().map(|j| (t, r, j)))
        .flat_map(|(t, r, j)| {
            let name = format!("{}.{}", t.name, r.name);
            [
                (name.clone(), j.child_view.as_str(), j.foreign_key.as_str()),
                (name, j.parent_view.as_str(), j.referenced_key.as_str()),
            ]
        })
        .collect();
    if columns.is_empty() {
        return Ok(());
    }
    let quote = |s: &str| s.replace('\'', "''");
    let values: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(i, (_, view, column))| {
            format!("({i}, to_regclass('{}'), '{}')", regclass_literal(view), quote(column))
        })
        .collect();
    let sql = format!(
        "SELECT v.i AS i FROM (VALUES {}) AS v(i, rel, col) \
         WHERE NOT EXISTS (SELECT 1 FROM pg_attribute a WHERE a.attrelid = v.rel \
         AND a.attname = v.col AND a.attnum > 0 AND NOT a.attisdropped) ORDER BY 1",
        values.join(", ")
    );
    let missing: Vec<String> = adapter
        .execute_raw_query(&sql)
        .await?
        .iter()
        .filter_map(|row| row.get("i").and_then(serde_json::Value::as_u64))
        .filter_map(|i| columns.get(usize::try_from(i).ok()?))
        .map(|(rel, view, column)| format!("{rel}: {view}.{column}"))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(FraiseQLError::Configuration {
        message: format!(
            "This schema was compiled with `--database` to join {} relationship column(s) this \
             database does not have:\n  - {}\nRecompile with `fraiseql compile --database` \
             against this database.",
            missing.len(),
            missing.join("\n  - ")
        ),
    })
}
