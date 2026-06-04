//! Tests for `graphql/` modules.
//! Re-export items not in `crate::graphql::*` so submodules can reach them via `use super::*`.
#![allow(unused_imports)] // Reason: blanket re-exports for test convenience
#![allow(clippy::panic)] // Reason: test code, panics acceptable
pub use std::collections::{HashMap, HashSet};

pub use graphql_parser::query;
pub use serde_json::json;

mod fragment_resolver_tests {

    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    fn make_field(name: &str, nested: Vec<FieldSelection>) -> FieldSelection {
        FieldSelection {
            name:          name.to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: nested,
            directives:    vec![],
        }
    }

    fn make_fragment(name: &str, selections: Vec<FieldSelection>) -> FragmentDefinition {
        FragmentDefinition {
            name: name.to_string(),
            type_condition: "User".to_string(),
            selections,
            fragment_spreads: vec![],
        }
    }

    #[test]
    fn test_simple_fragment_spread_resolution() {
        let fragment =
            make_fragment("UserFields", vec![make_field("id", vec![]), make_field("name", vec![])]);

        let selections = vec![FieldSelection {
            name:          "...UserFields".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        }];

        let resolver = FragmentResolver::new(&[fragment]);
        let result_selections = resolver.resolve_spreads(&selections).unwrap();

        assert_eq!(result_selections.len(), 2);
        assert_eq!(result_selections[0].name, "id");
        assert_eq!(result_selections[1].name, "name");
    }

    #[test]
    fn test_fragment_not_found() {
        let selections = vec![FieldSelection {
            name:          "...NonexistentFragment".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        }];

        let resolver = FragmentResolver::new(&[]);
        let result = resolver.resolve_spreads(&selections);

        assert!(matches!(result, Err(FragmentError::FragmentNotFound(_))));
    }

    #[test]
    fn test_nested_fragment_spreads() {
        // Fragment A references fields
        let fragment_a = make_fragment("FragmentA", vec![make_field("id", vec![])]);

        // Fragment B spreads Fragment A
        let fragment_b = make_fragment(
            "FragmentB",
            vec![
                FieldSelection {
                    name:          "...FragmentA".to_string(),
                    alias:         None,
                    arguments:     vec![],
                    nested_fields: vec![],
                    directives:    vec![],
                },
                make_field("name", vec![]),
            ],
        );

        // Query spreads Fragment B
        let selections = vec![FieldSelection {
            name:          "...FragmentB".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        }];

        let resolver = FragmentResolver::new(&[fragment_a, fragment_b]);
        let result_selections = resolver.resolve_spreads(&selections).unwrap();

        assert_eq!(result_selections.len(), 2);
        assert_eq!(result_selections[0].name, "id");
        assert_eq!(result_selections[1].name, "name");
    }

    #[test]
    fn test_inline_fragment_matching_type() {
        let selections = vec![make_field("id", vec![]), make_field("name", vec![])];

        let result = FragmentResolver::evaluate_inline_fragment(&selections, Some("User"), "User");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "id");
    }

    #[test]
    fn test_inline_fragment_non_matching_type() {
        let selections = vec![make_field("id", vec![]), make_field("name", vec![])];

        let result = FragmentResolver::evaluate_inline_fragment(&selections, Some("User"), "Post");

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_inline_fragment_without_type_condition() {
        let selections = vec![make_field("id", vec![]), make_field("name", vec![])];

        let result = FragmentResolver::evaluate_inline_fragment(&selections, None, "User");

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_merge_non_conflicting_fields() {
        let base = vec![make_field("id", vec![]), make_field("name", vec![])];

        let additional = vec![make_field("email", vec![])];

        let merged = FragmentResolver::merge_selections(&base, additional);

        assert_eq!(merged.len(), 3);
        let names: Vec<_> = merged.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"id"));
        assert!(names.contains(&"name"));
        assert!(names.contains(&"email"));
    }

    #[test]
    fn test_merge_conflicting_fields_with_alias() {
        let base = vec![FieldSelection {
            name:          "user".to_string(),
            alias:         Some("primaryUser".to_string()),
            arguments:     vec![],
            nested_fields: vec![make_field("id", vec![])],
            directives:    vec![],
        }];

        let additional = vec![FieldSelection {
            name:          "user".to_string(),
            alias:         Some("primaryUser".to_string()),
            arguments:     vec![],
            nested_fields: vec![make_field("name", vec![])],
            directives:    vec![],
        }];

        let merged = FragmentResolver::merge_selections(&base, additional);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].nested_fields.len(), 2); // id and name merged
    }

    #[test]
    fn test_depth_limit() {
        // Create deeply nested fragment references
        let mut fragments = vec![];
        for i in 0..12 {
            let name = format!("Fragment{i}");
            let next_spread = if i < 11 {
                FieldSelection {
                    name:          format!("...Fragment{}", i + 1),
                    alias:         None,
                    arguments:     vec![],
                    nested_fields: vec![],
                    directives:    vec![],
                }
            } else {
                make_field("field", vec![])
            };

            fragments.push(FragmentDefinition {
                name,
                type_condition: "User".to_string(),
                selections: vec![next_spread],
                fragment_spreads: vec![],
            });
        }

        let selections = vec![FieldSelection {
            name:          "...Fragment0".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        }];

        let resolver = FragmentResolver::new(&fragments);
        let result = resolver.resolve_spreads(&selections);

        assert!(matches!(result, Err(FragmentError::FragmentDepthExceeded(_))));
    }

    #[test]
    fn test_circular_reference_detection() {
        // FragmentA -> FragmentB -> FragmentA (circular)
        let fragment_a = FragmentDefinition {
            name:             "FragmentA".to_string(),
            type_condition:   "User".to_string(),
            selections:       vec![FieldSelection {
                name:          "...FragmentB".to_string(),
                alias:         None,
                arguments:     vec![],
                nested_fields: vec![],
                directives:    vec![],
            }],
            fragment_spreads: vec!["FragmentB".to_string()],
        };

        let fragment_b = FragmentDefinition {
            name:             "FragmentB".to_string(),
            type_condition:   "User".to_string(),
            selections:       vec![FieldSelection {
                name:          "...FragmentA".to_string(),
                alias:         None,
                arguments:     vec![],
                nested_fields: vec![],
                directives:    vec![],
            }],
            fragment_spreads: vec!["FragmentA".to_string()],
        };

        let selections = vec![FieldSelection {
            name:          "...FragmentA".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        }];

        let resolver = FragmentResolver::new(&[fragment_a, fragment_b]);
        let result = resolver.resolve_spreads(&selections);

        assert!(matches!(result, Err(FragmentError::CircularFragmentReference)));
    }
}

mod fragments_tests {

    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_no_cycles() {
        let graph = FragmentGraph {
            dependencies: HashMap::from([
                ("FragA".to_string(), HashSet::from(["FragB".to_string()])),
                ("FragB".to_string(), HashSet::from(["FragC".to_string()])),
                ("FragC".to_string(), HashSet::new()),
            ]),
        };
        graph
            .detect_cycles()
            .unwrap_or_else(|c| panic!("expected no cycles, got: {c:?}"));
    }

    #[test]
    fn test_simple_cycle() {
        let graph = FragmentGraph {
            dependencies: HashMap::from([
                ("FragA".to_string(), HashSet::from(["FragB".to_string()])),
                ("FragB".to_string(), HashSet::from(["FragA".to_string()])),
            ]),
        };
        let cycle = graph.detect_cycles().expect_err("expected cycle to be detected");
        // Cycle can start from either FragA or FragB depending on iteration order
        assert!(cycle.len() >= 2, "cycle must contain at least 2 fragments, got: {cycle:?}");
    }

    #[test]
    fn test_complex_cycle() {
        let graph = FragmentGraph {
            dependencies: HashMap::from([
                ("FragA".to_string(), HashSet::from(["FragB".to_string()])),
                ("FragB".to_string(), HashSet::from(["FragC".to_string()])),
                ("FragC".to_string(), HashSet::from(["FragA".to_string()])),
                ("FragD".to_string(), HashSet::from(["FragE".to_string()])),
                ("FragE".to_string(), HashSet::new()),
            ]),
        };
        let result = graph.detect_cycles();
        assert!(result.is_err(), "expected A→B→C→A cycle to be detected, got: {result:?}");
    }

    #[test]
    fn test_multiple_cycles() {
        let graph = FragmentGraph {
            dependencies: HashMap::from([
                ("FragA".to_string(), HashSet::from(["FragB".to_string()])),
                ("FragB".to_string(), HashSet::from(["FragA".to_string()])),
                ("FragC".to_string(), HashSet::from(["FragD".to_string()])),
                ("FragD".to_string(), HashSet::from(["FragC".to_string()])),
            ]),
        };
        let cycle = graph.detect_cycles().expect_err("expected at least one cycle to be detected");
        // Should detect one of the cycles (DFS order dependent)
        assert!(
            cycle.len() >= 2,
            "cycle must contain at least 2 fragments (A→B or C→D), got: {cycle:?}"
        );
    }

    #[test]
    fn test_self_reference_cycle() {
        let graph = FragmentGraph {
            dependencies: HashMap::from([(
                "FragA".to_string(),
                HashSet::from(["FragA".to_string()]),
            )]),
        };
        let result = graph.detect_cycles();
        assert!(
            result.is_err(),
            "expected self-reference FragA→FragA to be detected as cycle, got: {result:?}"
        );
    }
}

mod parser_tests {

    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;
    use crate::graphql::parser::{MAX_SERIALIZE_DEPTH, serialize_value};

    #[test]
    fn test_parse_simple_query() {
        let query = "query { users { id name } }";
        let parsed = parse_query(query).unwrap();

        assert_eq!(parsed.operation_type, "query");
        assert_eq!(parsed.root_field, "users");
        assert_eq!(parsed.selections.len(), 1);
        assert_eq!(parsed.selections[0].nested_fields.len(), 2);
    }

    #[test]
    fn test_parse_query_with_arguments() {
        let query = r#"
            query {
                users(where: {status: "active"}, limit: 10) {
                    id
                    name
                }
            }
        "#;
        let parsed = parse_query(query).unwrap();

        let first_field = &parsed.selections[0];
        assert_eq!(first_field.arguments.len(), 2);
        assert_eq!(first_field.arguments[0].name, "where");
        assert_eq!(first_field.arguments[1].name, "limit");
    }

    #[test]
    fn test_parse_mutation() {
        let query = "mutation { createUser(input: {}) { id } }";
        let parsed = parse_query(query).unwrap();

        assert_eq!(parsed.operation_type, "mutation");
        assert_eq!(parsed.root_field, "createUser");
    }

    #[test]
    fn test_parse_query_with_variables() {
        let query = r"
            query GetUsers($where: UserWhere!) {
                users(where: $where) {
                    id
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        assert_eq!(parsed.variables.len(), 1);
        assert_eq!(parsed.variables[0].name, "where");
    }

    #[test]
    fn test_parse_query_with_integer_argument() {
        let query = r"
            query {
                users(limit: 42, offset: 100) {
                    id
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        let first_field = &parsed.selections[0];
        assert_eq!(first_field.arguments.len(), 2);

        assert_eq!(first_field.arguments[0].name, "limit");
        assert_eq!(first_field.arguments[0].value_type, "int");
        assert_eq!(first_field.arguments[0].value_json, "42");

        assert_eq!(first_field.arguments[1].name, "offset");
        assert_eq!(first_field.arguments[1].value_type, "int");
        assert_eq!(first_field.arguments[1].value_json, "100");
    }

    #[test]
    fn test_parse_query_with_fragment() {
        let query = r"
            fragment UserFields on User {
                id
                name
                email
            }

            query {
                users {
                    ...UserFields
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        // Should have fragment definition
        assert_eq!(parsed.fragments.len(), 1);
        assert_eq!(parsed.fragments[0].name, "UserFields");
        assert_eq!(parsed.fragments[0].type_condition, "User");
        assert_eq!(parsed.fragments[0].selections.len(), 3);

        // Selection should have fragment spread
        assert_eq!(parsed.selections[0].nested_fields.len(), 1);
        assert_eq!(parsed.selections[0].nested_fields[0].name, "...UserFields");
    }

    #[test]
    fn test_parse_query_with_directives() {
        let query = r"
            query($skipEmail: Boolean!) {
                users {
                    id
                    email @skip(if: $skipEmail)
                    name @include(if: true)
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        let user_fields = &parsed.selections[0].nested_fields;
        assert_eq!(user_fields.len(), 3);

        // id has no directives
        assert!(user_fields[0].directives.is_empty());

        // email has @skip
        assert_eq!(user_fields[1].directives.len(), 1);
        assert_eq!(user_fields[1].directives[0].name, "skip");

        // name has @include
        assert_eq!(user_fields[2].directives.len(), 1);
        assert_eq!(user_fields[2].directives[0].name, "include");
    }

    #[test]
    fn test_parse_query_with_alias() {
        let query = r"
            query {
                users {
                    id
                    writer: author {
                        name
                    }
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        let user_fields = &parsed.selections[0].nested_fields;
        assert_eq!(user_fields.len(), 2);

        // Check aliased field
        let aliased_field = &user_fields[1];
        assert_eq!(aliased_field.name, "author");
        assert_eq!(aliased_field.alias, Some("writer".to_string()));
    }

    #[test]
    fn test_parse_inline_fragment() {
        let query = r"
            query {
                users {
                    id
                    ... on Admin {
                        permissions
                    }
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        let user_fields = &parsed.selections[0].nested_fields;
        assert_eq!(user_fields.len(), 2);

        // Check inline fragment
        assert_eq!(user_fields[1].name, "...on Admin");
        assert_eq!(user_fields[1].nested_fields.len(), 1);
        assert_eq!(user_fields[1].nested_fields[0].name, "permissions");
    }

    // ── serialize_value depth guard ────────────────────────────────────────────

    #[test]
    fn test_serialize_value_flat_list_accepted() {
        // A flat list of scalars is well within the depth limit.
        let value = query::Value::List(vec![
            query::Value::Int(graphql_parser::query::Number::from(1_i32)),
            query::Value::String("hello".to_string()),
            query::Value::Boolean(true),
        ]);
        let result = serialize_value(&value);
        assert_eq!(result, r#"[1,"hello",true]"#);
    }

    #[test]
    fn test_serialize_value_nested_at_limit_accepted() {
        // Build a list nested exactly MAX_SERIALIZE_DEPTH levels — must serialize.
        let mut v: query::Value<String> = query::Value::Boolean(true);
        for _ in 0..MAX_SERIALIZE_DEPTH {
            v = query::Value::List(vec![v]);
        }
        let result = serialize_value(&v);
        // Verify it didn't fall back to "null" — it should contain "true".
        assert!(result.contains("true"), "value at limit should serialize correctly: {result}");
    }

    #[test]
    fn test_serialize_value_exceeds_depth_returns_null() {
        // Build a list nested MAX_SERIALIZE_DEPTH + 1 levels — must return "null".
        let mut v: query::Value<String> = query::Value::Boolean(true);
        for _ in 0..=MAX_SERIALIZE_DEPTH {
            v = query::Value::List(vec![v]);
        }
        let result = serialize_value(&v);
        assert_eq!(result, "null", "over-limit value must fall back to null: {result}");
    }

    #[test]
    fn test_serialize_value_deeply_nested_object_returns_null() {
        // Deeply nested object should also hit the depth cap.
        let mut v: query::Value<String> = query::Value::Boolean(false);
        for i in 0..=MAX_SERIALIZE_DEPTH {
            let mut map = std::collections::BTreeMap::new();
            map.insert(format!("k{i}"), v);
            v = query::Value::Object(map);
        }
        let result = serialize_value(&v);
        assert_eq!(result, "null", "over-limit object must fall back to null: {result}");
    }
}

mod require_permission_directive_tests {

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_permission_matches_exact() {
        assert!(RequirePermissionDirective::permission_matches(
            "query:users:read",
            "query:users:read"
        ));
        assert!(!RequirePermissionDirective::permission_matches(
            "query:users:read",
            "query:users:write"
        ));
    }

    #[test]
    fn test_permission_matches_wildcard() {
        assert!(RequirePermissionDirective::permission_matches("*:*", "query:users:read"));
        assert!(RequirePermissionDirective::permission_matches("query:*", "query:users:read"));
        assert!(!RequirePermissionDirective::permission_matches(
            "mutation:*",
            "query:users:read"
        ));
    }
}

mod types_tests {

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_parsed_query_signature() {
        let query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: Some("GetUsers".to_string()),
            root_field:     "users".to_string(),
            selections:     vec![],
            variables:      vec![],
            fragments:      vec![],
            source:         std::sync::Arc::from("{ users { id name } }"),
        };

        assert_eq!(query.signature(), "query::users");
    }

    #[test]
    fn test_parsed_query_cacheable() {
        let query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: None,
            root_field:     "users".to_string(),
            selections:     vec![],
            variables:      vec![], // No variables = cacheable
            fragments:      vec![],
            source:         std::sync::Arc::from("{ users { id } }"),
        };

        assert!(query.is_cacheable());
    }

    #[test]
    fn test_parsed_query_not_cacheable() {
        let query = ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: None,
            root_field:     "users".to_string(),
            selections:     vec![],
            variables:      vec![VariableDefinition {
                name:          "limit".to_string(),
                var_type:      GraphQLType {
                    name:          "Int".to_string(),
                    nullable:      false,
                    list:          false,
                    list_nullable: false,
                },
                default_value: None,
            }],
            fragments:      vec![],
            source:         std::sync::Arc::from(
                "query($limit: Int) { users(limit: $limit) { id } }",
            ),
        };

        assert!(!query.is_cacheable());
    }

    #[test]
    fn test_field_selection_response_key() {
        let field_no_alias = FieldSelection {
            name:          "author".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        };
        assert_eq!(field_no_alias.response_key(), "author");

        let field_with_alias = FieldSelection {
            name:          "author".to_string(),
            alias:         Some("writer".to_string()),
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        };
        assert_eq!(field_with_alias.response_key(), "writer");
    }

    #[test]
    fn test_graphql_argument_equality() {
        let arg1 = GraphQLArgument {
            name:       "where".to_string(),
            value_type: "object".to_string(),
            value_json: r#"{"id": 1}"#.to_string(),
        };

        let arg2 = GraphQLArgument {
            name:       "where".to_string(),
            value_type: "object".to_string(),
            value_json: r#"{"id": 1}"#.to_string(),
        };

        assert_eq!(arg1, arg2);
    }

    #[test]
    fn test_fragment_definition() {
        let fragment = FragmentDefinition {
            name:             "UserFields".to_string(),
            type_condition:   "User".to_string(),
            selections:       vec![],
            fragment_spreads: vec![],
        };

        assert_eq!(fragment.name, "UserFields");
        assert_eq!(fragment.type_condition, "User");
    }

    #[test]
    fn test_parsed_query_default() {
        let query = ParsedQuery::default();

        assert_eq!(query.operation_type, "query");
        assert_eq!(query.root_field, "");
        assert!(query.operation_name.is_none());
        assert!(query.selections.is_empty());
        assert!(query.variables.is_empty());
        assert!(query.fragments.is_empty());
    }
}

mod complexity_tests {

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    // ── Regression tests: operation names and arguments must NOT be counted ──

    #[test]
    fn test_operation_name_not_counted_as_field() {
        let validator = RequestValidator::default();
        let metrics = validator
            .analyze("query getUserPosts { users { id name } }")
            .expect("valid query");
        // "getUserPosts" is the operation name — must not count as a field.
        // Fields: users→(id, name) = complexity 3
        assert!(
            metrics.complexity <= 10,
            "operation name must not inflate complexity; got {metrics:?}"
        );
    }

    #[test]
    fn test_arguments_not_counted_as_fields() {
        let validator = RequestValidator::default();
        let metrics = validator
            .analyze("{ users(limit: 10, offset: 0) { id } }")
            .expect("valid query");
        // "limit" and "offset" are arguments, NOT fields.
        assert!(
            metrics.complexity < 50,
            "arguments must not be counted as fields; got {metrics:?}"
        );
    }

    // ── Depth ──

    #[test]
    fn test_simple_query_depth() {
        let validator = RequestValidator::default();
        let metrics = validator.analyze("{ users { id name } }").expect("valid");
        assert_eq!(metrics.depth, 2);
    }

    #[test]
    fn test_deeply_nested_query_depth() {
        let validator = RequestValidator::default();
        let query = "{ a { b { c { d { e { f { g { h } } } } } } } }";
        let metrics = validator.analyze(query).expect("valid");
        assert!(metrics.depth >= 8, "expected depth ≥ 8, got {}", metrics.depth);
    }

    #[test]
    fn test_depth_validation_pass() {
        let validator = RequestValidator::default().with_max_depth(5);
        validator
            .validate_query("{ user { id } }")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_depth_validation_fail() {
        let validator = RequestValidator::default().with_max_depth(3);
        let deep = "{ user { profile { settings { theme } } } }";
        let result = validator.validate_query(deep);
        assert!(
            matches!(result, Err(ComplexityValidationError::QueryTooDeep { .. })),
            "expected QueryTooDeep, got: {result:?}"
        );
    }

    // ── Fragment depth bypass ──

    #[test]
    fn test_fragment_depth_bypass_blocked() {
        let validator = RequestValidator::new().with_max_depth(3);
        let query = "
            fragment Deep on User { a { b { c { d { e } } } } }
            query { ...Deep }
        ";
        assert!(
            validator.validate_query(query).is_err(),
            "fragment depth bypass must be blocked"
        );
    }

    #[test]
    fn test_shallow_fragment_allowed() {
        let validator = RequestValidator::new().with_max_depth(5);
        let query = "
            fragment UserFields on User { id name email }
            query { user { ...UserFields } }
        ";
        validator.validate_query(query).unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    // ── Complexity ──

    #[test]
    fn test_complexity_validation_pass() {
        let validator = RequestValidator::default().with_max_complexity(20);
        validator
            .validate_query("query { user { id name email } }")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_pagination_limit_multiplier() {
        let validator = RequestValidator::new().with_max_complexity(50);
        let query = "query { users(first: 100) { id name } }";
        assert!(
            validator.validate_query(query).is_err(),
            "high pagination limits must increase complexity"
        );
    }

    #[test]
    fn test_nested_list_multiplier() {
        let validator = RequestValidator::new().with_max_complexity(50);
        let query = "query { users(first: 10) { friends(first: 10) { id } } }";
        assert!(
            validator.validate_query(query).is_err(),
            "nested list multipliers must compound"
        );
    }

    // ── Overflow safety (integer-overflow DoS / fuzz no-panic invariant) ──

    /// Build `query { f0(first: 100) { f1(first: 100) { ... { scalar } } } }`
    /// with `levels` nested multiplier fields. Each `first: 100` compounds
    /// multiplicatively in the complexity scorer, so ~12 levels score past
    /// `usize::MAX` (100^12 ≈ 1e24).
    fn deep_multiplied_query(levels: usize) -> String {
        let mut inner = String::from("scalar");
        for i in 0..levels {
            inner = format!("f{i}(first: 100) {{ {inner} }}");
        }
        format!("query {{ {inner} }}")
    }

    #[test]
    fn analyze_saturates_instead_of_overflowing_on_deep_pagination() {
        // `analyze()` has no depth pre-check, so the scorer walks the full tree.
        // The compounding multiplier must saturate at usize::MAX, never panic
        // (the fuzz invariant) nor wrap to a small value.
        let validator = RequestValidator::new();
        let metrics = validator
            .analyze(&deep_multiplied_query(12))
            .expect("deep query must analyze without panicking");
        assert_eq!(
            metrics.complexity,
            usize::MAX,
            "overflowing complexity must saturate, not wrap"
        );
    }

    #[test]
    fn validate_fails_closed_on_overflowing_complexity() {
        // With depth validation off, the deep query reaches the complexity gate.
        // A saturated score must FAIL CLOSED (rejected) — never wrap under the
        // limit and slip through.
        let validator =
            RequestValidator::new().with_max_complexity(100).with_depth_validation(false);
        let result = validator.validate_query(&deep_multiplied_query(12));
        assert!(
            matches!(result, Err(ComplexityValidationError::QueryTooComplex { .. })),
            "overflowing query must be rejected, got {result:?}"
        );
    }

    // ── Aliases ──

    #[test]
    fn test_alias_count_within_limit() {
        let validator = RequestValidator::new().with_max_aliases(5);
        let query = "query { a: user { id } b: user { id } c: user { id } }";
        validator.validate_query(query).unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_alias_count_exceeds_limit() {
        let validator = RequestValidator::new().with_max_aliases(2);
        let query = "query { a: user { id } b: user { id } c: user { id } }";
        assert!(
            matches!(
                validator.validate_query(query),
                Err(ComplexityValidationError::TooManyAliases {
                    actual_aliases: 3,
                    ..
                })
            ),
            "should report alias count"
        );
    }

    #[test]
    fn test_default_alias_limit_is_30() {
        let validator = RequestValidator::new();
        let fields_30: String = (0..30).fold(String::new(), |mut s, i| {
            use std::fmt::Write;
            let _ = write!(s, "f{i}: user {{ id }} ");
            s
        });
        validator
            .validate_query(&format!("query {{ {fields_30} }}"))
            .unwrap_or_else(|e| panic!("expected Ok for 30 aliases: {e}"));

        let fields_31: String = (0..31).fold(String::new(), |mut s, i| {
            use std::fmt::Write;
            let _ = write!(s, "f{i}: user {{ id }} ");
            s
        });
        let result_31 = validator.validate_query(&format!("query {{ {fields_31} }}"));
        assert!(
            matches!(result_31, Err(ComplexityValidationError::TooManyAliases { .. })),
            "expected TooManyAliases for 31 aliases, got: {result_31:?}"
        );
    }

    // ── Parse errors ──

    #[test]
    fn test_empty_query_rejected() {
        let validator = RequestValidator::new();
        let r1 = validator.validate_query("");
        assert!(
            matches!(r1, Err(ComplexityValidationError::MalformedQuery(_))),
            "expected MalformedQuery for empty string, got: {r1:?}"
        );
        let r2 = validator.validate_query("   ");
        assert!(
            matches!(r2, Err(ComplexityValidationError::MalformedQuery(_))),
            "expected MalformedQuery for whitespace, got: {r2:?}"
        );
    }

    #[test]
    fn test_malformed_query_rejected() {
        let validator = RequestValidator::new();
        let result = validator.validate_query("{ invalid query {{}}");
        assert!(
            matches!(result, Err(ComplexityValidationError::MalformedQuery(_))),
            "expected MalformedQuery, got: {result:?}"
        );
    }

    // ── Variables ──

    #[test]
    fn test_valid_variables() {
        let validator = RequestValidator::new();
        let vars = serde_json::json!({"id": "123"});
        validator
            .validate_variables(Some(&vars))
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_invalid_variables_not_object() {
        let validator = RequestValidator::new();
        let vars = serde_json::json!([1, 2, 3]);
        let result = validator.validate_variables(Some(&vars));
        assert!(
            matches!(result, Err(ComplexityValidationError::InvalidVariables(_))),
            "expected InvalidVariables, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_variables_too_many() {
        let validator = RequestValidator::new();
        // Build an object with MAX_VARIABLES_COUNT + 1 keys — must be rejected.
        let vars: serde_json::Value = serde_json::Value::Object(
            (0..=MAX_VARIABLES_COUNT)
                .map(|i| (format!("v{i}"), serde_json::Value::Null))
                .collect(),
        );
        let result = validator.validate_variables(Some(&vars));
        assert!(
            matches!(result, Err(ComplexityValidationError::InvalidVariables(_))),
            "expected InvalidVariables for too-many-variables, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_variables_at_limit_is_ok() {
        let validator = RequestValidator::new();
        // Exactly MAX_VARIABLES_COUNT keys — must be accepted.
        let vars: serde_json::Value = serde_json::Value::Object(
            (0..MAX_VARIABLES_COUNT)
                .map(|i| (format!("v{i}"), serde_json::Value::Null))
                .collect(),
        );
        validator
            .validate_variables(Some(&vars))
            .unwrap_or_else(|e| panic!("expected Ok at limit, got: {e}"));
    }

    // ── Disabled validation ──

    #[test]
    fn test_disable_depth_and_complexity_validation() {
        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(false)
            .with_max_depth(1)
            .with_max_complexity(1);
        let deep = "{ a { b { c { d { e { f } } } } } }";
        validator
            .validate_query(deep)
            .unwrap_or_else(|e| panic!("expected Ok when depth/complexity disabled: {e}"));
    }

    // ── Boundary / mutation-test sentinels ──

    #[test]
    fn test_complexity_at_limit_is_allowed() {
        let validator = RequestValidator::new().with_max_complexity(3);
        validator
            .validate_query("query { a b c }")
            .unwrap_or_else(|e| panic!("complexity == max must be allowed: {e}"));
    }

    #[test]
    fn test_complexity_just_over_limit_is_rejected() {
        let validator = RequestValidator::new().with_max_complexity(3);
        assert!(
            matches!(
                validator.validate_query("query { a b c d }"),
                Err(ComplexityValidationError::QueryTooComplex { .. })
            ),
            "complexity > max must be rejected"
        );
    }

    #[test]
    fn test_depth_at_limit_is_allowed() {
        let validator = RequestValidator::default().with_max_depth(3);
        validator
            .validate_query("{ a { b { c } } }")
            .unwrap_or_else(|e| panic!("depth == max must be allowed: {e}"));
    }

    #[test]
    fn test_depth_just_over_limit_is_rejected() {
        let validator = RequestValidator::default().with_max_depth(3);
        assert!(
            matches!(
                validator.validate_query("{ a { b { c { d } } } }"),
                Err(ComplexityValidationError::QueryTooDeep { .. })
            ),
            "depth > max must be rejected"
        );
    }

    #[test]
    fn test_skip_validation_requires_aliases_also_zero() {
        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(false)
            .with_max_aliases(2);
        let query = "query { a: user { id } b: user { id } c: user { id } }";
        assert!(
            validator.validate_query(query).is_err(),
            "alias check must run even when depth/complexity validation is disabled"
        );
    }

    #[test]
    fn test_early_return_requires_depth_disabled() {
        let validator = RequestValidator::new()
            .with_depth_validation(true)
            .with_complexity_validation(false)
            .with_max_aliases(0)
            .with_max_depth(2);
        assert!(
            matches!(
                validator.validate_query("{ a { b { c } } }"),
                Err(ComplexityValidationError::QueryTooDeep { .. })
            ),
            "depth validation must still run when only complexity is disabled"
        );
    }

    #[test]
    fn test_early_return_requires_complexity_disabled() {
        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(true)
            .with_max_aliases(0)
            .with_max_complexity(2);
        assert!(
            matches!(
                validator.validate_query("query { users(first: 100) { id name } }"),
                Err(ComplexityValidationError::QueryTooComplex { .. })
            ),
            "complexity validation must still run when only depth is disabled"
        );
    }

    #[test]
    fn test_deep_fragment_recursion_guard() {
        let validator = RequestValidator::new().with_max_depth(5);
        let mut query = String::from("query { ...F0 }\n");
        for i in 0..34_usize {
            use std::fmt::Write;
            let _ = writeln!(query, "fragment F{i} on T {{ ...F{} }}", i + 1);
        }
        query.push_str("fragment F34 on T { id }\n");
        assert!(
            validator.validate_query(&query).is_err(),
            "deeply nested fragment chain must be rejected by recursion guard"
        );
    }

    #[test]
    fn test_nested_aliases_counted_correctly() {
        let validator = RequestValidator::new().with_max_aliases(3);
        assert!(
            matches!(
                validator.validate_query("query { a: user { id } b: user { c: name d: email } }"),
                Err(ComplexityValidationError::TooManyAliases {
                    actual_aliases: 4,
                    ..
                })
            ),
            "nested aliases must be summed, not subtracted"
        );
    }

    // ── from_config ──

    #[test]
    fn test_from_config() {
        let config = ComplexityConfig {
            max_depth:      5,
            max_complexity: 20,
            max_aliases:    3,
        };
        let validator = RequestValidator::from_config(&config);
        // Depth-6 query should fail
        let result = validator.validate_query("{ a { b { c { d { e { f } } } } } }");
        assert!(
            matches!(result, Err(ComplexityValidationError::QueryTooDeep { .. })),
            "expected QueryTooDeep for depth-6 query with max 5, got: {result:?}"
        );
    }
}
