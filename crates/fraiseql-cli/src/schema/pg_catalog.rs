//! Live-PostgreSQL catalog access for the `--against-db` checks.
//!
//! A thin wrapper over a [`deadpool_postgres`] pool that answers the two
//! questions the `validate --against-db` (#397) and `doctor --against-db`
//! (#409) checks need:
//!
//! - **Function resolution / signature** — for a `sql_source` name, which overloads exist, what are
//!   their *input* argument types/names (in call order), and what columns does each return?
//!   (`resolve_functions`)
//! - **PL/pgSQL body resolution** — does the `plpgsql_check` extension exist, and which managed
//!   functions call something the catalog can't resolve? (`plpgsql_check_available` /
//!   `plpgsql_check_unresolved_calls`)
//!
//! All catalog queries are PostgreSQL-specific (`pg_proc`, `pg_type`,
//! `pg_attribute`, `plpgsql_check`); the surrounding logic in
//! [`super::mutation_contract`] is database-agnostic and unit-tested without a
//! connection.

use anyhow::{Context, Result};
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;

/// A single output column of a PostgreSQL function's result row.
///
/// Populated from either the `OUT`/`TABLE` parameters of a `RETURNS TABLE(…)`
/// function or the attributes of a composite `RETURNS <type>` — both are how
/// FraiseQL mutation functions declare the row the server decodes into
/// `MutationResponse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutColumn {
    /// Column name as it appears in the result row (matched against the
    /// `MutationResponse` field names).
    pub name:      String,
    /// `format_type` rendering of the column type (e.g. `"boolean"`,
    /// `"jsonb"`, `"text[]"`, `"app.mutation_error_class"`).
    pub type_name: String,
    /// Whether the column type is a PostgreSQL `enum` (`pg_type.typtype = 'e'`).
    /// Mutation functions legitimately type `error_class` as either `text` or a
    /// project-specific enum, so the contract check accepts both.
    pub is_enum:   bool,
}

/// One overload of a PostgreSQL function, reduced to what the mutation-contract
/// check needs: the *input* arguments (in positional call order, excluding
/// `OUT`/`TABLE` columns) and the output columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgFunction {
    /// Schema the function lives in.
    pub schema:      String,
    /// Function name (unqualified).
    pub name:        String,
    /// Input argument types in call order (`format_type`), modes `IN`/`INOUT`/
    /// `VARIADIC` only — these are the positions the server binds.
    pub in_types:    Vec<String>,
    /// Input argument names in call order; `None` for unnamed positions.
    pub in_names:    Vec<Option<String>>,
    /// Output columns of the result row (see [`OutColumn`]). Empty when the
    /// function returns a scalar / `record` the catalog cannot expand.
    pub out_columns: Vec<OutColumn>,
    /// Whether the function returns a set (`RETURNS SETOF …` / `RETURNS
    /// TABLE(…)`). Informational only.
    pub returns_set: bool,
}

/// A single unresolved internal call surfaced by `plpgsql_check` (#409).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyError {
    /// Calling function, rendered as `schema.fn(argtypes)` (`regprocedure`).
    pub caller:  String,
    /// Line number within the function body, when reported.
    pub lineno:  Option<i32>,
    /// The `plpgsql_check` diagnostic message.
    pub message: String,
}

/// Outcome of the PL/pgSQL body-resolution pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlpgsqlCheckOutcome {
    /// `plpgsql_check` is not available on this server — the pass was skipped.
    Unavailable,
    /// The pass ran; `errors` is empty when every internal call resolved.
    Ran {
        /// Unresolved internal calls found.
        errors: Vec<BodyError>,
    },
}

/// A live PostgreSQL connection pool for catalog introspection.
pub struct PgCatalog {
    pool: Pool,
}

impl PgCatalog {
    /// Connect to `db_url` (PostgreSQL only) for catalog introspection.
    ///
    /// # Errors
    ///
    /// Returns an error if `db_url` is not a `postgres://` URL or the pool
    /// cannot be created. (Connection failures surface lazily on first query.)
    pub fn connect(db_url: &str) -> Result<Self> {
        if !db_url.starts_with("postgres") {
            anyhow::bail!(
                "--against-db requires a PostgreSQL connection URL (postgres://…); got: {db_url}"
            );
        }
        let mut cfg = Config::new();
        cfg.url = Some(db_url.to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });
        cfg.pool = Some(deadpool_postgres::PoolConfig::new(2));
        let pool = cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .context("failed to create PostgreSQL connection pool for --against-db")?;
        Ok(Self { pool })
    }

    /// Resolve all overloads of a (possibly schema-qualified) function name.
    ///
    /// Unqualified names resolve against the connection's `search_path`
    /// (`current_schemas(false)`), mirroring how the runtime resolves an
    /// unqualified `sql_source`. Matching is case-sensitive — the runtime quotes
    /// each identifier component, so `proname` is compared verbatim.
    ///
    /// Returns an empty vec when no function of that name is visible.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or any catalog query fails.
    pub async fn resolve_functions(&self, sql_source: &str) -> Result<Vec<PgFunction>> {
        // One row per overload: input-arg types/names (modes IN/INOUT/VARIADIC
        // only — OUT/TABLE columns are excluded so the count matches what the
        // server binds positionally) plus oid + proretset for follow-up.
        const BASE: &str = "\
            SELECT p.oid, n.nspname, p.proname, p.proretset, \
              COALESCE((SELECT array_agg(format_type(t, NULL) ORDER BY ord) \
                FROM unnest(COALESCE(p.proallargtypes, p.proargtypes::oid[])) \
                  WITH ORDINALITY AS a(t, ord) \
                LEFT JOIN unnest(p.proargmodes) WITH ORDINALITY AS m(mode, mord) ON mord = ord \
                WHERE COALESCE(m.mode, 'i') IN ('i','b','v')), ARRAY[]::text[]) AS in_types, \
              COALESCE((SELECT array_agg(COALESCE(nm, '') ORDER BY ord) \
                FROM unnest(COALESCE(p.proallargtypes, p.proargtypes::oid[])) \
                  WITH ORDINALITY AS a(t, ord) \
                LEFT JOIN unnest(p.proargmodes) WITH ORDINALITY AS m(mode, mord) ON mord = ord \
                LEFT JOIN unnest(p.proargnames) WITH ORDINALITY AS nmt(nm, nord) ON nord = ord \
                WHERE COALESCE(m.mode, 'i') IN ('i','b','v')), ARRAY[]::text[]) AS in_names \
            FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace ";

        let client = self.pool.get().await.context("failed to acquire DB connection")?;
        let (schema, name) = split_qualified(sql_source);

        let rows = if let Some(schema) = schema {
            client
                .query(
                    &format!("{BASE} WHERE n.nspname = $1 AND p.proname = $2"),
                    &[&schema, &name],
                )
                .await
        } else {
            client
                .query(
                    &format!(
                        "{BASE} WHERE p.proname = $1 AND n.nspname = ANY(current_schemas(false))"
                    ),
                    &[&name],
                )
                .await
        }
        .context("failed to query pg_proc for function resolution")?;

        let mut functions = Vec::with_capacity(rows.len());
        for row in rows {
            let oid: u32 = row.get("oid");
            let in_names: Vec<String> = row.get("in_names");
            functions.push(PgFunction {
                schema:      row.get("nspname"),
                name:        row.get("proname"),
                in_types:    row.get("in_types"),
                in_names:    in_names
                    .into_iter()
                    .map(|n| if n.is_empty() { None } else { Some(n) })
                    .collect(),
                out_columns: self.out_columns(oid).await?,
                returns_set: row.get("proretset"),
            });
        }
        Ok(functions)
    }

    /// Output columns of one function (by oid).
    ///
    /// Tries the `OUT`/`TABLE` parameter path first (`RETURNS TABLE(…)`); if the
    /// function has none, falls back to the attributes of a composite return
    /// type (`RETURNS <composite>` / `RETURNS SETOF <composite>`). A scalar or
    /// bare-`record` return yields an empty vec.
    async fn out_columns(&self, oid: u32) -> Result<Vec<OutColumn>> {
        let client = self.pool.get().await.context("failed to acquire DB connection")?;

        // Path 1: OUT/TABLE output parameters.
        let table_rows = client
            .query(
                "SELECT COALESCE(nm, '') AS name, format_type(t, NULL) AS type_name, \
                   (tt.typtype = 'e') AS is_enum \
                 FROM pg_proc p, \
                   unnest(p.proallargtypes) WITH ORDINALITY AS a(t, ord) \
                   JOIN unnest(p.proargmodes) WITH ORDINALITY AS m(mode, mord) ON mord = ord \
                   LEFT JOIN unnest(p.proargnames) WITH ORDINALITY AS nmt(nm, nord) ON nord = ord \
                   JOIN pg_type tt ON tt.oid = t \
                 WHERE p.oid = $1 AND m.mode IN ('o','b','t') ORDER BY ord",
                &[&oid],
            )
            .await
            .context("failed to query OUT/TABLE result columns")?;
        if !table_rows.is_empty() {
            return Ok(table_rows.iter().map(row_to_out_column).collect());
        }

        // Path 2: composite return type attributes.
        let comp_rows = client
            .query(
                "SELECT att.attname AS name, \
                   format_type(att.atttypid, att.atttypmod) AS type_name, \
                   (at.typtype = 'e') AS is_enum \
                 FROM pg_proc p \
                   JOIN pg_type rt ON rt.oid = p.prorettype \
                   JOIN pg_attribute att ON att.attrelid = rt.typrelid \
                     AND att.attnum > 0 AND NOT att.attisdropped \
                   JOIN pg_type at ON at.oid = att.atttypid \
                 WHERE p.oid = $1 ORDER BY att.attnum",
                &[&oid],
            )
            .await
            .context("failed to query composite result columns")?;
        Ok(comp_rows.iter().map(row_to_out_column).collect())
    }

    /// Whether the `plpgsql_check` extension is installable (present in
    /// `pg_available_extensions`).
    ///
    /// # Errors
    ///
    /// Returns an error if the catalog query fails.
    pub async fn plpgsql_check_available(&self) -> Result<bool> {
        let client = self.pool.get().await.context("failed to acquire DB connection")?;
        let row = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM pg_available_extensions WHERE name = 'plpgsql_check')",
                &[],
            )
            .await
            .context("failed to probe for the plpgsql_check extension")?;
        Ok(row.get(0))
    }

    /// Run the PL/pgSQL body-resolution pass over `schemas` and return every
    /// unresolved internal call (#409).
    ///
    /// Skips gracefully when `plpgsql_check` is unavailable
    /// ([`PlpgsqlCheckOutcome::Unavailable`]). Trigger functions are excluded
    /// (they need a relation OID to analyse), and only `error`-level
    /// `function … does not exist` diagnostics are reported — warnings and
    /// runtime-only temp-table references are not contract breakage.
    ///
    /// # Errors
    ///
    /// Returns an error if `CREATE EXTENSION` or the analysis query fails (e.g.
    /// insufficient privileges to create the extension).
    pub async fn plpgsql_check_unresolved_calls(
        &self,
        schemas: &[String],
    ) -> Result<PlpgsqlCheckOutcome> {
        if !self.plpgsql_check_available().await? {
            return Ok(PlpgsqlCheckOutcome::Unavailable);
        }
        let client = self.pool.get().await.context("failed to acquire DB connection")?;
        client
            .batch_execute("CREATE EXTENSION IF NOT EXISTS plpgsql_check")
            .await
            .context("failed to CREATE EXTENSION plpgsql_check")?;

        let rows = client
            .query(
                "SELECT (p.oid::regprocedure)::text AS caller, t.lineno, t.message \
                 FROM pg_proc p \
                   JOIN pg_namespace n ON n.oid = p.pronamespace \
                   JOIN pg_language  l ON l.oid = p.prolang AND l.lanname = 'plpgsql' \
                   CROSS JOIN LATERAL plpgsql_check_function_tb(p.oid, fatal_errors := false) t \
                 WHERE n.nspname = ANY($1) \
                   AND p.prorettype <> 'trigger'::regtype \
                   AND t.level = 'error' \
                   AND t.message ~* 'function .* does not exist' \
                 ORDER BY 1, 2",
                &[&schemas],
            )
            .await
            .context("failed to run the plpgsql_check body-resolution pass")?;

        let errors = rows
            .iter()
            .map(|row| BodyError {
                caller:  row.get("caller"),
                lineno:  row.get("lineno"),
                message: row.get("message"),
            })
            .collect();
        Ok(PlpgsqlCheckOutcome::Ran { errors })
    }
}

/// Build an [`OutColumn`] from a `(name, type_name, is_enum)` catalog row.
fn row_to_out_column(row: &tokio_postgres::Row) -> OutColumn {
    OutColumn {
        name:      row.get("name"),
        type_name: row.get("type_name"),
        is_enum:   row.get("is_enum"),
    }
}

/// Split a possibly schema-qualified function name into `(schema, name)`.
///
/// Mirrors the runtime's identifier handling: an unqualified name (no `.`)
/// resolves via `search_path`; a `schema.fn` name pins the schema. Split on the
/// first `.` so `schema.fn` → `(Some("schema"), "fn")`.
fn split_qualified(sql_source: &str) -> (Option<String>, String) {
    match sql_source.split_once('.') {
        Some((schema, name)) => (Some(schema.to_string()), name.to_string()),
        None => (None, sql_source.to_string()),
    }
}
