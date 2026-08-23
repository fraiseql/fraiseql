#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use base64::engine::general_purpose;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

use super::*;

#[test]
fn test_invalid_signature() {
    let verifier = TwilioVerifier;
    let url = "https://example.com/webhook";
    let payload = b"some body";
    assert!(!verifier.verify(payload, "invalidsig==", "secret", None, Some(url)).unwrap());
}

#[test]
fn test_missing_url_returns_error() {
    let verifier = TwilioVerifier;
    let result = verifier.verify(b"payload", "sig", "secret", None, None);
    assert!(matches!(result, Err(SignatureError::KeyMaterial(_))));
}

#[test]
fn test_form_params_sorted_alphabetically() {
    // "Zfirst=1&Asecond=2" → sorted: Asecond, Zfirst
    let url = "https://example.com/w";
    let payload = b"Zfirst=1&Asecond=2";
    let signing = build_signing_string(url, payload).unwrap();
    assert_eq!(signing, "https://example.com/wAsecond2Zfirst1");
}

// ── H44: percent-decoding must be UTF-8-correct and decode '+' as space ───────

/// Sign an *independently-constructed* signing string per Twilio's published
/// algorithm (HMAC-SHA1, Base64). Deliberately does NOT call the in-repo
/// `build_signing_string`, so the test cannot pass by sharing a bug with it.
fn twilio_sign(signing_string: &str, secret: &str) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signing_string.as_bytes());
    general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

#[test]
fn verifies_form_payload_with_space_and_utf8() {
    let verifier = TwilioVerifier;
    let url = "https://example.com/webhook";
    let secret = "my_auth_token";
    // `Body` carries a space (sent as '+') and an accented character
    // (é = %C3%A9 in UTF-8). Keys sort as Body, Name.
    let payload = b"Body=hello+world&Name=Jos%C3%A9";

    // Twilio signs URL + sorted (key + decoded value) pairs: '+' decodes to a
    // space and %C3%A9 decodes (as UTF-8, not Latin-1 per byte) to 'é'.
    let expected_signing = "https://example.com/webhookBodyhello worldNameJosé";
    let signature = twilio_sign(expected_signing, secret);

    assert!(
        verifier.verify(payload, &signature, secret, None, Some(url)).unwrap(),
        "a signature computed per Twilio's published algorithm must verify"
    );
}

// ── #1069: a JSON body must be covered by the signature ──────────────────────
//
// These replace `verifies_json_payload_against_url_only`, which pinned the defect as
// intended behaviour: it asserted that `HMAC(url)` alone verifies a JSON body, which is
// exactly the property that made `X-Twilio-Signature` a static bearer token.

/// Twilio's documented non-form scheme: `bodySHA256=<hex>` on the URI, and the URI
/// *including* it is what gets signed.
fn twilio_json_url(base: &str, body: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    format!("{base}?bodySHA256={}", hex::encode(Sha256::digest(body)))
}

#[test]
fn verifies_json_payload_against_url_and_body_hash() {
    let verifier = TwilioVerifier;
    let secret = "my_auth_token";
    let payload = br#"{"event":"call"}"#;
    let url = twilio_json_url("https://example.com/webhook", payload);

    let signature = twilio_sign(&url, secret);

    assert!(
        verifier.verify(payload, &signature, secret, None, Some(&url)).unwrap(),
        "a signature computed per Twilio's published body-hash algorithm must verify"
    );
}

/// The defect, stated as a test: one captured signature must not authorise a second body.
#[test]
fn a_json_signature_does_not_carry_over_to_a_different_body() {
    let verifier = TwilioVerifier;
    let secret = "my_auth_token";
    let genuine = br#"{"event":"call","amount":1}"#;
    let url = twilio_json_url("https://example.com/webhook", genuine);
    let signature = twilio_sign(&url, secret);

    // Same URL, same header, attacker-chosen body — the shape the finding describes.
    let forged = br#"{"event":"call","amount":999999}"#;
    assert!(
        !verifier.verify(forged, &signature, secret, None, Some(&url)).unwrap(),
        "a signature genuine for one body must not verify another"
    );
}

/// Flipping a single byte changes the digest, so the declared hash no longer describes
/// the body and the delivery is refused — the property `every_tampered_delivery_is_rejected`
/// could never reach while the only Twilio fixture was form-encoded.
#[test]
fn a_tampered_json_body_is_rejected() {
    let verifier = TwilioVerifier;
    let secret = "my_auth_token";
    let genuine = br#"{"event":"call","sid":"CA1"}"#;
    let url = twilio_json_url("https://example.com/webhook", genuine);
    let signature = twilio_sign(&url, secret);

    let mut tampered = genuine.to_vec();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;

    assert!(!verifier.verify(&tampered, &signature, secret, None, Some(&url)).unwrap());
}

/// The pre-#1069 signature — `HMAC(public_url)` with no body material — must no longer
/// verify anything. This is the regression guard for the static-bearer-token defect.
#[test]
fn the_body_free_url_only_signature_no_longer_verifies_a_json_body() {
    let verifier = TwilioVerifier;
    let base = "https://example.com/webhook";
    let secret = "my_auth_token";
    let payload = br#"{"event":"call"}"#;

    let legacy_signature = twilio_sign(base, secret);

    assert!(
        !verifier.verify(payload, &legacy_signature, secret, None, Some(base)).unwrap(),
        "the constant HMAC(public_url) must not authorise a JSON body"
    );
}

#[test]
fn verifies_form_payload_with_encoded_plus_sign() {
    let verifier = TwilioVerifier;
    let url = "https://example.com/webhook";
    let secret = "my_auth_token";
    // `%2B` is a literal '+' (e.g. an E.164 phone number), distinct from a
    // space-encoding '+'. Keys sort as CallSid, From, To.
    let payload = b"To=%2B15551234567&From=%2B15557654321&CallSid=CA123";

    let expected_signing = "https://example.com/webhookCallSidCA123From+15557654321To+15551234567";
    let signature = twilio_sign(expected_signing, secret);

    assert!(verifier.verify(payload, &signature, secret, None, Some(url)).unwrap());
}
