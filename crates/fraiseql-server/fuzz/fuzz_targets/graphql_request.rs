#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // Deserialization must never panic — all malformed input returns Err
    let _ = serde_json::from_str::<fraiseql_server::routes::graphql::GraphQLRequest>(s);
});
