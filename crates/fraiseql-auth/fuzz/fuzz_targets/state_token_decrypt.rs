#![no_main]
use libfuzzer_sys::fuzz_target;

use fraiseql_auth::state_encryption::{EncryptionAlgorithm, StateEncryptionService};

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // Construct with a fixed all-zeros key for determinism.
    let key = [0u8; 32];
    let service = StateEncryptionService::from_raw_key(&key, EncryptionAlgorithm::Chacha20Poly1305);

    // Decrypt arbitrary base64url strings. Must never panic — only Ok or Err.
    let _ = service.decrypt(s);
});
