#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let result = fraiseql_core::graphql::parse_query(data);

    match result {
        Ok(parsed) => {
            // Successful parse must produce a JSON-serializable AST
            let json = serde_json::to_string(&parsed);
            assert!(json.is_ok(), "ParsedQuery failed to serialize to JSON");

            // JSON roundtrip: deserialize back and compare
            let json_str = json.unwrap();
            let reparsed: Result<fraiseql_core::graphql::ParsedQuery, _> =
                serde_json::from_str(&json_str);
            assert!(
                reparsed.is_ok(),
                "ParsedQuery failed JSON roundtrip: serialized OK but deserialization failed"
            );

            // Structural equality after roundtrip
            let reparsed = reparsed.unwrap();
            assert_eq!(
                parsed.operation_type, reparsed.operation_type,
                "operation_type changed after JSON roundtrip"
            );
            assert_eq!(
                parsed.operation_name, reparsed.operation_name,
                "operation_name changed after JSON roundtrip"
            );
            assert_eq!(
                parsed.root_field, reparsed.root_field,
                "root_field changed after JSON roundtrip"
            );
        }
        Err(e) => {
            // Parse errors must produce non-empty, well-formed messages
            let msg = e.to_string();
            assert!(!msg.is_empty(), "Parse error produced empty message");
        }
    }
});
