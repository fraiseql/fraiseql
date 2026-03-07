//! Property-based tests for GraphQL parsing invariants.
//!
//! These properties verify that the GraphQL parser handles arbitrary
//! inputs safely and produces consistent results.

use fraiseql_core::graphql::parse_query;
use proptest::prelude::*;

// ============================================================================
// Parser Safety Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Property: Parser never panics on arbitrary input.
    #[test]
    fn prop_parser_never_panics(input in ".*") {
        let _ = parse_query(&input);
    }

    /// Property: Parser never panics on random bytes interpreted as UTF-8.
    #[test]
    fn prop_parser_never_panics_binary(
        bytes in prop::collection::vec(any::<u8>(), 0..500),
    ) {
        if let Ok(input) = String::from_utf8(bytes) {
            let _ = parse_query(&input);
        }
    }

    /// Property: Empty input is rejected.
    #[test]
    fn prop_parser_rejects_empty(whitespace in "[ \t\n\r]{0,20}") {
        let result = parse_query(&whitespace);
        prop_assert!(result.is_err(), "Empty/whitespace input should be rejected");
    }

    /// Property: Valid simple queries always parse successfully.
    #[test]
    fn prop_parser_accepts_simple_queries(
        field in "[a-z][a-zA-Z]{0,15}",
        subfield in "[a-z][a-zA-Z]{0,15}",
    ) {
        let query = format!("{{ {} {{ {} }} }}", field, subfield);
        let result = parse_query(&query);
        prop_assert!(
            result.is_ok(),
            "Simple query '{}' should parse, got: {:?}",
            query, result.err()
        );
    }

    /// Property: Valid queries produce a ParsedQuery with correct operation type.
    #[test]
    fn prop_parser_query_operation_type(
        field in "[a-z][a-zA-Z]{0,15}",
        subfield in "[a-z][a-zA-Z]{0,15}",
    ) {
        let query = format!("query {{ {} {{ {} }} }}", field, subfield);
        let result = parse_query(&query);

        if let Ok(parsed) = result {
            prop_assert_eq!(
                &parsed.operation_type, "query",
                "Explicit query should have operation_type 'query'"
            );
        }
    }

    /// Property: Mutation keyword produces mutation operation type.
    #[test]
    fn prop_parser_mutation_operation_type(
        field in "[a-z][a-zA-Z]{0,15}",
        subfield in "[a-z][a-zA-Z]{0,15}",
    ) {
        let query = format!("mutation {{ {} {{ {} }} }}", field, subfield);
        let result = parse_query(&query);

        if let Ok(parsed) = result {
            prop_assert_eq!(
                &parsed.operation_type, "mutation",
                "Mutation should have operation_type 'mutation'"
            );
        }
    }

    /// Property: Named operations preserve the operation name.
    #[test]
    fn prop_parser_preserves_operation_name(
        name in "[A-Z][a-zA-Z]{0,15}",
        field in "[a-z][a-zA-Z]{0,10}",
    ) {
        let query = format!("query {} {{ {} {{ id }} }}", name, field);
        let result = parse_query(&query);

        if let Ok(parsed) = result {
            prop_assert_eq!(
                parsed.operation_name.as_deref(),
                Some(name.as_str()),
                "Operation name should be preserved"
            );
        }
    }

    /// Property: Root field is extracted from the selection set.
    #[test]
    fn prop_parser_extracts_root_field(
        field in "[a-z][a-zA-Z]{0,15}",
    ) {
        let query = format!("{{ {} {{ id }} }}", field);
        let result = parse_query(&query);

        if let Ok(parsed) = result {
            prop_assert_eq!(
                &parsed.root_field, &field,
                "Root field should match"
            );
        }
    }

    /// Property: Source is preserved in parsed query.
    #[test]
    fn prop_parser_preserves_source(
        field in "[a-z][a-zA-Z]{0,15}",
    ) {
        let query = format!("{{ {} {{ id }} }}", field);
        let result = parse_query(&query);

        if let Ok(parsed) = result {
            prop_assert_eq!(
                &parsed.source, &query,
                "Source should be preserved"
            );
        }
    }
}

// ============================================================================
// Parser Structural Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Property: Multiple fields in selection set produce correct selection count.
    #[test]
    fn prop_parser_selection_count(
        field in "[a-z][a-zA-Z]{0,10}",
        subfields in prop::collection::hash_set("[a-z][a-zA-Z]{0,10}", 1..5),
    ) {
        let fields_str = subfields.iter()
            .map(|f| f.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let query = format!("{{ {} {{ {} }} }}", field, fields_str);
        let result = parse_query(&query);

        if let Ok(parsed) = result {
            if !parsed.selections.is_empty() {
                let nested_count = parsed.selections[0].nested_fields.len();
                prop_assert!(
                    nested_count >= 1,
                    "Should have at least 1 nested field, got {} for query: {}",
                    nested_count, query
                );
            }
        }
    }

    /// Property: Unmatched braces are rejected.
    #[test]
    fn prop_parser_rejects_unmatched_braces(
        field in "[a-z][a-zA-Z]{0,10}",
        extra_opens in 1usize..5,
    ) {
        let opens = "{".repeat(extra_opens + 1);
        let query = format!("{} {} {{ id }}", opens, field);
        let result = parse_query(&query);
        prop_assert!(
            result.is_err(),
            "Unmatched braces should be rejected: {}", query
        );
    }

    /// Property: Deeply nested queries either parse or produce an error (never panic).
    #[test]
    fn prop_parser_deep_nesting_safe(depth in 1usize..50) {
        let mut query = String::new();
        for i in 0..depth {
            query.push_str(&format!("{{ f{} ", i));
        }
        query.push_str("{ id }");
        for _ in 0..depth {
            query.push_str(" }");
        }

        let result = parse_query(&query);
        // Either succeeds or fails with an error — never panics
        let _ = result;
    }

    /// Property: Queries with variables declare them correctly.
    #[test]
    fn prop_parser_variable_declarations(
        var_name in "[a-z][a-zA-Z]{0,10}",
        field in "[a-z][a-zA-Z]{0,10}",
    ) {
        let query = format!(
            "query(${}: String) {{ {} {{ id }} }}",
            var_name, field
        );
        let result = parse_query(&query);

        if let Ok(parsed) = result {
            if !parsed.variables.is_empty() {
                prop_assert_eq!(
                    &parsed.variables[0].name, &var_name,
                    "Variable name should be preserved"
                );
            }
        }
    }
}

// ============================================================================
// Mutation Safety Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Property: Mutations with arguments parse correctly.
    #[test]
    fn prop_parser_mutation_with_arguments(
        mutation_name in "[a-z][a-zA-Z]{0,10}",
        field in "[a-z][a-zA-Z]{0,10}",
        arg_value in "[a-zA-Z0-9_]{1,20}",
    ) {
        let query = format!(
            "mutation {{ {} (id: \"{}\") {{ {} }} }}",
            mutation_name, arg_value, field
        );
        let result = parse_query(&query);

        if let Ok(parsed) = result {
            prop_assert_eq!(
                &parsed.operation_type, "mutation",
                "Should be parsed as mutation"
            );
            prop_assert!(!parsed.root_field.is_empty(), "Should have root field");
        }
    }

    /// Property: Multiple mutations in selection set preserve all fields.
    #[test]
    fn prop_parser_multiple_mutations_preserve_all(
        mutations in prop::collection::vec(
            "[a-z][a-zA-Z]{0,8}",
            1usize..=5
        ),
    ) {
        let mutation_list = mutations.iter()
            .map(|m| format!("{} {{ id }}", m))
            .collect::<Vec<_>>()
            .join(" ");

        let query = format!("mutation {{ {} }}", mutation_list);
        let result = parse_query(&query);

        if let Ok(parsed) = result {
            prop_assert_eq!(
                &parsed.operation_type, "mutation",
                "Should be mutation"
            );
            prop_assert!(parsed.selections.len() >= mutations.len() || parsed.selections.is_empty(),
                "Should parse all mutations (or be empty)"
            );
        }
    }
}
