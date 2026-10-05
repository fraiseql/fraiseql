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

    use fraiseql_core::schema::{CompiledSchema, FederationConfig, FederationEntity};

    fn entity(name: &str, extends: bool) -> FederationEntity {
        FederationEntity {
            name: name.to_string(),
            key_fields: vec!["id".to_string()],
            extends,
            ..FederationEntity::default()
        }
    }

    /// Write a compiled schema for one subgraph and return its path.
    fn subgraph(dir: &std::path::Path, service: &str, entities: Vec<FederationEntity>) -> String {
        let mut schema = CompiledSchema::new();
        schema.federation = Some(FederationConfig {
            enabled: true,
            service_name: Some(service.to_string()),
            entities,
            ..FederationConfig::default()
        });
        let path = dir.join(format!("{service}.compiled.json"));
        std::fs::write(&path, schema.to_json().unwrap()).unwrap();
        path.to_str().unwrap().to_string()
    }

    fn graph_json(paths: &[String]) -> serde_json::Value {
        let result = run(paths, GraphFormat::Json).unwrap();
        assert_eq!(result.status, "success", "{result:?}");
        result.data.unwrap()
    }

    /// The graph is the inputs': one node per subgraph, and an edge from a subgraph that
    /// extends an entity to the one that owns it (#1404).
    #[test]
    fn the_graph_is_built_from_the_compiled_schemas() {
        let dir = tempfile::tempdir().unwrap();
        let users = subgraph(dir.path(), "users", vec![entity("User", false)]);
        let orders =
            subgraph(dir.path(), "orders", vec![entity("Order", false), entity("User", true)]);
        let graph = graph_json(&[users, orders]);
        assert_eq!(
            graph,
            serde_json::json!({
                "subgraphs": [
                    {"name": "users", "owns": ["User"], "extends": []},
                    {"name": "orders", "owns": ["Order"], "extends": ["User"]},
                ],
                "edges": [{"from": "orders", "to": "users", "entity": "User"}],
                "unresolved": [],
            })
        );
    }

    /// An entity extended but owned by no input is reported, not dropped.
    #[test]
    fn an_extension_with_no_owner_among_the_inputs_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let orders = subgraph(dir.path(), "orders", vec![entity("User", true)]);
        let graph = graph_json(&[orders]);
        assert_eq!(graph["edges"], serde_json::json!([]));
        assert_eq!(
            graph["unresolved"],
            serde_json::json!([{"subgraph": "orders", "entity": "User"}])
        );
    }

    /// The old command printed a fixed three-subgraph graph for any input, `{}` included.
    #[test]
    fn an_input_that_is_not_a_federated_subgraph_fails() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty.json");
        std::fs::write(&empty, "{}").unwrap();
        let plain = dir.path().join("plain.compiled.json");
        std::fs::write(&plain, CompiledSchema::new().to_json().unwrap()).unwrap();
        let disabled = dir.path().join("disabled.compiled.json");
        let mut schema = CompiledSchema::new();
        schema.federation = Some(FederationConfig {
            enabled: false,
            service_name: Some("users".to_string()),
            entities: vec![entity("User", false)],
            ..FederationConfig::default()
        });
        std::fs::write(&disabled, schema.to_json().unwrap()).unwrap();
        let unnamed = dir.path().join("unnamed.compiled.json");
        let mut schema = CompiledSchema::new();
        schema.federation = Some(FederationConfig {
            enabled: true,
            entities: vec![entity("User", false)],
            ..FederationConfig::default()
        });
        std::fs::write(&unnamed, schema.to_json().unwrap()).unwrap();
        for path in [empty, plain, disabled, unnamed] {
            let result = run(&[path.to_str().unwrap().to_string()], GraphFormat::Json).unwrap();
            assert_eq!(result.status, "validation-failed", "{path:?}: {result:?}");
        }
    }

    /// Names reach DOT and Mermaid as quoted labels: a quote in a service name cannot end
    /// the label and inject syntax.
    #[test]
    fn names_are_quoted_in_dot_and_mermaid() {
        let graph = FederationGraph {
            subgraphs:  vec![Subgraph {
                name:    "evil\"] ; x -> y [".to_string(),
                owns:    vec!["A".to_string()],
                extends: vec![],
            }],
            edges:      vec![],
            unresolved: vec![],
        };
        let dot = to_dot(&graph);
        assert!(dot.contains(r#"label="evil\"] ; x -> y [\n[A]""#), "{dot}");
        let mermaid = to_mermaid(&graph);
        assert!(!mermaid.contains(r#"evil"]"#), "{mermaid}");
        assert!(mermaid.contains("evil#quot;]"), "{mermaid}");
    }
}

mod check_tests {
    use std::fs;

    use serde_json::{Value, json};
    use tempfile::TempDir;

    use super::super::check::*;
    use crate::output::CommandResult;

    /// A compiled schema with `User` (id, email, name) and the given federation block.
    fn subgraph(federation: &Value) -> Value {
        json!({
            "types": [
                {"name": "User", "sql_source": "v_user", "fields": [
                    {"name": "id", "field_type": "ID"},
                    {"name": "email", "field_type": "String"},
                    {"name": "name", "field_type": "String"}
                ]}
            ],
            "queries": [], "mutations": [], "subscriptions": [],
            "federation": federation
        })
    }

    fn entity(key: &[&str]) -> Value {
        json!({"enabled": true, "version": "v2", "entities": [{"name": "User", "key_fields": key}]})
    }

    fn write(dir: &TempDir, name: &str, doc: &Value) -> String {
        let path = dir.path().join(name);
        fs::write(&path, serde_json::to_string_pretty(doc).unwrap()).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn check_one(doc: &Value) -> CommandResult {
        let dir = TempDir::new().unwrap();
        run(&write(&dir, "schema.compiled.json", doc), None, false).unwrap()
    }

    fn check_pair(local: &Value, other: &Value) -> CommandResult {
        let dir = TempDir::new().unwrap();
        let local = write(&dir, "local.json", local);
        let other = write(&dir, "other.json", other);
        run(&local, Some(&other), false).unwrap()
    }

    #[test]
    fn a_missing_file_is_an_error() {
        assert!(run("/nonexistent/schema.json", None, false).is_err());
    }

    #[test]
    fn a_valid_entity_passes_and_is_counted() {
        let result = check_one(&subgraph(&entity(&["id"])));
        assert_eq!(result.status, "success", "{result:?}");
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(result.data.unwrap()["entity_count"], 1);
    }

    #[test]
    fn a_key_naming_a_missing_field_fails_naming_type_and_field() {
        let result = check_one(&subgraph(&entity(&["id", "region"])));
        assert_eq!(result.status, "validation-failed");
        assert_eq!(result.code.as_deref(), Some("COMPOSITION_ERROR"));
        assert!(
            result.errors.iter().any(|e| e.contains("'User'") && e.contains("'region'")),
            "{:?}",
            result.errors
        );
    }

    #[test]
    fn an_embedded_entity_fails() {
        let fed = json!({"enabled": true, "version": "v2",
            "entities": [{"name": "Reading", "key_fields": ["id"]}]});
        let mut doc = subgraph(&fed);
        doc["types"].as_array_mut().unwrap().push(json!({
            "name": "Reading", "sql_source": "", "embedded": true,
            "fields": [{"name": "serial", "field_type": "String"}]
        }));
        let result = check_one(&doc);
        assert_eq!(result.status, "validation-failed");
        assert!(result.errors.iter().any(|e| e.contains("embedded")), "{:?}", result.errors);
    }

    /// The shape this command used to read. No producer writes it; it is refused, never
    /// reported as a subgraph with nothing to check.
    #[test]
    fn the_federation_types_shape_is_refused() {
        let fed = json!({"enabled": true, "version": "v2",
            "types": [{"name": "User", "keys": [{"fields": ["id"]}]}]});
        let result = check_one(&subgraph(&fed));
        assert_eq!(result.status, "validation-failed");
        assert_eq!(result.code.as_deref(), Some("INVALID_SCHEMA"));
        assert!(result.errors[0].contains("`types`"), "{:?}", result.errors);
    }

    #[test]
    fn a_schema_without_federation_is_an_error() {
        let mut doc = subgraph(&Value::Null);
        doc.as_object_mut().unwrap().remove("federation");
        let result = check_one(&doc);
        assert_eq!(result.status, "error");
        assert_eq!(result.code.as_deref(), Some("NO_FEDERATION_METADATA"));
    }

    #[test]
    fn disabled_federation_and_no_entities_warn() {
        let disabled = check_one(&subgraph(&json!({"enabled": false, "version": "v2",
            "entities": [{"name": "User", "key_fields": ["id"]}]})));
        assert!(disabled.warnings.iter().any(|w| w.contains("not enabled")), "{disabled:?}");

        let empty =
            check_one(&subgraph(&json!({"enabled": true, "version": "v2", "entities": []})));
        assert_eq!(empty.status, "success");
        assert!(empty.warnings.iter().any(|w| w.contains("no entities")), "{empty:?}");
    }

    // ── --against another subgraph ───────────────────────────────────────────────────

    #[test]
    fn an_entity_keyed_differently_in_the_other_subgraph_fails() {
        let result = check_pair(&subgraph(&entity(&["id"])), &subgraph(&entity(&["email"])));
        assert_eq!(result.status, "validation-failed");
        assert!(
            result.errors.iter().any(|e| e.contains("keyed identically")),
            "{:?}",
            result.errors
        );
    }

    #[test]
    fn a_composite_key_written_two_ways_agrees() {
        let local = json!({"enabled": true, "version": "v2", "entities": [
            {"name": "User", "key_fields": ["id email"], "shareable_fields": ["name"]}]});
        let other = json!({"enabled": true, "version": "v2", "entities": [
            {"name": "User", "key_fields": ["id", "email"], "shareable_fields": ["name"]}]});
        let result = check_pair(&subgraph(&local), &subgraph(&other));
        assert_eq!(result.status, "success", "{:?}", result.errors);
        assert!(result.warnings.iter().any(|w| w.contains("gateway composer")));
    }

    #[test]
    fn a_field_both_subgraphs_resolve_must_be_shareable_on_both() {
        let shared_here_only = json!({"enabled": true, "version": "v2", "entities": [
            {"name": "User", "key_fields": ["id"], "shareable_fields": ["email", "name"]}]});
        let result = check_pair(&subgraph(&shared_here_only), &subgraph(&entity(&["id"])));
        assert_eq!(result.status, "validation-failed");
        let sharing = result.errors.iter().filter(|e| e.contains("INVALID_FIELD_SHARING")).count();
        // `id` is part of the key on both sides, so only `email` and `name` collide.
        assert_eq!(sharing, 2, "{:?}", result.errors);
    }

    #[test]
    fn external_shareable_and_shareable_type_fields_compose() {
        let external = json!({"enabled": true, "version": "v2", "entities": [
            {"name": "User", "key_fields": ["id"], "extends": true,
             "external_fields": ["email", "name"]}]});
        let result = check_pair(&subgraph(&external), &subgraph(&entity(&["id"])));
        assert_eq!(result.status, "success", "{:?}", result.errors);

        let value_type = json!({"enabled": true, "version": "v2", "entities": [],
            "shareable_types": ["User"]});
        let result = check_pair(&subgraph(&value_type), &subgraph(&value_type));
        assert_eq!(result.status, "success", "{:?}", result.errors);
    }

    #[test]
    fn an_other_file_that_is_not_a_compiled_schema_fails() {
        let other = json!({"federation": {"types": []}});
        let result = check_pair(&subgraph(&entity(&["id"])), &other);
        assert_eq!(result.status, "validation-failed");
        assert!(result.errors.iter().any(|e| e.contains("not a compiled schema")));
    }
}
