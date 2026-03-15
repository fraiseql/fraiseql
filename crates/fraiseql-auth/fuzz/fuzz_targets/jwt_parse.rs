#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only process valid UTF-8; invalid UTF-8 is rejected at the HTTP layer.
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // Fuzz JWT validation with a fixed HMAC key.
    // The goal is to ensure no panic on any input string.
    let Ok(validator) =
        fraiseql_auth::jwt::JwtValidator::new("fuzz-issuer", jsonwebtoken::Algorithm::HS256)
    else {
        return;
    };

    let secret = b"fuzz-test-secret-key-32-bytes!!!";
    let _ = validator.validate_hmac(s, secret);
});
