//! Reads of one TVIEW row by its id, evicted by that row (#1576).
//!
//! A write sweeps every cached read of the views it touches: a list may gain or lose any
//! row, so it must (#741, #742, #763). A read of **one** row of a `pg_tviews` TVIEW by its
//! `id` column, with no other predicate, is different: its rows are that row, or nothing, and
//! change only when that row does. `pg_tviews` journals every row a transaction changes,
//! cascaded ones included, and reports them by GraphQL type and public id
//! (`tviews.pg_tviews_flush_and_report()`). So such a read is indexed under its row, a write
//! asks for that report, and evicts these reads by the rows it lists, every read of a type it
//! truncated, and nothing else of them.
//!
//! A read qualifies only when its view **is** a TVIEW's table (not a view over one, which may
//! filter or join), the table has no row security (a policy may read other tables), its only
//! predicate is `id = $1` on the native column (an injected parameter or an engine RLS filter
//! adds one), and it reads no other view. Anything else keeps the sweep. The sources are read
//! from the catalog at boot, like the embedding graph; a catalog without
//! `tviews.registry.typename` leaves every read to the sweep.
//!
//! The precision rests on the report being complete: every TVIEW row a write changes goes
//! through `pg_tviews`' journal. A function that writes a TVIEW's table directly, or flushes
//! the journal itself, leaves these reads stale until their TTL.

use std::collections::{BTreeSet, HashMap};

use super::embedding_graph::reads_cte;
use crate::{
    db::{DatabaseAdapter, WhereClause, WhereOperator},
    error::Result,
    schema::CompiledSchema,
};

/// Whether the registry names each TVIEW's GraphQL type, as the report does.
const TYPENAME_PROBE: &str = "SELECT EXISTS (SELECT 1 FROM pg_attribute \
     WHERE attrelid = to_regclass('tviews.registry') AND attname = 'typename' \
     AND NOT attisdropped) AS has_typename";

/// Every view name of `schema` that is a TVIEW's table without row security, with the type
/// `pg_tviews` reports its rows as: the sources a read by id may be indexed under its row for.
///
/// # Errors
///
/// `FraiseQLError::Database` when a catalog query fails; the caller keeps the sweep.
pub async fn read_by_id_sources<A: DatabaseAdapter + ?Sized>(
    adapter: &A,
    schema: &CompiledSchema,
) -> Result<HashMap<String, String>> {
    let names: BTreeSet<String> = super::swept_view_names(schema);
    if names.is_empty() {
        return Ok(HashMap::new());
    }
    let has_typename = adapter
        .execute_raw_query(TYPENAME_PROBE)
        .await?
        .first()
        .and_then(|row| row.get("has_typename"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !has_typename {
        return Ok(HashMap::new());
    }
    let rows = adapter
        .execute_raw_query(&format!(
            "{reads}, \
             tv AS (SELECT to_regclass(quote_ident(g.schema) || '.' || quote_ident(g.name))::oid \
                    AS rel, g.typename FROM tviews.registry g) \
             SELECT r.name, tv.typename FROM roots r JOIN tv ON tv.rel = r.rel \
             JOIN pg_class c ON c.oid = r.rel WHERE NOT c.relrowsecurity",
            reads = reads_cte(&names),
        ))
        .await?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let text = |key: &str| row.get(key).and_then(serde_json::Value::as_str);
            Some((text("name")?.to_string(), text("typename")?.to_string()))
        })
        .collect())
}

/// The `(type, id)` a read of `view` filtered by `where_clause` is indexed under, when it
/// reads one row of a source in `sources` by its id and no other view (`accessed_views` is
/// that view alone); `None` when it must keep the sweep.
pub(super) fn by_id_tag(
    sources: &HashMap<String, String>,
    view: &str,
    where_clause: Option<&WhereClause>,
    accessed_views: &[String],
) -> Option<(String, String)> {
    let type_name = sources.get(view)?;
    if accessed_views.len() != 1 {
        return None;
    }
    let Some(WhereClause::NativeField {
        column,
        pg_cast,
        operator: WhereOperator::Eq,
        value,
    }) = where_clause
    else {
        return None;
    };
    if column != "id" {
        return None;
    }
    Some((type_name.clone(), id_text(pg_cast, value)?))
}

/// An id as `pg_tviews` reports it: a uuid in its canonical form (PostgreSQL reads
/// `…157A` and `{…}` as the same value), an integer's decimal digits, other text as given.
/// `None` for a value the column could not hold, which reads nothing to tag.
fn id_text(pg_cast: &str, value: &serde_json::Value) -> Option<String> {
    let text = match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Number(number) => number.to_string(),
        _ => return None,
    };
    match pg_cast {
        "uuid" => uuid::Uuid::parse_str(&text).ok().map(|id| id.hyphenated().to_string()),
        "int2" | "int4" | "int8" => text.trim().parse::<i64>().ok().map(|id| id.to_string()),
        _ => Some(text),
    }
}

#[cfg(test)]
#[path = "by_id_tests.rs"]
mod by_id_tests;
