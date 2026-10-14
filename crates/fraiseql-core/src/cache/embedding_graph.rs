//! Which cached views a write to another view reaches (#1575), read from the database.
//!
//! A mutation sweeps the views it can name: its declared `invalidates_views`, its return
//! type's view, its cascade's types. A cached read of a *different* view that embeds the
//! written rows survives that sweep: `tv_post` embedding its author's `bio` is stale after
//! `updateUser` sweeps `tv_user`, for the entry's whole TTL, forever at TTL 0.
//!
//! The database knows which relations read which, so the graph comes from its catalog,
//! at boot, and is installed as the cache's [`CascadeInvalidator`]:
//!
//! - **What a view reads**: itself, every relation it reaches through view definitions
//!   (`pg_rewrite` → `pg_depend`, transitively, including views the schema never names), and the
//!   base tables of every `pg_tviews` TVIEW among those (`tviews.registry.base_tables`).
//! - **What a sweep of a view says was written** (what it *owns*): a plain or materialized view
//!   owns everything it reads, since any of it may be what the write changed; a TVIEW owns itself
//!   and its own base table (`cascade_kinds` = `local`), not the tables it embeds; any other
//!   relation owns itself.
//!
//! Sweeping `V` then also sweeps every `W` that reads something `V` owns. A user's write
//! (`tv_user`, owning `tb_user`) reaches `tv_post`, which reads `tb_user` as an embed; a
//! post's write does not reach `tv_user`. Two names resolving to one relation sweep together.
//!
//! The graph is read once, at boot, like the rest of the cache's schema-derived state: a
//! TVIEW or view redefined while the server runs is seen at its next start, with no
//! recompile. A catalog that cannot be read leaves the sweep as it was, and says so.

use std::collections::BTreeSet;

use super::CascadeInvalidator;
use crate::{
    db::DatabaseAdapter,
    error::Result,
    schema::{CompiledSchema, regclass_literal},
};

/// Every view name a cached entry can be indexed under, or a mutation can sweep: query
/// sources and their `additional_views`, type sources, and `invalidates_views`.
#[must_use]
pub fn swept_view_names(schema: &CompiledSchema) -> BTreeSet<String> {
    let queries = schema
        .queries
        .iter()
        .flat_map(|q| q.sql_source.iter().chain(&q.additional_views));
    let types = schema.types.iter().map(|t| t.sql_source.as_str());
    let declared = schema.mutations.iter().flat_map(|m| &m.invalidates_views);
    queries
        .map(String::as_str)
        .chain(types)
        .chain(declared.map(String::as_str))
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether the database has `pg_tviews`' registry with the `cascade_kinds` that tells a
/// TVIEW's own base table from those it embeds. Without it, TVIEWs are read as tables.
const REGISTRY_PROBE: &str = "SELECT EXISTS (SELECT 1 FROM pg_attribute \
     WHERE attrelid = to_regclass('tviews.registry') AND attname = 'cascade_kinds' \
     AND NOT attisdropped) AS has_registry";

/// The registry's TVIEWs, each with every base table it reads and the kind of each. A
/// table whose kind cannot be read counts as the TVIEW's own (`local`): that sweeps more,
/// never less.
const REGISTRY_TVIEWS: &str = "SELECT to_regclass(quote_ident(g.schema) || '.' || \
     quote_ident(g.name))::oid AS tview, b::oid AS base, \
     coalesce(g.cascade_kinds ->> b::text, 'local') AS kind \
     FROM tviews.registry g CROSS JOIN LATERAL unnest(g.base_tables) b";

/// The same columns, empty: no TVIEWs.
const NO_TVIEWS: &str = "SELECT NULL::oid AS tview, NULL::oid AS base, NULL::text AS kind \
     WHERE false";

/// The opening of a `WITH RECURSIVE` over `names` (non-empty), for a caller to continue
/// with `, <more CTEs> SELECT …`:
///
/// - `roots(name, rel)`: each name that resolves to a relation, and its oid;
/// - `reads(root, rel)`: every relation each root reads through view definitions (`pg_rewrite` →
///   `pg_depend`, transitively), itself included.
pub(super) fn reads_cte(names: &BTreeSet<String>) -> String {
    let src: Vec<String> = names
        .iter()
        .map(|name| {
            format!("('{}', to_regclass('{}'))", name.replace('\'', "''"), regclass_literal(name))
        })
        .collect();
    format!(
        "WITH RECURSIVE \
         src(name, rel) AS (VALUES {src}), \
         roots AS (SELECT name, rel::oid AS rel FROM src WHERE rel IS NOT NULL), \
         reads(root, rel) AS ( \
           SELECT rel, rel FROM roots \
           UNION \
           SELECT r.root, d.refobjid FROM reads r \
           JOIN pg_rewrite rw ON rw.ev_class = r.rel \
           JOIN pg_depend d ON d.classid = 'pg_rewrite'::regclass AND d.objid = rw.oid \
                            AND d.refclassid = 'pg_class'::regclass AND d.refobjid <> r.rel \
         )",
        src = src.join(", "),
    )
}

/// `(dependency, dependent)` name pairs over `names`: a sweep of `dependency` must also
/// sweep `dependent`. `tviews` is [`REGISTRY_TVIEWS`] or [`NO_TVIEWS`].
fn edges_sql(names: &BTreeSet<String>, tviews: &str) -> String {
    format!(
        "{reads}, \
         tv AS ({tviews}), \
         reads_all(root, rel) AS ( \
           SELECT root, rel FROM reads \
           UNION SELECT r.root, tv.base FROM reads r JOIN tv ON tv.tview = r.rel \
         ), \
         owns_read(root, rel) AS ( \
           SELECT r.root, r.rel FROM reads r JOIN pg_class c ON c.oid = r.root \
           WHERE c.relkind IN ('v', 'm') OR r.rel = r.root \
         ), \
         owns(root, rel) AS ( \
           SELECT root, rel FROM owns_read \
           UNION SELECT o.root, tv.base FROM owns_read o \
           JOIN tv ON tv.tview = o.rel AND tv.kind = 'local' \
         ) \
         SELECT DISTINCT v.name AS dependency, w.name AS dependent \
         FROM roots v JOIN owns o ON o.root = v.rel \
         JOIN reads_all a ON a.rel = o.rel \
         JOIN roots w ON w.rel = a.root \
         WHERE w.name <> v.name \
         ORDER BY 1, 2",
        reads = reads_cte(names),
    )
}

/// Read the embedding graph of `schema`'s views from the database `adapter` serves.
///
/// # Errors
///
/// `FraiseQLError::Database` if a catalog query fails; the caller keeps the plain sweep.
pub async fn read_embedding_graph<A: DatabaseAdapter + ?Sized>(
    adapter: &A,
    schema: &CompiledSchema,
) -> Result<CascadeInvalidator> {
    let mut graph = CascadeInvalidator::new();
    let names = swept_view_names(schema);
    if names.is_empty() {
        return Ok(graph);
    }
    let has_registry = adapter
        .execute_raw_query(REGISTRY_PROBE)
        .await?
        .first()
        .and_then(|row| row.get("has_registry"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let tviews = if has_registry {
        REGISTRY_TVIEWS
    } else {
        NO_TVIEWS
    };
    for row in adapter.execute_raw_query(&edges_sql(&names, tviews)).await? {
        let name = |key: &str| row.get(key).and_then(serde_json::Value::as_str);
        if let (Some(dependency), Some(dependent)) = (name("dependency"), name("dependent")) {
            graph.add_dependency(dependent, dependency)?;
        }
    }
    Ok(graph)
}

#[cfg(test)]
#[path = "embedding_graph_tests.rs"]
mod embedding_graph_tests;
