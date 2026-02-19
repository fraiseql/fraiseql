#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Try to deserialize fuzzed JSON as a CompiledSchema, then verify roundtrip
    let Ok(schema) = fraiseql_core::schema::CompiledSchema::from_json(data) else {
        return;
    };

    // Compiled schema must serialize to valid JSON
    let json_str = schema
        .to_json()
        .expect("Compiled schema failed to serialize");

    // Re-deserialization must succeed (no panics, no errors)
    let reloaded = fraiseql_core::schema::CompiledSchema::from_json(&json_str)
        .expect("Compiled schema failed roundtrip deserialization");

    // Verify typed fields roundtrip exactly. We skip security/federation
    // (opaque serde_json::Value) because serde_json has f64 formatting
    // instability for numbers at the edge of precision — the same f64 can
    // serialize to different decimal strings across passes.
    assert_eq!(schema.types, reloaded.types, "types changed after roundtrip");
    assert_eq!(schema.enums, reloaded.enums, "enums changed after roundtrip");
    assert_eq!(
        schema.input_types, reloaded.input_types,
        "input_types changed after roundtrip"
    );
    assert_eq!(
        schema.interfaces, reloaded.interfaces,
        "interfaces changed after roundtrip"
    );
    assert_eq!(
        schema.unions, reloaded.unions,
        "unions changed after roundtrip"
    );
    assert_eq!(
        schema.queries, reloaded.queries,
        "queries changed after roundtrip"
    );
    assert_eq!(
        schema.mutations, reloaded.mutations,
        "mutations changed after roundtrip"
    );
    assert_eq!(
        schema.subscriptions, reloaded.subscriptions,
        "subscriptions changed after roundtrip"
    );
    assert_eq!(
        schema.directives, reloaded.directives,
        "directives changed after roundtrip"
    );
    assert_eq!(
        schema.observers, reloaded.observers,
        "observers changed after roundtrip"
    );
    assert_eq!(
        schema.schema_sdl, reloaded.schema_sdl,
        "schema_sdl changed after roundtrip"
    );

    // Validate type names are non-empty if types exist
    for type_def in &schema.types {
        assert!(!type_def.name.is_empty(), "Type definition has empty name");
    }

    // Validate query names are non-empty if queries exist
    for query_def in &schema.queries {
        assert!(
            !query_def.name.is_empty(),
            "Query definition has empty name"
        );
    }
});
