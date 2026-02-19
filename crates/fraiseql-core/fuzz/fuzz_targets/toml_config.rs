#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // FraiseQLConfig::from_toml must never panic on arbitrary TOML input
    let result = fraiseql_core::config::FraiseQLConfig::from_toml(data);

    if let Ok(config) = result {
        // Successful parse must produce a serializable config
        let json = serde_json::to_string(&config);
        assert!(json.is_ok(), "FraiseQLConfig failed to serialize to JSON");
    }
});
