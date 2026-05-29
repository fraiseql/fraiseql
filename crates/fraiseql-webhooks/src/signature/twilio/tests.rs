#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use base64::{Engine as _, engine::general_purpose};
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

use super::*;

fn make_signature(url: &str, payload: &[u8], secret: &str) -> String {
    let signing = build_signing_string(url, payload);
    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signing.as_bytes());
    general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

#[test]
fn test_valid_signature_json_payload() {
    let verifier = TwilioVerifier;
    let url = "https://example.com/webhook";
    let payload = br#"{"event":"call"}"#;
    let secret = "my_auth_token";
    let sig = make_signature(url, payload, secret);

    assert!(verifier.verify(payload, &sig, secret, None, Some(url)).unwrap());
}

#[test]
fn test_valid_signature_form_payload() {
    let verifier = TwilioVerifier;
    let url = "https://example.com/webhook";
    // Form params: CallSid, From, To — should be sorted: CallSid, From, To
    let payload = b"To=%2B15551234567&From=%2B15557654321&CallSid=CA123";
    let secret = "my_auth_token";
    let sig = make_signature(url, payload, secret);

    assert!(verifier.verify(payload, &sig, secret, None, Some(url)).unwrap());
}

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
