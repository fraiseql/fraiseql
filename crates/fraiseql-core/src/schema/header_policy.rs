//! The single, cross-crate definition of "a row-level security policy reads a session
//! variable taken from a request header".
//!
//! A `source = "header"` session variable is client-controlled: any caller sends any value.
//! A policy that reads one scopes rows by what the client claims. Two consumers ask the
//! question and must give the same answer:
//!
//! - `fraiseql-server` refuses to boot ([`header_variables_read_by_policies`], through the live
//!   `DatabaseAdapter`).
//! - `fraiseql-cli` `doctor --against-db` reports a failure (the same SQL, through its own catalog
//!   connection).
//!
//! Both build the query with [`policies_reading_settings_sql`], read its rows with
//! [`policy_setting_reads`] and describe each with [`PolicySettingRead::described`], so the
//! two cannot drift on what is matched or on what is said.
//!
//! **What is matched.** The expression PostgreSQL stores for the policy (`pg_policies.qual`
//! and `with_check`, both deparsed by `pg_get_expr`), searched for `current_setting('<name>'`
//! with the name compared case-insensitively. The deparser rewrites whatever was written, so
//! whitespace, the optional second argument and the spelling of the name do not hide a read.
//! **What is not:** a policy that reads the setting only through a function it calls, or
//! that builds the setting's name from parts.

use std::collections::HashMap;

use crate::{
    db::DatabaseAdapter,
    error::{FraiseQLError, Result},
    schema::{CompiledSchema, SessionVariableSource},
};

/// What to do about a policy that reads a header-sourced session variable.
pub const HEADER_VARIABLE_POLICY_REMEDY: &str = "Scope rows with a `jwt` or `enrichment` session variable; a header must never feed \
     row-level security or tenant scoping.";

/// A row-level security policy whose expression reads a session setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySettingRead {
    /// The policy's table schema.
    pub schema:  String,
    /// The policy's table.
    pub table:   String,
    /// The policy's name.
    pub policy:  String,
    /// The setting its `USING` or `WITH CHECK` expression reads, as the schema names it.
    pub setting: String,
}

impl PolicySettingRead {
    /// The one sentence both the server's refusal and `doctor` say about this read.
    #[must_use]
    pub fn described(&self) -> String {
        format!(
            "policy {} on {}.{} reads {}, which comes from a request header: any client can \
             send any value",
            self.policy, self.schema, self.table, self.setting
        )
    }
}

/// The settings `schema` takes from a request header, in declaration order.
#[must_use]
pub fn header_sourced_settings(schema: &CompiledSchema) -> Vec<String> {
    schema
        .session_variables
        .variables
        .iter()
        .filter(|m| matches!(m.source, SessionVariableSource::Header { .. }))
        .map(|m| m.name.clone())
        .collect()
}

/// The catalog query naming each `(policy, setting)` pair where a policy's `USING` or
/// `WITH CHECK` expression reads one of `settings`. Columns: `schema_name`, `table_name`,
/// `policy_name`, `setting`.
///
/// The setting names are inlined as quote-doubled literals: they come from the compiled
/// schema, and both consumers run it as a plain statement.
#[must_use]
pub fn policies_reading_settings_sql(settings: &[String]) -> String {
    let literals: Vec<String> =
        settings.iter().map(|s| format!("'{}'", s.replace('\'', "''"))).collect();
    format!(
        "SELECT p.schemaname::text AS schema_name, p.tablename::text AS table_name, \
                p.policyname::text AS policy_name, s.name AS setting \
         FROM pg_catalog.pg_policies p CROSS JOIN unnest(ARRAY[{}]::text[]) AS s(name) \
         WHERE strpos(lower(coalesce(p.qual, '') || ' ' || coalesce(p.with_check, '')), \
                      'current_setting(''' || lower(s.name) || '''') > 0 \
         ORDER BY 1, 2, 3, 4",
        literals.join(", ")
    )
}

/// Read the rows of [`policies_reading_settings_sql`].
///
/// # Errors
///
/// `FraiseQLError::Database` if a row lacks one of the four text columns: the answer would
/// otherwise be read as "no policy", which is the permissive one.
pub fn policy_setting_reads<H: std::hash::BuildHasher>(
    rows: &[HashMap<String, serde_json::Value, H>],
) -> Result<Vec<PolicySettingRead>> {
    rows.iter()
        .map(|row| {
            let column = |name: &str| {
                row.get(name)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| FraiseQLError::Database {
                        message:    format!(
                            "reading pg_policies: a row has no text column `{name}`: {row:?}"
                        ),
                        sql_state:  None,
                        constraint: None,
                    })
            };
            Ok(PolicySettingRead {
                schema:  column("schema_name")?,
                table:   column("table_name")?,
                policy:  column("policy_name")?,
                setting: column("setting")?,
            })
        })
        .collect()
}

/// The row-level security policies that read a session variable `schema` takes from a
/// request header. Empty, with no query, when the schema declares no header source.
///
/// Matches the policy's stored expression, so a policy that reads the setting only through a
/// function it calls, or that builds the setting's name from parts, is not seen.
///
/// # Errors
///
/// `FraiseQLError::Database` if the catalog cannot be read or answers in another shape.
pub async fn header_variables_read_by_policies<A: DatabaseAdapter + ?Sized>(
    schema: &CompiledSchema,
    adapter: &A,
) -> Result<Vec<PolicySettingRead>> {
    let settings = header_sourced_settings(schema);
    if settings.is_empty() {
        return Ok(Vec::new());
    }
    let rows = adapter.execute_raw_query(&policies_reading_settings_sql(&settings)).await?;
    policy_setting_reads(&rows)
}

#[cfg(test)]
mod tests;
