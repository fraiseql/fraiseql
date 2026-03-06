//! Twilio webhook signature verification.
//!
//! Algorithm: HMAC-SHA1 of `URL + sorted-params`, Base64 encoded.
//!
//! The `url` parameter is required. Without it, verification fails with
//! `SignatureError::InvalidFormat`. For form-encoded bodies, append sorted
//! key=value pairs to the URL before signing. For JSON/other content types,
//! sign the URL alone (or URL + body hash per Twilio docs).
//!
//! See: <https://www.twilio.com/docs/usage/webhooks/webhooks-security>

use base64::{Engine as _, engine::general_purpose};
use hmac::{Hmac, Mac};
use sha1::Sha1;

use crate::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

/// Verifies Twilio webhook signatures using HMAC-SHA1.
///
/// Twilio signs `URL + sorted-form-params` (or just `URL` for non-form payloads) with
/// HMAC-SHA1 and Base64-encodes the result. The signature is sent in the
/// `X-Twilio-Signature` header. The `url` parameter to `verify` is required because
/// Twilio includes the full request URL in the signed payload.
pub struct TwilioVerifier;

/// Build the Twilio signing string: URL + sorted form params (if any).
///
/// For form-encoded payloads (`application/x-www-form-urlencoded`), parse the
/// body, sort parameters alphabetically, and append each `name + value` pair
/// (no delimiter between pairs) to the URL. For other content types, sign the
/// URL alone.
fn build_signing_string(url: &str, payload: &[u8]) -> String {
    // Attempt to parse body as form-urlencoded (key=value&...)
    let body_str = match std::str::from_utf8(payload) {
        Ok(s) if !s.is_empty() => s,
        _ => return url.to_string(),
    };

    // Only parse if it looks like form data (no '{' or '[' at start)
    let first = body_str.trim_start().chars().next();
    if matches!(first, Some('{' | '[') | None) {
        return url.to_string();
    }

    let mut params: Vec<(String, String)> = body_str
        .split('&')
        .filter_map(|pair| {
            let mut kv = pair.splitn(2, '=');
            let k = kv.next()?.to_string();
            let v = kv.next().unwrap_or("").to_string();
            Some((k, v))
        })
        .collect();

    // Sort alphabetically by key (Twilio requirement)
    params.sort_by(|a, b| a.0.cmp(&b.0));

    let mut signing = url.to_string();
    for (k, v) in params {
        signing.push_str(&k);
        signing.push_str(&v);
    }
    signing
}

impl SignatureVerifier for TwilioVerifier {
    fn name(&self) -> &'static str {
        "twilio"
    }

    fn signature_header(&self) -> &'static str {
        "X-Twilio-Signature"
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
        url: Option<&str>,
    ) -> Result<bool, SignatureError> {
        // Twilio signatures are computed over the URL, not just the body.
        let url = url.ok_or_else(|| {
            SignatureError::Crypto(
                "Twilio signature verification requires the request URL. \
                 Pass the full request URL as the `url` parameter."
                    .to_string(),
            )
        })?;

        let signing_string = build_signing_string(url, payload);

        let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(signing_string.as_bytes());

        let expected = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
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
}
