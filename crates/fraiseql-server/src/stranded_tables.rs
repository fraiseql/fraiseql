//! Tables an earlier release created outside `core`.
//!
//! Earlier releases created some of FraiseQL's tables unqualified, so in whichever schema
//! led that deployment's `search_path`, which need not be on today's. A release that keeps
//! them in `core` creates an empty `core` table at boot; running over it would start from
//! nothing while the rows sit elsewhere. Each owner of such a table refuses to start in
//! that case, and says how to move the table.

use sqlx::PgPool;

use crate::ServerError;

/// A table that moves into `core`, with the table that moves with it, if any.
pub struct Strandable {
    /// The table.
    pub table:     &'static str,
    /// A table holding rows that belong to `table`'s, moved in the same transaction.
    pub companion: Option<&'static str>,
}

/// The statements that move each of `tables` into `core`, for every schema other than
/// `core` that holds a populated copy while `core`'s copy is empty or absent. Empty when
/// nothing is stranded.
///
/// A table that cannot be read is logged and skipped: its contents are unknown, not known
/// to be rows.
///
/// # Errors
///
/// Returns `ServerError::Database` if the catalog cannot be read.
pub async fn stranded_moves(
    pool: &PgPool,
    tables: &[Strandable],
) -> Result<Vec<String>, ServerError> {
    let mut moves = Vec::new();
    for strandable in tables {
        if has_rows(pool, "core", strandable.table).await? == Some(true) {
            continue;
        }
        let schemas: Vec<(String,)> = sqlx::query_as(
            "SELECT n.nspname::text FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relname = $1 AND c.relkind IN ('r', 'p') AND n.nspname <> 'core' \
             ORDER BY 1",
        )
        .bind(strandable.table)
        .fetch_all(pool)
        .await
        .map_err(|e| ServerError::Database(e.to_string()))?;
        for (schema,) in schemas {
            if has_rows(pool, &schema, strandable.table).await? == Some(true) {
                moves.push(move_into_core(&schema, strandable));
            }
        }
    }
    Ok(moves)
}

/// Whether `schema.table` holds a row: `None` when it is absent or unreadable.
async fn has_rows(pool: &PgPool, schema: &str, table: &str) -> Result<Option<bool>, ServerError> {
    let qualified = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let exists: (Option<String>,) = sqlx::query_as("SELECT to_regclass($1)::text")
        .bind(&qualified)
        .fetch_one(pool)
        .await
        .map_err(|e| ServerError::Database(e.to_string()))?;
    if exists.0.is_none() {
        return Ok(None);
    }
    match sqlx::query_as::<_, (bool,)>(&format!("SELECT EXISTS (SELECT 1 FROM {qualified})"))
        .fetch_one(pool)
        .await
    {
        Ok((populated,)) => Ok(Some(populated)),
        Err(e) => {
            tracing::warn!(
                table = %qualified,
                error = %e,
                "Cannot read a possible legacy table; not checking it"
            );
            Ok(None)
        },
    }
}

/// The statements that replace the empty `core` table boot created with the populated
/// one in `schema`.
fn move_into_core(schema: &str, strandable: &Strandable) -> String {
    let schema = quote_ident(schema);
    let table = strandable.table;
    match strandable.companion {
        Some(companion) => format!(
            "  BEGIN;\n  \
             DROP TABLE IF EXISTS core.{companion}, core.{table} CASCADE;\n  \
             ALTER TABLE {schema}.{table} SET SCHEMA core;\n  \
             ALTER TABLE IF EXISTS {schema}.{companion} SET SCHEMA core;\n  \
             COMMIT;"
        ),
        None => format!(
            "  BEGIN;\n  \
             DROP TABLE IF EXISTS core.{table} CASCADE;\n  \
             ALTER TABLE {schema}.{table} SET SCHEMA core;\n  \
             COMMIT;"
        ),
    }
}

/// `ident` as a PostgreSQL identifier, quoted only when it has to be.
fn quote_ident(ident: &str) -> String {
    let plain = ident.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && ident.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if plain {
        ident.to_owned()
    } else {
        format!("\"{}\"", ident.replace('"', "\"\""))
    }
}

/// The source cursor table: one row per source, the watermark it resumes from.
#[cfg(any(feature = "sources", feature = "inbound-email"))]
const SOURCE_CURSOR: [Strandable; 1] = [Strandable {
    table:     "_fraiseql_source_cursor",
    companion: None,
}];

/// Refuse to poll over an empty `core._fraiseql_source_cursor` while an earlier release's
/// cursor table holds rows elsewhere: every source and mailbox would start from no cursor
/// and re-read, or skip, what the stored one covered.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` naming the statement that moves the table into
/// `core`, or `ServerError::Database` if the catalog cannot be read.
#[cfg(any(feature = "sources", feature = "inbound-email"))]
pub async fn refuse_stranded_source_cursor(pool: &PgPool) -> Result<(), ServerError> {
    let moves = stranded_moves(pool, &SOURCE_CURSOR).await?;
    if moves.is_empty() {
        return Ok(());
    }
    Err(ServerError::ConfigError(format!(
        "the source cursor table from an earlier release holds rows outside `core`, and the \
         `core` table is empty. FraiseQL keeps it in `core` whatever the search_path; \
         starting now would poll every source and mailbox from no cursor. Move it, then \
         restart:\n{}",
        moves.join("\n")
    )))
}

#[cfg(test)]
mod tests;
