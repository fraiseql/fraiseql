#![no_main]

//! Fuzz the GraphQL-JSON → `WhereClause` parse boundary.
//!
//! The parse must either produce a clause or return a non-empty error. It must
//! never panic, and it must never silently drop a condition — a dropped `where:`
//! condition *widens* a result set, which is an authorization-adjacent failure
//! rather than a cosmetic one (#719).

use std::{collections::HashMap, sync::Arc};

use fraiseql_db::{
    ScalarFieldType, WhereClause,
    where_clause::{FieldTypeMap, SharedFieldTypes, WhereFieldInfo, WhereFieldSchema},
};
use libfuzzer_sys::fuzz_target;

/// One declared `where` key, spelled the way the published `{Entity}WhereInput`
/// spells it — which is what the parser compares a client's key against.
fn scalar(name: &str, cast: ScalarFieldType) -> (String, WhereFieldInfo) {
    (
        name.to_string(),
        WhereFieldInfo {
            declared_name: name.to_string(),
            is_relation:   false,
            relation_type: None,
            cast:          Some(cast),
        },
    )
}

/// A key the schema calls a relation, so a nested predicate on it is descended
/// into wholesale rather than read as an operator map.
fn relation(name: &str, target: &str) -> (String, WhereFieldInfo) {
    (
        name.to_string(),
        WhereFieldInfo {
            declared_name: name.to_string(),
            is_relation:   true,
            relation_type: Some(target.to_string()),
            cast:          None,
        },
    )
}

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

    // Three schemas, because `WhereFieldSchema` has three states and only the
    // third reaches the adjudication branch. `casts_only` leaves `known: None`,
    // which is "cannot adjudicate" — every key passes (#939). `with_known_keys`
    // is the shape a compiled schema produces, and it is the only one that can
    // reach the refuse-undeclared-key arm #1198 added; without it this target
    // would fuzz the parser as it stood *before* that change and could not see a
    // condition silently dropped by a key the type never declared, which is the
    // property this target exists for.
    let known: HashMap<String, WhereFieldInfo> = [
        scalar("id", ScalarFieldType::Text),
        scalar("count", ScalarFieldType::Integer),
        scalar("price", ScalarFieldType::Numeric),
        scalar("active", ScalarFieldType::Boolean),
        scalar("created_at", ScalarFieldType::DateTime),
        relation("machine", "Machine"),
    ]
    .into_iter()
    .collect();

    let schemas = [
        WhereFieldSchema::casts_only(Arc::clone(&untyped)),
        WhereFieldSchema::casts_only(Arc::clone(&typed)),
        WhereFieldSchema::with_known_keys(Arc::clone(&typed), known),
    ];

    for schema in &schemas {
        match WhereClause::from_graphql_json(&value, schema) {
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
