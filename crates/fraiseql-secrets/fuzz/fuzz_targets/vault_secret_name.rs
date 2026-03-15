#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    let result =
        fraiseql_secrets::secrets_manager::backends::vault::validation::validate_vault_secret_name(
            s,
        );

    // Invariant: names over 1,024 bytes must always be rejected (S19-I2 guard)
    if s.len() > 1024 {
        debug_assert!(result.is_err(), "secret name > 1024 bytes must be rejected");
    }

    // Invariant: empty names must always be rejected
    if s.is_empty() {
        debug_assert!(result.is_err(), "empty secret name must be rejected");
    }
});
