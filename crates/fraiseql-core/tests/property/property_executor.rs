//! Property-based tests for the runtime executor entry points.
//!
//! The runtime executor needs a database to do anything useful, so these
//! tests target the executor's pre-database stages — the public no-DB
//! surface that every request still flows through:
//!
//! - [`QueryMatcher::match_query`] (parse + fragment-resolve + directive-eval
//!   + lookup against the compiled schema)
//! - [`parse_query`] (raw GraphQL → `ParsedQuery`)
//! - [`extract_root_field_names`] (response-key iteration over root selections)
//!
//! These are the request-hot-path entry points whose invariants must hold
//! over arbitrary inputs: they must never panic, must be deterministic, must
//! return a structured `FraiseQLError` for unknown queries (not a panic), and
//! must propagate variables through to `QueryMatch.arguments` exactly.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::needless_collect)] // Reason: intermediate collect documents the invariant

use fraiseql_core::{
    error::FraiseQLError,
    graphql::parse_query,
    runtime::{QueryMatch, QueryMatcher},
    schema::{CompiledSchema, QueryDefinition, TypeDefinition},
};
use proptest::prelude::*;

// ============================================================================
// Test fixture helpers
// ============================================================================

/// Build a small compiled schema covering the two query roots most property
/// tests want to exercise: `users` (returns `[User]`) and `posts` (returns
/// `[Post]`). Schema is built from public API only.
fn build_test_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    schema.types.push(TypeDefinition::new("User", "v_user"));
    schema.types.push(TypeDefinition::new("Post", "v_post"));

    let mut users_query = QueryDefinition::new("users", "User");
    users_query.returns_list = true;
    users_query.sql_source = Some("v_user".to_string());
    schema.queries.push(users_query);

    let mut posts_query = QueryDefinition::new("posts", "Post");
    posts_query.returns_list = true;
    posts_query.sql_source = Some("v_post".to_string());
    schema.queries.push(posts_query);

    schema
}

/// Build a `QueryMatcher` over the test schema. `QueryMatcher::new` rebuilds
/// the index internally — this helper exists so each test starts from a
/// fresh matcher without sharing mutable state.
fn build_matcher() -> QueryMatcher {
    QueryMatcher::new(build_test_schema())
}

// ============================================================================
// Property tests: parser invariants on arbitrary input
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Invariant: `parse_query` never panics on arbitrary string input.
    /// Either returns `Ok(ParsedQuery)` or a structured `GraphQLParseError`.
    ///
    /// Uses `(?s).{0,400}` so the `.` metacharacter matches newlines too —
    /// the bare `.` form excluded `\n` and never exercised multi-line queries,
    /// which is the realistic case for any non-toy GraphQL document.
    #[test]
    fn prop_parse_query_never_panics(input in "(?s).{0,400}") {
        let _ = parse_query(&input);
    }

    /// Invariant: `parse_query` is deterministic — parsing the same input
    /// twice produces the same outcome.  When both calls succeed, every
    /// observable field of `ParsedQuery` must agree (not just the tuple
    /// `(operation_type, root_field, selections.len())`).  We compare via
    /// `serde_json::to_value` so all transitively-owned data — selections,
    /// variable defaults, fragment spreads, alias maps, directive args —
    /// participates in the equality check; the previous tuple subset would
    /// have passed a parser that non-deterministically reordered selections
    /// or expanded fragments differently across calls.
    #[test]
    fn prop_parse_query_deterministic(input in "(?s).{0,200}") {
        let r1 = parse_query(&input);
        let r2 = parse_query(&input);

        match (&r1, &r2) {
            (Ok(p1), Ok(p2)) => {
                let v1 = serde_json::to_value(p1)
                    .expect("ParsedQuery serializes infallibly");
                let v2 = serde_json::to_value(p2)
                    .expect("ParsedQuery serializes infallibly");
                prop_assert_eq!(v1, v2, "parse_query is non-deterministic for this input");
            }
            (Err(_), Err(_)) => {
                // Both errored — that's a consistent outcome.
            }
            _ => {
                prop_assert!(false, "Parser produced inconsistent results across calls");
            }
        }
    }
}

// ============================================================================
// Property tests: QueryMatcher::match_query invariants
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Invariant: `match_query` never panics on arbitrary input. Either
    /// returns a valid `QueryMatch` or a structured `FraiseQLError`. This is
    /// the "the security boundary doesn't crash" guarantee.
    ///
    /// Uses `(?s).{0,200}` so multi-line query bodies (newlines in the input)
    /// are also exercised; the bare `.` regex never reached the multi-line
    /// parser branches.
    #[test]
    fn prop_match_query_never_panics(input in "(?s).{0,200}") {
        let matcher = build_matcher();
        let _ = matcher.match_query(&input, None);
    }

    /// Invariant: for any query referencing a field NOT in the schema, the
    /// matcher must return a `FraiseQLError::Validation` (not a panic, not a
    /// parser error). Unknown roots are user input, not a parser bug.
    #[test]
    fn prop_match_query_unknown_root_returns_validation_error(
        unknown_root in "[a-z][a-zA-Z]{5,15}",
    ) {
        // The "users" and "posts" roots are in the test schema — generate a
        // root name that we are statistically very unlikely to collide with.
        prop_assume!(unknown_root != "users" && unknown_root != "posts");

        let matcher = build_matcher();
        let query = format!("{{ {} {{ id }} }}", unknown_root);
        let result = matcher.match_query(&query, None);

        match result {
            Err(FraiseQLError::Validation { .. }) => {
                // Expected — the matcher rejected the unknown root.
            }
            Ok(_) => {
                prop_assert!(false, "Matcher unexpectedly accepted unknown root '{}'", unknown_root);
            }
            Err(other) => {
                prop_assert!(
                    false,
                    "Unknown root should produce Validation error, got: {:?}",
                    other
                );
            }
        }
    }

    /// Invariant: when `match_query` succeeds against a known root, the
    /// returned `QueryMatch.query_def.name` equals the requested root field.
    /// This is the "we routed to the right SQL template" guarantee.
    #[test]
    fn prop_match_query_known_root_returns_correct_query_def(
        root in prop_oneof!["users", "posts"],
    ) {
        let matcher = build_matcher();
        let query = format!("{{ {} {{ id }} }}", root);
        let result = matcher.match_query(&query, None);

        let matched = result.expect("known root should match");
        let root_ref = root.as_str();
        prop_assert_eq!(matched.query_def.name.as_str(), root_ref);
        prop_assert_eq!(matched.parsed_query.root_field.as_str(), root_ref);
    }

    /// Invariant: `QueryMatch.arguments` reflects the variables JSON object
    /// passed in. Every top-level key in the input variables must appear in
    /// `arguments` with the same value. This guards against silently dropping
    /// a variable on the path from JSON → `QueryMatch`.
    #[test]
    fn prop_match_query_preserves_variables(
        keys in prop::collection::vec("[a-z][a-zA-Z0-9_]{1,10}", 0..6),
        values in prop::collection::vec(prop_oneof![
            any::<i64>().prop_map(serde_json::Value::from),
            "[a-zA-Z0-9 ]{0,30}".prop_map(serde_json::Value::from),
            Just(serde_json::Value::Bool(true)),
            Just(serde_json::Value::Null),
        ], 0..6),
    ) {
        // Pair keys with values up to the shorter length; dedup by key.
        let mut map = serde_json::Map::new();
        for (k, v) in keys.iter().zip(values.iter()) {
            map.insert(k.clone(), v.clone());
        }
        let variables = serde_json::Value::Object(map.clone());

        let matcher = build_matcher();
        let query = "{ users { id } }".to_string();
        let matched = matcher
            .match_query(&query, Some(&variables))
            .expect("static query must match");

        for (k, v) in &map {
            prop_assert!(
                matched.arguments.contains_key(k),
                "Variable '{}' missing from QueryMatch.arguments",
                k
            );
            prop_assert_eq!(
                matched.arguments.get(k),
                Some(v),
                "Variable '{}' value mismatched in QueryMatch.arguments",
                k
            );
        }
    }

    /// Invariant: `match_query` is deterministic — repeating the same call
    /// with the same input produces a fully-equal `QueryMatch`.  Cache-key
    /// derivation downstream depends on this, so we assert **complete**
    /// structural equality, not a curated tuple subset.
    ///
    /// Each component is compared explicitly so a failure points at the
    /// non-deterministic field directly:
    ///
    /// - `query_def` — derives `PartialEq`, compared directly (covers
    ///   `name`, `return_type`, `sql_source`, `returns_list`, etc.).
    /// - `fields`, `operation_name`, `arguments` — equality compares the
    ///   full `Vec` / `Option` / `HashMap` contents.
    /// - `selections` and `parsed_query` — neither derives `PartialEq` so
    ///   we compare via `serde_json::to_value`, which traverses every owned
    ///   field (selection sub-trees, parsed variables, fragment spreads,
    ///   directive arguments).  A regression in `extract_arguments` ordering
    ///   or in `FragmentResolver` expansion order would surface here.
    #[test]
    fn prop_match_query_deterministic(
        root in prop_oneof!["users", "posts"],
    ) {
        let matcher = build_matcher();
        let query = format!("{{ {} {{ id name }} }}", root);

        let r1: QueryMatch = matcher.match_query(&query, None).unwrap();
        let r2: QueryMatch = matcher.match_query(&query, None).unwrap();

        prop_assert_eq!(&r1.query_def, &r2.query_def);
        prop_assert_eq!(&r1.fields, &r2.fields);
        prop_assert_eq!(&r1.operation_name, &r2.operation_name);
        prop_assert_eq!(&r1.arguments, &r2.arguments);

        let selections_v1 = serde_json::to_value(&r1.selections)
            .expect("FieldSelection serializes infallibly");
        let selections_v2 = serde_json::to_value(&r2.selections)
            .expect("FieldSelection serializes infallibly");
        prop_assert_eq!(
            selections_v1,
            selections_v2,
            "QueryMatch.selections is non-deterministic for this input"
        );

        let parsed_v1 = serde_json::to_value(&r1.parsed_query)
            .expect("ParsedQuery serializes infallibly");
        let parsed_v2 = serde_json::to_value(&r2.parsed_query)
            .expect("ParsedQuery serializes infallibly");
        prop_assert_eq!(
            parsed_v1,
            parsed_v2,
            "QueryMatch.parsed_query is non-deterministic for this input"
        );
    }
}

// ============================================================================
// Property tests: extract_root_field_names invariants
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Invariant: `extract_root_field_names` returns exactly one item per
    /// root selection in the parsed query. Used by the multi-root pipeline
    /// to size buffers and route fields — an off-by-one here corrupts
    /// downstream response assembly.
    #[test]
    fn prop_extract_root_field_names_count_matches_selections(
        roots in prop::collection::vec("[a-z][a-zA-Z]{0,10}", 1..5),
    ) {
        let mut query = String::from("{ ");
        for r in &roots {
            query.push_str(r);
            query.push_str(" { id } ");
        }
        query.push('}');

        let Ok(parsed) = parse_query(&query) else {
            return Ok(()); // Skip inputs the parser rejects
        };

        let names: Vec<&str> =
            fraiseql_core::runtime::extract_root_field_names(&parsed).collect();

        prop_assert_eq!(names.len(), parsed.selections.len());
    }
}

// ============================================================================
// Multi-root + alias invariants
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Invariant: an alias on the root field overrides the field name in
    /// `extract_root_field_names`'s output (the response key, not the SQL
    /// name). Cache-key derivation and response assembly downstream both
    /// depend on the alias-wins convention.
    #[test]
    fn prop_extract_root_field_names_uses_alias(
        alias in "[a-z][a-zA-Z]{1,10}",
    ) {
        let query = format!("{{ {}: users {{ id }} }}", alias);
        let Ok(parsed) = parse_query(&query) else {
            return Ok(());
        };

        let names: Vec<&str> =
            fraiseql_core::runtime::extract_root_field_names(&parsed).collect();

        prop_assert_eq!(names.len(), 1);
        prop_assert_eq!(names[0], alias.as_str());
    }
}
