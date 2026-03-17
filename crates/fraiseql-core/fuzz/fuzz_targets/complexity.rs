#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let validator = fraiseql_core::graphql::complexity::RequestValidator::default();
    // Must never panic on arbitrary input — only return Ok or Err
    let _ = validator.validate_query(data);
});
