#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::scheme::CredentialLocation;

fn config(credential: &str, encoding: SignatureEncoding, prefix: Option<&str>) -> SchemeConfig {
    SchemeConfig {
        credential: Some(credential.parse::<CredentialLocation>().unwrap()),
        encoding:   Some(encoding),
        prefix:     prefix.map(str::to_string),
    }
}

#[test]
fn test_hmac_sha256() {
    let verifier = HmacSha256Verifier::default();
    let payload = b"test";
    let secret = "secret";

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let signature = hex::encode(mac.finalize().into_bytes());

    assert_eq!(
        verifier.verify(payload, &signature, secret, None, None).unwrap(),
        Verified::Body
    );
}

#[test]
fn test_hmac_sha1() {
    let verifier = HmacSha1Verifier::default();
    let payload = b"test";
    let secret = "secret";

    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let signature = hex::encode(mac.finalize().into_bytes());

    assert_eq!(
        verifier.verify(payload, &signature, secret, None, None).unwrap(),
        Verified::Body
    );
}

/// The three keys, one at a time against the same delivery, so that a scheme which
/// honoured the header and ignored the encoding (or the prefix) is still red here.
#[test]
fn a_configured_credential_is_decoded_as_configured() {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    let payload = b"test";
    let secret = "secret";
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let raw = mac.finalize().into_bytes();

    for (name, scheme, signature) in [
        (
            "base64, no prefix",
            config("header:X-Lago-Signature", SignatureEncoding::Base64, None),
            BASE64.encode(raw),
        ),
        (
            "hex behind a prefix",
            config("header:X-Hub-Signature-256", SignatureEncoding::Hex, Some("sha256=")),
            format!("sha256={}", hex::encode(raw)),
        ),
    ] {
        let verifier = HmacSha256Verifier::from_config("hmac-sha256", &scheme).unwrap();
        assert_eq!(
            verifier.verify(payload, &signature, secret, None, None).unwrap(),
            Verified::Body,
            "{name}: a credential written as the route describes it must verify"
        );
    }
}

#[test]
fn the_configured_header_is_what_the_scheme_asks_the_request_for() {
    let scheme = config("header:X-Lago-Signature", SignatureEncoding::Base64, None);
    let verifier = HmacSha256Verifier::from_config("hmac-sha256", &scheme).unwrap();
    assert_eq!(verifier.signature_header(), "X-Lago-Signature");
    assert_eq!(HmacSha256Verifier::default().signature_header(), "X-Signature");
}

/// A prefix that is configured and absent, and a value that is not the configured
/// encoding, are both the **sender's** fault: `InvalidFormat` (401), never
/// `KeyMaterial` (5xx), which an unauthenticated caller could otherwise produce on
/// demand (#1045).
#[test]
fn a_credential_that_does_not_match_the_configured_shape_is_the_senders_fault() {
    let scheme = config("header:X-Hub-Signature-256", SignatureEncoding::Hex, Some("sha256="));
    let verifier = HmacSha256Verifier::from_config("hmac-sha256", &scheme).unwrap();

    for bad in ["deadbeef", "sha256=zzzz"] {
        assert!(
            matches!(
                verifier.verify(b"test", bad, "secret", None, None),
                Err(SignatureError::InvalidFormat)
            ),
            "{bad:?} must be InvalidFormat"
        );
    }
}

/// Configurability must not become "anything verifies": the same shape with the
/// wrong MAC is still refused.
#[test]
fn a_forged_credential_in_the_configured_shape_still_fails() {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    let scheme = config("header:X-Lago-Signature", SignatureEncoding::Base64, None);
    let verifier = HmacSha256Verifier::from_config("hmac-sha256", &scheme).unwrap();

    let mut mac = Hmac::<Sha256>::new_from_slice(b"secret".as_slice()).unwrap();
    mac.update(b"a different body entirely");
    let forged = BASE64.encode(mac.finalize().into_bytes());

    assert!(matches!(
        verifier.verify(b"test", &forged, "secret", None, None),
        Err(SignatureError::Mismatch)
    ));
}

/// Hex is case-insensitive and the comparison is on bytes, so a correct MAC written
/// in upper case verifies. Before #1321 the two were compared as text and it did not.
#[test]
fn an_upper_case_hex_credential_verifies() {
    let payload = b"test";
    let secret = "secret";
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let signature = hex::encode(mac.finalize().into_bytes()).to_uppercase();

    assert_eq!(
        HmacSha256Verifier::default()
            .verify(payload, &signature, secret, None, None)
            .unwrap(),
        Verified::Body
    );
}
