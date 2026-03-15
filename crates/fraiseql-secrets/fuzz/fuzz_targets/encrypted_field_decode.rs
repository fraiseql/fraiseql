#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let key = [0u8; 32]; // Fixed test key for determinism

    let Ok(enc) = fraiseql_secrets::FieldEncryption::new(&key) else {
        return;
    };

    // Decrypt must never panic. Any byte sequence must return Ok or Err cleanly.
    let _ = enc.decrypt(data);
});
