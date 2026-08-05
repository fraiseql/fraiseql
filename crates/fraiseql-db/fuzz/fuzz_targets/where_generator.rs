#![no_main]

//! Fuzz WHERE-clause SQL generation.
//!
//! Generation must never panic on any clause the parser accepted, and the SQL it
//! emits must be structurally intact: a client-controlled field name or value
//! must not be able to close a quote or unbalance the parentheses. That is the
//! property #833 was about — its MySQL half went away with the dialect, but the
//! boundary it guards protects PostgreSQL too, so the field name is validated at
//! the parse boundary and this target holds the invariant downstream of it.

use fraiseql_db::{
    GenericWhereGenerator, PostgresDialect, ScalarFieldType, WhereClause,
    where_clause::{FieldTypeMap, SharedFieldTypes},
};
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;

/// Quotes are balanced and parentheses are balanced and never go negative.
///
/// Counting is enough here because the generator emits single-quoted literals
/// with doubled inner quotes; an unbalanced count is exactly the escape this is
/// looking for.
fn structurally_intact(sql: &str) -> bool {
    let quotes = sql.chars().filter(|c| *c == '\'').count();
    if quotes % 2 != 0 {
        return false;
    }
    let mut depth = 0i32;
    let mut in_literal = false;
    for c in sql.chars() {
        match c {
            '\'' => in_literal = !in_literal,
            '(' if !in_literal => depth += 1,
            ')' if !in_literal => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            },
            _ => {},
        }
    }
    depth == 0
}

fuzz_target!(|data: &str| {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return;
    };

    let types: SharedFieldTypes = Arc::new(FieldTypeMap::from_pairs([
        ("id", ScalarFieldType::Text),
        ("count", ScalarFieldType::Integer),
        ("created_at", ScalarFieldType::DateTime),
    ]));

    let Ok(clause) = WhereClause::from_graphql_json(&value, &types) else {
        return;
    };

    // PostgreSQL is the only dialect since v2.15.0 (#374). MySQL, SQLite and
    // SQL Server were removed; this target used to run all four.
    if let Ok(sql) = GenericWhereGenerator::new(PostgresDialect).generate(&clause) {
        assert!(
            structurally_intact(&sql.0),
            "generated SQL is not structurally intact: {sql:?}"
        );
    }
});
