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

/// Whether a `sql_source` names a relation (query backing) or a function
/// (mutation backing). They are resolved differently: a relation via
/// `to_regclass` / the relation catalog, a function via `pg_proc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// A table / view / materialized view a query reads from.
    Relation,
    /// A SQL function a mutation calls.
    Function,
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
/// Queries contribute a [`SourceKind::Relation`] probe (their `sql_source` view),
/// mutations a [`SourceKind::Function`] probe (their `sql_source`, or operation
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
                kind: SourceKind::Relation,
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
/// replicas configured every query over such a source fails on every replica — and
/// `pg_tviews` creates its `tv_*` tables UNLOGGED by default. Views are followed to the
/// relations they read (a logged view over an unlogged table fails the same way);
/// materialized views are not, because they store their own rows.
///
/// # Errors
///
/// `FraiseQLError::Configuration` naming each source and the unlogged relation it
/// depends on.
pub async fn refuse_standby_unreadable_sources<A: DatabaseAdapter + ?Sized>(
    adapter: &A,
    schema: &CompiledSchema,
) -> Result<()> {
    if !adapter.serves_reads_from_standbys() {
        return Ok(());
    }
    let unreadable = standby_unreadable_sources(adapter, schema).await?;
    if unreadable.is_empty() {
        return Ok(());
    }
    Err(FraiseQLError::Configuration {
        message: format!(
            "Read replicas are configured, but {} source relation(s) of this schema depend \
             on an UNLOGGED or temporary table, which a hot standby cannot read: every query \
             over them would fail on every replica.\n  - {}\nMake those tables LOGGED \
             (`ALTER TABLE … SET LOGGED`; for pg_tviews, set \
             `pg_tviews.unlogged_by_default = off` and recreate the TVIEWs) — at the cost \
             of WAL for every refresh — or remove `read_replica_urls`.",
            unreadable.len(),
            unreadable.join("\n  - ")
        ),
    })
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
    let mut unreadable: Vec<String> = Vec::new();
    for probe in sql_source_probes(schema) {
        if probe.kind != SourceKind::Relation {
            continue;
        }
        let ident = match &probe.schema {
            Some(s) => format!(
                "{}.{}",
                quote_postgres_identifier(s),
                quote_postgres_identifier(&probe.name)
            ),
            None => quote_postgres_identifier(&probe.name),
        };
        let literal = ident.replace('\'', "''");
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
             SELECT c.oid::regclass::text AS relation FROM rel \
             JOIN pg_class c ON c.oid = rel.oid WHERE c.relpersistence IN ('u', 't') \
             ORDER BY 1"
        );
        for row in adapter.execute_raw_query(&sql).await? {
            if let Some(relation) = row.get("relation").and_then(serde_json::Value::as_str) {
                unreadable.push(format!("{} — reads {relation}", probe.display_name()));
            }
        }
    }
    Ok(unreadable)
}

#[cfg(test)]
#[path = "source_probe_tests.rs"]
mod source_probe_tests;
