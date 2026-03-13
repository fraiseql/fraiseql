//! SR-3: O1 — TwilioVerifier used HMAC-SHA1 of body instead of
//!       HMAC-SHA1 of URL + sorted-form-params (the Twilio-specified algorithm).
//!       Fix: signing string = URL + alphabetically-sorted key+value pairs.
//!
//! Tests use self-computed reference signatures so the test does not depend on
//! network access or a Twilio account. The reference computation uses the same
//! algorithm as the production verifier.
//!
//! **Infrastructure:** none
//! **Parallelism:** safe

#![allow(clippy::unwrap_used)]  // Reason: test code, panics are acceptable
#![allow(clippy::doc_markdown)] // Reason: doc comments use type names without backticks for readability

use base64::{Engine as _, engine::general_purpose};
use fraiseql_webhooks::signature::twilio::TwilioVerifier;
use fraiseql_webhooks::traits::SignatureVerifier as _;
use hmac::{Hmac, Mac};
use sha1::Sha1;

// ---------------------------------------------------------------------------
// Reference implementation (mirrors build_signing_string in twilio.rs)
// ---------------------------------------------------------------------------

fn make_twilio_signature(url: &str, form_body: &[u8], secret: &str) -> String {
    // Parse and sort form params — mirrors the production signing string builder.
    let body_str = std::str::from_utf8(form_body).unwrap_or("");
    let signing = if body_str.is_empty()
        || matches!(body_str.trim_start().chars().next(), Some('{' | '[') | None)
    {
        // JSON or empty body: sign URL only.
        url.to_string()
    } else {
        let mut params: Vec<(&str, &str)> = body_str
            .split('&')
            .filter_map(|pair| {
                let mut kv = pair.splitn(2, '=');
                let k = kv.next()?;
                let v = kv.next().unwrap_or("");
                Some((k, v))
            })
            .collect();
        params.sort_by_key(|(k, _)| *k);

        let mut s = url.to_string();
        for (k, v) in params {
            s.push_str(k);
            s.push_str(v);
        }
        s
    };

    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signing.as_bytes());
    general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

// ---------------------------------------------------------------------------
// SR-3 regression tests
// ---------------------------------------------------------------------------

/// A correctly-signed Twilio form-encoded webhook must be accepted.
///
/// The signature is computed over URL + sorted form params (Twilio algorithm).
/// Before the O1 fix, it was incorrectly computed over the body only.
#[test]
fn known_twilio_form_signature_verifies_correctly() {
    const SECRET: &str = "test_auth_token_12345";
    const URL: &str = "https://example.com/twilio/webhook";
    // Form params in unsorted order (Twilio sends them alphabetically after sorting)
    const BODY: &[u8] = b"To=%2B18005551212&From=%2B14158675309&CallSid=CA1234567890ABCDE";

    let signature = make_twilio_signature(URL, BODY, SECRET);
    let verifier = TwilioVerifier;

    let result = verifier.verify(BODY, &signature, SECRET, None, Some(URL));

    assert!(
        result.unwrap_or(false),
        "O1 regression: known-good Twilio form-encoded signature was rejected"
    );
}

/// A correctly-signed Twilio JSON webhook must be accepted.
///
/// For JSON payloads, Twilio signs the URL alone (no body params).
#[test]
fn known_twilio_json_signature_verifies_correctly() {
    const SECRET: &str = "test_auth_token_12345";
    const URL: &str = "https://example.com/twilio/json-webhook";
    const BODY: &[u8] = br#"{"event":"call.completed","callSid":"CA123"}"#;

    let signature = make_twilio_signature(URL, BODY, SECRET);
    let verifier = TwilioVerifier;

    let result = verifier.verify(BODY, &signature, SECRET, None, Some(URL));

    assert!(
        result.unwrap_or(false),
        "O1 regression: known-good Twilio JSON signature was rejected"
    );
}

/// A forged signature (e.g., computed only over the body, ignoring the URL)
/// must be rejected. This is the specific failure mode of the O1 bug.
#[test]
fn forged_twilio_signature_computed_over_body_only_is_rejected() {
    const SECRET: &str = "test_auth_token_12345";
    const URL: &str = "https://example.com/twilio/webhook";
    const BODY: &[u8] = b"To=%2B18005551212&From=%2B14158675309&CallSid=CA1234567890ABCDE";

    // Forge: sign body only (the old wrong algorithm)
    let mut mac = Hmac::<Sha1>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(BODY);
    let forged_sig = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let verifier = TwilioVerifier;
    let result = verifier.verify(BODY, &forged_sig, SECRET, None, Some(URL));

    assert!(
        !result.unwrap_or(true),
        "O1 regression: forged Twilio signature (body-only HMAC) was accepted"
    );
}

/// Verifying without the request URL must return an error (not a false positive).
///
/// URL is required for the Twilio algorithm — accepting without URL would be a
/// security bug (it would make the URL forgeable).
#[test]
fn twilio_verification_without_url_returns_error() {
    const SECRET: &str = "test_auth_token_12345";
    const BODY: &[u8] = b"CallSid=CA123&From=%2B14158675309";

    let verifier = TwilioVerifier;
    let result = verifier.verify(BODY, "AAAAAAAAAAAAAAAAAAAAAAAAAAAA=", SECRET, None, None);

    assert!(
        result.is_err(),
        "O1 regression: Twilio verification without URL must return an error, not Ok(false)"
    );
}

/// Signing order must be alphabetical — if params are reordered in the body,
/// the signature must still verify because params are re-sorted before signing.
#[test]
fn twilio_form_params_sorted_alphabetically_before_signing() {
    const SECRET: &str = "sorting_test_secret";
    const URL: &str = "https://example.com/hook";
    // Body with params in reverse alphabetical order
    const BODY_REVERSED: &[u8] = b"Zfield=last&Afield=first";

    let signature = make_twilio_signature(URL, BODY_REVERSED, SECRET);
    let verifier = TwilioVerifier;

    // Verification must succeed because the verifier also sorts the params.
    assert!(
        verifier.verify(BODY_REVERSED, &signature, SECRET, None, Some(URL)).unwrap(),
        "O1 regression: Twilio verifier must sort params alphabetically"
    );
}
