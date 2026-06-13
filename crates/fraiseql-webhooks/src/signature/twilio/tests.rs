#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use base64::{Engine as _, engine::general_purpose};
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
    assert!(matches!(result, Err(SignatureError::Crypto(_))));
}

#[test]
fn test_form_params_sorted_alphabetically() {
    // "Zfirst=1&Asecond=2" → sorted: Asecond, Zfirst
    let url = "https://example.com/w";
    let payload = b"Zfirst=1&Asecond=2";
    let signing = build_signing_string(url, payload);
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

#[test]
fn verifies_json_payload_against_url_only() {
    let verifier = TwilioVerifier;
    let url = "https://example.com/webhook";
    let secret = "my_auth_token";
    let payload = br#"{"event":"call"}"#;

    // JSON bodies are not form-encoded, so Twilio signs the URL alone.
    let signature = twilio_sign(url, secret);

    assert!(verifier.verify(payload, &signature, secret, None, Some(url)).unwrap());
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
