#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let _ = fraiseql_server::config::env::resolve_env_value(data);
    let _ = fraiseql_server::config::env::parse_size(data);
    let _ = fraiseql_server::config::env::parse_duration(data);
});
