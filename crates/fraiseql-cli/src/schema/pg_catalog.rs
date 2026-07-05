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

/// A single live column of a table, as the change-log contract drift check
/// (#380) reads it from `information_schema.columns`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveColumn {
    /// Column name (`information_schema.columns.column_name`).
    pub name:     String,
    /// PostgreSQL base type (`information_schema.columns.udt_name`): the
    /// lower-case underlying type, e.g. `"uuid"`, `"int8"`, `"text"`, `"jsonb"`,
    /// `"timestamptz"`, `"_text"` (the element form of `text[]`). Compared
    /// against [`fraiseql_observers::migrations::ContractColumn::udt`].
    pub udt_name: String,
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

/// Live Row-Level Security posture of `core.tb_entity_change_log` and whether the
/// connecting role can read it under that posture (for the `fraiseql doctor`
/// change-log RLS check, #437 F6 / #443).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeLogRlsStatus {
    /// Whether `ENABLE ROW LEVEL SECURITY` is active on the table.
    pub rls_enabled: bool,
    /// Whether the connecting role bypasses RLS (`BYPASSRLS` or superuser).
    pub can_bypass:  bool,
    /// Whether the connecting role owns the table (owners are exempt under `ENABLE`).
    pub is_owner:    bool,
    /// The connecting role name (for the diagnostic message).
    pub role_name:   String,
}

/// PUBLIC-held privileges on one change-log relation, for the `fraiseql doctor`
/// least-privilege check (#443).
///
/// Migration 12 `REVOKE ALL … FROM PUBLIC` keeps the change-log — every tenant's
/// before/after payload — off the world-readable `PUBLIC` pseudo-role; a non-empty
/// `privileges` means that REVOKE is not in force and the relation is
/// world-accessible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicGrant {
    /// Relation name (`tb_entity_change_log` / `v_entity_change_log` /
    /// `v_entity_change_log_debezium`).
    pub relname:    String,
    /// Privilege types currently granted to PUBLIC (e.g. `["SELECT"]`); empty when
    /// the least-privilege baseline is intact.
    pub privileges: Vec<String>,
}

/// Security posture of `core.fn_entity_change_log_capture()` for the
/// `fraiseql doctor` capture-function check (#443 / #437 F6).
///
/// The external-write capture function must be `SECURITY DEFINER` (so it runs as
/// the table owner, exempt under change-log RLS) **with a pinned `search_path`** (a
/// DEFINER function with a mutable `search_path` is a privilege-escalation vector).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureFnSecurity {
    /// Whether the function is `SECURITY DEFINER` (`pg_proc.prosecdef`).
    pub security_definer: bool,
    /// The pinned `search_path` value from `pg_proc.proconfig` (the text after
    /// `search_path=`), or `None` when no `search_path` is pinned.
    pub search_path:      Option<String>,
}

/// Result of auditing `sql_source` views for the `security_invoker` requirement.
#[derive(Debug)]
pub struct SecurityInvokerAudit {
    /// Whether any RLS policy exists in the database (RLS is in use at all).
    pub rls_in_use:            bool,
    /// The audited views (by bare relname) that are NOT `security_invoker`.
    pub views_without_invoker: Vec<String>,
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

    /// Read the columns of `schema.table` from `information_schema.columns`
    /// (name + `udt_name`), in declaration order.
    ///
    /// Returns an **empty** vec when the table does not exist (or is invisible to
    /// the connecting role) — the change-log contract drift check (#380) reads
    /// that as "table absent, the migration will install it" rather than an
    /// error.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or the catalog query fails.
    pub async fn table_columns(&self, schema: &str, table: &str) -> Result<Vec<LiveColumn>> {
        let client = self.pool.get().await.context("failed to acquire DB connection")?;
        let rows = client
            .query(
                "SELECT column_name, udt_name FROM information_schema.columns \
                 WHERE table_schema = $1 AND table_name = $2 ORDER BY ordinal_position",
                &[&schema, &table],
            )
            .await
            .context("failed to query information_schema.columns")?;
        Ok(rows
            .iter()
            .map(|row| LiveColumn {
                name:     row.get("column_name"),
                udt_name: row.get("udt_name"),
            })
            .collect())
    }

    /// Read the live RLS posture of `core.tb_entity_change_log` and whether the
    /// connecting role can read it under that posture (#437 F6 / #443).
    ///
    /// Returns `None` when the table is absent (or invisible to the connecting
    /// role) — the doctor check reads that as "not present, skipped".
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or the catalog query fails.
    pub async fn change_log_rls_status(&self) -> Result<Option<ChangeLogRlsStatus>> {
        let client = self.pool.get().await.context("failed to acquire DB connection")?;
        let rows = client
            .query(
                "SELECT c.relrowsecurity AS rls_enabled, \
                        (r.rolbypassrls OR r.rolsuper) AS can_bypass, \
                        (pg_get_userbyid(c.relowner) = current_user) AS is_owner, \
                        current_user::text AS role_name \
                 FROM pg_class c \
                   JOIN pg_namespace n ON n.oid = c.relnamespace \
                   JOIN pg_roles r ON r.rolname = current_user \
                 WHERE n.nspname = 'core' AND c.relname = 'tb_entity_change_log'",
                &[],
            )
            .await
            .context("failed to query change-log RLS status from pg_class/pg_roles")?;
        Ok(rows.first().map(|row| ChangeLogRlsStatus {
            rls_enabled: row.get("rls_enabled"),
            can_bypass:  row.get("can_bypass"),
            is_owner:    row.get("is_owner"),
            role_name:   row.get("role_name"),
        }))
    }

    /// Audit the given `sql_source` views (bare relnames) for `security_invoker`.
    ///
    /// A *default* view runs with the view owner's privileges and bypasses the
    /// caller's RLS; only a `security_invoker` view (PG 15+) honours base-table RLS
    /// — the requirement the cascade RLS boundary (and the query path) rely on.
    /// Returns which audited views lack the option, plus whether any RLS policy
    /// exists at all (the requirement only bites under RLS).
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or a catalog query fails.
    pub async fn security_invoker_audit(&self, views: &[String]) -> Result<SecurityInvokerAudit> {
        let client = self.pool.get().await.context("failed to acquire DB connection")?;
        let rls_in_use: bool = client
            .query_one("SELECT EXISTS (SELECT 1 FROM pg_policies) AS in_use", &[])
            .await
            .context("failed to query pg_policies for RLS usage")?
            .get("in_use");
        let rows = client
            .query(
                "SELECT c.relname \
                 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE c.relkind = 'v' AND c.relname = ANY($1) \
                   AND NOT EXISTS ( \
                     SELECT 1 FROM pg_options_to_table(c.reloptions) o \
                     WHERE o.option_name = 'security_invoker' \
                       AND lower(o.option_value) IN ('true', 'on', '1'))",
                &[&views],
            )
            .await
            .context("failed to audit view security_invoker options from pg_class")?;
        Ok(SecurityInvokerAudit {
            rls_in_use,
            views_without_invoker: rows.iter().map(|r| r.get::<_, String>("relname")).collect(),
        })
    }

    /// Read the privileges granted to the `PUBLIC` pseudo-role on the three
    /// change-log relations (the table + its two views) that exist in `core`
    /// (#443).
    ///
    /// Returns one [`PublicGrant`] per *present* relation, listing the privileges
    /// PUBLIC currently holds (empty when none). An empty vec means none of the
    /// three relations exist — the doctor check reads that as "not present,
    /// skipped". PUBLIC grants are read from `pg_class.relacl` via `aclexplode`
    /// (the PUBLIC grantee has OID `0`); a `NULL` ACL (the default for a fresh
    /// relation) explodes to no rows, i.e. PUBLIC holds nothing.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or the catalog query fails.
    pub async fn change_log_public_grants(&self) -> Result<Vec<PublicGrant>> {
        let client = self.pool.get().await.context("failed to acquire DB connection")?;
        let rows = client
            .query(
                "SELECT c.relname, \
                        COALESCE( \
                          array_agg(DISTINCT a.privilege_type) FILTER (WHERE a.grantee = 0), \
                          ARRAY[]::text[]) AS public_privs \
                 FROM pg_class c \
                   JOIN pg_namespace n ON n.oid = c.relnamespace \
                   LEFT JOIN LATERAL aclexplode(c.relacl) a ON true \
                 WHERE n.nspname = 'core' \
                   AND c.relname IN ('tb_entity_change_log', 'v_entity_change_log', \
                                     'v_entity_change_log_debezium') \
                 GROUP BY c.relname \
                 ORDER BY c.relname",
                &[],
            )
            .await
            .context("failed to query change-log PUBLIC grants from pg_class.relacl")?;
        Ok(rows
            .iter()
            .map(|row| PublicGrant {
                relname:    row.get("relname"),
                privileges: row.get("public_privs"),
            })
            .collect())
    }

    /// Read the security posture of `core.fn_entity_change_log_capture()` (#443 /
    /// #437 F6): whether it is `SECURITY DEFINER` (`pg_proc.prosecdef`) and the
    /// pinned `search_path` from `pg_proc.proconfig`, if any.
    ///
    /// Returns `None` when the function is absent — the doctor check reads that as
    /// "not present, skipped".
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or the catalog query fails.
    pub async fn capture_fn_security(&self) -> Result<Option<CaptureFnSecurity>> {
        let client = self.pool.get().await.context("failed to acquire DB connection")?;
        let rows = client
            .query(
                "SELECT p.prosecdef AS security_definer, \
                        (SELECT cfg FROM unnest(p.proconfig) AS cfg \
                          WHERE cfg LIKE 'search_path=%' LIMIT 1) AS search_path_cfg \
                 FROM pg_proc p \
                   JOIN pg_namespace n ON n.oid = p.pronamespace \
                 WHERE n.nspname = 'core' AND p.proname = 'fn_entity_change_log_capture' \
                 LIMIT 1",
                &[],
            )
            .await
            .context("failed to query capture-function security from pg_proc")?;
        Ok(rows.first().map(|row| {
            let raw: Option<String> = row.get("search_path_cfg");
            CaptureFnSecurity {
                security_definer: row.get("security_definer"),
                search_path:      raw
                    .map(|s| s.strip_prefix("search_path=").unwrap_or(&s).to_string()),
            }
        }))
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
