//! SR-4: O2 — SendGridVerifier used HMAC-SHA256 instead of ECDSA P-256.
//!       Any HMAC-signed payload was being accepted as a valid SendGrid webhook.
//!       Fix: verifier now uses ECDSA P-256 with SHA-256 (the SendGrid-specified algorithm).
//!
//! Tests generate a real P-256 key pair at runtime so no hard-coded key material
//! is required. This also ensures the test is not brittle to key rotation.
//!
//! **Infrastructure:** none
//! **Parallelism:** safe

#![allow(clippy::unwrap_used)]  // Reason: test code, panics are acceptable
#![allow(clippy::doc_markdown)] // Reason: doc comments use type names without backticks for readability

use base64::{Engine as _, engine::general_purpose};
use fraiseql_webhooks::signature::sendgrid::SendGridVerifier;
use fraiseql_webhooks::traits::SignatureVerifier as _;
use p256::{
    ecdsa::{DerSignature, SigningKey, signature::Signer as _},
    pkcs8::{EncodePublicKey as _, LineEnding},
};
use rand_core::OsRng;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a fresh P-256 signing key pair and return
/// `(signing_key, pem_public_key)`.
fn generate_p256_key() -> (SigningKey, String) {
    let signing_key = SigningKey::random(&mut OsRng);
    let pem = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("public key to PEM must succeed");
    (signing_key, pem)
}

/// Sign `timestamp + payload` with the provided signing key and return a
/// Base64-encoded DER signature (the format SendGrid uses).
fn sendgrid_sign(signing_key: &SigningKey, payload: &[u8], timestamp: &str) -> String {
    let mut message = timestamp.as_bytes().to_vec();
    message.extend_from_slice(payload);
    let signature: DerSignature = signing_key.sign(&message);
    general_purpose::STANDARD.encode(signature.as_ref())
}

// ---------------------------------------------------------------------------
// SR-4 regression tests
// ---------------------------------------------------------------------------

/// A valid ECDSA P-256 signature over `timestamp + payload` must be accepted.
///
/// Before the O2 fix, SendGridVerifier used HMAC-SHA256, which would have
/// rejected a real ECDSA signature.
#[test]
fn valid_ecdsa_p256_signature_over_timestamp_and_payload_verifies() {
    let (signing_key, pem_public_key) = generate_p256_key();
    let payload = b"[{\"email\":\"test@example.com\",\"event\":\"delivered\"}]";
    let timestamp = "1609459200";

    let sig_b64 = sendgrid_sign(&signing_key, payload, timestamp);
    let verifier = SendGridVerifier;

    let result = verifier.verify(payload, &sig_b64, &pem_public_key, Some(timestamp), None);

    assert!(
        result.as_ref().is_ok_and(|&v| v),
        "O2 regression: valid ECDSA P-256 signature was rejected: {result:?}"
    );
}

/// An HMAC-SHA256 signature (the old wrong algorithm) must NOT verify.
///
/// Before the O2 fix, the verifier accepted HMAC signatures. This test ensures
/// a forged HMAC signature does NOT pass the ECDSA verifier.
#[test]
fn hmac_sha256_forged_signature_is_rejected_by_ecdsa_verifier() {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let (_signing_key, pem_public_key) = generate_p256_key();
    let payload = b"[{\"email\":\"test@example.com\",\"event\":\"bounce\"}]";
    let timestamp = "1609459200";

    // Forge: compute HMAC-SHA256 over timestamp+payload (wrong algorithm)
    let mut mac = Hmac::<Sha256>::new_from_slice(b"forged_secret").unwrap();
    mac.update(timestamp.as_bytes());
    mac.update(payload);
    let forged_sig = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let verifier = SendGridVerifier;
    let result = verifier.verify(payload, &forged_sig, &pem_public_key, Some(timestamp), None);

    assert!(
        result.is_err() || !result.unwrap_or(true),
        "O2 regression: HMAC-SHA256 forged signature was accepted by ECDSA verifier"
    );
}

/// A signature computed over the correct payload but a different timestamp must
/// be rejected. SendGrid includes the timestamp in the signed message to prevent
/// replay attacks where only the body matches.
#[test]
fn signature_over_wrong_timestamp_is_rejected() {
    let (signing_key, pem_public_key) = generate_p256_key();
    let payload = b"[{\"event\":\"click\"}]";

    // Sign with timestamp "100"
    let sig_b64 = sendgrid_sign(&signing_key, payload, "100");

    let verifier = SendGridVerifier;
    // Verify claiming timestamp "999" — signed message does not match
    let result = verifier.verify(payload, &sig_b64, &pem_public_key, Some("999"), None);

    assert!(
        result.is_err() || !result.unwrap_or(true),
        "O2 regression: signature over wrong timestamp was accepted"
    );
}

/// A signature computed by one key must be rejected when verified against a
/// different public key. This ensures the verifier actually checks the key.
#[test]
fn signature_from_different_key_is_rejected() {
    let (signing_key_a, _pem_a) = generate_p256_key();
    let (_signing_key_b, pem_b) = generate_p256_key();

    let payload = b"[{\"event\":\"open\"}]";
    let timestamp = "1609459200";

    // Sign with key A
    let sig_b64 = sendgrid_sign(&signing_key_a, payload, timestamp);

    // Verify against key B — must fail
    let verifier = SendGridVerifier;
    let result = verifier.verify(payload, &sig_b64, &pem_b, Some(timestamp), None);

    assert!(
        result.is_err() || !result.unwrap_or(true),
        "O2 regression: signature from key A verified against key B"
    );
}

/// A completely invalid signature string must return an error, not Ok(false).
#[test]
fn invalid_base64_signature_returns_error() {
    let (_signing_key, pem_public_key) = generate_p256_key();
    let verifier = SendGridVerifier;
    let result = verifier.verify(b"payload", "not-valid-base64!!!", &pem_public_key, Some("1609459200"), None);
    assert!(result.is_err(), "O2 regression: invalid base64 signature must return Err");
}
