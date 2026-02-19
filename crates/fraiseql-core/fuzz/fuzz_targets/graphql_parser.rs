#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // parse_query must never panic on arbitrary input
    let _ = fraiseql_core::graphql::parse_query(data);
});
