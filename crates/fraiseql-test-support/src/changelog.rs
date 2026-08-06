//! The ONE provisioner for `core.tb_entity_change_log` in tests (#942/#982).
//!
//! The change-log contract table had no single owner and no single shape:
//! ten test suites across five crates each created their own flavour
//! (`object_id UUID NOT NULL`, `object_id TEXT NOT NULL`, subsets of the
//! contract), so a warm shared database served whichever shape ran last —
//! red-on-fresh for suites that provisioned nothing, red-on-warm for suites
//! whose additive DDL could not undo a stricter twin.
//!
//! [`entity_change_log_provision_sql`] is the single script every consumer
//! executes: a normalizing preamble (drop any differently-shaped twin — safe in
//! tests, where each suite seeds its own rows) followed by the CONTRACT DDL,
//! included byte-for-byte from the owning migration
//! (`crates/fraiseql-observers/migrations/08_create_entity_change_log_contract.sql`;
//! `fraiseql setup` vendors the same file, drift-tested). One producer, one
//! shape, no drift.

/// The contract DDL, byte-for-byte from the owning observers migration.
pub const ENTITY_CHANGE_LOG_CONTRACT_SQL: &str =
    include_str!("../../fraiseql-observers/migrations/08_create_entity_change_log_contract.sql");

/// The full provisioning script: normalize, then apply the contract.
///
/// Normalizing drops a possibly differently-shaped twin. Execute as a
/// multi-statement batch (`tokio_postgres::Client::batch_execute`,
/// `PostgresAdapter::execute_raw_query`, or equivalent).
#[must_use]
pub fn entity_change_log_provision_sql() -> String {
    format!(
        "CREATE SCHEMA IF NOT EXISTS core;\n\
         DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE;\n\
         DROP SEQUENCE IF EXISTS core.seq_entity_change_log CASCADE;\n\
         {ENTITY_CHANGE_LOG_CONTRACT_SQL}"
    )
}

/// The provisioning script as single statements.
///
/// For executors that cannot run a multi-statement batch
/// (`PostgresAdapter::execute_raw_query` prepares one statement per call).
/// Splits on `;` outside `$$`-quoted bodies, with `--` line comments stripped;
/// unit-tested against the real contract file.
#[must_use]
pub fn entity_change_log_provision_statements() -> Vec<String> {
    split_sql_statements(&entity_change_log_provision_sql())
}

fn split_sql_statements(script: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_dollar = false;
    let mut in_string = false;
    let mut chars = script.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            current.push(c);
            if c == '\'' {
                // '' is an escaped quote inside the string.
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    current.push('\'');
                } else {
                    in_string = false;
                }
            }
        } else if in_dollar {
            current.push(c);
            if c == '$' && chars.peek() == Some(&'$') {
                chars.next();
                current.push('$');
                in_dollar = false;
            }
        } else if c == '\'' {
            in_string = true;
            current.push(c);
        } else if c == '$' && chars.peek() == Some(&'$') {
            chars.next();
            current.push(c);
            current.push('$');
            in_dollar = true;
        } else if c == '-' && chars.peek() == Some(&'-') {
            // Line comment: consume to end of line.
            for c2 in chars.by_ref() {
                if c2 == '\n' {
                    current.push('\n');
                    break;
                }
            }
        } else if c == ';' {
            let stmt = current.trim();
            if !stmt.is_empty() {
                statements.push(stmt.to_string());
            }
            current.clear();
        } else {
            current.push(c);
        }
    }
    let tail = current.trim();
    if !tail.is_empty() {
        statements.push(tail.to_string());
    }
    statements
}

#[cfg(test)]
mod tests;
