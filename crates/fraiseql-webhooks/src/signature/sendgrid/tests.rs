#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test module — a `let ... else` that cannot bind the expected error variant must fail loudly and say what it got (#1174)

use base64::engine::general_purpose;

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

    // #1174: `"sig"` is not valid Base64 either, so `KeyMaterial(_)` alone would be
    // satisfied by a run that never reached the key at all — the exact shape #1174
    // found, where the fixture failed one stage earlier than the test intended.
    let Err(SignatureError::KeyMaterial(message)) = result else {
        panic!("an unparseable PEM key must be KeyMaterial; got {result:?}")
    };
    assert!(
        message.contains("P-256 public key"),
        "the error must name the KEY parse as the fault; got {message:?}"
    );
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

/// A genuinely valid SPKI PEM for a freshly generated P-256 key.
///
/// The previous fixture here was a hand-written PEM introduced with the comment
/// "use a real PEM key stub to get past key parsing"; its point is not on the curve,
/// so `from_public_key_pem` rejected it and this test never reached the signature
/// decode it is named for. Generating the key is what makes the name true (#1174).
fn valid_public_key_pem() -> String {
    use p256::{ecdsa::SigningKey, pkcs8::EncodePublicKey as _};
    use rand_core::OsRng;

    SigningKey::random(&mut OsRng)
        .verifying_key()
        .to_public_key_pem(p256::pkcs8::der::pem::LineEnding::default())
        .expect("P-256 VerifyingKey serializes to SPKI PEM")
}

#[test]
fn test_invalid_signature_base64() {
    let verifier = SendGridVerifier::new();
    let ts = fresh_timestamp();

    let result =
        verifier.verify(b"body", "not-base64!!!", &valid_public_key_pem(), Some(&ts), None);

    // #1045: the signature is the *sender's* input, so an unparseable one is
    // `InvalidFormat` (401) — never `KeyMaterial`, which is reserved for the server's
    // own configured key and maps to a 5xx.
    assert!(
        matches!(result, Err(SignatureError::InvalidFormat)),
        "an unparseable sender signature must be the sender's fault; got {result:?}"
    );
}

#[test]
fn test_empty_secret_rejected() {
    let verifier = SendGridVerifier::new();
    let ts = fresh_timestamp();
    let result = verifier.verify(b"body", "sig", "", Some(&ts), None);

    // #1174: an empty key and an unparseable one are different faults with the same
    // variant; only the message separates them.
    let Err(SignatureError::KeyMaterial(message)) = result else {
        panic!("an empty key must be KeyMaterial; got {result:?}")
    };
    assert!(
        message.contains("must not be empty"),
        "the error must name the EMPTY KEY as the fault, not a parse failure; got {message:?}"
    );
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
