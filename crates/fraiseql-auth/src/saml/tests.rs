//! Unit + attack-fixture tests for the SAML SP login + ACS slice (#381).
//!
//! Valid signed responses are minted with `samael`'s IdP side (an ephemeral RSA keypair +
//! self-signed cert), then mutated to construct each attack. The XSW / comment-truncation /
//! unsigned tests run against the **full** `verify_saml_response` extraction path (the seam),
//! not against the crypto backend in isolation.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use base64::Engine as _;
use chrono::{Duration, Utc};
use samael::{
    crypto::{CertificateDer, Crypto, CryptoProvider as _},
    idp::{
        CertificateParams, IdentityProvider, KeyType, Rsa,
        response_builder::{ResponseAttribute, build_response_template},
        sp_extractor::RequiredAttribute,
    },
    traits::ToXml as _,
};

use super::{
    SamlError, SamlIdpConfig, SamlReplayCache, effective_saml_email_verified,
    handler::{AcsForm, LoginQuery, SamlAuthState, saml_acs, saml_login, saml_routes},
    saml_provider_key,
    verify::{reject_doctype, verify_saml_response},
};
use crate::{
    account_linking::{AccountStore, InMemoryAccountStore},
    session::InMemorySessionStore,
    state_store::{InMemoryStateStore, StateStore},
};

const IDP_ENTITY: &str = "https://idp.example.com";
const IDP_SSO: &str = "https://idp.example.com/sso";
const SP_ENTITY: &str = "https://sp.example.com/metadata";
const SP_ACS: &str = "https://sp.example.com/acs";
const REQ_ID: &str = "id-request-1";

/// An ephemeral test IdP: a fresh keypair plus its self-signed signing certificate.
struct TestIdp {
    idp:  IdentityProvider,
    cert: CertificateDer,
}

fn new_idp() -> TestIdp {
    let idp = IdentityProvider::generate_new(KeyType::Rsa(Rsa::Rsa2048)).unwrap();
    let cert = idp
        .create_certificate(&CertificateParams {
            common_name:           IDP_ENTITY,
            issuer_name:           IDP_ENTITY,
            days_until_expiration: 3650,
        })
        .unwrap();
    TestIdp { idp, cert }
}

/// Build a [`SamlIdpConfig`] trusting `cert` as the IdP signing certificate.
///
/// samael's IdP-side signer emits an RSA-SHA256 signature with a **SHA-1 digest**, which the
/// production default algorithm allow-list (SHA-256+) correctly rejects. So these
/// verification-logic fixtures relax the allow-list to `None`; the allow-list itself is
/// exercised separately by [`strict_algorithm_allowlist_rejects_weak_digest`]. Relaxing it
/// here is what makes the audience/recipient/XSW tests meaningful — otherwise every fixture
/// would be rejected at the algorithm gate before that logic ever ran.
fn config_with_cert(cert: &CertificateDer) -> SamlIdpConfig {
    let mut config = SamlIdpConfig::builder("test-idp", SP_ENTITY, SP_ACS)
        .idp_parts(IDP_ENTITY, IDP_SSO, cert.der_data())
        .unwrap()
        .build()
        .unwrap();
    config.sp.allowed_signature_algorithms = None;
    config
}

fn email_attr(value: &str) -> Vec<ResponseAttribute<'_>> {
    vec![ResponseAttribute {
        required_attribute: RequiredAttribute {
            name:   "email".to_string(),
            format: Some("urn:oasis:names:tc:SAML:2.0:attrname-format:uri".to_string()),
        },
        value,
    }]
}

/// Mint a base64-encoded, signed `SAMLResponse`.
///
/// We build the template and sign it ourselves rather than calling
/// `IdentityProvider::sign_authn_response`, because samael's template omits
/// `SubjectConfirmationData/NotOnOrAfter` — which its own validator *requires* — so the
/// high-level helper produces responses that never pass `parse_xml_response`. We set a valid
/// future confirmation window, then sign the whole response envelope.
fn signed_response(
    test_idp: &TestIdp,
    name_id: &str,
    audience: &str,
    acs_url: &str,
    request_id: &str,
    attributes: &[ResponseAttribute],
) -> String {
    let mut response = build_response_template(
        &test_idp.cert,
        name_id,
        audience,
        IDP_ENTITY,
        acs_url,
        request_id,
        attributes,
    );
    if let Some(data) = response
        .assertion
        .as_mut()
        .and_then(|a| a.subject.as_mut())
        .and_then(|s| s.subject_confirmations.as_mut())
        .and_then(|c| c.first_mut())
        .and_then(|c| c.subject_confirmation_data.as_mut())
    {
        data.not_on_or_after = Some(Utc::now() + Duration::minutes(5));
    }

    let xml = response.to_string().unwrap();
    let signed = Crypto::sign_xml(xml, &test_idp.idp.export_private_key_der().unwrap()).unwrap();
    base64::engine::general_purpose::STANDARD.encode(signed)
}

/// Decode a base64 response back to its XML (for attack mutations).
fn decode(b64: &str) -> String {
    String::from_utf8(base64::engine::general_purpose::STANDARD.decode(b64).unwrap()).unwrap()
}

fn encode(xml: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(xml)
}

// ─── Happy path ──────────────────────────────────────────────────────────────

#[test]
fn valid_response_verifies_and_extracts_identity() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    let attrs = email_attr("user@example.com");
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &attrs);

    // Document the fixture structure the attack tests mutate.
    let xml = decode(&b64);
    assert!(xml.contains("<saml2:Assertion"), "expected saml2-prefixed Assertion: {xml}");
    assert!(xml.contains("Signature"), "expected an XML signature: {xml}");

    let verified =
        verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now()).expect("should verify");
    assert_eq!(verified.name_id, "nameid-123");
    assert_eq!(verified.email.as_deref(), Some("user@example.com"));
}

// ─── Signature / trust ───────────────────────────────────────────────────────

#[test]
fn wrong_idp_certificate_is_rejected() {
    let signer = new_idp();
    // Config trusts a DIFFERENT IdP's certificate than the one that signed.
    let other = new_idp();
    let config = config_with_cert(&other.cert);
    let replay = SamlReplayCache::new();
    let b64 = signed_response(&signer, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &[]);

    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now());
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[test]
fn tampered_attribute_breaks_signature() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    let attrs = email_attr("user@example.com");
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &attrs);

    // Flip the signed email value — the digest no longer matches.
    let tampered = decode(&b64).replace("user@example.com", "attacker@evil.test");
    let result = verify_saml_response(&config, &encode(&tampered), &[REQ_ID], &replay, Utc::now());
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[test]
fn unsigned_response_is_rejected() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();

    // build_response_template returns an UNSIGNED Response.
    let unsigned = build_response_template(
        &test_idp.cert,
        "nameid-123",
        SP_ENTITY,
        IDP_ENTITY,
        SP_ACS,
        REQ_ID,
        &[],
    );
    let b64 = encode(&unsigned.to_string().unwrap());

    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now());
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[test]
fn strict_algorithm_allowlist_rejects_weak_digest() {
    let test_idp = new_idp();
    let replay = SamlReplayCache::new();
    // samael signs RSA-SHA256 but with a SHA-1 reference digest.
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &[]);

    // Production default (SHA-256+ allow-list) must reject the SHA-1 digest.
    let strict = SamlIdpConfig::builder("test-idp", SP_ENTITY, SP_ACS)
        .idp_parts(IDP_ENTITY, IDP_SSO, test_idp.cert.der_data())
        .unwrap()
        .build()
        .unwrap();
    let strict_result = verify_saml_response(&strict, &b64, &[REQ_ID], &replay, Utc::now());
    assert!(
        matches!(strict_result, Err(SamlError::Verification(_))),
        "strict allow-list must reject the weak SHA-1 digest: {strict_result:?}"
    );

    // The very same fixture verifies once the allow-list is relaxed — proving it was the
    // algorithm gate, not some other defect, that rejected it.
    let lenient = config_with_cert(&test_idp.cert);
    let replay2 = SamlReplayCache::new();
    assert!(
        verify_saml_response(&lenient, &b64, &[REQ_ID], &replay2, Utc::now()).is_ok(),
        "relaxed allow-list should accept the same fixture"
    );
}

// ─── Audience / recipient / request binding ──────────────────────────────────

#[test]
fn wrong_audience_is_rejected() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    let b64 = signed_response(&test_idp, "nameid-123", "https://evil.example", SP_ACS, REQ_ID, &[]);

    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now());
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[test]
fn wrong_recipient_is_rejected() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    // Recipient/Destination set to an attacker ACS, but the SP's ACS is SP_ACS.
    let b64 = signed_response(
        &test_idp,
        "nameid-123",
        SP_ENTITY,
        "https://evil.example/acs",
        REQ_ID,
        &[],
    );

    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now());
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[test]
fn unsolicited_in_response_to_is_rejected() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    // Assertion is bound to a request ID we never issued.
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, "id-attacker", &[]);

    let result = verify_saml_response(&config, &b64, &["id-legit"], &replay, Utc::now());
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[test]
fn expired_response_is_rejected() {
    let test_idp = new_idp();
    let mut config = config_with_cert(&test_idp.cert);
    // Make the SP intolerant: a just-issued response is past max_issue_delay.
    config.sp.max_issue_delay = Duration::seconds(-300);
    let replay = SamlReplayCache::new();
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &[]);

    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now());
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

// ─── Replay ──────────────────────────────────────────────────────────────────

#[test]
fn replayed_assertion_is_rejected() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &[]);

    let first = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now());
    assert!(first.is_ok(), "first presentation should verify: {first:?}");

    let second = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now());
    assert!(matches!(second, Err(SamlError::Replay)), "replay must be rejected: {second:?}");
}

#[test]
fn replay_cache_detects_duplicate_and_prunes_expired() {
    let cache = SamlReplayCache::new();
    let now = Utc::now();
    let exp = now + Duration::minutes(5);

    assert!(cache.check_and_record("a1", exp, now), "first record is fresh");
    assert!(!cache.check_and_record("a1", exp, now), "duplicate is a replay");
    assert_eq!(cache.len(), 1);

    // After the window closes the entry is pruned, so the same id is fresh again — by then
    // the signature's own time-check would already reject it.
    let later = exp + Duration::seconds(1);
    assert!(cache.check_and_record("a1", later + Duration::minutes(5), later));
}

// ─── XXE ─────────────────────────────────────────────────────────────────────

#[test]
fn doctype_entity_is_rejected_before_parsing() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();

    let xxe = r#"<?xml version="1.0"?>
<!DOCTYPE Response [ <!ENTITY xxe SYSTEM "file:///etc/passwd"> ]>
<Response>&xxe;</Response>"#;
    let result = verify_saml_response(&config, &encode(xxe), &[REQ_ID], &replay, Utc::now());
    assert!(matches!(result, Err(SamlError::DocTypeForbidden)), "got {result:?}");
}

#[test]
fn reject_doctype_guards_dtd_and_entities() {
    assert!(reject_doctype("<Response/>").is_ok());
    assert!(matches!(reject_doctype("<!DOCTYPE x><x/>"), Err(SamlError::DocTypeForbidden)));
    // Case-insensitive + billion-laughs style internal entity.
    assert!(matches!(
        reject_doctype("<!doctype x [ <!ENTITY lol \"ha\"> ]><x/>"),
        Err(SamlError::DocTypeForbidden)
    ));
}

// ─── XML Signature Wrapping (seam test) ──────────────────────────────────────

#[test]
fn signature_wrapping_never_yields_forged_identity() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    let b64 = signed_response(&test_idp, "victim", SP_ENTITY, SP_ACS, REQ_ID, &[]);

    // Inject a forged, unsigned assertion (attacker NameID) as a sibling, right before the
    // legitimately-signed one. A naive "read an Assertion's NameID" would pick the attacker.
    let xml = decode(&b64);
    let forged = format!(
        r#"<saml2:Assertion xmlns:saml2="urn:oasis:names:tc:SAML:2.0:assertion" ID="forged" Version="2.0" IssueInstant="{}"><saml2:Issuer>{IDP_ENTITY}</saml2:Issuer><saml2:Subject><saml2:NameID>attacker</saml2:NameID></saml2:Subject></saml2:Assertion>"#,
        Utc::now().to_rfc3339()
    );
    let idx = xml.find("<saml2:Assertion").expect("assertion marker");
    let wrapped = format!("{}{forged}{}", &xml[..idx], &xml[idx..]);

    let result = verify_saml_response(&config, &encode(&wrapped), &[REQ_ID], &replay, Utc::now());
    // Either rejected outright, or the *signed* identity is returned — never the attacker's.
    match result {
        Err(_) => {},
        Ok(v) => assert_eq!(v.name_id, "victim", "must never surface the wrapped/forged NameID"),
    }
}

// ─── Comment-truncation NameID confusion (seam test) ─────────────────────────

#[test]
fn comment_truncation_never_truncates_nameid() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    // Signed NameID is an address under the attacker's own verified subdomain.
    let signed_name_id = "victim@example.com.attacker.test";
    let b64 = signed_response(&test_idp, signed_name_id, SP_ENTITY, SP_ACS, REQ_ID, &[]);

    // XML comments are excluded from C14N, so injecting one does not break the signature.
    // A vulnerable parser would split the text node and return "victim@example.com".
    let xml = decode(&b64).replace(signed_name_id, "victim@example.com<!---->.attacker.test");
    let result = verify_saml_response(&config, &encode(&xml), &[REQ_ID], &replay, Utc::now());
    match result {
        Err(_) => {},
        Ok(v) => {
            assert_eq!(v.name_id, signed_name_id, "must return the full signed NameID");
            assert_ne!(v.name_id, "victim@example.com", "must not truncate at the comment");
        },
    }
}

// ─── Tenant-bounded email-trust policy (#381 / #368) ─────────────────────────

#[test]
fn provider_key_is_namespaced() {
    assert_eq!(saml_provider_key("okta"), "saml:okta");
}

#[test]
fn effective_email_verified_is_fail_closed_by_default() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    assert!(!config.trust_asserted_email);
    assert!(!effective_saml_email_verified(&config), "default must be off");
}

#[test]
fn effective_email_verified_optin_single_tenant() {
    let test_idp = new_idp();
    let config = SamlIdpConfig::builder("test-idp", SP_ENTITY, SP_ACS)
        .idp_parts(IDP_ENTITY, IDP_SSO, test_idp.cert.der_data())
        .unwrap()
        .trust_asserted_email(true)
        .build()
        .unwrap();
    assert!(effective_saml_email_verified(&config), "opt-in single-tenant is honored");
}

#[test]
fn effective_email_verified_optin_multitenant_fails_closed() {
    let test_idp = new_idp();
    let config = SamlIdpConfig::builder("test-idp", SP_ENTITY, SP_ACS)
        .idp_parts(IDP_ENTITY, IDP_SSO, test_idp.cert.der_data())
        .unwrap()
        .trust_asserted_email(true)
        .tenant_id(Some("tenant-a".to_string()))
        .build()
        .unwrap();
    // Multi-tenant intent the global store can't bound -> fail closed even though opted in.
    assert!(!effective_saml_email_verified(&config));
}

#[tokio::test]
async fn default_saml_does_not_merge_into_trusted_email_account() {
    let store = InMemoryAccountStore::new();
    // A Google account verified the email globally.
    let google = store
        .link_or_create_user(Some("shared@example.com"), true, "google", "g-1")
        .await
        .unwrap();
    // A SAML login (default: email_verified=false) for the same email keys on (saml, NameID).
    let saml = store
        .link_or_create_user(Some("shared@example.com"), false, "saml:okta", "nameid-1")
        .await
        .unwrap();
    assert_ne!(google.user_id, saml.user_id, "default SAML must not merge on email");
}

#[tokio::test]
async fn optin_single_tenant_saml_merges_with_trusted_email_account() {
    let store = InMemoryAccountStore::new();
    let google = store
        .link_or_create_user(Some("shared@example.com"), true, "google", "g-1")
        .await
        .unwrap();
    // Opt-in single-tenant -> email_verified=true -> merges on the verified email.
    let saml = store
        .link_or_create_user(Some("shared@example.com"), true, "saml:okta", "nameid-1")
        .await
        .unwrap();
    assert_eq!(google.user_id, saml.user_id, "opt-in single-tenant should link on email");
}

#[tokio::test]
async fn pre_hijack_unverified_local_is_not_absorbed_by_trusted_saml() {
    let store = InMemoryAccountStore::new();
    // Attacker pre-seeds an UNVERIFIED local account under the victim's email.
    let local = store
        .link_or_create_user(Some("victim@example.com"), false, "local", "victim@example.com")
        .await
        .unwrap();
    // Victim later signs in via an opt-in trusted SAML IdP (email_verified=true).
    let saml = store
        .link_or_create_user(Some("victim@example.com"), true, "saml:okta", "nameid-1")
        .await
        .unwrap();
    assert_ne!(
        local.user_id, saml.user_id,
        "trusted sign-in must not absorb the pre-seeded local"
    );
}

// ─── Handlers (routing + binding) ────────────────────────────────────────────

fn auth_state_with(idp: SamlIdpConfig) -> (SamlAuthState, Arc<InMemoryStateStore>) {
    let state_store = Arc::new(InMemoryStateStore::new());
    let state = SamlAuthState::new(state_store.clone(), Arc::new(InMemorySessionStore::new()))
        .with_idp(idp)
        .with_user_store(Arc::new(InMemoryAccountStore::new()));
    (state, state_store)
}

#[tokio::test]
async fn saml_router_constructs() {
    // axum validates path-capture syntax at Router::route construction (CLAUDE.md gate).
    let state_store = Arc::new(InMemoryStateStore::new());
    let state = SamlAuthState::new(state_store, Arc::new(InMemorySessionStore::new()));
    let _router = saml_routes(state);
}

#[tokio::test]
async fn login_redirects_to_idp_with_relay_state() {
    use axum::{extract::Query, response::IntoResponse};
    let test_idp = new_idp();
    let (state, _) = auth_state_with(config_with_cert(&test_idp.cert));

    let resp = saml_login(
        axum::extract::State(state),
        Query(LoginQuery {
            idp: "test-idp".to_string(),
        }),
    )
    .await
    .into_response();

    assert!(resp.status().is_redirection(), "expected redirect, got {}", resp.status());
    let location = resp.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with(IDP_SSO), "redirect to IdP SSO: {location}");
    assert!(location.contains("SAMLRequest="), "carries SAMLRequest: {location}");
    assert!(location.contains("RelayState="), "carries RelayState: {location}");
}

#[tokio::test]
async fn login_unknown_idp_is_rejected() {
    use axum::{extract::Query, response::IntoResponse};
    let test_idp = new_idp();
    let (state, _) = auth_state_with(config_with_cert(&test_idp.cert));

    let resp = saml_login(
        axum::extract::State(state),
        Query(LoginQuery {
            idp: "nope".to_string(),
        }),
    )
    .await
    .into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn acs_happy_path_creates_session() {
    use axum::{extract::Form, response::IntoResponse};
    let test_idp = new_idp();
    let (state, state_store) = auth_state_with(config_with_cert(&test_idp.cert));

    // Seed a RelayState binding as login would have, then present a matching response.
    let relay = "relay-token-1".to_string();
    let now = crate::session::unix_now().unwrap();
    state_store
        .store(relay.clone(), format!("test-idp\n{REQ_ID}"), now + 600)
        .await
        .unwrap();
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &[]);

    let resp = saml_acs(
        axum::extract::State(state),
        Form(AcsForm {
            saml_response: b64,
            relay_state:   relay,
        }),
    )
    .await
    .into_response();

    assert_eq!(resp.status(), axum::http::StatusCode::OK, "ACS should succeed");
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("access_token").is_some(), "session token returned: {json}");
    assert_eq!(json.get("provider").and_then(|p| p.as_str()), Some("saml:test-idp"));
}

#[tokio::test]
async fn acs_rejects_missing_relay_state() {
    use axum::{extract::Form, response::IntoResponse};
    let test_idp = new_idp();
    let (state, _) = auth_state_with(config_with_cert(&test_idp.cert));

    let resp = saml_acs(
        axum::extract::State(state),
        Form(AcsForm {
            saml_response: "irrelevant".to_string(),
            relay_state:   String::new(),
        }),
    )
    .await
    .into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
}
