//! Twilio webhook signature verification.
//!
//! Algorithm: HMAC-SHA1 of the signing string, Base64 encoded. Twilio has **two**
//! signing strings, and which one applies is decided by the request URI:
//!
//! * **Form-encoded** (`application/x-www-form-urlencoded` — the SMS/voice shape): the URL with the
//!   body's parameters sorted by decoded key and appended as `name + value`.
//! * **Any other body** (JSON): Twilio computes `SHA-256(body)`, appends it to the request URI as
//!   `bodySHA256=<hex>`, and signs **the URI including that parameter**. The receiver re-computes
//!   the hash over the body it actually received and compares.
//!
//! The `url` parameter is required. Without it, verification fails with
//! `SignatureError::KeyMaterial`.
//!
//! # The `bodySHA256` parameter is not trusted input (#1069)
//!
//! It arrives on the request, so an attacker can choose it — but choosing it buys nothing:
//! it is *inside* the HMAC, so changing it invalidates the signature, and it is compared
//! against the digest of the body actually received, so a captured signature cannot be
//! replayed over a different body. Both properties are needed; either alone is not enough.
//!
//! This module previously implemented only half of the second scheme: it signed the URL
//! alone for any body starting with `{` or `[` and never read a body hash — the module doc
//! itself conceded the gap. That made the MAC cover no request-specific material at all, so
//! `X-Twilio-Signature` degenerated from a per-message authenticator into a permanent static
//! bearer token, constant for a given `(auth_token, public_url)` pair forever (Twilio's
//! scheme carries no timestamp, and there is no freshness check here). One exposure of that
//! value — a proxy log with header capture, an APM trace, a debug log, or the copy the spine
//! itself persists into `_fraiseql_inbound_message.payload` — permanently authorised
//! arbitrary bodies into `emit_in_tx` and `after:ingest`. The same gap meant no genuine
//! Twilio JSON delivery could ever verify, since real Twilio sends the `bodySHA256` form.
//!
//! See: <https://www.twilio.com/docs/usage/webhooks/webhooks-security>

use base64::{Engine as _, engine::general_purpose};
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use sha2::{Digest as _, Sha256};

use crate::{
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

/// The query parameter Twilio appends for non-form bodies, carrying the hex SHA-256
/// of the raw request body.
const BODY_SHA256_PARAM: &str = "bodySHA256";

/// Verifies Twilio webhook signatures using HMAC-SHA1.
///
/// Twilio signs `URL + sorted-form-params` (or just `URL` for non-form payloads) with
/// HMAC-SHA1 and Base64-encodes the result. The signature is sent in the
/// `X-Twilio-Signature` header. The `url` parameter to `verify` is required because
/// Twilio includes the full request URL in the signed payload.
pub struct TwilioVerifier;

/// Percent-decode an `application/x-www-form-urlencoded` string (H44).
///
/// `%XX` sequences are decoded to their raw byte values and `+` to a space, then
/// the resulting byte sequence is interpreted as UTF-8 (lossily). Decoding into
/// bytes first — rather than pushing each decoded byte as its own `char` — is
/// what makes multi-byte UTF-8 (e.g. `%C3%A9` → `é`) decode correctly instead of
/// Latin-1 per byte (`Ã©`). Invalid `%XX` sequences are left verbatim.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    // Reason: h and l are hex digits (0–15), so h*16+l is always 0–255.
                    #[allow(clippy::cast_possible_truncation)]
                    out.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                i += 1;
            },
            b => {
                out.push(b);
                i += 1;
            },
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The raw `bodySHA256` value from a URL's query string, if it carries one.
///
/// Deliberately a plain scan rather than a URL parse: the only thing wanted from the
/// query is this one parameter's verbatim value, and the signing string must be the URL
/// exactly as given — re-serialising it through a parser could reorder or re-encode it
/// and change the bytes under the HMAC.
fn body_sha256_param(url: &str) -> Option<&str> {
    url.split_once('?')?
        .1
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(key, value)| (key == BODY_SHA256_PARAM).then_some(value))
}

/// Whether `payload`'s SHA-256 matches the hex digest the request URI declares.
///
/// Case-insensitive on the hex, and constant-time on the comparison — the digest is not
/// secret, but keeping every signature-path comparison the same shape is cheaper than
/// auditing which ones may vary.
fn body_hash_matches(payload: &[u8], expected_hex: &str) -> bool {
    let actual = hex::encode(Sha256::digest(payload));
    constant_time_eq(actual.as_bytes(), expected_hex.to_ascii_lowercase().as_bytes())
}

/// Build the Twilio signing string.
///
/// * **`bodySHA256` present** (Twilio's non-form scheme) — returns `None` unless the body's digest
///   matches the declared one, and otherwise signs the URL **exactly as given**, body hash
///   included. This is what binds a JSON body to its signature.
/// * **absent** — the form-encoded scheme: parse the body, sort parameters alphabetically by their
///   **decoded** key (Twilio spec), and append each decoded `name + value` pair (no delimiter
///   between pairs) to the URL.
///
/// There is no longer any non-empty body for which the signing string is the URL alone
/// (#1069). A JSON body that arrives *without* a `bodySHA256` parameter falls into the
/// form arm, where the body still reaches the signing string — it will not verify against
/// anything a sender computes, which is the correct outcome for a delivery that follows
/// neither Twilio scheme, and it is not forgeable. An empty body still signs the URL alone:
/// there is nothing to bind, and nothing an attacker gains by replaying emptiness.
///
/// Returns `None` when the declared body hash does not match the body — the caller must
/// treat that as a failed verification, never as a fallback to another signing string.
pub(crate) fn build_signing_string(url: &str, payload: &[u8]) -> Option<String> {
    if let Some(expected_hex) = body_sha256_param(url) {
        return body_hash_matches(payload, expected_hex).then(|| url.to_string());
    }

    // Attempt to parse body as form-urlencoded (key=value&...)
    let body_str = match std::str::from_utf8(payload) {
        Ok(s) if !s.is_empty() => s,
        _ => return Some(url.to_string()),
    };

    let mut params: Vec<(String, String)> = body_str
        .split('&')
        .filter_map(|pair| {
            let mut kv = pair.splitn(2, '=');
            let raw_k = kv.next()?;
            let raw_v = kv.next().unwrap_or("");
            // Decode key and value per Twilio's signing algorithm
            Some((percent_decode(raw_k), percent_decode(raw_v)))
        })
        .collect();

    // Sort alphabetically by decoded key (Twilio requirement)
    params.sort_by(|a, b| a.0.cmp(&b.0));

    let mut signing = url.to_string();
    for (k, v) in params {
        signing.push_str(&k);
        signing.push_str(&v);
    }
    Some(signing)
}

impl SignatureVerifier for TwilioVerifier {
    fn name(&self) -> &'static str {
        "twilio"
    }

    fn signature_header(&self) -> &'static str {
        "X-Twilio-Signature"
    }

    fn requires_url(&self) -> bool {
        true
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
            SignatureError::KeyMaterial(
                "Twilio signature verification requires the request URL. \
                 Pass the full request URL as the `url` parameter."
                    .to_string(),
            )
        })?;

        if secret.is_empty() {
            return Err(SignatureError::KeyMaterial(
                "Twilio auth token must not be empty".to_string(),
            ));
        }

        // `None` means the URI declared a `bodySHA256` that is not this body's digest:
        // the delivery is refused outright rather than falling back to another signing
        // string, which would let a sender opt out of body binding by supplying a hash
        // it knows is wrong.
        let Some(signing_string) = build_signing_string(url, payload) else {
            return Ok(false);
        };

        let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::KeyMaterial(e.to_string()))?;
        mac.update(signing_string.as_bytes());

        let expected = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}

#[cfg(test)]
mod tests;
