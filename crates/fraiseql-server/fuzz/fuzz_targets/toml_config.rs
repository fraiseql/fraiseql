#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Attempt to parse arbitrary TOML as RuntimeConfig — must never panic
    let _ = toml::from_str::<fraiseql_server::config::RuntimeConfig>(data);
});
