#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

mod graph_tests {
    use super::super::graph::*;

    #[test]
    fn test_graph_format_from_str() {
        assert_eq!("json".parse::<GraphFormat>().unwrap(), GraphFormat::Json);
        assert_eq!("dot".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
        assert_eq!("mermaid".parse::<GraphFormat>().unwrap(), GraphFormat::Mermaid);
    }

    #[test]
    fn test_graph_format_case_insensitive() {
        assert_eq!("JSON".parse::<GraphFormat>().unwrap(), GraphFormat::Json);
        assert_eq!("DOT".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
    }

    #[test]
    fn test_graph_format_invalid() {
        assert!(
            "invalid".parse::<GraphFormat>().is_err(),
            "expected Err for unknown federation graph format"
        );
    }

    #[test]
    fn test_to_dot_format() {
        let graph = FederationGraph {
            subgraphs: vec![Subgraph {
                name: "a".to_string(),
                url: "http://a".to_string(),
                entities: vec!["A".to_string()],
            }],
            edges: vec![],
        };

        let dot = to_dot(&graph);
        assert!(dot.contains("digraph"));
        assert!(dot.contains('a'));
    }

    #[test]
    fn test_to_mermaid_format() {
        let graph = FederationGraph {
            subgraphs: vec![Subgraph {
                name: "a".to_string(),
                url: "http://a".to_string(),
                entities: vec!["A".to_string()],
            }],
            edges: vec![],
        };

        let mermaid = to_mermaid(&graph);
        assert!(mermaid.contains("graph"));
        assert!(mermaid.contains('a'));
    }
}

mod check_tests {
    use std::fs;

    use serde_json::json;

    use super::super::check::*;

    #[test]
    fn test_check_missing_file() {
        let result = run("/nonexistent/schema.json", None, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_valid_schema() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {}
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success");
        assert_eq!(result.data.unwrap()["type_count"], 1);
    }

    #[test]
    fn test_check_no_federation_metadata() {
        let schema = json!({"types": []});

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "error");
        assert!(result.message.unwrap().contains("No federation metadata"));
    }

    #[test]
    fn test_check_type_without_key() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {}
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "validation-failed");
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("no @key directive"));
    }

    #[test]
    fn test_check_federation_disabled_warning() {
        let schema = json!({
            "federation": {
                "enabled": false,
                "version": "v2",
                "types": []
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success");
        assert!(!result.warnings.is_empty());
        assert!(result.warnings[0].contains("not enabled"));
    }

    #[test]
    fn test_check_key_field_not_on_type_errors() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["userId"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "name": {"external": false, "shareable": false, "inaccessible": false, "requires": [], "provides": []}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "validation-failed");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("userId") && e.contains("no field named")),
            "Expected error about missing key field 'userId': {:?}",
            result.errors
        );
    }

    #[test]
    fn test_check_key_field_exists_passes() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "id": {"external": false, "shareable": false, "inaccessible": false, "requires": [], "provides": []}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
    }

    #[test]
    fn test_check_key_field_in_external_fields_passes() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": true,
                        "external_fields": ["id"],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {}
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
    }

    #[test]
    fn test_check_override_empty_string_errors() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Product",
                        "keys": [{"fields": ["sku"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "sku": {},
                            "price": {"override_from": ""}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "validation-failed", "Result: {result:?}");
        assert!(
            result.errors.iter().any(|e| e.contains("empty string")),
            "Expected empty override error: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_check_override_unknown_subgraph_with_against_errors() {
        // Local schema with @override referencing "old-pricing"
        let local = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Product",
                        "keys": [{"fields": ["sku"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "sku": {},
                            "price": {"override_from": "nonexistent-service"}
                        }
                    }
                ]
            }
        });

        // Supergraph with no subgraph named "nonexistent-service"
        let supergraph = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": []
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let local_path = dir.path().join("local.compiled.json");
        let super_path = dir.path().join("supergraph.json");
        fs::write(&local_path, serde_json::to_string_pretty(&local).unwrap()).unwrap();
        fs::write(&super_path, serde_json::to_string_pretty(&supergraph).unwrap()).unwrap();

        let result =
            run(local_path.to_str().unwrap(), Some(super_path.to_str().unwrap()), false).unwrap();
        assert_eq!(result.status, "validation-failed", "Result: {result:?}");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("nonexistent-service") && e.contains("unknown")),
            "Expected unknown subgraph error: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_check_override_local_no_against_passes() {
        // @override(from: "old") without --against should pass (can't verify)
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Product",
                        "keys": [{"fields": ["sku"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "sku": {},
                            "price": {"override_from": "old-pricing"}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Result: {result:?}");
    }

    #[test]
    fn test_check_requires_nonexistent_field_errors() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": true,
                        "external_fields": ["id"],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "id": {"external": true},
                            "profile": {
                                "requires": [{"path": ["nonexistent"]}]
                            }
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "validation-failed", "Result: {result:?}");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("nonexistent") && e.contains("@requires")),
            "Expected @requires error about nonexistent field: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_check_requires_existing_field_passes() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": true,
                        "external_fields": ["id", "email"],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "id": {"external": true},
                            "email": {"external": true},
                            "profile": {
                                "requires": [{"path": ["email"]}]
                            }
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
    }

    #[test]
    fn test_check_provides_emits_warning() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Order",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "id": {},
                            "user": {
                                "provides": [{"path": ["name"]}]
                            }
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("@provides") && w.contains("cannot be fully validated")),
            "Expected @provides warning: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_check_inaccessible_on_query_root_warns() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Query",
                        "keys": [{"fields": ["_service"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": ["secretField"],
                        "field_directives": {
                            "_service": {}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("@inaccessible")
                && w.contains("Query")
                && w.contains("secretField")),
            "Expected warning about @inaccessible on Query root field: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_check_inaccessible_on_mutation_root_warns() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Mutation",
                        "keys": [{"fields": ["_service"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": ["dangerousAction"],
                        "field_directives": {
                            "_service": {}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("@inaccessible")
                && w.contains("Mutation")
                && w.contains("dangerousAction")),
            "Expected warning about @inaccessible on Mutation root field: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_check_resolvable_false_key_warns() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Product",
                        "keys": [{"fields": ["sku"], "resolvable": false}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "sku": {}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("resolvable: false") && w.contains("Product")),
            "Expected resolvable: false warning: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_check_multiple_keys_validated_independently() {
        // One key field exists, the other doesn't
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Account",
                        "keys": [
                            {"fields": ["id"], "resolvable": true},
                            {"fields": ["missingField"], "resolvable": true}
                        ],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "id": {},
                            "name": {}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "validation-failed", "Result: {result:?}");
        // Should error on missingField but not on id
        assert!(
            result.errors.iter().any(|e| e.contains("missingField")),
            "Expected error about missingField: {:?}",
            result.errors
        );
        assert!(
            !result.errors.iter().any(|e| e.contains("'id'")),
            "Should NOT error about 'id' which exists: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_check_inaccessible_on_regular_type_no_warning() {
        // @inaccessible on a non-root type should NOT produce the root-field warning
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": ["ssn"],
                        "field_directives": {
                            "id": {},
                            "ssn": {"inaccessible": true}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
        assert!(
            !result.warnings.iter().any(|w| w.contains("@inaccessible")),
            "Should NOT warn about @inaccessible on non-root type: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_check_against_missing_supergraph() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": []
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result =
            run(path.to_str().unwrap(), Some("/nonexistent/supergraph.json"), false).unwrap();
        assert_eq!(result.status, "validation-failed");
        assert!(result.errors[0].contains("not found"));
    }

    #[test]
    fn test_check_json_false_does_not_print() {
        // json=false: run() should return Ok without panicking.
        // Printing is suppressed; only the returned CommandResult matters.
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {}
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success");
    }

    #[test]
    fn test_check_json_output_is_valid_json() {
        // When json=true, run() prints to stdout. We verify the returned
        // CommandResult is serialisable so the print cannot fail.
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {}
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        // json=true: run() prints JSON to stdout and still returns the result
        let result = run(path.to_str().unwrap(), None, true).unwrap();
        assert_eq!(result.status, "success");
        // Verify the result itself is JSON-serialisable (the print path is exercised above)
        let serialized = serde_json::to_string(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["data"]["type_count"], 1);
    }
}
