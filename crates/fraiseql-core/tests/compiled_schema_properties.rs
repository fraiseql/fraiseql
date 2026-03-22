//! Property-based tests for `CompiledSchema` serialization roundtrips.
//!
//! Verifies that `CompiledSchema` survives JSON serialize → deserialize for
//! arbitrary type/query/mutation combinations. Indexes (built from deserialized
//! data) are not compared — they are runtime-only `#[serde(skip)]` fields.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use fraiseql_core::schema::{
    CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, MutationDefinition,
    QueryDefinition, TypeDefinition,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn arb_field_name() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,19}"
}

fn arb_type_name() -> impl Strategy<Value = String> {
    "[A-Z][a-zA-Z]{0,14}"
}

fn arb_field_type() -> impl Strategy<Value = FieldType> {
    prop_oneof![
        Just(FieldType::String),
        Just(FieldType::Int),
        Just(FieldType::Float),
        Just(FieldType::Boolean),
        Just(FieldType::Id),
    ]
}

fn arb_field_def() -> impl Strategy<Value = FieldDefinition> {
    (arb_field_name(), arb_field_type(), prop::bool::ANY).prop_map(
        |(name, field_type, nullable)| FieldDefinition {
            name:           name.into(),
            field_type,
            nullable,
            description:    None,
            default_value:  None,
            vector_config:  None,
            alias:          None,
            deprecation:    None,
            requires_scope: None,
            on_deny:        FieldDenyPolicy::default(),
            encryption:     None,
        },
    )
}

fn arb_type_def() -> impl Strategy<Value = TypeDefinition> {
    (
        arb_type_name(),
        arb_field_name(),
        prop::collection::vec(arb_field_def(), 1..5),
    )
        .prop_map(|(name, sql_source, fields)| {
            let mut td = TypeDefinition::new(&name, format!("v_{sql_source}"));
            td.fields = fields;
            td
        })
}

fn arb_query_def() -> impl Strategy<Value = QueryDefinition> {
    (arb_field_name(), arb_type_name(), prop::bool::ANY).prop_map(
        |(name, return_type, returns_list)| {
            let mut qd = QueryDefinition::new(&name, &return_type);
            qd.returns_list = returns_list;
            qd.sql_source = Some(format!("v_{name}"));
            qd
        },
    )
}

fn arb_mutation_def() -> impl Strategy<Value = MutationDefinition> {
    (arb_field_name(), arb_type_name()).prop_map(|(name, return_type)| {
        let mut md = MutationDefinition::new(&name, &return_type);
        md.sql_source = Some(format!("fn_{name}"));
        md
    })
}

fn arb_compiled_schema() -> impl Strategy<Value = CompiledSchema> {
    (
        prop::collection::vec(arb_type_def(), 0..5),
        prop::collection::vec(arb_query_def(), 0..5),
        prop::collection::vec(arb_mutation_def(), 0..3),
    )
        .prop_map(|(types, queries, mutations)| {
            let mut schema = CompiledSchema::new();
            schema.types = types;
            schema.queries = queries;
            schema.mutations = mutations;
            schema
        })
}

// ---------------------------------------------------------------------------
// Property: JSON roundtrip
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn compiled_schema_json_roundtrip(schema in arb_compiled_schema()) {
        let json = schema.to_json().unwrap();

        // Must deserialize without error
        let restored = CompiledSchema::from_json(&json).unwrap();

        // Serialized fields must be identical
        prop_assert_eq!(&schema.types, &restored.types);
        prop_assert_eq!(&schema.queries, &restored.queries);
        prop_assert_eq!(&schema.mutations, &restored.mutations);
        prop_assert_eq!(&schema.enums, &restored.enums);
        prop_assert_eq!(&schema.subscriptions, &restored.subscriptions);
    }
}

// ---------------------------------------------------------------------------
// Property: Double roundtrip stability
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn compiled_schema_double_roundtrip_stable(schema in arb_compiled_schema()) {
        let json1 = schema.to_json().unwrap();
        let restored1 = CompiledSchema::from_json(&json1).unwrap();
        let json2 = restored1.to_json().unwrap();

        // Second serialization must be byte-identical to first
        prop_assert_eq!(&json1, &json2, "Double roundtrip must produce identical JSON");
    }
}

// ---------------------------------------------------------------------------
// Property: Indexes are built after deserialization
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn from_json_builds_query_indexes(schema in arb_compiled_schema()) {
        let json = schema.to_json().unwrap();
        let restored = CompiledSchema::from_json(&json).unwrap();

        // Every query should be findable by name via the index
        for query in &restored.queries {
            prop_assert!(
                restored.find_query(&query.name).is_some(),
                "Query '{}' should be findable after from_json",
                query.name
            );
        }

        // Every mutation should be findable by name via the index
        for mutation in &restored.mutations {
            prop_assert!(
                restored.find_mutation(&mutation.name).is_some(),
                "Mutation '{}' should be findable after from_json",
                mutation.name
            );
        }
    }
}
