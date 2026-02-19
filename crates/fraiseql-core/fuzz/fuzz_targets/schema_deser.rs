#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // CompiledSchema::from_json must never panic on arbitrary input
    let _ = fraiseql_core::schema::CompiledSchema::from_json(data);
});
