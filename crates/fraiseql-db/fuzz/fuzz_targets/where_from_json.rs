#![no_main]

//! Fuzz the GraphQL-JSON → `WhereClause` parse boundary.
//!
//! The parse must either produce a clause or return a non-empty error. It must
//! never panic, and it must never silently drop a condition — a dropped `where:`
//! condition *widens* a result set, which is an authorization-adjacent failure
//! rather than a cosmetic one (#719).

use fraiseql_db::{
    ScalarFieldType, WhereClause,
    where_clause::{FieldTypeMap, SharedFieldTypes},
};
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(s) else {
        return;
    };

    // Both halves of the seam. An empty map leaves the clause untyped and the
    // generator falls back to the JSON value's shape; a populated map takes the
    // `WhereClause::Typed` path, where the cast is chosen from the declared field
    // type. Those are different code paths and the typed one is the one #798/#800
    // were about, so fuzzing only the empty map would miss it.
    let untyped: SharedFieldTypes = Arc::new(FieldTypeMap::default());
    let typed: SharedFieldTypes = Arc::new(FieldTypeMap::from_pairs([
        ("id", ScalarFieldType::Text),
        ("count", ScalarFieldType::Integer),
        ("price", ScalarFieldType::Numeric),
        ("active", ScalarFieldType::Boolean),
        ("created_at", ScalarFieldType::DateTime),
        ("machine.serial_number", ScalarFieldType::Text),
    ]));

    for types in [&untyped, &typed] {
        match WhereClause::from_graphql_json(&value, types) {
            Ok(clause) => {
                let _ = serde_json::to_string(&clause);
            },
            Err(e) => {
                let msg = e.to_string();
                assert!(!msg.is_empty(), "Error must produce non-empty message");
            },
        }
    }
});
