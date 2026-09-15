#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::BTreeMap;

use super::*;
use crate::scheme::CredentialLocation;

/// Drive a generic scheme the way the route does: put the credential under the
/// header named here and hand the scheme the whole request.
///
/// The header is a parameter rather than a constant because these schemes read the
/// one their route configured — a helper that hard-coded `X-Signature` would pass
/// whether or not the configuration was honoured, which is the defect.
fn check_at(
    header: &str,
    verifier: &impl SignatureVerifier,
    payload: &[u8],
    signature: &str,
    secret: &str,
) -> Result<Verified, SignatureError> {
    let mut headers = BTreeMap::new();
    headers.insert(header.to_ascii_lowercase(), signature.to_string());
    verifier.verify(&InboundRequest::new(&headers, payload, None), secret)
}

/// [`check_at`] under the pre-#1321 default header.
fn check(
    verifier: &impl SignatureVerifier,
    payload: &[u8],
    signature: &str,
    secret: &str,
) -> Result<Verified, SignatureError> {
    check_at("X-Signature", verifier, payload, signature, secret)
}

fn config(credential: &str, encoding: SignatureEncoding, prefix: Option<&str>) -> SchemeConfig {
    SchemeConfig {
        credential:    Some(credential.parse::<CredentialLocation>().unwrap()),
        encoding:      Some(encoding),
        prefix:        prefix.map(str::to_string),
        header_prefix: None,
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

    assert_eq!(check(&verifier, payload, &signature, secret).unwrap(), Verified::Body);
}

#[test]
fn test_hmac_sha1() {
    let verifier = HmacSha1Verifier::default();
    let payload = b"test";
    let secret = "secret";

    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let signature = hex::encode(mac.finalize().into_bytes());

    assert_eq!(check(&verifier, payload, &signature, secret).unwrap(), Verified::Body);
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

    for (name, header, scheme, signature) in [
        (
            "base64, no prefix",
            "X-Lago-Signature",
            config("header:X-Lago-Signature", SignatureEncoding::Base64, None),
            BASE64.encode(raw),
        ),
        (
            "hex behind a prefix",
            "X-Hub-Signature-256",
            config("header:X-Hub-Signature-256", SignatureEncoding::Hex, Some("sha256=")),
            format!("sha256={}", hex::encode(raw)),
        ),
    ] {
        let verifier = HmacSha256Verifier::from_config("hmac-sha256", &scheme).unwrap();
        assert_eq!(
            check_at(header, &verifier, payload, &signature, secret).unwrap(),
            Verified::Body,
            "{name}: a credential written as the route describes it must verify"
        );
    }
}

/// The configured header is the one the scheme reads **out of the request**, and
/// the only one: a credential under the default name is not found.
///
/// Asserted through behaviour rather than through an accessor — since #1321 the
/// scheme locates its own credential, so there is no header name to read back, and
/// an accessor would have been a second place for the answer to be right.
#[test]
fn the_configured_header_is_the_one_the_scheme_reads() {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    let payload = b"test";
    let secret = "secret";
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    let signature = BASE64.encode(mac.finalize().into_bytes());

    let scheme = config("header:X-Lago-Signature", SignatureEncoding::Base64, None);
    let verifier = HmacSha256Verifier::from_config("hmac-sha256", &scheme).unwrap();

    assert_eq!(
        check_at("X-Lago-Signature", &verifier, payload, &signature, secret).unwrap(),
        Verified::Body
    );
    assert!(
        matches!(
            check(&verifier, payload, &signature, secret),
            Err(SignatureError::MissingCredential(_))
        ),
        "the same credential under the default header must not be found"
    );
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
                check_at("X-Hub-Signature-256", &verifier, b"test", bad, "secret"),
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
        check_at("X-Lago-Signature", &verifier, b"test", &forged, "secret"),
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
        check(&HmacSha256Verifier::default(), payload, &signature, secret).unwrap(),
        Verified::Body
    );
}
