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

// ── Cycle 2: the negatives, each on both sides of its threshold ───────────────
//
// GREEN landed with cycle 1, so none of these could be written red. Each is
// recovered by mutation instead — reverting the behaviour it pins and reading the
// diagnosis — which is the stronger form: it shows the case fails for the stated
// reason rather than merely that it fails.

/// Sign `body` under `id` at `timestamp`, and verify it with the clock at `now`.
fn verify_at(now: u64, id: &str, timestamp: u64, body: &[u8]) -> Result<Verified, SignatureError> {
    let timestamp = timestamp.to_string();
    let signature = sign(SECRET, id, &timestamp, body);
    let headers = headers(id, &timestamp, &signature);
    at(now).verify(&InboundRequest::new(&headers, body, None), SECRET)
}

/// The window the registry hands every timestamped scheme, and this scheme's
/// default: the 300 s Stripe and Slack use.
const TOLERANCE: u64 = 300;

#[test]
fn a_timestamp_exactly_at_the_tolerance_verifies() {
    // Both sides of the threshold, in both directions. A guard written `>=` instead
    // of `>` refuses a delivery that is exactly at the edge of a window the operator
    // was told is 300 seconds wide, and a size-dependent defect that hides below its
    // threshold is invisible to a test that only ever probes one side.
    for (label, now) in [
        ("as old as the window allows", SIGNED_AT + TOLERANCE),
        ("as far ahead", SIGNED_AT - TOLERANCE),
    ] {
        assert!(
            verify_at(now, MSG_ID, SIGNED_AT, PAYLOAD).is_ok(),
            "a delivery {label} must verify: the window is inclusive at {TOLERANCE}s"
        );
    }
}

#[test]
fn a_timestamp_one_second_beyond_the_tolerance_is_refused() {
    for (label, now) in [
        ("one second too old", SIGNED_AT + TOLERANCE + 1),
        ("one second too far ahead", SIGNED_AT - TOLERANCE - 1),
    ] {
        assert!(
            matches!(
                verify_at(now, MSG_ID, SIGNED_AT, PAYLOAD),
                Err(SignatureError::TimestampExpired)
            ),
            "a delivery {label} must be refused as expired — a captured delivery has to \
             stop being replayable, and the refusal has to say so rather than read as a \
             mismatch"
        );
    }
}

#[test]
fn a_genuine_delivery_whose_id_header_was_changed_is_refused() {
    // The #751 attack, closed by the signature rather than by ignoring the header:
    // the delivery below is genuine in every byte except the id, and the id is
    // inside the signed content. Before this scheme the receiver keyed the ledger on
    // exactly this header before anything signed it, so one captured delivery
    // replayed under a fresh id claimed a fresh key and re-fired every `after:ingest`
    // function.
    let timestamp = SIGNED_AT.to_string();
    let signature = sign(SECRET, MSG_ID, &timestamp, PAYLOAD);
    let tampered = headers("msg_a_fresh_id_the_attacker_chose", &timestamp, &signature);

    assert!(
        matches!(
            at(SIGNED_AT).verify(&InboundRequest::new(&tampered, PAYLOAD, None), SECRET),
            Err(SignatureError::Mismatch)
        ),
        "the signature covers the id, so replacing it must be a MISMATCH — not a \
         missing credential, and certainly not an acceptance"
    );
}

#[test]
fn a_rotating_sender_verifies_on_either_of_its_signatures() {
    // A sender mid-rotation sends one entry per active secret, space-separated, in no
    // guaranteed order. Taking only the first was #787's shape for Stripe: a genuine
    // delivery whose matching signature was not first answered 401 for the whole
    // rotation window. Both positions are tested, because a scheme that only ever
    // reads one of them passes whichever case matches that position.
    let timestamp = SIGNED_AT.to_string();
    let genuine = sign(SECRET, MSG_ID, &timestamp, PAYLOAD);
    let rotated_out = sign("whsec_cm90YXRlZC1vdXQta2V5", MSG_ID, &timestamp, PAYLOAD);
    assert_ne!(genuine, rotated_out, "the premise: the two secrets give different MACs");

    for (position, header) in [
        ("first", format!("{genuine} {rotated_out}")),
        ("second", format!("{rotated_out} {genuine}")),
    ] {
        let headers = headers(MSG_ID, &timestamp, &header);
        assert_eq!(
            at(SIGNED_AT)
                .verify(&InboundRequest::new(&headers, PAYLOAD, None), SECRET)
                .unwrap(),
            Verified::BodyWithId {
                id: MSG_ID.to_string(),
            },
            "the matching signature is {position} in the header, and the delivery is \
             genuine when ANY entry matches"
        );
    }
}

#[test]
fn an_unrecognised_version_tag_is_skipped_rather_than_refused() {
    // `v1a` is the spec's Ed25519 signature, which this scheme does not implement,
    // and `v2` is whatever comes next. A sender adding a version during a migration
    // sends both — so an unknown tag alongside a good `v1` must be ignored, not turn
    // the whole delivery into a 401.
    //
    // Nothing in the spec text states this rule. It is the only behaviour compatible
    // with a sender growing a version, so it is pinned here as behaviour rather than
    // claimed as a citation.
    let timestamp = SIGNED_AT.to_string();
    let genuine = sign(SECRET, MSG_ID, &timestamp, PAYLOAD);
    let v1_value = genuine.strip_prefix("v1,").unwrap();
    let header = format!("v1a,{v1_value} v2,{v1_value} {genuine}");
    let headers = headers(MSG_ID, &timestamp, &header);

    assert_eq!(
        at(SIGNED_AT)
            .verify(&InboundRequest::new(&headers, PAYLOAD, None), SECRET)
            .unwrap(),
        Verified::BodyWithId {
            id: MSG_ID.to_string(),
        },
        "a `v1a,` or `v2,` entry beside a good `v1,` one must be skipped"
    );
}

#[test]
fn a_header_with_no_usable_v1_entry_is_the_senders_fault_not_the_servers() {
    // #1045: sender-supplied bytes in a shape the scheme cannot read are
    // `InvalidFormat`, which maps to 401. They must NEVER map to `KeyMaterial`,
    // which maps to a 5xx — that would let any unauthenticated caller produce one on
    // demand.
    let timestamp = SIGNED_AT.to_string();
    let value = sign(SECRET, MSG_ID, &timestamp, PAYLOAD)
        .strip_prefix("v1,")
        .unwrap()
        .to_string();
    for (label, header) in [
        ("only an unimplemented version", format!("v1a,{value}")),
        ("only a future version", format!("v2,{value}")),
        ("no version tag at all", value.clone()),
        ("nothing", String::new()),
    ] {
        let headers = headers(MSG_ID, &timestamp, &header);
        let result = at(SIGNED_AT).verify(&InboundRequest::new(&headers, PAYLOAD, None), SECRET);
        assert!(
            matches!(result, Err(SignatureError::InvalidFormat)),
            "a signature header carrying {label} is unreadable, not unusable key \
             material: it must be InvalidFormat (401), never KeyMaterial (5xx); got \
             {result:?}"
        );
    }
}

#[test]
fn each_header_the_scheme_needs_is_refused_by_name_when_absent() {
    let timestamp = SIGNED_AT.to_string();
    let signature = sign(SECRET, MSG_ID, &timestamp, PAYLOAD);
    let full = headers(MSG_ID, &timestamp, &signature);

    for name in ["webhook-signature", "webhook-id"] {
        let mut headers = full.clone();
        headers.remove(name);
        let result = at(SIGNED_AT).verify(&InboundRequest::new(&headers, PAYLOAD, None), SECRET);
        assert!(
            matches!(result, Err(SignatureError::MissingCredential(ref missing)) if missing == name),
            "without `{name}` there is nothing to verify, and the refusal must name what \
             was looked for so the operator can see which header the sender omitted; \
             got {result:?}"
        );
    }

    // The timestamp is `MissingTimestamp`, matching the three other schemes that read
    // one — a more specific answer than "a credential is missing", and the one their
    // existing tests assert.
    let mut headers = full.clone();
    headers.remove("webhook-timestamp");
    assert!(matches!(
        at(SIGNED_AT).verify(&InboundRequest::new(&headers, PAYLOAD, None), SECRET),
        Err(SignatureError::MissingTimestamp)
    ));
}

#[test]
fn a_tampered_body_is_refused_even_with_a_genuine_id_and_timestamp() {
    let timestamp = SIGNED_AT.to_string();
    let signature = sign(SECRET, MSG_ID, &timestamp, PAYLOAD);
    let mut body = PAYLOAD.to_vec();
    let last = body.len() - 1;
    body[last] ^= 1;
    let headers = headers(MSG_ID, &timestamp, &signature);

    assert!(matches!(
        at(SIGNED_AT).verify(&InboundRequest::new(&headers, &body, None), SECRET),
        Err(SignatureError::Mismatch)
    ));
}
