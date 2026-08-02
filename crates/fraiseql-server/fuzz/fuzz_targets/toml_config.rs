#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Attempt to parse arbitrary TOML as ServerConfig — must never panic.
    // (The former RuntimeConfig layer this target used to parse was deleted in
    // #839; ServerConfig is the type the `--config` file actually feeds.)
    let _ = toml::from_str::<fraiseql_server::ServerConfig>(data);
});
