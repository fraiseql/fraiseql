//! Property-based tests for schema invariants.
//!
//! These properties verify that schema operations maintain invariants
//! across all valid input combinations.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use fraiseql_core::schema::{
    CompiledSchema, QueryDefinition, RoleDefinition, SecurityConfig, TypeDefinition,
};
use proptest::prelude::*;

// ============================================================================
// Strategies
// ============================================================================

/// Strategy for generating valid type names (`PascalCase`).
fn arb_type_name() -> impl Strategy<Value = String> {
    "[A-Z][a-zA-Z]{1,20}".prop_map(String::from)
}

/// Strategy for generating valid query names (camelCase).
fn arb_query_name() -> impl Strategy<Value = String> {
    "[a-z][a-zA-Z]{1,20}".prop_map(String::from)
}

/// Strategy for generating valid SQL source names.
fn arb_sql_source() -> impl Strategy<Value = String> {
    "v_[a-z_]{1,20}".prop_map(String::from)
}

/// Strategy for generating valid role names.
fn arb_role_name() -> impl Strategy<Value = String> {
    "[a-z_]{1,15}".prop_map(String::from)
}

/// Strategy for generating valid scope strings.
fn arb_scope() -> impl Strategy<Value = String> {
    "[a-z]+:[a-zA-Z.*]+".prop_map(String::from)
}

// ============================================================================
// Schema JSON Roundtrip Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Property: Schema with N types roundtrips through JSON with all types preserved.
    #[test]
    fn prop_schema_type_count_preserved(
        names in prop::collection::vec(arb_type_name(), 0..10),
        sources in prop::collection::vec(arb_sql_source(), 0..10),
    ) {
        let count = names.len().min(sources.len());
        let mut schema = CompiledSchema::new();
        for i in 0..count {
            schema.types.push(TypeDefinition::new(
                names[i].clone(),
                sources[i].clone(),
            ));
        }

        let json_str = schema.to_json().expect("serialization should succeed");
        let restored = CompiledSchema::from_json(&json_str).expect("deserialization should succeed");

        prop_assert_eq!(schema.types.len(), restored.types.len());
    }

    /// Property: Schema with N queries roundtrips through JSON with all queries preserved.
    #[test]
    fn prop_schema_query_count_preserved(
        names in prop::collection::vec(arb_query_name(), 0..10),
    ) {
        let mut schema = CompiledSchema::new();
        for name in &names {
            schema.queries.push(QueryDefinition::new(name.clone(), "String"));
        }

        let json_str = schema.to_json().expect("serialization should succeed");
        let restored = CompiledSchema::from_json(&json_str).expect("deserialization should succeed");

        prop_assert_eq!(schema.queries.len(), restored.queries.len());
    }

    /// Property: Type names are preserved exactly through JSON roundtrip.
    #[test]
    fn prop_schema_type_names_exact(
        name in arb_type_name(),
        source in arb_sql_source(),
    ) {
        let mut schema = CompiledSchema::new();
        schema.types.push(TypeDefinition::new(name.clone(), source.clone()));

        let json_str = schema.to_json().expect("serialization should succeed");
        let restored = CompiledSchema::from_json(&json_str).expect("deserialization should succeed");

        prop_assert_eq!(&restored.types[0].name, &name);
        prop_assert_eq!(&restored.types[0].sql_source, &source);
    }

    /// Property: Query names and return types are preserved exactly through JSON roundtrip.
    #[test]
    fn prop_schema_query_fields_exact(
        name in arb_query_name(),
        return_type in arb_type_name(),
    ) {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition::new(name.clone(), return_type.clone()));

        let json_str = schema.to_json().expect("serialization should succeed");
        let restored = CompiledSchema::from_json(&json_str).expect("deserialization should succeed");

        prop_assert_eq!(&restored.queries[0].name, &name);
        prop_assert_eq!(&restored.queries[0].return_type, &return_type);
    }

    /// Property: Empty schema roundtrips cleanly.
    #[test]
    fn prop_empty_schema_roundtrips(_dummy in 0..1u8) {
        let schema = CompiledSchema::new();
        let json_str = schema.to_json().expect("serialization should succeed");
        let restored = CompiledSchema::from_json(&json_str).expect("deserialization should succeed");

        prop_assert!(restored.types.is_empty());
        prop_assert!(restored.queries.is_empty());
    }
}

// ============================================================================
// Schema Validation Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Property: Duplicate type names always produce validation errors.
    #[test]
    fn prop_schema_duplicate_types_always_error(
        name in arb_type_name(),
        source1 in arb_sql_source(),
        source2 in arb_sql_source(),
    ) {
        let mut schema = CompiledSchema::new();
        schema.types.push(TypeDefinition::new(name.clone(), source1));
        schema.types.push(TypeDefinition::new(name.clone(), source2));

        let result = schema.validate();
        prop_assert!(
            result.is_err(),
            "Schema with duplicate type '{}' should fail validation", name
        );
    }

    /// Property: Schema with unique type names passes duplicate check.
    #[test]
    fn prop_schema_unique_types_pass_duplicate_check(
        names in prop::collection::hash_set(arb_type_name(), 1..5),
    ) {
        let mut schema = CompiledSchema::new();
        for name in names {
            schema.types.push(TypeDefinition::new(name, "v_table".to_string()));
        }

        let result = schema.validate();
        if let Err(errors) = &result {
            for err in errors {
                prop_assert!(
                    !err.contains("Duplicate type name"),
                    "Unique types should not produce duplicate error: {}", err
                );
            }
        }
    }

    /// Property: Validation errors always contain the offending type name.
    #[test]
    fn prop_schema_validation_error_mentions_name(
        name in arb_type_name(),
    ) {
        let mut schema = CompiledSchema::new();
        schema.types.push(TypeDefinition::new(name.clone(), "v_a".to_string()));
        schema.types.push(TypeDefinition::new(name.clone(), "v_b".to_string()));

        let result = schema.validate();
        prop_assert!(result.is_err());
        let errors = result.unwrap_err();
        prop_assert!(
            errors.iter().any(|e| e.contains(&name)),
            "Validation error should mention type name '{}', got: {:?}", name, errors
        );
    }
}

// ============================================================================
// SecurityConfig Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Property: SecurityConfig preserves role count through JSON roundtrip.
    #[test]
    fn prop_security_config_role_count_preserved(
        role_names in prop::collection::vec(arb_role_name(), 1..5),
        scopes in prop::collection::vec(arb_scope(), 1..3),
    ) {
        let mut config = SecurityConfig::new();
        for name in &role_names {
            config.add_role(RoleDefinition::new(name.clone(), scopes.clone()));
        }

        let json_str = serde_json::to_string(&config).expect("serialization should succeed");
        let restored: SecurityConfig =
            serde_json::from_str(&json_str).expect("deserialization should succeed");

        prop_assert_eq!(config.role_definitions.len(), restored.role_definitions.len());
    }

    /// Property: SecurityConfig preserves role names exactly through JSON roundtrip.
    #[test]
    fn prop_security_config_role_names_exact(
        name in arb_role_name(),
        scopes in prop::collection::vec(arb_scope(), 1..3),
    ) {
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new(name.clone(), scopes));

        let json_str = serde_json::to_string(&config).expect("serialization should succeed");
        let restored: SecurityConfig =
            serde_json::from_str(&json_str).expect("deserialization should succeed");

        prop_assert_eq!(&restored.role_definitions[0].name, &name);
    }

    /// Property: SecurityConfig preserves scopes exactly through JSON roundtrip.
    #[test]
    fn prop_security_config_scopes_exact(
        name in arb_role_name(),
        scopes in prop::collection::vec(arb_scope(), 1..5),
    ) {
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new(name, scopes.clone()));

        let json_str = serde_json::to_string(&config).expect("serialization should succeed");
        let restored: SecurityConfig =
            serde_json::from_str(&json_str).expect("deserialization should succeed");

        prop_assert_eq!(&restored.role_definitions[0].scopes, &scopes);
    }

    /// Property: Empty SecurityConfig roundtrips cleanly.
    #[test]
    fn prop_empty_security_config_roundtrips(_dummy in 0..1u8) {
        let config = SecurityConfig::new();
        let json_str = serde_json::to_string(&config).expect("serialization should succeed");
        let restored: SecurityConfig =
            serde_json::from_str(&json_str).expect("deserialization should succeed");

        prop_assert!(restored.role_definitions.is_empty());
    }

    /// Property: Adding a role increases the count by exactly 1.
    #[test]
    fn prop_add_role_increments_count(
        initial_roles in prop::collection::vec(arb_role_name(), 0..5),
        new_role in arb_role_name(),
        scopes in prop::collection::vec(arb_scope(), 1..3),
    ) {
        let mut config = SecurityConfig::new();
        for name in &initial_roles {
            config.add_role(RoleDefinition::new(name.clone(), scopes.clone()));
        }

        let before = config.role_definitions.len();
        config.add_role(RoleDefinition::new(new_role, scopes));
        let after = config.role_definitions.len();

        prop_assert_eq!(after, before + 1);
    }
}

// ============================================================================
// Schema Composition Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(150))]

    /// Property: Schema with both types and queries roundtrips with both intact.
    #[test]
    fn prop_schema_mixed_types_and_queries_roundtrip(
        type_names in prop::collection::vec(arb_type_name(), 1..5),
        query_names in prop::collection::vec(arb_query_name(), 1..5),
        sources in prop::collection::vec(arb_sql_source(), 1..5),
    ) {
        let mut schema = CompiledSchema::new();
        for (i, name) in type_names.iter().enumerate() {
            let source = sources.get(i).cloned().unwrap_or_else(|| "v_default".to_string());
            schema.types.push(TypeDefinition::new(name.clone(), source));
        }
        for name in &query_names {
            schema.queries.push(QueryDefinition::new(name.clone(), "String"));
        }

        let json_str = schema.to_json().expect("serialization should succeed");
        let restored = CompiledSchema::from_json(&json_str).expect("deserialization should succeed");

        prop_assert_eq!(schema.types.len(), restored.types.len());
        prop_assert_eq!(schema.queries.len(), restored.queries.len());
    }

    /// Property: Schema validation never panics on any input combination.
    #[test]
    fn prop_schema_validation_never_panics(
        type_count in 0usize..20,
        query_count in 0usize..20,
    ) {
        let mut schema = CompiledSchema::new();

        for i in 0..type_count {
            let name = format!("Type{}", i);
            let source = format!("v_source_{}", i);
            schema.types.push(TypeDefinition::new(name, source));
        }

        for i in 0..query_count {
            let name = format!("query{}", i);
            schema.queries.push(QueryDefinition::new(name, "String"));
        }

        // Must not panic on any configuration
        let _ = schema.validate();
    }
}
