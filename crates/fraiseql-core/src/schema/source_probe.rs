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

use crate::schema::{CompiledSchema, MutationOperation};

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

#[cfg(test)]
#[path = "source_probe_tests.rs"]
mod source_probe_tests;
