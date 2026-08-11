//! Unit + attack-fixture tests for the SAML SP login + ACS slice (#381).
//!
//! Valid signed responses are minted with `samael`'s IdP side (an ephemeral RSA keypair +
//! self-signed cert), then mutated to construct each attack. The XSW / comment-truncation /
//! unsigned tests run against the **full** `verify_saml_response` extraction path (the seam),
//! not against the crypto backend in isolation.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code, panics are acceptable

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
    replay::SamlReplayStore as _,
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

#[tokio::test]
async fn valid_response_verifies_and_extracts_identity() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    let attrs = email_attr("user@example.com");
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &attrs);

    // Document the fixture structure the attack tests mutate.
    let xml = decode(&b64);
    assert!(xml.contains("<saml2:Assertion"), "expected saml2-prefixed Assertion: {xml}");
    assert!(xml.contains("Signature"), "expected an XML signature: {xml}");

    let verified = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now())
        .await
        .expect("should verify");
    assert_eq!(verified.name_id, "nameid-123");
    assert_eq!(verified.email.as_deref(), Some("user@example.com"));
}

// ─── Signature / trust ───────────────────────────────────────────────────────

#[tokio::test]
async fn wrong_idp_certificate_is_rejected() {
    let signer = new_idp();
    // Config trusts a DIFFERENT IdP's certificate than the one that signed.
    let other = new_idp();
    let config = config_with_cert(&other.cert);
    let replay = SamlReplayCache::new();
    let b64 = signed_response(&signer, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &[]);

    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now()).await;
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[tokio::test]
async fn tampered_attribute_breaks_signature() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    let attrs = email_attr("user@example.com");
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &attrs);

    // Flip the signed email value — the digest no longer matches.
    let tampered = decode(&b64).replace("user@example.com", "attacker@evil.test");
    let result =
        verify_saml_response(&config, &encode(&tampered), &[REQ_ID], &replay, Utc::now()).await;
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[tokio::test]
async fn unsigned_response_is_rejected() {
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

    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now()).await;
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[tokio::test]
async fn strict_algorithm_allowlist_rejects_weak_digest() {
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
    let strict_result = verify_saml_response(&strict, &b64, &[REQ_ID], &replay, Utc::now()).await;
    assert!(
        matches!(strict_result, Err(SamlError::Verification(_))),
        "strict allow-list must reject the weak SHA-1 digest: {strict_result:?}"
    );

    // The very same fixture verifies once the allow-list is relaxed — proving it was the
    // algorithm gate, not some other defect, that rejected it.
    let lenient = config_with_cert(&test_idp.cert);
    let replay2 = SamlReplayCache::new();
    assert!(
        verify_saml_response(&lenient, &b64, &[REQ_ID], &replay2, Utc::now())
            .await
            .is_ok(),
        "relaxed allow-list should accept the same fixture"
    );
}

// ─── Audience / recipient / request binding ──────────────────────────────────

#[tokio::test]
async fn wrong_audience_is_rejected() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    let b64 = signed_response(&test_idp, "nameid-123", "https://evil.example", SP_ACS, REQ_ID, &[]);

    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now()).await;
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[tokio::test]
async fn wrong_recipient_is_rejected() {
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

    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now()).await;
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[tokio::test]
async fn unsolicited_in_response_to_is_rejected() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    // Assertion is bound to a request ID we never issued.
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, "id-attacker", &[]);

    let result = verify_saml_response(&config, &b64, &["id-legit"], &replay, Utc::now()).await;
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[tokio::test]
async fn expired_response_is_rejected() {
    let test_idp = new_idp();
    let mut config = config_with_cert(&test_idp.cert);
    // Make the SP intolerant: a just-issued response is past max_issue_delay.
    config.sp.max_issue_delay = Duration::seconds(-300);
    let replay = SamlReplayCache::new();
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &[]);

    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now()).await;
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

// ─── Replay ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn replayed_assertion_is_rejected() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    let b64 = signed_response(&test_idp, "nameid-123", SP_ENTITY, SP_ACS, REQ_ID, &[]);

    let first = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now()).await;
    assert!(first.is_ok(), "first presentation should verify: {first:?}");

    let second = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now()).await;
    assert!(matches!(second, Err(SamlError::Replay)), "replay must be rejected: {second:?}");
}

#[tokio::test]
async fn replay_cache_detects_duplicate_and_prunes_expired() {
    let cache = SamlReplayCache::new();
    let now = Utc::now();
    let exp = now + Duration::minutes(5);

    assert!(cache.check_and_record("a1", exp, now).await.unwrap(), "first record is fresh");
    assert!(!cache.check_and_record("a1", exp, now).await.unwrap(), "duplicate is a replay");
    assert_eq!(cache.len(), 1);

    // After the window closes the entry is pruned, so the same id is fresh again — by then
    // the signature's own time-check would already reject it.
    let later = exp + Duration::seconds(1);
    assert!(cache.check_and_record("a1", later + Duration::minutes(5), later).await.unwrap());
}

// ─── XXE ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn doctype_entity_is_rejected_before_parsing() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();

    let xxe = r#"<?xml version="1.0"?>
<!DOCTYPE Response [ <!ENTITY xxe SYSTEM "file:///etc/passwd"> ]>
<Response>&xxe;</Response>"#;
    let result = verify_saml_response(&config, &encode(xxe), &[REQ_ID], &replay, Utc::now()).await;
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

#[tokio::test]
async fn signature_wrapping_never_yields_forged_identity() {
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

    let result =
        verify_saml_response(&config, &encode(&wrapped), &[REQ_ID], &replay, Utc::now()).await;
    // Either rejected outright, or the *signed* identity is returned — never the attacker's.
    match result {
        Err(_) => {},
        Ok(v) => assert_eq!(v.name_id, "victim", "must never surface the wrapped/forged NameID"),
    }
}

// ─── Comment-truncation NameID confusion (seam test) ─────────────────────────

#[tokio::test]
async fn comment_truncation_never_truncates_nameid() {
    let test_idp = new_idp();
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    // Signed NameID is an address under the attacker's own verified subdomain.
    let signed_name_id = "victim@example.com.attacker.test";
    let b64 = signed_response(&test_idp, signed_name_id, SP_ENTITY, SP_ACS, REQ_ID, &[]);

    // XML comments are excluded from C14N, so injecting one does not break the signature.
    // A vulnerable parser would split the text node and return "victim@example.com".
    let xml = decode(&b64).replace(signed_name_id, "victim@example.com<!---->.attacker.test");
    let result = verify_saml_response(&config, &encode(&xml), &[REQ_ID], &replay, Utc::now()).await;
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

#[tokio::test]
async fn effective_email_verified_optin_multitenant_fails_closed() {
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

// ─── Tenant scoping of /auth/saml/login (#947) ───────────────────────────────

/// Build a config for `idp_name` bound to `tenant`, trusting `cert`.
fn tenant_config(cert: &CertificateDer, idp_name: &str, tenant: Option<&str>) -> SamlIdpConfig {
    let mut config = SamlIdpConfig::builder(idp_name, SP_ENTITY, SP_ACS)
        .idp_parts(IDP_ENTITY, IDP_SSO, cert.der_data())
        .unwrap()
        .tenant_id(tenant.map(str::to_string))
        .build()
        .unwrap();
    config.sp.allowed_signature_algorithms = None;
    config
}

/// `GET /auth/saml/login` through the real router, so query-string parsing is exercised.
async fn login_status(state: SamlAuthState, query: &str) -> axum::http::StatusCode {
    use tower::ServiceExt as _;
    let router = saml_routes(state);
    router
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/auth/saml/login?{query}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

const TENANT_A: &str = "11111111-1111-4111-8111-111111111111";
const TENANT_B: &str = "22222222-2222-4222-8222-222222222222";

#[tokio::test]
async fn login_refuses_tenant_bound_idp_without_a_matching_tenant() {
    let test_idp = new_idp();
    let (state, _) = auth_state_with(tenant_config(&test_idp.cert, "acme-okta", Some(TENANT_A)));

    // No tenant named at all: the caller cannot prove it may use a tenant's IdP.
    assert_eq!(
        login_status(state.clone(), "idp=acme-okta").await,
        axum::http::StatusCode::NOT_FOUND,
        "a tenant-bound IdP must not start a login for an untenanted caller"
    );
    // A *different* tenant naming it is the cross-tenant case the issue reports.
    assert_eq!(
        login_status(state, &format!("idp=acme-okta&tenant={TENANT_B}")).await,
        axum::http::StatusCode::NOT_FOUND,
        "another tenant's IdP name must be a 404, not a login"
    );
}

#[tokio::test]
async fn login_serves_tenant_bound_idp_to_its_own_tenant() {
    let test_idp = new_idp();
    let (state, _) = auth_state_with(tenant_config(&test_idp.cert, "acme-okta", Some(TENANT_A)));

    assert_eq!(
        login_status(state, &format!("idp=acme-okta&tenant={TENANT_A}")).await,
        axum::http::StatusCode::SEE_OTHER,
        "the owning tenant must still be able to start SSO"
    );
}

#[tokio::test]
async fn login_refuses_a_tenant_qualified_request_for_an_untenanted_idp() {
    let test_idp = new_idp();
    let (state, _) = auth_state_with(tenant_config(&test_idp.cert, "global-idp", None));

    // Single-tenant deployments keep working unqualified …
    assert_eq!(
        login_status(state.clone(), "idp=global-idp").await,
        axum::http::StatusCode::SEE_OTHER,
        "an untenanted IdP must keep serving the untenanted single-tenant path"
    );
    // … but "global" is not "belongs to whichever tenant asked".
    assert_eq!(
        login_status(state, &format!("idp=global-idp&tenant={TENANT_A}")).await,
        axum::http::StatusCode::NOT_FOUND,
        "an untenanted IdP must not answer a tenant-qualified request"
    );
}

#[tokio::test]
async fn login_redirects_to_idp_with_relay_state() {
    use axum::{extract::Query, response::IntoResponse};
    let test_idp = new_idp();
    let (state, _) = auth_state_with(config_with_cert(&test_idp.cert));

    let resp = saml_login(
        axum::extract::State(state),
        Query(LoginQuery {
            idp:    "test-idp".to_string(),
            tenant: None,
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
            idp:    "nope".to_string(),
            tenant: None,
        }),
    )
    .await
    .into_response();
    // 404, and identical to the tenant-mismatch refusal: an unknown name and another
    // tenant's name must be indistinguishable, or the route enumerates IdPs (#947).
    assert_eq!(resp.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn acs_happy_path_creates_session() {
    use axum::{extract::Form, response::IntoResponse};
    let test_idp = new_idp();
    let (state, state_store) = auth_state_with(config_with_cert(&test_idp.cert));

    // Seed a RelayState binding as login would have, then present a matching response.
    let relay = "relay-token-1".to_string();
    let now = crate::session::unix_now().unwrap();
    // Payload shape: idp_name \n tenant \n request_id (tenant empty = untenanted).
    state_store
        .store(relay.clone(), format!("test-idp\n\n{REQ_ID}"), now + 600)
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

// ─── SP signing, SP metadata, encrypted assertions (#948) ────────────────────

/// An SP key pair: a fresh RSA key and a matching self-signed certificate, in the PEM/DER
/// forms the builder accepts.
struct SpKeys {
    key_pem:  Vec<u8>,
    cert_pem: Vec<u8>,
    cert_der: Vec<u8>,
}

fn new_sp_keys(common_name: &str) -> SpKeys {
    use openssl::{
        asn1::Asn1Time, bn::BigNum, hash::MessageDigest, pkey::PKey, rsa::Rsa as OpenSslRsa,
        x509::X509Builder,
    };

    let rsa = OpenSslRsa::generate(2048).unwrap();
    let key = PKey::from_rsa(rsa).unwrap();

    let mut name = openssl::x509::X509Name::builder().unwrap();
    name.append_entry_by_text("CN", common_name).unwrap();
    let name = name.build();

    let mut builder = X509Builder::new().unwrap();
    builder.set_version(2).unwrap();
    builder
        .set_serial_number(&BigNum::from_u32(1).unwrap().to_asn1_integer().unwrap())
        .unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&key).unwrap();
    builder.set_not_before(&Asn1Time::days_from_now(0).unwrap()).unwrap();
    builder.set_not_after(&Asn1Time::days_from_now(3650).unwrap()).unwrap();
    builder.sign(&key, MessageDigest::sha256()).unwrap();
    let cert = builder.build();

    SpKeys {
        key_pem:  key.private_key_to_pem_pkcs8().unwrap(),
        cert_pem: cert.to_pem().unwrap(),
        cert_der: cert.to_der().unwrap(),
    }
}

/// Build an `EncryptedAssertion` element the way an IdP with assertion encryption would:
/// a random content key, the assertion encrypted under it, and that key wrapped to the SP
/// certificate's public key.
fn encrypt_assertion(
    assertion_xml: &str,
    sp_cert_der: &[u8],
    key_transport_alg: &str,
    content_alg: &str,
) -> String {
    use openssl::{rand::rand_bytes, rsa::Padding, symm::Cipher};

    let cipher = match content_alg {
        a if a.ends_with("aes128-gcm") => Cipher::aes_128_gcm(),
        a if a.ends_with("aes128-cbc") => Cipher::aes_128_cbc(),
        other => panic!("test fixture does not build {other}"),
    };
    let mut content_key = vec![0u8; cipher.key_len()];
    rand_bytes(&mut content_key).unwrap();
    let mut iv = vec![0u8; cipher.iv_len().unwrap()];
    rand_bytes(&mut iv).unwrap();

    // Layout samael's decryptor expects: iv || ciphertext [|| tag for GCM].
    let mut payload = iv.clone();
    if content_alg.ends_with("-gcm") {
        let mut tag = vec![0u8; 16];
        let ciphertext = openssl::symm::encrypt_aead(
            cipher,
            &content_key,
            Some(&iv),
            &[],
            assertion_xml.as_bytes(),
            &mut tag,
        )
        .unwrap();
        payload.extend_from_slice(&ciphertext);
        payload.extend_from_slice(&tag);
    } else {
        let ciphertext =
            openssl::symm::encrypt(cipher, &content_key, Some(&iv), assertion_xml.as_bytes())
                .unwrap();
        payload.extend_from_slice(&ciphertext);
    }

    let padding = if key_transport_alg.ends_with("rsa-1_5") {
        Padding::PKCS1
    } else {
        Padding::PKCS1_OAEP
    };
    let public = openssl::x509::X509::from_der(sp_cert_der).unwrap().public_key().unwrap();
    let rsa = public.rsa().unwrap();
    let mut wrapped = vec![0u8; rsa.size() as usize];
    let n = rsa.public_encrypt(&content_key, &mut wrapped, padding).unwrap();
    wrapped.truncate(n);

    let engine = base64::engine::general_purpose::STANDARD;
    format!(
        r#"<saml2:EncryptedAssertion xmlns:saml2="urn:oasis:names:tc:SAML:2.0:assertion"><xenc:EncryptedData xmlns:xenc="http://www.w3.org/2001/04/xmlenc#" Type="http://www.w3.org/2001/04/xmlenc#Element"><xenc:EncryptionMethod Algorithm="{content_alg}"/><ds:KeyInfo xmlns:ds="http://www.w3.org/2000/09/xmldsig#"><xenc:EncryptedKey><xenc:EncryptionMethod Algorithm="{key_transport_alg}"/><xenc:CipherData><xenc:CipherValue>{}</xenc:CipherValue></xenc:CipherData></xenc:EncryptedKey></ds:KeyInfo><xenc:CipherData><xenc:CipherValue>{}</xenc:CipherValue></xenc:CipherData></xenc:EncryptedData></saml2:EncryptedAssertion>"#,
        engine.encode(&wrapped),
        engine.encode(&payload),
    )
}

/// A `Response` whose assertion is encrypted to `sp_cert_der`. `sign` controls whether the
/// envelope carries the IdP signature — the only thing that makes the ciphertext
/// trustworthy, since the decrypted assertion's own signature is never checked.
fn encrypted_response(
    test_idp: &TestIdp,
    sp_cert_der: &[u8],
    sign: bool,
    key_transport_alg: &str,
    content_alg: &str,
) -> String {
    let template = build_response_template(
        &test_idp.cert,
        "nameid-enc",
        SP_ENTITY,
        IDP_ENTITY,
        SP_ACS,
        REQ_ID,
        &email_attr("encrypted@example.com"),
    );
    let mut template = template;
    if let Some(data) = template
        .assertion
        .as_mut()
        .and_then(|a| a.subject.as_mut())
        .and_then(|s| s.subject_confirmations.as_mut())
        .and_then(|c| c.first_mut())
        .and_then(|c| c.subject_confirmation_data.as_mut())
    {
        data.not_on_or_after = Some(Utc::now() + Duration::minutes(5));
    }
    let xml = template.to_string().unwrap();

    // Swap the plaintext Assertion element for its encrypted form, leaving the envelope
    // (IDs, Status, Destination, InResponseTo, Signature template) exactly as samael built it.
    let start = xml.find("<saml2:Assertion").expect("template has a plaintext assertion");
    let end_tag = "</saml2:Assertion>";
    let end = xml.find(end_tag).expect("assertion is closed") + end_tag.len();
    let assertion_xml = &xml[start..end];
    let encrypted = encrypt_assertion(assertion_xml, sp_cert_der, key_transport_alg, content_alg);
    let swapped = format!("{}{}{}", &xml[..start], encrypted, &xml[end..]);

    let out = if sign {
        Crypto::sign_xml(swapped, &test_idp.idp.export_private_key_der().unwrap()).unwrap()
    } else {
        swapped
    };
    encode(&out)
}

/// Config trusting `cert` as the IdP signing cert, with an SP key pair for decryption.
fn config_with_sp_key(cert: &CertificateDer, sp: &SpKeys) -> SamlIdpConfig {
    let mut config = SamlIdpConfig::builder("test-idp", SP_ENTITY, SP_ACS)
        .idp_parts(IDP_ENTITY, IDP_SSO, cert.der_data())
        .unwrap()
        .sp_key_pair(&sp.key_pem, &sp.cert_pem)
        .unwrap()
        .build()
        .unwrap();
    config.sp.allowed_signature_algorithms = None;
    config
}

const OAEP: &str = "http://www.w3.org/2001/04/xmlenc#rsa-oaep-mgf1p";
const AES_GCM: &str = "http://www.w3.org/2009/xmlenc11#aes128-gcm";

#[tokio::test]
async fn encrypted_assertion_decrypts_and_runs_the_existing_verification_path() {
    let test_idp = new_idp();
    let sp = new_sp_keys("sp.example.com");
    let config = config_with_sp_key(&test_idp.cert, &sp);
    let replay = SamlReplayCache::new();
    let b64 = encrypted_response(&test_idp, &sp.cert_der, true, OAEP, AES_GCM);

    let verified = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now())
        .await
        .expect("a signed, encrypted assertion should verify");
    assert_eq!(verified.name_id, "nameid-enc");
    assert_eq!(verified.email.as_deref(), Some("encrypted@example.com"));

    // Decryption did not become a bypass: the same response is still single-use, so the
    // replay guard downstream of the decrypt runs exactly as on the plaintext path.
    let again = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now()).await;
    assert!(matches!(again, Err(SamlError::Replay)), "got {again:?}");
}

#[tokio::test]
async fn encrypted_assertion_still_enforces_the_request_binding() {
    let test_idp = new_idp();
    let sp = new_sp_keys("sp.example.com");
    let config = config_with_sp_key(&test_idp.cert, &sp);
    let replay = SamlReplayCache::new();
    let b64 = encrypted_response(&test_idp, &sp.cert_der, true, OAEP, AES_GCM);

    // An InResponseTo we never issued must fail after decryption exactly as before it.
    let result =
        verify_saml_response(&config, &b64, &["id-never-issued"], &replay, Utc::now()).await;
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[tokio::test]
async fn tampered_encrypted_assertion_does_not_verify_after_decryption() {
    let test_idp = new_idp();
    let sp = new_sp_keys("sp.example.com");
    let config = config_with_sp_key(&test_idp.cert, &sp);
    let replay = SamlReplayCache::new();
    let b64 = encrypted_response(&test_idp, &sp.cert_der, true, OAEP, AES_GCM);

    // Flip a byte inside the ciphertext. The envelope signature covers it, so this must be
    // refused — the decrypted assertion carries no signature of its own to fall back on.
    let xml = decode(&b64);
    let marker = "<xenc:CipherValue>";
    let start = xml.rfind(marker).unwrap() + marker.len();
    let mut bytes: Vec<u8> = xml.clone().into_bytes();
    bytes[start + 4] = if bytes[start + 4] == b'A' { b'B' } else { b'A' };
    let tampered = String::from_utf8(bytes).unwrap();

    let result =
        verify_saml_response(&config, &encode(&tampered), &[REQ_ID], &replay, Utc::now()).await;
    assert!(
        matches!(result, Err(SamlError::Verification(_))),
        "a tampered encrypted assertion must not verify: {result:?}"
    );
}

#[tokio::test]
async fn unsigned_response_carrying_an_encrypted_assertion_is_refused() {
    let test_idp = new_idp();
    let sp = new_sp_keys("sp.example.com");
    let config = config_with_sp_key(&test_idp.cert, &sp);
    let replay = SamlReplayCache::new();

    // This is the trust boundary of the whole encrypted path: samael verifies no signature
    // on the *decrypted* assertion, so the envelope signature over the ciphertext is the
    // only integrity there is. Without it, an attacker who knows the SP's public key can
    // mint any assertion they like.
    let b64 = encrypted_response(&test_idp, &sp.cert_der, false, OAEP, AES_GCM);
    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now()).await;
    assert!(
        matches!(result, Err(SamlError::Verification(_))),
        "an unsigned encrypted response must be refused: {result:?}"
    );
}

#[tokio::test]
async fn encrypted_assertion_without_an_sp_key_is_refused_not_ignored() {
    let test_idp = new_idp();
    let sp = new_sp_keys("sp.example.com");
    // The IdP encrypts, but this SP has no key configured at all.
    let config = config_with_cert(&test_idp.cert);
    let replay = SamlReplayCache::new();
    let b64 = encrypted_response(&test_idp, &sp.cert_der, true, OAEP, AES_GCM);

    let result = verify_saml_response(&config, &b64, &[REQ_ID], &replay, Utc::now()).await;
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

#[tokio::test]
async fn weak_encryption_algorithms_are_refused() {
    let test_idp = new_idp();
    let sp = new_sp_keys("sp.example.com");
    let config = config_with_sp_key(&test_idp.cert, &sp);

    // RSA-1_5 key transport — the Bleichenbacher-style break of XML Encryption.
    let rsa15 = encrypted_response(
        &test_idp,
        &sp.cert_der,
        true,
        "http://www.w3.org/2001/04/xmlenc#rsa-1_5",
        AES_GCM,
    );
    let result =
        verify_saml_response(&config, &rsa15, &[REQ_ID], &SamlReplayCache::new(), Utc::now()).await;
    assert!(
        matches!(&result, Err(SamlError::Verification(m)) if m.contains("key-transport")),
        "rsa-1_5 must be refused by name: {result:?}"
    );

    // AES-CBC content encryption — unauthenticated, the padding-oracle shape.
    let cbc = encrypted_response(
        &test_idp,
        &sp.cert_der,
        true,
        OAEP,
        "http://www.w3.org/2001/04/xmlenc#aes128-cbc",
    );
    let result =
        verify_saml_response(&config, &cbc, &[REQ_ID], &SamlReplayCache::new(), Utc::now()).await;
    assert!(
        matches!(&result, Err(SamlError::Verification(m)) if m.contains("content-encryption")),
        "aes-cbc must be refused by name: {result:?}"
    );
}

#[tokio::test]
async fn the_previous_sp_key_decrypts_during_a_rotation_window() {
    let test_idp = new_idp();
    let old = new_sp_keys("sp.example.com");
    let new = new_sp_keys("sp.example.com");

    // The SP has rotated to `new` and published it, but the IdP still encrypts to `old`.
    let mut config = SamlIdpConfig::builder("test-idp", SP_ENTITY, SP_ACS)
        .idp_parts(IDP_ENTITY, IDP_SSO, test_idp.cert.der_data())
        .unwrap()
        .sp_key_pair(&new.key_pem, &new.cert_pem)
        .unwrap()
        .sp_previous_key_pair(&old.key_pem, &old.cert_pem)
        .unwrap()
        .build()
        .unwrap();
    config.sp.allowed_signature_algorithms = None;

    let b64 = encrypted_response(&test_idp, &old.cert_der, true, OAEP, AES_GCM);
    let verified =
        verify_saml_response(&config, &b64, &[REQ_ID], &SamlReplayCache::new(), Utc::now())
            .await
            .expect("the previous key must still decrypt during the rollover window");
    assert_eq!(verified.name_id, "nameid-enc");

    // And the new key works too, so the window is genuinely two-key.
    let b64_new = encrypted_response(&test_idp, &new.cert_der, true, OAEP, AES_GCM);
    assert!(
        verify_saml_response(&config, &b64_new, &[REQ_ID], &SamlReplayCache::new(), Utc::now())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn an_unrelated_key_never_decrypts() {
    let test_idp = new_idp();
    let sp = new_sp_keys("sp.example.com");
    let stranger = new_sp_keys("attacker.example.com");
    let config = config_with_sp_key(&test_idp.cert, &sp);

    // Encrypted to a key this SP has never held — rotation must not become "try anything".
    let b64 = encrypted_response(&test_idp, &stranger.cert_der, true, OAEP, AES_GCM);
    let result =
        verify_saml_response(&config, &b64, &[REQ_ID], &SamlReplayCache::new(), Utc::now()).await;
    assert!(matches!(result, Err(SamlError::Verification(_))), "got {result:?}");
}

// ─── SP request signing ──────────────────────────────────────────────────────

#[tokio::test]
async fn authn_request_is_unsigned_by_default_and_signed_when_configured() {
    let test_idp = new_idp();
    let sp = new_sp_keys("sp.example.com");

    // Default: no signature parameters on the redirect.
    let (state, _) = auth_state_with(config_with_cert(&test_idp.cert));
    let location = login_location(state).await;
    assert!(!location.contains("SigAlg="), "unsigned must stay the default: {location}");
    assert!(!location.contains("Signature="), "unsigned must stay the default: {location}");

    // Opted in: the HTTP-Redirect binding's signature parameters appear.
    let mut signing = SamlIdpConfig::builder("test-idp", SP_ENTITY, SP_ACS)
        .idp_parts(IDP_ENTITY, IDP_SSO, test_idp.cert.der_data())
        .unwrap()
        .sp_key_pair(&sp.key_pem, &sp.cert_pem)
        .unwrap()
        .sign_authn_requests(true)
        .build()
        .unwrap();
    signing.sp.allowed_signature_algorithms = None;
    assert!(signing.signs_authn_requests());

    let (state, _) = auth_state_with(signing);
    let location = login_location(state).await;
    assert!(location.contains("SigAlg="), "a signed request carries SigAlg: {location}");
    assert!(
        location.contains("Signature="),
        "a signed request carries Signature: {location}"
    );
}

/// Drive `GET /auth/saml/login` through the router and return the `Location` header.
async fn login_location(state: SamlAuthState) -> String {
    use tower::ServiceExt as _;

    let resp = saml_routes(state)
        .oneshot(
            axum::http::Request::builder()
                .uri("/auth/saml/login?idp=test-idp")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.status().is_redirection(), "expected a redirect, got {}", resp.status());
    resp.headers().get("location").unwrap().to_str().unwrap().to_string()
}

#[test]
fn signing_without_a_key_refuses_to_build_rather_than_downgrading() {
    let test_idp = new_idp();
    let result = SamlIdpConfig::builder("test-idp", SP_ENTITY, SP_ACS)
        .idp_parts(IDP_ENTITY, IDP_SSO, test_idp.cert.der_data())
        .unwrap()
        .sign_authn_requests(true)
        .build();
    assert!(matches!(result, Err(SamlError::Config(_))), "got {result:?}");
}

#[test]
fn a_mismatched_sp_key_pair_is_refused_at_configuration_time() {
    let test_idp = new_idp();
    let a = new_sp_keys("sp.example.com");
    let b = new_sp_keys("sp.example.com");
    let result = SamlIdpConfig::builder("test-idp", SP_ENTITY, SP_ACS)
        .idp_parts(IDP_ENTITY, IDP_SSO, test_idp.cert.der_data())
        .unwrap()
        .sp_key_pair(&a.key_pem, &b.cert_pem);
    assert!(
        matches!(&result, Err(SamlError::Config(m)) if m.contains("does not match")),
        "a key/cert mismatch must be caught before deployment"
    );
}

// ─── SP metadata publishing ──────────────────────────────────────────────────

#[tokio::test]
async fn sp_metadata_publishes_the_certificate_and_reflects_the_signing_posture() {
    let test_idp = new_idp();
    let sp = new_sp_keys("sp.example.com");
    let old = new_sp_keys("sp.example.com");

    // No key: a valid document an IdP can still consume for the ACS endpoint.
    let bare = config_with_cert(&test_idp.cert);
    let xml = bare.sp_metadata_xml();
    assert!(xml.contains(&format!(r#"entityID="{SP_ENTITY}""#)), "{xml}");
    assert!(xml.contains(SP_ACS), "{xml}");
    assert!(xml.contains(r#"AuthnRequestsSigned="false""#), "{xml}");
    assert!(!xml.contains("<KeyDescriptor"), "no key, no certificate to publish: {xml}");

    // Signing + rotation: current cert for signing and encryption, previous for encryption.
    let mut config = SamlIdpConfig::builder("test-idp", SP_ENTITY, SP_ACS)
        .idp_parts(IDP_ENTITY, IDP_SSO, test_idp.cert.der_data())
        .unwrap()
        .sp_key_pair(&sp.key_pem, &sp.cert_pem)
        .unwrap()
        .sp_previous_key_pair(&old.key_pem, &old.cert_pem)
        .unwrap()
        .sign_authn_requests(true)
        .build()
        .unwrap();
    config.sp.allowed_signature_algorithms = None;

    let xml = config.sp_metadata_xml();
    let engine = base64::engine::general_purpose::STANDARD;
    assert!(xml.contains(r#"AuthnRequestsSigned="true""#), "{xml}");
    assert!(xml.contains(&engine.encode(&sp.cert_der)), "current certificate is published");
    assert!(
        xml.contains(&engine.encode(&old.cert_der)),
        "the previous certificate stays published for encryption during the window"
    );
    assert_eq!(xml.matches(r#"use="signing""#).count(), 1, "exactly one signing key: {xml}");
    assert_eq!(xml.matches(r#"use="encryption""#).count(), 2, "current + previous: {xml}");

    // The route serves it, and is tenant-scoped exactly like login.
    let (state, _) = auth_state_with(config);
    let resp = tower::ServiceExt::oneshot(
        saml_routes(state),
        axum::http::Request::builder()
            .uri("/auth/saml/metadata?idp=test-idp")
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("SPSSODescriptor"));
}
