#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Try to deserialize fuzzed JSON as a CompiledSchema, then verify roundtrip
    let Ok(schema) = fraiseql_core::schema::CompiledSchema::from_json(data) else {
        return;
    };

    // Compiled schema must serialize to valid JSON
    let json = schema.to_json();
    assert!(json.is_ok(), "Compiled schema failed to serialize");

    // Re-deserialization must succeed
    let json_str = json.unwrap();
    let reloaded = fraiseql_core::schema::CompiledSchema::from_json(&json_str);
    assert!(
        reloaded.is_ok(),
        "Compiled schema failed roundtrip deserialization"
    );

    // Structural equality
    let reloaded = reloaded.unwrap();
    assert_eq!(schema, reloaded, "Compiled schema changed after roundtrip");

    // Validate type names are non-empty if types exist
    for type_def in &schema.types {
        assert!(
            !type_def.name.is_empty(),
            "Type definition has empty name"
        );
    }

    // Validate query names are non-empty if queries exist
    for query_def in &schema.queries {
        assert!(
            !query_def.name.is_empty(),
            "Query definition has empty name"
        );
    }
});
