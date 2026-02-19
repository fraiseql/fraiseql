#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Parse query and validate variable definitions are well-formed
    let Ok(parsed) = fraiseql_core::graphql::parse_query(data) else {
        return;
    };

    // Variable names must not be empty
    for var in &parsed.variables {
        assert!(
            !var.name.is_empty(),
            "Variable definition has empty name"
        );
        // Variable type name must not be empty
        assert!(
            !var.var_type.name.is_empty(),
            "Variable type has empty name for variable '{}'",
            var.name
        );
    }

    // Fragment names must not be empty
    for frag in &parsed.fragments {
        assert!(
            !frag.name.is_empty(),
            "Fragment definition has empty name"
        );
    }

    // Operation type must be a known value
    assert!(
        ["query", "mutation", "subscription"].contains(&parsed.operation_type.as_str()),
        "Unknown operation type: '{}'",
        parsed.operation_type
    );

    // Root field must not be empty
    assert!(
        !parsed.root_field.is_empty(),
        "Parsed query has empty root_field"
    );
});
