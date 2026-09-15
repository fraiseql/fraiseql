#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics are acceptable

use std::{collections::BTreeMap, sync::Arc};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use hmac::{Hmac, KeyInit as _, Mac as _};
use sha2::Sha256;

use super::*;
use crate::testing::mocks::MockClock;

/// Svix's manual-verification example. Its key decodes to 18 bytes — below the
/// spec's stated 24–64 range, which is why this scheme validates that a secret
/// decodes and not how long it is. See `tests/standard_webhooks_test.rs`, which
/// holds the published vectors and the transcription guard for them.
const SECRET: &str = "whsec_plJ3nmyCDGBKInavdOK15jsl";
const MSG_ID: &str = "msg_loFOjxBNrRLzqYUf";
const SIGNED_AT: u64 = 1_731_705_121;
const PAYLOAD: &[u8] = br#"{"event_type":"ping","data":{"success":true}}"#;
const PUBLISHED: &str = "v1,rAvfW3dJ/X/qxhsaXPOyyCGmRKsaKWcsNccKXlIktD0=";

/// The documented algorithm, implemented here rather than by calling the verifier:
/// strip `whsec_`, base64-decode, HMAC-SHA256 over the raw bytes of
/// `id.timestamp.body`, base64-encode behind a `v1,` tag.
fn sign(secret: &str, id: &str, timestamp: &str, body: &[u8]) -> String {
    let key = BASE64.decode(secret.strip_prefix("whsec_").unwrap_or(secret)).unwrap();
    let mut signed = format!("{id}.{timestamp}.").into_bytes();
    signed.extend_from_slice(body);
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).unwrap();
    mac.update(&signed);
    format!("v1,{}", BASE64.encode(mac.finalize().into_bytes()))
}

fn headers(id: &str, timestamp: &str, signature: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("webhook-id".to_string(), id.to_string()),
        ("webhook-timestamp".to_string(), timestamp.to_string()),
        ("webhook-signature".to_string(), signature.to_string()),
    ])
}

/// The scheme with its clock frozen at `now`, so a published vector's fixed
/// timestamp can be verified at the moment it was signed rather than under a
/// tolerance wide enough to accept anything.
fn at(now: u64) -> StandardWebhooksVerifier {
    StandardWebhooksVerifier::new().with_clock(Arc::new(MockClock::new(now)))
}

#[test]
fn the_published_vector_verifies_with_the_clock_at_the_signing_instant() {
    let headers = headers(MSG_ID, &SIGNED_AT.to_string(), PUBLISHED);
    let result = at(SIGNED_AT).verify(&InboundRequest::new(&headers, PAYLOAD, None), SECRET);
    assert_eq!(
        result.unwrap(),
        Verified::BodyWithId {
            id: MSG_ID.to_string(),
        },
        "the delivery must verify AND report the id it authenticated — the id is the \
         point of the scheme, and `Verified::Body` would drop it"
    );
}

#[test]
fn the_authenticated_id_is_the_signed_one_and_not_anything_in_the_body() {
    // The body carries its own `id`, and it is not the signed one. Whatever keys the
    // replay defence must come from the signature; a scheme that answered with the
    // body's id would pass a fixture where the two agree.
    let body = br#"{"id":"whatever-the-sender-put-here","event_type":"ping"}"#;
    let timestamp = SIGNED_AT.to_string();
    let signature = sign(SECRET, MSG_ID, &timestamp, body);
    let headers = headers(MSG_ID, &timestamp, &signature);

    assert_eq!(
        at(SIGNED_AT)
            .verify(&InboundRequest::new(&headers, body, None), SECRET)
            .unwrap(),
        Verified::BodyWithId {
            id: MSG_ID.to_string(),
        },
    );
}

/// A configured `header_prefix` must be **honoured**, not merely accepted.
///
/// Without this case the key can be read, validated and then ignored, and the only
/// test that notices is the malformed-value one — which passes whether or not the
/// value is ever used. That is the silent drop the whole #1321 seam exists to
/// remove, one key later: an operator configures `svix`, the scheme keeps reading
/// `webhook-*`, and every genuine delivery 401s.
///
/// Discriminating on both sides: the same delivery under the other spelling must be
/// refused, so "honours the prefix" is not satisfiable by a scheme that reads both.
#[test]
fn a_configured_header_prefix_is_the_one_the_scheme_reads() {
    let timestamp = SIGNED_AT.to_string();
    let signature = sign(SECRET, MSG_ID, &timestamp, PAYLOAD);
    let configured = SchemeConfig {
        header_prefix: Some("svix".to_string()),
        ..SchemeConfig::default()
    };
    let scheme = StandardWebhooksVerifier::from_config("standard-webhooks", &configured)
        .expect("`svix` can form a header name")
        .with_clock(Arc::new(MockClock::new(SIGNED_AT)));

    let svix = BTreeMap::from([
        ("svix-id".to_string(), MSG_ID.to_string()),
        ("svix-timestamp".to_string(), timestamp.clone()),
        ("svix-signature".to_string(), signature.clone()),
    ]);
    assert_eq!(
        scheme.verify(&InboundRequest::new(&svix, PAYLOAD, None), SECRET).unwrap(),
        Verified::BodyWithId {
            id: MSG_ID.to_string(),
        },
        "a route configured with `header_prefix = \"svix\"` must read `svix-*`"
    );

    // The identical delivery under the spec's default spelling: a scheme that read
    // both prefixes would pass the assertion above and this one is what refuses it.
    let webhook = headers(MSG_ID, &timestamp, &signature);
    assert!(
        matches!(
            scheme.verify(&InboundRequest::new(&webhook, PAYLOAD, None), SECRET),
            Err(SignatureError::MissingCredential(_))
        ),
        "a route configured for `svix-*` must not also accept `webhook-*`: the prefix \
         is the sender's spelling, and accepting both would let a delivery signed for \
         one endpoint be replayed at another that spells its headers differently"
    );
}
