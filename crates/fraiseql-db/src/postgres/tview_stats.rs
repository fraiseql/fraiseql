//! The TVIEWs a schema reads, and their refresh counters from `tviews.stats`.
//!
//! `/metrics` reports two series per TVIEW: its physical health (`pg_tviews_profile()`,
//! #1392) and its refresh counters (`tviews.stats`). Both select their TVIEWs with
//! [`tviews_read_query`], so they cannot disagree on which TVIEWs a schema reads.

use fraiseql_error::Result;

use crate::traits::{TviewStats, TviewStatsReading};

/// SQLSTATE `object_not_in_prerequisite_state`: `tviews.stats` read without
/// `shared_preload_libraries = 'pg_tviews'`.
const NOT_PRELOADED: &str = "55000";

/// A query selecting `select` from `from`, one row per TVIEW that `$1` (a `text[]` of
/// sources) reads: a source that is a TVIEW's table, or a TVIEW a view among them reads.
/// `schema` and `name` are the columns of `from` naming the TVIEW's table.
///
/// A TVIEW in a schema this role cannot use matches no source.
pub fn tviews_read_query(select: &str, from: &str, schema: &str, name: &str) -> String {
    format!(
        r"
        WITH src AS (
            SELECT to_regclass(s) AS oid FROM unnest($1::text[]) AS s
        ), rel AS (
            SELECT oid FROM src WHERE oid IS NOT NULL
            UNION
            SELECT d.refobjid
            FROM src
            JOIN pg_rewrite r ON r.ev_class = src.oid
            JOIN pg_depend d ON d.objid = r.oid AND d.refclassid = 'pg_class'::regclass
        )
        SELECT {select}
        FROM {from}
        WHERE {} IN (SELECT oid FROM rel)
        ORDER BY 1
    ",
        crate::usable_relation(schema, name)
    )
}

/// The refresh counters of every TVIEW one of `sources` reads, on `client`.
///
/// [`TviewStatsReading::Unavailable`] when the database has no `tviews.stats` (no
/// `pg_tviews`, or one older than the view) or reading it needs the library preloaded.
///
/// # Errors
///
/// Returns [`fraiseql_error::FraiseQLError::Database`] if the catalog probe or the read fails for
/// any other reason.
pub async fn query_tview_stats(
    client: &tokio_postgres::Client,
    sources: &[String],
) -> Result<TviewStatsReading> {
    let present: bool = client
        .query_one("SELECT to_regclass('tviews.stats') IS NOT NULL", &[])
        .await
        .map_err(|e| {
            crate::postgres::database_error(format!("tviews.stats probe failed: {e}"), &e)
        })?
        .get(0);
    if !present {
        return Ok(TviewStatsReading::Unavailable);
    }
    if sources.is_empty() {
        return Ok(TviewStatsReading::Read(Vec::new()));
    }
    let query = tviews_read_query(
        "format('%I.%I', s.schema, s.name) AS tview, s.view_recomputes, s.noop_skipped, \
         s.patch_captured, s.patch_applied, s.patch_fallbacks, s.propagation_pruned, \
         s.rows_written, s.rows_deleted, s.full_refreshes, s.refresh_ms, \
         extract(epoch FROM s.stats_reset)::float8 AS stats_reset, s.untracked",
        "tviews.stats AS s",
        "s.schema",
        "s.name",
    );
    let rows = match client.query(&query, &[&sources]).await {
        Ok(rows) => rows,
        Err(e) if e.code().is_some_and(|code| code.code() == NOT_PRELOADED) => {
            return Ok(TviewStatsReading::Unavailable);
        },
        Err(e) => {
            return Err(crate::postgres::database_error(
                format!("tviews.stats read failed: {e}"),
                &e,
            ));
        },
    };
    Ok(TviewStatsReading::Read(
        rows.iter()
            .map(|row| TviewStats {
                tview:              row.get("tview"),
                view_recomputes:    row.get("view_recomputes"),
                noop_skipped:       row.get("noop_skipped"),
                patch_captured:     row.get("patch_captured"),
                patch_applied:      row.get("patch_applied"),
                patch_fallbacks:    row.get("patch_fallbacks"),
                propagation_pruned: row.get("propagation_pruned"),
                rows_written:       row.get("rows_written"),
                rows_deleted:       row.get("rows_deleted"),
                full_refreshes:     row.get("full_refreshes"),
                refresh_ms:         row.get("refresh_ms"),
                stats_reset:        row.get("stats_reset"),
                untracked:          row.get("untracked"),
            })
            .collect(),
    ))
}
