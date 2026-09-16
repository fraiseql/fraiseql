//! #1322: the three identity providers that authenticate a delivery with a JWT.
//!
//! Hanko, Kinde and FusionAuth do not send an HMAC over the body. They send a
//! token signed by a key they publish in a JWKS, and the event is *inside* the
//! token — so none of them could be received at all before this: the receiving
//! path had a header-only credential, a mandatory shared secret, and a
//! synchronous verify with nowhere to look a key up.
//!
//! Every case here goes through [`build_scheme`], the one construction (#1321),
//! so a fixture cannot pass against a verifier the route would not have been
//! given. The key source is local — no network, no wiremock — because what these
//! pin is the *scheme*, and the bound on fetching is pinned in `fraiseql-jwks`.
//!
//! [`token_fixtures`] is the corpus the crate-wide coverage gate in
//! `signature::tests` reads: a scheme cannot reach `KNOWN_SCHEMES` without a
//! genuine delivery and a tampered twin living here.
//!
//! **Execution engine:** in-process · **Infrastructure:** none

// Reason: test code, panics acceptable. `redundant_pub_crate` is allowed because
// `LocalKeys` and `token_fixtures` are read by the sibling test module that owns
// the crate-wide coverage gate — and `pub` in a `#[cfg(test)]` module would then
// demand rustdoc and `#[must_use]` on test fixtures.
#![allow(clippy::unwrap_used, clippy::panic, clippy::redundant_pub_crate)]

use std::{collections::BTreeMap, sync::Arc};

use fraiseql_jwks::{BoxFuture, Jwk, JwksError, JwksKeys};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};

use crate::{
    request::InboundRequest,
    scheme::{CredentialLocation, SchemeConfig, SchemeContext, build_scheme},
    signature::{SignatureError, Verified},
    traits::SignatureVerifier,
};

// ============================================================================
// The fixture publisher: one RSA key, served without a network
// ============================================================================

/// A 2048-bit RSA private key, PKCS#8 PEM. Generated offline for tests only.
const RSA_PRIVATE_PEM: &str = "\
-----BEGIN PRIVATE KEY-----\n\
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCpf3bisHt/omOk\n\
VFHz/xb4p14mkeOerg4balAN0NznbieVbmnKwPjaaUfS9ZspwwCn9bLbIAaMIa3G\n\
oKqsSyfIITWNikiLp8ZnzaQH8JbgPLGaSfvy4w6dTp0cm9kL4te6KRk2J7owbVdp\n\
wW6nfFKyYwtNAJLSDg6aX7HCJ9QAoWT9rWC8lKvCodTwYvrf2T5PkLje8UDZYHx1\n\
WC+T9bal+0uKl+hP7j5NIM84Kh2W0KfMEOkMlGdQ6r8Y7aSuum2qYnH5K8gSoWlB\n\
Tn7393F+dbkTGmfGlo9k36flmIhu1eFWUgrYhG5HdKwofOI4HQEH9cuW+RHplrpt\n\
S6D9BJnRAgMBAAECggEAOz6FVGjxUcR15YtfddR0uAbwHrUhhWY7IhP/1URq4i2b\n\
glysd6UJlnX0F+WnDWrOgOadVIAWKcbf0ax4224NgqMw778k6kODUucK7YeHhOtR\n\
/KbdfKEmi49d1REYRVJNqxEQceBi8OhXBG0K+1m2IgoCejC4INmu+wB1xnJbZLhz\n\
B4uGLljKaqBssFIfsV4n+zcZcqesCTpsCcqcWURjcCWbWVBpqG7EkTFRtq35T/+c\n\
eQQTeH/UR/Bv9IHALJeTXQ41GYskZ4UPV0OMQ/bQtpojB6KZfTyz+CNn2iUeLNN6\n\
HXE8oAg3h4Unhajq8jT4XrWxY69HZhb/8zSeXRxEfQKBgQDdwvNmcG2GH1quysD+\n\
9qvO+w19lRun4AC886nQoaalrhaXAeK/GjS1D6vnUiJoN/rhkfI8mzhWdjHVgtql\n\
yJjKWb6C2bwTGsF1eDn1JTOZx+O4E/ToU/h1OzyAjrRTTPIiabSLNsCugwF45KHM\n\
ACEctgfUKVel/KAPeN0dkpPjEwKBgQDDqs/RZWj/FqJtj+SGBl6xcOL6F7L9d2hW\n\
0nZj/8/bgmRyvncO8A0YooqcJnMsYUWuhxdkkOH5f/q6FEuDrxJn9EdUxJNp4g4H\n\
65pcTJynQEF0QN/cc/1zR2H0h2TblS5mTW/Ya1GbLmu5KYshLjqDfKGDUgqpV73+\n\
6juxARHICwKBgQC3YgaLmL9JYVZJIwu0C+IJ2JvQVOS4z0ls94ZfK742Vh8CIyIR\n\
7CbX76y1LrubOWey71DFA4r0HOua54nN/HM1Kj+bz1hy5/ZBIPm0ml3wdlb+myo0\n\
kXPt5d1jZh8Cn6fAA2+0i8OMzHMEOPT/UMAREQqqTMHZVm46PTWExfiblwKBgQCH\n\
EYqTyaVJMZ6+cu4VdqA3bO3CJknwnlTwWihPr28U4FXmv4QAU8U2lD2KvSAUKrGn\n\
YKnNShYz3Rx/BzN5m4jhKcdzxJ7eIKX+4ayUum4JJloInh/qVkdHJKeB3VTKH5kA\n\
FcR3aN3UeZ7zGrJoHTlXOtljhWbGr0MAjUDXVx2nMQKBgAOyNRVrUMJiwamU2Gc1\n\
BapbzTBbUEbvWImcNE0hk7GyENBNhse6z/nIdp/DPMEJH8N45qpePcHGsgCBjS2M\n\
uwC0NSetnZwbndQWR409pzWQL9oeQL1vo0w+lHGhX7Ll7onkWgzJg7rPMc7swmoC\n\
AKX8L9QxXylh0eeeaWhGmS8M\n\
-----END PRIVATE KEY-----\n";

/// The matching modulus, base64url.
const RSA_N: &str = "qX924rB7f6JjpFRR8_8W-KdeJpHjnq4OG2pQDdDc524nlW5pysD42mlH0vWbKcMAp_Wy2yAGjCGtxqCqrEsnyCE1jYpIi6fGZ82kB_CW4Dyxmkn78uMOnU6dHJvZC-LXuikZNie6MG1XacFup3xSsmMLTQCS0g4Oml-xwifUAKFk_a1gvJSrwqHU8GL639k-T5C43vFA2WB8dVgvk_W2pftLipfoT-4-TSDPOCodltCnzBDpDJRnUOq_GO2krrptqmJx-SvIEqFpQU5-9_dxfnW5ExpnxpaPZN-n5ZiIbtXhVlIK2IRuR3SsKHziOB0BB_XLlvkR6Za6bUug_QSZ0Q";

/// The `kid` the fixture publisher publishes.
const PUBLISHED_KID: &str = "fixture-key";

/// A key source holding one key, answering with no network at all.
///
/// The seam exists for exactly this: a scheme's own suite must be able to verify a
/// genuine token without a JWKS server, and the bound on *fetching* belongs to
/// `fraiseql-jwks`, which pins it there.
#[derive(Debug)]
pub(crate) struct LocalKeys {
    kid: String,
    jwk: Jwk,
}

impl LocalKeys {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::published())
    }

    /// The same key source by value, for a wrapper that decorates it.
    fn published() -> Self {
        Self {
            kid: PUBLISHED_KID.to_string(),
            jwk: serde_json::from_value(json!({
                "kty": "RSA", "kid": PUBLISHED_KID, "alg": "RS256", "use": "sig",
                "n": RSA_N, "e": "AQAB",
            }))
            .unwrap(),
        }
    }
}

impl JwksKeys for LocalKeys {
    fn key<'a>(&'a self, kid: &'a str) -> BoxFuture<'a, Result<Option<Jwk>, JwksError>> {
        let answer = if kid == self.kid {
            Some(self.jwk.clone())
        } else {
            None
        };
        Box::pin(std::future::ready(Ok(answer)))
    }
}

/// Sign `claims` as the fixture publisher would, under `kid`.
fn sign_as(kid: &str, claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());
    let key = EncodingKey::from_rsa_pem(RSA_PRIVATE_PEM.as_bytes()).unwrap();
    jsonwebtoken::encode(&header, claims, &key).unwrap()
}

/// Sign as the publisher under the key it actually publishes.
fn sign(claims: &Value) -> String {
    sign_as(PUBLISHED_KID, claims)
}

/// Unix seconds, for a token that has to be live.
fn now() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

/// Build the scheme a route configured, with the fixture key source attached.
fn scheme_for(provider: &str, config: &SchemeConfig) -> Arc<dyn SignatureVerifier> {
    build_scheme(provider, config, &SchemeContext::with_tolerance(300).with_jwks(LocalKeys::new()))
        .unwrap_or_else(|error| {
            panic!("#1322: `provider = {provider:?}` must name a scheme build_scheme can build, and it does not: {error}")
        })
}

/// Resolve the key for a delivery and verify it, as the pipeline does.
///
/// The two steps are separate on the trait because only the first can need the
/// network: `resolve_key` parses the credential and refuses an algorithm outside
/// the allow-list *before* looking a key up, so a token this route would refuse on
/// its header alone costs no outbound request.
async fn receive(
    scheme: &dyn SignatureVerifier,
    headers: &BTreeMap<String, String>,
    body: &[u8],
) -> Result<Verified, SignatureError> {
    let request = InboundRequest::new(headers, body, None);
    let key = scheme.resolve_key(&request, None).await?;
    scheme.verify(&request, &key)
}

fn no_headers() -> BTreeMap<String, String> {
    BTreeMap::new()
}

// ============================================================================
// Cycle 1: Hanko — the event comes out of the token, never out of the envelope
// ============================================================================

/// Hanko posts `{"token": "<jwt>", "event": "…"}`. The outer `event` is **not
/// signed**; `evt` and `data` inside the token are.
///
/// So the envelope and the token are made to **disagree on every field**. A
/// verifier that reported `Verified::Body` — or a route that read the outer JSON —
/// would pass a test where the two agreed, and this is what such a fixture hides:
/// whoever sends the HTTP request chooses the outer object, and #751 is what
/// happens when the delivery's identity comes from there.
#[tokio::test]
async fn hanko_reports_the_event_inside_the_token_and_not_the_envelope() {
    let issued = now();
    let token = sign(&json!({
        "sub":  "hanko webhooks",
        "aud":  ["my-app"],
        "iat":  issued,
        "exp":  issued + 300,
        "evt":  "user.create",
        "data": { "id": "user-in-the-token", "email": "signed@example.com" },
    }));
    // Every outer field contradicts the token.
    let body = json!({
        "token": token,
        "event": "user.delete",
        "data":  { "id": "user-in-the-envelope", "email": "forged@example.com" },
        "evt":   "user.delete",
    })
    .to_string();

    let scheme = scheme_for("hanko", &SchemeConfig::default());
    let verified = receive(&*scheme, &no_headers(), body.as_bytes())
        .await
        .expect("a genuine Hanko delivery must verify");

    let Verified::Event {
        id,
        event_type,
        payload,
    } = verified
    else {
        panic!(
            "Hanko's body is an untrusted envelope, so verification must report \
             `Verified::Event`; got {verified:?}"
        )
    };
    assert_eq!(event_type, "user.create", "the type is the signed `evt`, not the outer `event`");
    assert_eq!(
        payload,
        json!({ "id": "user-in-the-token", "email": "signed@example.com" }),
        "the payload is the signed `data`, not the outer one"
    );
    assert!(!id.is_empty(), "and a delivery the ledger can key on");
}

/// Hanko's token carries **no** `jti` and no event id — the issue's "dedup on
/// `jti`, or the provider's event id" has nothing to name here. The only signed,
/// delivery-stable material is the token itself, so the id is a digest of it.
#[tokio::test]
async fn hanko_keys_the_ledger_on_a_digest_of_the_verified_token() {
    let issued = now();
    let claims = json!({
        "sub": "hanko webhooks", "aud": ["my-app"], "iat": issued, "exp": issued + 300,
        "evt": "user.create", "data": { "id": "u1" },
    });
    let token = sign(&claims);
    let scheme = scheme_for("hanko", &SchemeConfig::default());

    let first = json!({ "token": token, "event": "user.create" }).to_string();
    // The same token inside a different envelope is the same delivery: the envelope
    // is not signed, so it cannot be part of the identity.
    let second = json!({ "token": token, "event": "something.else", "extra": 1 }).to_string();

    let a = receive(&*scheme, &no_headers(), first.as_bytes()).await.unwrap();
    let b = receive(&*scheme, &no_headers(), second.as_bytes()).await.unwrap();
    let (Verified::Event { id: id_a, .. }, Verified::Event { id: id_b, .. }) = (&a, &b) else {
        panic!("both are events")
    };
    assert_eq!(
        id_a, id_b,
        "the id must come from the token alone — an id that moved with the envelope would let \
         a captured delivery be replayed indefinitely under a fresh outer field (#751)"
    );

    // A different token is a different delivery, even for the same event.
    let other = sign(&json!({
        "sub": "hanko webhooks", "aud": ["my-app"], "iat": issued + 1, "exp": issued + 301,
        "evt": "user.create", "data": { "id": "u1" },
    }));
    let third = json!({ "token": other }).to_string();
    let c = receive(&*scheme, &no_headers(), third.as_bytes()).await.unwrap();
    let Verified::Event { id: id_c, .. } = &c else {
        panic!("an event")
    };
    assert_ne!(id_a, id_c, "two distinct tokens are two distinct deliveries");
}

/// The credential is in the body, so a body that is not JSON — or that has no
/// `token` field — is the sender's fault and must read as one.
#[tokio::test]
async fn hanko_refuses_a_delivery_that_carries_no_token() {
    let scheme = scheme_for("hanko", &SchemeConfig::default());

    let error = receive(&*scheme, &no_headers(), b"not json at all").await.expect_err("refused");
    assert!(
        matches!(error, SignatureError::InvalidFormat),
        "a body that will not parse is `InvalidFormat`, which maps to 401. Never \
         `KeyMaterial`, which maps to 5xx and would let any caller mint one (#1045): {error}"
    );

    let error = receive(&*scheme, &no_headers(), br#"{"event":"user.create"}"#)
        .await
        .expect_err("refused");
    assert!(
        matches!(error, SignatureError::MissingCredential(ref what) if what == "body:token"),
        "and an envelope with no token names what was looked for: {error}"
    );
}

/// A token signed by a key the publisher does not publish is refused, and refused
/// as the **sender's** problem.
#[tokio::test]
async fn hanko_refuses_a_token_signed_under_an_unpublished_kid() {
    let issued = now();
    let token = sign_as(
        "not-the-published-kid",
        &json!({
            "sub": "hanko webhooks", "aud": ["my-app"], "iat": issued, "exp": issued + 300,
            "evt": "user.create", "data": {},
        }),
    );
    let body = json!({ "token": token }).to_string();
    let scheme = scheme_for("hanko", &SchemeConfig::default());

    let error = receive(&*scheme, &no_headers(), body.as_bytes()).await.expect_err("refused");
    assert!(
        matches!(error, SignatureError::Mismatch),
        "the publisher not publishing that key means this signature cannot be attributed to \
         them — the sender's problem, a 401, not the operator's: {error}"
    );
}

/// `credential` is phase 31's existing key and it is what says where the token is.
/// A Hanko-shaped sender that puts its token somewhere else is a configuration,
/// not a code change — but the `hanko` **preset** fixes it and refuses the key,
/// because a preset that reads it is not a preset.
#[test]
fn the_generic_scheme_reads_the_token_location_and_the_preset_refuses_it() {
    let elsewhere = SchemeConfig {
        credential: Some(CredentialLocation::BodyField("jwt".to_string())),
        jwks_uri: Some("https://idp.example.com/.well-known/jwks.json".to_string()),
        ..SchemeConfig::default()
    };
    build_scheme(
        "jwt-jwks",
        &elsewhere,
        &SchemeContext::with_tolerance(300).with_jwks(LocalKeys::new()),
    )
    .expect("the generic scheme is the one whose signing details the operator owns");

    let error = build_scheme(
        "hanko",
        &elsewhere,
        &SchemeContext::with_tolerance(300).with_jwks(LocalKeys::new()),
    )
    .err()
    .expect("a preset fixes its own token location, so it must refuse the key");
    assert!(
        error.to_string().contains("credential"),
        "and says which key it does not read, rather than ignoring it: {error}"
    );
}

// ============================================================================
// Cycle 2: Kinde — the whole body is the token
// ============================================================================

/// Kinde posts the token as the body, `Content-Type: application/jwt`. Before
/// this, such a delivery answered 400 `webhook body is not valid JSON` — the
/// receiving path had to parse the body before it could find a credential.
///
/// The fixture carries **no `exp`**, as Kinde's documentation shows. That is the
/// trap: `jsonwebtoken`'s default `required_spec_claims` is `{exp}`, so a fixture
/// written *with* one passes while every genuine Kinde delivery is refused.
#[tokio::test]
async fn kinde_receives_a_token_that_is_the_entire_body_and_carries_no_exp() {
    let token = sign(&json!({
        "type":            "user.updated",
        "event_id":        "event_01HQ8",
        "source":          "admin",
        "timestamp":       "2026-09-16T10:00:00Z",
        "event_timestamp": "2026-09-16T10:00:00Z",
        "data":            { "user": { "id": "kp_abc" } },
    }));
    let scheme = scheme_for("kinde", &SchemeConfig::default());

    let verified = receive(&*scheme, &no_headers(), token.as_bytes())
        .await
        .expect("a genuine Kinde delivery carries no `exp`, and must still verify");

    let Verified::Event {
        id,
        event_type,
        payload,
    } = verified
    else {
        panic!("the body IS the token, so there is no body to be the event: {verified:?}")
    };
    assert_eq!(
        event_type, "user.updated",
        "Kinde's type claim is `type` — the issue proposed `evt` as a shared default, which is \
         Hanko's and would read nothing here"
    );
    assert_eq!(
        id, "event_01HQ8",
        "and its id claim is `event_id`. The `webhook-id` header is stable across retries but \
         UNSIGNED, so it cannot be what the ledger keys on (#751)"
    );
    assert_eq!(payload, json!({ "user": { "id": "kp_abc" } }));
}

/// A trailing newline is not a different token. The body arm of the credential
/// grammar trims, because a sender that ends its body with one has not re-signed.
#[tokio::test]
async fn kinde_accepts_a_token_body_with_trailing_whitespace() {
    let token = sign(&json!({
        "type": "user.updated", "event_id": "e1", "source": "admin", "data": {},
    }));
    let scheme = scheme_for("kinde", &SchemeConfig::default());
    let body = format!("{token}\n");
    receive(&*scheme, &no_headers(), body.as_bytes())
        .await
        .expect("trimmed, not rejected");
}

// ============================================================================
// Cycle 3: FusionAuth — the header token is bound to the body by a digest claim
// ============================================================================

/// FusionAuth's token is in a header and its `request_body_sha256` claim is
/// `base64(SHA-256(raw body))`. That claim is what makes the body verified
/// material — so this is a body-signing scheme and reports [`Verified::Body`].
///
/// The body carries **non-canonical whitespace**: a digest computed over
/// re-serialized JSON, or written as hex, does not match it. A fixture with
/// canonical JSON would pass against either mistake.
#[tokio::test]
async fn fusionauth_binds_the_header_token_to_the_raw_body() {
    let body = b"{  \"id\" : \"fa-1\" ,\n  \"type\":\"user.create\"  }";
    let token = sign(&json!({
        "request_body_sha256": body_digest(body),
        "iat":                 now(),
    }));
    let scheme = scheme_for("fusionauth", &SchemeConfig::default());

    let verified = receive(&*scheme, &fusionauth_header(&token), body)
        .await
        .expect("a genuine FusionAuth delivery must verify");
    assert_eq!(
        verified,
        Verified::Body,
        "the digest claim makes the body verified material, and `Verified::Body` says exactly \
         that: the body IS the event, and the receiver's own id and type rules apply to it. \
         Reporting `Verified::Event` would discard the body the provider actually signed"
    );
}

/// The same genuine token over a **different** body is refused. This is the whole
/// point of the digest claim: without the check, a captured header could be
/// replayed over any payload at all.
#[tokio::test]
async fn fusionauth_refuses_a_genuine_token_over_a_different_body() {
    let signed_body = br#"{"id":"fa-1","type":"user.create"}"#;
    let token = sign(&json!({
        "request_body_sha256": body_digest(signed_body),
        "iat":                 now(),
    }));
    let scheme = scheme_for("fusionauth", &SchemeConfig::default());

    let forged = br#"{"id":"fa-1","type":"user.delete"}"#;
    let error = receive(&*scheme, &fusionauth_header(&token), forged)
        .await
        .expect_err("the token is genuine but it does not cover these bytes");
    assert!(matches!(error, SignatureError::Mismatch), "got {error}");
}

/// A token with no digest claim covers nothing about the body, so it cannot be
/// accepted as covering it — even though the signature itself is genuine.
#[tokio::test]
async fn fusionauth_refuses_a_genuine_token_with_no_digest_claim() {
    let token = sign(&json!({ "iat": now(), "sub": "someone" }));
    let scheme = scheme_for("fusionauth", &SchemeConfig::default());
    let error = receive(&*scheme, &fusionauth_header(&token), b"{}").await.expect_err("refused");
    assert!(
        matches!(error, SignatureError::Mismatch),
        "a signature that covers no statement about the body must not be read as covering the \
         body: {error}"
    );
}

/// `base64(SHA-256(bytes))`, as FusionAuth computes it.
fn body_digest(body: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use sha2::{Digest as _, Sha256};
    BASE64.encode(Sha256::digest(body))
}

/// The header FusionAuth puts its token in.
fn fusionauth_header(token: &str) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    headers.insert("x-fusionauth-signature-jwt".to_string(), token.to_string());
    headers
}

// ============================================================================
// Cycle 4: token confusion — the requirement the issue does not state
// ============================================================================

/// A publisher's webhook JWKS is usually the **same** key set that signs its
/// end-user sessions. Hanko's is (#708's issuer-less mode exists to validate those
/// tokens), and Kinde's webhook JWKS is its access-token JWKS.
///
/// So a scheme that accepts "any token that verifies against this key set" accepts
/// **any logged-in end user posting their own session token as a webhook**, and
/// the signature is genuine. Each token below is signed by the route's real key
/// and has the audience the route expects; the only thing wrong with it is that it
/// is a *user* token.
#[tokio::test]
async fn a_genuinely_signed_user_token_is_not_a_webhook_delivery() {
    let issued = now();
    // A Hanko end-user session token: right key, right audience, `sub` is a user.
    let session = sign(&json!({
        "sub":   "1f8c4e2a-user-id",
        "aud":   ["my-app"],
        "iat":   issued,
        "exp":   issued + 3600,
        "email": "victim@example.com",
    }));
    let scheme = scheme_for("hanko", &with_audience("my-app"));
    let body = json!({ "token": session, "event": "user.delete" }).to_string();

    let error = receive(&*scheme, &no_headers(), body.as_bytes())
        .await
        .expect_err("a user's own session token must not be accepted as a webhook");
    assert!(
        matches!(error, SignatureError::Mismatch),
        "and refused as the sender's fault, which it is — this is an end user POSTing: {error}"
    );

    // The same shape for Kinde: a token that verifies and has no `event_id`,
    // `type` or `source` is not one of its webhook deliveries.
    let access = sign(&json!({
        "sub": "kp_user", "aud": ["my-app"], "iat": issued, "exp": issued + 3600,
        "scope": "openid profile",
    }));
    let kinde = scheme_for("kinde", &with_audience("my-app"));
    let error = receive(&*kinde, &no_headers(), access.as_bytes()).await.expect_err("refused");
    assert!(matches!(error, SignatureError::Mismatch), "got {error}");
}

/// The claim guard and `audience` are **defence in depth**: each blocks confusion
/// on its own, so each has to be asserted on its own.
///
/// Here the token has every claim a Hanko webhook token has — so the claim guard
/// is satisfied — and the wrong `aud`. Only the audience check can refuse it.
#[tokio::test]
async fn a_webhook_shaped_token_for_a_different_audience_is_refused() {
    let issued = now();
    let token = sign(&json!({
        "sub":  "hanko webhooks",
        "aud":  ["some-other-service"],
        "iat":  issued,
        "exp":  issued + 300,
        "evt":  "user.create",
        "data": { "id": "u1" },
    }));
    let scheme = scheme_for("hanko", &with_audience("my-app"));
    let body = json!({ "token": token }).to_string();

    let error = receive(&*scheme, &no_headers(), body.as_bytes()).await.expect_err(
        "a token minted for another service, by the same publisher, is the other half of the \
         confusion class — and the claim guard cannot see it",
    );
    assert!(matches!(error, SignatureError::Mismatch), "got {error}");

    // And the same token for the right audience is accepted, so the assertion
    // above is about the audience and not about something else in the fixture.
    let right = sign(&json!({
        "sub": "hanko webhooks", "aud": ["my-app"], "iat": issued, "exp": issued + 300,
        "evt": "user.create", "data": { "id": "u1" },
    }));
    let body = json!({ "token": right }).to_string();
    receive(&*scheme, &no_headers(), body.as_bytes())
        .await
        .expect("the audience matches");
}

/// Hanko's guard is a claim *value*, not merely a claim's presence: an end-user
/// token has a `sub` too. A guard that only checked presence would accept one.
#[tokio::test]
async fn hankos_guard_is_the_value_of_sub_and_not_its_presence() {
    let issued = now();
    let token = sign(&json!({
        "sub":  "an-end-user",
        "aud":  ["my-app"],
        "iat":  issued,
        "exp":  issued + 300,
        // Everything else a webhook token has, so `sub` is the only discriminator.
        "evt":  "user.create",
        "data": { "id": "u1" },
    }));
    let scheme = scheme_for("hanko", &with_audience("my-app"));
    let body = json!({ "token": token }).to_string();

    let error = receive(&*scheme, &no_headers(), body.as_bytes()).await.expect_err(
        "every end-user token carries a `sub`, so requiring only its presence is no guard at \
         all — it must equal the value Hanko signs its webhook tokens with",
    );
    assert!(matches!(error, SignatureError::Mismatch), "got {error}");
}

/// A route with no `audience` configured still has the claim guard, and that is
/// what makes leaving it unset safe rather than merely permissive.
///
/// `audience` is not required because a publisher's webhook token may carry no
/// `aud` at all — a route that demanded one would refuse every genuine delivery
/// from such a publisher.
#[tokio::test]
async fn with_no_audience_configured_the_claim_guard_still_refuses_a_user_token() {
    let issued = now();
    let session = sign(&json!({
        "sub": "an-end-user", "aud": ["my-app"], "iat": issued, "exp": issued + 3600,
    }));
    let scheme = scheme_for("hanko", &SchemeConfig::default());
    let body = json!({ "token": session }).to_string();

    let error = receive(&*scheme, &no_headers(), body.as_bytes()).await.expect_err(
        "an unconfigured audience must not leave the route accepting any token its \
         publisher signed",
    );
    assert!(matches!(error, SignatureError::Mismatch), "got {error}");
}

fn with_audience(audience: &str) -> SchemeConfig {
    SchemeConfig {
        audience: Some(audience.to_string()),
        ..SchemeConfig::none()
    }
}

// ============================================================================
// Cycle 5: keys and algorithms — and what must cost no outbound request
// ============================================================================

/// `alg: none` is the absence of a signature, and `HS*` against a *public* key set
/// is the algorithm-confusion attack — anyone who can read the key set can forge a
/// token. Both are refused at **boot**, by name, whatever `algorithms` says.
#[test]
fn none_and_the_hmac_family_are_refused_at_boot_by_name() {
    // The expectation is a phrase from the REASON, not the configured value.
    // `UnusableAlgorithm` prints the value it was given, so asserting only that
    // the name appears cannot tell these arms from the generic
    // "not a JWT algorithm name" fallback — and a mutation deleting the `none`
    // arm then survives, because `none` is not a `jsonwebtoken::Algorithm` either
    // and the fallback refuses it with a different explanation.
    for (configured, expected_in_message) in [
        ("none", "absence of a signature"),
        ("HS256", "algorithm-confusion"),
        ("hs512", "algorithm-confusion"),
    ] {
        let config = SchemeConfig {
            jwks_uri: Some("https://idp.example.com/jwks".to_string()),
            algorithms: Some(vec![configured.to_string()]),
            ..SchemeConfig::none()
        };
        let error = build_scheme(
            "jwt-jwks",
            &config,
            &SchemeContext::with_tolerance(300).with_jwks(LocalKeys::new()),
        )
        .err()
        .unwrap_or_else(|| panic!("algorithms = [{configured:?}] must be refused at boot"));
        let message = error.to_string();
        assert!(
            message.contains(expected_in_message),
            "the refusal must explain which problem this is, so an operator can act on it \
             rather than re-reading the docs — and so that deleting the arm is visible: \
             {message}"
        );
    }
}

/// A token whose `alg` is outside the allow-list is refused **before** the key
/// lookup, so it costs no outbound request to the publisher.
///
/// This is #1335's rule seen from its most exposed caller: a webhook route is
/// unauthenticated by construction. The key source here counts its lookups, which
/// is what makes the claim measurable rather than asserted.
#[tokio::test]
async fn a_disallowed_algorithm_costs_no_key_lookup() {
    let keys = CountingKeys::new();
    let config = SchemeConfig {
        jwks_uri: Some("https://idp.example.com/jwks".to_string()),
        algorithms: Some(vec!["RS512".to_string()]),
        credential: Some(CredentialLocation::Body),
        ..SchemeConfig::none()
    };
    let scheme = build_scheme(
        "jwt-jwks",
        &config,
        &SchemeContext::with_tolerance(300).with_jwks(keys.clone()),
    )
    .expect("RS512 is a usable allow-list");

    // Signed with RS256, which this route does not accept.
    let token = sign(&json!({ "event_type": "x", "data": {} }));
    let error = receive(&*scheme, &no_headers(), token.as_bytes()).await.expect_err("refused");
    assert!(matches!(error, SignatureError::InvalidFormat), "got {error}");
    assert_eq!(
        keys.lookups(),
        0,
        "a token this route refuses on its header alone must not reach the publisher. The \
         allow-list is checked before the key lookup, and that ordering is the fix (#1335), \
         not an optimisation"
    );

    // The counterweight: an accepted algorithm does reach it, so the assertion
    // above is about the ordering and not about the lookup never happening.
    let allowed =
        sign_with(Algorithm::RS512, PUBLISHED_KID, &json!({ "event_type": "x", "data": {} }));
    let _ = receive(&*scheme, &no_headers(), allowed.as_bytes()).await;
    assert_eq!(keys.lookups(), 1, "an accepted algorithm is looked up exactly once");
}

/// A token with no `kid` is refused without a lookup.
///
/// Trying every key in the set instead would make the publisher's key rotation a
/// signature oracle: an attacker could learn which of the published keys a forged
/// token happened to verify under.
#[tokio::test]
async fn a_token_with_no_kid_is_refused_without_a_lookup() {
    let keys = CountingKeys::new();
    let config = SchemeConfig {
        jwks_uri: Some("https://idp.example.com/jwks".to_string()),
        credential: Some(CredentialLocation::Body),
        ..SchemeConfig::none()
    };
    let scheme = build_scheme(
        "jwt-jwks",
        &config,
        &SchemeContext::with_tolerance(300).with_jwks(keys.clone()),
    )
    .unwrap();

    // No `kid` in the header.
    let header = Header::new(Algorithm::RS256);
    let key = EncodingKey::from_rsa_pem(RSA_PRIVATE_PEM.as_bytes()).unwrap();
    let token =
        jsonwebtoken::encode(&header, &json!({ "event_type": "x", "data": {} }), &key).unwrap();

    let error = receive(&*scheme, &no_headers(), token.as_bytes()).await.expect_err("refused");
    assert!(matches!(error, SignatureError::InvalidFormat), "got {error}");
    assert_eq!(keys.lookups(), 0, "there is nothing to look up");
}

/// An expired token is refused, and refused as expiry rather than as a generic
/// mismatch — the two have different operator meanings.
#[tokio::test]
async fn an_expired_token_is_refused_as_expired() {
    let issued = now() - 7200;
    let token = sign(&json!({
        "sub": "hanko webhooks", "aud": ["my-app"], "iat": issued, "exp": issued + 300,
        "evt": "user.create", "data": {},
    }));
    let scheme = scheme_for("hanko", &with_audience("my-app"));
    let body = json!({ "token": token }).to_string();

    let error = receive(&*scheme, &no_headers(), body.as_bytes()).await.expect_err("refused");
    assert!(
        matches!(error, SignatureError::TimestampExpired),
        "an expired token is `TimestampExpired`, not `Mismatch`: an operator reading the log \
         needs to know the clock is the problem and not the key: {error}"
    );
}

/// An unreachable key set is the **operator's or the publisher's** problem, and
/// must not be reported to the sender as a 401.
///
/// Providers treat sustained authentication failures as a reason to disable an
/// endpoint, so a 401 here loses the whole misconfiguration window (#1045). This
/// is why `JwksKeys` keeps `Ok(None)` and `Err` as separate answers.
#[tokio::test]
async fn an_unreachable_key_set_is_the_operators_error_and_not_the_senders() {
    #[derive(Debug)]
    struct Unreachable;
    impl JwksKeys for Unreachable {
        fn key<'a>(&'a self, _kid: &'a str) -> BoxFuture<'a, Result<Option<Jwk>, JwksError>> {
            Box::pin(std::future::ready(Err(JwksError::Unreachable {
                uri:    "https://idp.example.com/jwks".to_string(),
                reason: "connection refused".to_string(),
            })))
        }
    }
    let config = SchemeConfig {
        jwks_uri: Some("https://idp.example.com/jwks".to_string()),
        ..SchemeConfig::none()
    };
    let scheme = build_scheme(
        "hanko",
        &config,
        &SchemeContext::with_tolerance(300).with_jwks(Arc::new(Unreachable)),
    )
    .unwrap();

    let token = sign(&json!({
        "sub": "hanko webhooks", "iat": now(), "evt": "user.create", "data": {},
    }));
    let body = json!({ "token": token }).to_string();
    let error = receive(&*scheme, &no_headers(), body.as_bytes()).await.expect_err("refused");
    assert!(
        matches!(error, SignatureError::KeyMaterial(_)),
        "`KeyMaterial` is the one variant that maps to a 5xx. Reporting our own broken key \
         fetch to the sender as a 401 is how a provider comes to disable an endpoint: {error}"
    );
}

/// Sign with a chosen algorithm, for the allow-list cases.
fn sign_with(algorithm: Algorithm, kid: &str, claims: &Value) -> String {
    let mut header = Header::new(algorithm);
    header.kid = Some(kid.to_string());
    let key = EncodingKey::from_rsa_pem(RSA_PRIVATE_PEM.as_bytes()).unwrap();
    jsonwebtoken::encode(&header, claims, &key).unwrap()
}

/// The fixture key source, counting how many times it was asked.
///
/// A count is what makes "no outbound request" a measurement rather than a claim.
#[derive(Debug)]
struct CountingKeys {
    inner:   LocalKeys,
    lookups: std::sync::atomic::AtomicUsize,
}

impl CountingKeys {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner:   LocalKeys::published(),
            lookups: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    fn lookups(&self) -> usize {
        self.lookups.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl JwksKeys for CountingKeys {
    fn key<'a>(&'a self, kid: &'a str) -> BoxFuture<'a, Result<Option<Jwk>, JwksError>> {
        self.lookups.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.key(kid)
    }
}

// ============================================================================
// The corpus the crate-wide coverage gate reads
// ============================================================================

/// One genuine delivery from a token-verifying provider, with its tampered twin.
///
/// The twin is stated **explicitly** rather than derived, because what a tamper
/// has to corrupt differs by scheme: for Hanko and Kinde the signature covers the
/// token, and for FusionAuth it covers the body through a digest claim. A
/// generic "flip the last byte of the body" would corrupt Hanko's envelope —
/// producing an error for the wrong reason, and passing.
pub(crate) struct TokenFixture {
    pub provider: &'static str,
    pub headers:  BTreeMap<String, String>,
    pub body:     Vec<u8>,
    /// What verifying the genuine delivery must establish.
    pub verified: Verified,
    /// The same delivery with the signed material corrupted.
    pub tampered: (BTreeMap<String, String>, Vec<u8>),
}

/// A genuine + tampered pair for every scheme in the `jwt-jwks` family.
///
/// Read by `signature::tests::every_registered_provider_has_genuine_and_tampered_fixtures`,
/// so a name cannot reach [`crate::scheme::KNOWN_SCHEMES`] without a delivery here.
pub(crate) fn token_fixtures() -> Vec<TokenFixture> {
    let issued = now();
    let mut all = Vec::new();

    // ── hanko: token in a body field, event in the claims ──────────────────
    let hanko_token = sign(&json!({
        "sub": "hanko webhooks", "aud": ["my-app"], "iat": issued, "exp": issued + 300,
        "evt": "user.create", "data": { "id": "u1" },
    }));
    all.push(TokenFixture {
        provider: "hanko",
        headers:  no_headers(),
        body:     json!({ "token": &hanko_token, "event": "user.create" })
            .to_string()
            .into_bytes(),
        verified: Verified::Event {
            id:         hex::encode(<sha2::Sha256 as sha2::Digest>::digest(hanko_token.as_bytes())),
            event_type: "user.create".to_string(),
            payload:    json!({ "id": "u1" }),
        },
        // The token's own bytes, so the signature is what fails — not the JSON.
        tampered: (
            no_headers(),
            json!({ "token": flip_last(&hanko_token), "event": "user.create" })
                .to_string()
                .into_bytes(),
        ),
    });

    // ── kinde: the body IS the token ───────────────────────────────────────
    let kinde_token = sign(&json!({
        "type": "user.updated", "event_id": "e_1", "source": "admin",
        "data": { "user": { "id": "kp_1" } },
    }));
    all.push(TokenFixture {
        provider: "kinde",
        headers:  no_headers(),
        body:     kinde_token.clone().into_bytes(),
        verified: Verified::Event {
            id:         "e_1".to_string(),
            event_type: "user.updated".to_string(),
            payload:    json!({ "user": { "id": "kp_1" } }),
        },
        tampered: (no_headers(), flip_last(&kinde_token).into_bytes()),
    });

    // ── fusionauth: the digest claim binds the body, so the BODY is tampered ─
    let fa_body = br#"{"id":"fa-1","type":"user.create"}"#.to_vec();
    let fa_token = sign(&json!({
        "request_body_sha256": body_digest(&fa_body),
        "iat":                 issued,
    }));
    let mut forged_body = fa_body.clone();
    let last = forged_body.len() - 2;
    forged_body[last] ^= 1;
    all.push(TokenFixture {
        provider: "fusionauth",
        headers:  fusionauth_header(&fa_token),
        body:     fa_body,
        verified: Verified::Body,
        tampered: (fusionauth_header(&fa_token), forged_body),
    });

    // ── jwt-jwks: the generic scheme, whose details the operator describes ──
    let generic_token = sign(&json!({
        "iat": issued, "kind": "order.placed", "event": { "total": 1900 }, "ref": "ord_7",
    }));
    all.push(TokenFixture {
        provider: "jwt-jwks",
        headers:  no_headers(),
        body:     generic_token.clone().into_bytes(),
        verified: Verified::Event {
            id:         "ord_7".to_string(),
            event_type: "order.placed".to_string(),
            payload:    json!({ "total": 1900 }),
        },
        tampered: (no_headers(), flip_last(&generic_token).into_bytes()),
    });

    all
}

/// The configuration each fixture's route carries.
///
/// A preset reads only `jwks_uri` and `audience`; the generic scheme is where the
/// claim names come from, and this is the one place they are all spelled out.
pub(crate) fn token_fixture_config(provider: &str) -> SchemeConfig {
    let base = SchemeConfig {
        jwks_uri: Some("https://idp.example.com/.well-known/jwks.json".to_string()),
        ..SchemeConfig::none()
    };
    if provider == "jwt-jwks" {
        return SchemeConfig {
            credential: Some(CredentialLocation::Body),
            event_type_claim: Some("kind".to_string()),
            payload_claim: Some("event".to_string()),
            id_claim: Some("ref".to_string()),
            ..base
        };
    }
    base
}

/// Corrupt the last character of a token's signature segment.
///
/// Flipping a byte of the *signature* rather than of the payload keeps the token
/// structurally a JWT, so what fails is the signature check and not the parse.
fn flip_last(token: &str) -> String {
    let mut bytes = token.as_bytes().to_vec();
    let last = bytes.len() - 1;
    // Base64url alphabet: step to a different character that is still in it.
    bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
    String::from_utf8(bytes).expect("still ASCII")
}

#[tokio::test]
async fn every_genuine_token_delivery_verifies() {
    for fixture in token_fixtures() {
        let scheme = scheme_for(fixture.provider, &token_fixture_config(fixture.provider));
        let result = receive(&*scheme, &fixture.headers, &fixture.body).await;
        assert_eq!(
            result.as_ref().ok(),
            Some(&fixture.verified),
            "{}: a genuine delivery must verify AND report what it authenticated. A scheme \
             whose token carries the event must hand that event back — answering \
             `Verified::Body` instead would dispatch the envelope the sender chose. Got \
             {result:?}",
            fixture.provider
        );
    }
}

#[tokio::test]
async fn every_tampered_token_delivery_is_rejected() {
    for fixture in token_fixtures() {
        let scheme = scheme_for(fixture.provider, &token_fixture_config(fixture.provider));
        let (headers, body) = fixture.tampered;
        let result = receive(&*scheme, &headers, &body).await;
        assert!(
            result.is_err(),
            "{}: a tampered delivery must not verify — and must say so as an error, not as \
             some other `Ok` variant a caller could mistake for success; got {result:?}",
            fixture.provider
        );
    }
}

// ============================================================================
// Format versus policy: what a preset fixes, and what the deployment still owns
// ============================================================================

/// A P-256 private key, PKCS#8 PEM. Generated offline for tests only.
const EC_PRIVATE_PEM: &str = "\
-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgdyZZd/JOBTOvigyJ\n\
6zeaSBEznZzh3M0iNvrTVLfgNQKhRANCAARwcrDR+OEvHS3JFXUzavVGJsKJCp7X\n\
jZ8jKJz+hjjgP/Qgkhisqp8hSR9JYlAWSVV8Z/sbUw6pAbiZEb5cXpya\n\
-----END PRIVATE KEY-----\n";

/// The matching public point, base64url — the `x` and `y` of its JWK.
const EC_X: &str = "cHKw0fjhLx0tyRV1M2r1RibCiQqe142fIyic_oY44D8";
const EC_Y: &str = "9CCSGKyqnyFJH0liUBZJVXxn-xtTDqkBuJkRvlxenJo";

/// The `kid` the EC key is published under.
const EC_KID: &str = "fixture-ec-key";

/// A key source publishing the EC key alongside the RSA one.
#[derive(Debug)]
struct RsaAndEcKeys {
    rsa: Jwk,
    ec:  Jwk,
}

impl RsaAndEcKeys {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            rsa: serde_json::from_value(json!({
                "kty": "RSA", "kid": PUBLISHED_KID, "alg": "RS256", "use": "sig",
                "n": RSA_N, "e": "AQAB",
            }))
            .unwrap(),
            ec:  serde_json::from_value(json!({
                "kty": "EC", "kid": EC_KID, "crv": "P-256", "alg": "ES256", "use": "sig",
                "x": EC_X, "y": EC_Y,
            }))
            .unwrap(),
        })
    }
}

impl JwksKeys for RsaAndEcKeys {
    fn key<'a>(&'a self, kid: &'a str) -> BoxFuture<'a, Result<Option<Jwk>, JwksError>> {
        let answer = match kid {
            PUBLISHED_KID => Some(self.rsa.clone()),
            EC_KID => Some(self.ec.clone()),
            _ => None,
        };
        Box::pin(std::future::ready(Ok(answer)))
    }
}

/// FusionAuth's signing key may be RSA **or EC** — an operator picks when they
/// configure it, and neither is more standard.
///
/// So an `RS256`-only default would refuse every genuine delivery from a perfectly
/// ordinary FusionAuth deployment, with a 401 and nothing to say why. Its preset
/// accepts both out of the box.
#[tokio::test]
async fn fusionauth_accepts_an_ec_signed_delivery_out_of_the_box() {
    let body = br#"{"id":"fa-ec-1","type":"user.create"}"#;
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(EC_KID.to_string());
    let key = EncodingKey::from_ec_pem(EC_PRIVATE_PEM.as_bytes()).unwrap();
    let token = jsonwebtoken::encode(
        &header,
        &json!({ "request_body_sha256": body_digest(body), "iat": now() }),
        &key,
    )
    .unwrap();

    let scheme = build_scheme(
        "fusionauth",
        &SchemeConfig::none(),
        &SchemeContext::with_tolerance(300).with_jwks(RsaAndEcKeys::new()),
    )
    .unwrap();

    let verified = receive(&*scheme, &fusionauth_header(&token), body)
        .await
        .expect("an ES256 FusionAuth delivery is genuine and must verify with no extra config");
    assert_eq!(verified, Verified::Body);
}

/// An operator may still narrow the list to the algorithm their own deployment
/// uses — `algorithms` is policy, not format, which is why a preset reads it.
#[tokio::test]
async fn a_preset_route_may_narrow_the_algorithms_it_accepts() {
    let body = br#"{"id":"fa-ec-2","type":"user.create"}"#;
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(EC_KID.to_string());
    let key = EncodingKey::from_ec_pem(EC_PRIVATE_PEM.as_bytes()).unwrap();
    let token = jsonwebtoken::encode(
        &header,
        &json!({ "request_body_sha256": body_digest(body), "iat": now() }),
        &key,
    )
    .unwrap();

    let rsa_only = SchemeConfig {
        algorithms: Some(vec!["RS256".to_string()]),
        ..SchemeConfig::none()
    };
    let scheme = build_scheme(
        "fusionauth",
        &rsa_only,
        &SchemeContext::with_tolerance(300).with_jwks(RsaAndEcKeys::new()),
    )
    .expect("`algorithms` is one of the keys a preset reads");

    let error = receive(&*scheme, &fusionauth_header(&token), body).await.expect_err(
        "a deployment that has narrowed its allow-list to RS256 must refuse an ES256 token, \
         even one this provider could legitimately have sent",
    );
    assert!(matches!(error, SignatureError::InvalidFormat), "got {error}");
}

/// `max_age_secs` is this receiver's risk appetite, not the provider's format, so
/// a preset reads it too — and it is honoured, not merely accepted.
#[tokio::test]
async fn a_preset_route_honours_a_configured_max_age() {
    let issued = now() - 3600;
    // A token still well inside its own `exp`, so only `max_age_secs` can refuse it.
    let token = sign(&json!({
        "sub": "hanko webhooks", "aud": ["my-app"], "iat": issued, "exp": issued + 86_400,
        "evt": "user.create", "data": { "id": "u1" },
    }));
    let body = json!({ "token": token }).to_string();

    let tight = SchemeConfig {
        audience: Some("my-app".to_string()),
        max_age_secs: Some(60),
        ..SchemeConfig::none()
    };
    let scheme = scheme_for("hanko", &tight);
    let error = receive(&*scheme, &no_headers(), body.as_bytes()).await.expect_err(
        "a configured window must be HONOURED and not merely parsed: an hour-old token is \
         outside a 60-second one",
    );
    assert!(matches!(error, SignatureError::TimestampExpired), "got {error}");

    // The same token with no window configured is accepted, so the refusal above
    // is about `max_age_secs` and nothing else in the fixture.
    let scheme = scheme_for("hanko", &with_audience("my-app"));
    receive(&*scheme, &no_headers(), body.as_bytes())
        .await
        .expect("the token's own `exp` is the only freshness rule when no window is set");
}

/// The generic scheme has to be **told** where the token is.
///
/// Inheriting the HMAC families' `header:X-Signature` would be sharing their
/// default by a coincidence of code rather than by any decision about tokens — no
/// provider puts a JWT there, so it would only ever produce a 401 per delivery
/// naming a header the sender never set.
#[test]
fn the_generic_scheme_must_be_told_where_the_token_is() {
    let no_location = SchemeConfig {
        jwks_uri: Some("https://idp.example.com/jwks".to_string()),
        ..SchemeConfig::none()
    };
    let error = build_scheme(
        "jwt-jwks",
        &no_location,
        &SchemeContext::with_tolerance(300).with_jwks(LocalKeys::new()),
    )
    .err()
    .expect("a token location cannot be guessed");
    assert!(
        error.to_string().contains("credential"),
        "and the refusal must name the key to set: {error}"
    );

    // Its presets each fix their own, so the refusal is about the generic scheme.
    for provider in ["hanko", "kinde", "fusionauth"] {
        build_scheme(
            provider,
            &no_location,
            &SchemeContext::with_tolerance(300).with_jwks(LocalKeys::new()),
        )
        .unwrap_or_else(|error| panic!("{provider} fixes its own token location: {error}"));
    }
}
