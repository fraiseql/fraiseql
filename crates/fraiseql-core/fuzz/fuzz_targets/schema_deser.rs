#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let Ok(schema) = fraiseql_core::schema::CompiledSchema::from_json(data) else {
        return;
    };

    // Roundtrip: serialize back to JSON
    let json = schema.to_json();
    assert!(json.is_ok(), "CompiledSchema failed to serialize to JSON");

    // Re-deserialize and compare
    let json_str = json.unwrap();
    let roundtripped = fraiseql_core::schema::CompiledSchema::from_json(&json_str);
    assert!(
        roundtripped.is_ok(),
        "CompiledSchema failed JSON roundtrip: serialized OK but deserialization failed"
    );

    // Structural equality (CompiledSchema implements PartialEq)
    let roundtripped = roundtripped.unwrap();
    assert_eq!(
        schema, roundtripped,
        "CompiledSchema changed after JSON roundtrip"
    );
});
