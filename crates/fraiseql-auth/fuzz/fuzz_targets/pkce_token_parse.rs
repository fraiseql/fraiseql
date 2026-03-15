#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // Fuzz the S256 challenge derivation — must never panic on any input.
    let _ = fraiseql_auth::pkce::PkceStateStore::s256_challenge(s);
});
