#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use base64::{Engine as _, engine::general_purpose};

use super::*;

fn fresh_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}

#[test]
fn test_missing_timestamp_returns_error() {
    let verifier = SendGridVerifier::new();
    // Timestamp is now required; passing None must fail.
    let result = verifier.verify(b"body", "sig", "not-a-pem-key", None, None);
    assert!(matches!(result, Err(SignatureError::MissingTimestamp)));
}

#[test]
fn test_invalid_public_key_returns_error() {
    let verifier = SendGridVerifier::new();
    let ts = fresh_timestamp();
    let result = verifier.verify(b"body", "sig", "not-a-pem-key", Some(&ts), None);
    assert!(matches!(result, Err(SignatureError::Crypto(_))));
}

#[test]
fn test_expired_timestamp_rejected() {
    let verifier = SendGridVerifier::new();
    let old_ts = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 600)
        .to_string();
    // Even before key parsing, an expired timestamp must be rejected.
    let result = verifier.verify(b"body", "sig", "not-a-pem-key", Some(&old_ts), None);
    assert!(matches!(result, Err(SignatureError::TimestampExpired)));
}

#[test]
fn test_invalid_signature_base64() {
    let verifier = SendGridVerifier::new();
    // Use a real PEM key stub to get past key parsing
    let pem = concat!(
        "-----BEGIN PUBLIC KEY-----\n",
        "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE0OaghMQgGMiXbDEsGDFvZJeXRrwv\n",
        "oHSoitCAYeOSe9tqLl9xn7xbFvs5N2H+FzP9Y+sX7jlGRzW5/3D3OQ==\n",
        "-----END PUBLIC KEY-----\n"
    );
    let ts = fresh_timestamp();
    let result = verifier.verify(b"body", "not-base64!!!", pem, Some(&ts), None);
    assert!(matches!(result, Err(SignatureError::Crypto(_))));
}

#[test]
fn test_empty_secret_rejected() {
    let verifier = SendGridVerifier::new();
    let ts = fresh_timestamp();
    let result = verifier.verify(b"body", "sig", "", Some(&ts), None);
    assert!(matches!(result, Err(SignatureError::Crypto(_))));
}

/// Round-trip test: generate a P-256 key pair, sign, and verify.
///
/// This is the only acceptance-path test — all other tests cover rejection.
/// It proves that the message construction (`timestamp_bytes + body_bytes`)
/// matches what a real SendGrid webhook would produce.
#[test]
fn test_valid_signature_round_trip() {
    use p256::{
        ecdsa::{DerSignature, Signature, SigningKey, signature::Signer as _},
        pkcs8::EncodePublicKey,
    };
    use rand_core::OsRng;

    let signing_key = SigningKey::random(&mut OsRng);
    let verifying_key = *signing_key.verifying_key();

    // Export as SPKI PEM — same format SendGrid public keys use
    let public_key_pem = verifying_key
        .to_public_key_pem(p256::pkcs8::der::pem::LineEnding::default())
        .expect("P-256 VerifyingKey serializes to SPKI PEM");

    let ts = fresh_timestamp();
    let body = b"[{\"event\":\"delivered\",\"email\":\"user@example.com\"}]";

    // Build the exact message the verifier reconstructs
    let mut message = ts.as_bytes().to_vec();
    message.extend_from_slice(body);

    // Sign and encode as DER (the format SendGrid sends)
    let sig: Signature = signing_key.sign(&message);
    let sig_der: DerSignature = sig.to_der();
    let sig_b64 = general_purpose::STANDARD.encode(sig_der.as_ref());

    let verifier = SendGridVerifier::new();
    let result = verifier.verify(body, &sig_b64, &public_key_pem, Some(&ts), None);
    assert!(
        matches!(result, Ok(true)),
        "valid ECDSA P-256 signature must verify successfully"
    );
}
