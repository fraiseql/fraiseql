//! #1323: the Standard Webhooks scheme, against the vectors its publishers ship.
//!
//! Every Svix sender — Clerk included — signs `"{id}.{timestamp}.{body}"` with
//! HMAC-SHA256 under a base64 `whsec_` secret, and puts the id, the timestamp and
//! one or more `v1,<base64>` signatures in three separate headers. No verifier
//! implemented it, and `hmac-sha256` cannot be configured into it: it signs the
//! body alone and its credential is a single value in a single header.
//!
//! The cases below are **published by the scheme's own publishers**, not
//! synthesized here, which is what makes them worth having:
//!
//! | source | secret decodes to | why it is here |
//! |---|---|---|
//! | Svix's manual-verification documentation | **18 bytes** | the provider's own worked example |
//! | the spec's Rust reference library (`libraries/rust/src/lib.rs`) | 24 bytes | a second, independent publisher |
//!
//! The spec document itself publishes **no** vectors; these two are what exist.
//!
//! ⚠ 18 bytes is **below** the 24–64 byte range the Standard Webhooks spec states
//! for secrets. A decoder that enforced the range would refuse the provider's own
//! documented key, so this scheme validates that the secret *decodes* and does not
//! validate its length. The first row is the case that pins that.
//!
//! Each expected signature is also recomputed here from the documented algorithm
//! and compared against the published string, so a mistyped vector fails as a
//! mistyped vector rather than as a broken verifier.
//!
//! **Infrastructure:** none
//! **Parallelism:** safe

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — a fixture that cannot be built must stop the run

use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use fraiseql_webhooks::{SchemeConfig, SignatureError, Verified, build_scheme};
use hmac::{Hmac, KeyInit as _, Mac as _};
use sha2::Sha256;

/// The freshness window is not what these cases are about: the published vectors
/// carry fixed timestamps from 2022 and 2024, and re-signing them under a fresh
/// timestamp would replace the published vector with one of my own. So the
/// scheme is built with an effectively infinite window — which
/// `check_timestamp_freshness` already saturates rather than wraps (#1049) — and
/// freshness gets its own cases, on both sides of the threshold, in
/// `signature::standard_webhooks::tests`.
const NO_FRESHNESS_WINDOW: u64 = u64::MAX;

/// One published vector, exactly as its publisher writes it.
struct Vector {
    source:    &'static str,
    id:        &'static str,
    timestamp: &'static str,
    payload:   &'static str,
    secret:    &'static str,
    /// The `webhook-signature` header value, verbatim from the publication.
    signature: &'static str,
}

/// Svix's manual-verification page. Its secret decodes to **18** bytes.
const SVIX_DOCS: Vector = Vector {
    source:    "Svix manual-verification documentation",
    id:        "msg_loFOjxBNrRLzqYUf",
    timestamp: "1731705121",
    payload:   r#"{"event_type":"ping","data":{"success":true}}"#,
    secret:    "whsec_plJ3nmyCDGBKInavdOK15jsl",
    signature: "v1,rAvfW3dJ/X/qxhsaXPOyyCGmRKsaKWcsNccKXlIktD0=",
};

/// The spec's Rust reference library, `libraries/rust/src/lib.rs`, fixed-timestamp
/// test. A second publisher, and a secret inside the spec's stated length range.
const REFERENCE_LIBRARY: Vector = Vector {
    source:    "standard-webhooks/libraries/rust fixed-timestamp test",
    id:        "msg_27UH4WbU6Z5A5EzD8u03UvzRbpk",
    timestamp: "1649367553",
    payload:   r#"{"email":"test@example.com","username":"test_user"}"#,
    secret:    "whsec_C2FVsBQIhrscChlQIMV+b5sSYspob7oD",
    signature: "v1,tZ1I4/hDygAJgO5TYxiSd6Sd0kDW6hPenDe+bTa3Kkw=",
};

/// HMAC-SHA256 over `message` under the raw `key` bytes.
fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

/// The documented algorithm, implemented here rather than by calling the scheme:
/// strip `whsec_`, base64-decode, HMAC-SHA256 over the raw bytes of
/// `id.timestamp.body`, base64-encode.
fn sign(secret: &str, id: &str, timestamp: &str, body: &[u8]) -> String {
    let key = BASE64.decode(secret.strip_prefix("whsec_").unwrap_or(secret)).unwrap();
    let mut signed = format!("{id}.{timestamp}.").into_bytes();
    signed.extend_from_slice(body);
    format!("v1,{}", BASE64.encode(hmac_sha256(&key, &signed)))
}

/// The three headers a Standard Webhooks sender sets, under the `webhook` prefix,
/// lower-cased the way the server's `collect_headers` stores them.
fn headers(id: &str, timestamp: &str, signature: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("webhook-id".to_string(), id.to_string()),
        ("webhook-timestamp".to_string(), timestamp.to_string()),
        ("webhook-signature".to_string(), signature.to_string()),
    ])
}

/// Verify a delivery through the **real construction**: `build_scheme` is the one
/// place a route's scheme comes from (#1321), so a case that went around it would
/// prove the verifier works and not that the scheme can be configured at all.
fn verify(
    provider: &str,
    headers: &BTreeMap<String, String>,
    body: &[u8],
    secret: &str,
) -> Result<Verified, SignatureError> {
    let scheme = build_scheme(provider, &SchemeConfig::default(), NO_FRESHNESS_WINDOW)
        .unwrap_or_else(|error| {
            panic!(
                "#1323: `provider = \"{provider}\"` must name a scheme `build_scheme` can \
                 build. Today it names none, so no route can receive a Svix or Clerk \
                 delivery at all; got: {error}"
            )
        });
    scheme.verify(&fraiseql_webhooks::InboundRequest::new(headers, body, None), secret)
}

#[test]
fn each_published_vector_is_recorded_as_its_publisher_wrote_it() {
    // The guard on the two cases below: if a vector were mistyped here, they would
    // fail as "the verifier is broken" and the diagnosis would be wrong.
    for vector in [SVIX_DOCS, REFERENCE_LIBRARY] {
        assert_eq!(
            sign(vector.secret, vector.id, vector.timestamp, vector.payload.as_bytes()),
            vector.signature,
            "{}: the documented algorithm must reproduce the published signature. If this \
             fails, the vector above is transcribed wrong, not the scheme.",
            vector.source
        );
    }
}

#[test]
fn a_published_vector_verifies_and_authenticates_its_id() {
    for vector in [SVIX_DOCS, REFERENCE_LIBRARY] {
        let result = verify(
            "standard-webhooks",
            &headers(vector.id, vector.timestamp, vector.signature),
            vector.payload.as_bytes(),
            vector.secret,
        );
        assert!(
            result.is_ok(),
            "{}: the publisher's own worked example must verify. The signed content is \
             `{{id}}.{{timestamp}}.{{body}}` and the secret is base64 behind `whsec_` — \
             using the undecoded `whsec_…` string as the key gives a different MAC, so \
             this case discriminates the decode; got {result:?}",
            vector.source
        );
    }
}

#[test]
fn the_secret_is_not_refused_for_being_shorter_than_the_spec_states() {
    // Svix's own documented secret decodes to 18 bytes; the spec says 24–64. A
    // decoder that enforced the range would refuse the provider's published key, so
    // this scheme validates that the secret decodes and nothing about its length.
    let key = BASE64.decode(SVIX_DOCS.secret.strip_prefix("whsec_").unwrap()).unwrap();
    assert_eq!(key.len(), 18, "the premise: this key is below the spec's stated range");

    let result = verify(
        "standard-webhooks",
        &headers(SVIX_DOCS.id, SVIX_DOCS.timestamp, SVIX_DOCS.signature),
        SVIX_DOCS.payload.as_bytes(),
        SVIX_DOCS.secret,
    );
    assert!(
        result.is_ok(),
        "an 18-byte secret must verify: it is what the provider's documentation \
         publishes. Got {result:?}"
    );
}

#[test]
fn a_body_that_is_not_utf8_verifies_over_its_raw_bytes() {
    // The published vectors' bodies are ASCII, so they cannot tell a raw-bytes
    // implementation from one that signs `String::from_utf8_lossy(body)` —
    // `StripeVerifier` does exactly that, and this case exists so the new scheme
    // cannot copy it. Under lossy conversion each of these two bytes becomes a
    // three-byte U+FFFD, so the signed content differs and the MAC cannot match.
    let mut body = br#"{"id":"evt_raw","type":"ping","note":""#.to_vec();
    body.extend_from_slice(&[0xff, 0xfe]);
    body.extend_from_slice(br#""}"#);
    assert!(std::str::from_utf8(&body).is_err(), "the premise: this body is not UTF-8");

    let id = "msg_raw_bytes";
    let timestamp = "1731705121";
    let signature = sign(SVIX_DOCS.secret, id, timestamp, &body);

    let result = verify(
        "standard-webhooks",
        &headers(id, timestamp, &signature),
        &body,
        SVIX_DOCS.secret,
    );
    assert!(
        result.is_ok(),
        "a delivery signed over the body's raw bytes must verify. Signing \
         `String::from_utf8_lossy(body)` instead replaces each of the two bytes above \
         with U+FFFD and the MAC no longer matches; got {result:?}"
    );
}

#[test]
fn a_clerk_route_reads_the_same_scheme_under_svix_header_names() {
    // Clerk is Svix under `svix-*` header names and nothing else, so it is a preset
    // over this scheme rather than a second implementation of it.
    let result = verify(
        "clerk",
        &BTreeMap::from([
            ("svix-id".to_string(), SVIX_DOCS.id.to_string()),
            ("svix-timestamp".to_string(), SVIX_DOCS.timestamp.to_string()),
            ("svix-signature".to_string(), SVIX_DOCS.signature.to_string()),
        ]),
        SVIX_DOCS.payload.as_bytes(),
        SVIX_DOCS.secret,
    );
    assert!(
        result.is_ok(),
        "#1323: `provider = \"clerk\"` must be `standard-webhooks` under the `svix` \
         header prefix; got {result:?}"
    );
}
