#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use axum::{
    extract::{Query, State},
    http::{StatusCode, header::LOCATION},
};

use super::*;
use crate::{
    error::Result,
    provider::{TokenResponse, UserInfo},
    session::InMemorySessionStore,
    state_store::InMemoryStateStore,
};

// ── redirect_uri allow-list matcher (the open-redirect security boundary) ──────────

#[test]
fn matcher_exact_match_allowed() {
    let allow = vec!["https://app.example.com/cb".to_string()];
    assert!(is_redirect_uri_allowed("https://app.example.com/cb", &allow));
}

#[test]
fn matcher_query_string_allowed_on_exact_path() {
    let allow = vec!["https://app.example.com/cb".to_string()];
    assert!(is_redirect_uri_allowed("https://app.example.com/cb?code=x&y=z", &allow));
}

#[test]
fn matcher_path_prefix_root_boundary_allowed() {
    let allow = vec!["https://app.example.com/".to_string()];
    assert!(is_redirect_uri_allowed("https://app.example.com/deep/link", &allow));
}

#[test]
fn matcher_subpath_allowed_at_segment_boundary() {
    let allow = vec!["https://app.example.com/cb".to_string()];
    assert!(is_redirect_uri_allowed("https://app.example.com/cb/extra", &allow));
}

#[test]
fn matcher_default_https_port_matches_explicit_443() {
    let allow = vec!["https://app.example.com/cb".to_string()];
    assert!(is_redirect_uri_allowed("https://app.example.com:443/cb", &allow));
}

#[test]
fn matcher_rejects_suffix_host_attack() {
    // The classic open-redirect bypass: attacker host that has the allow-listed host as a
    // prefix. Host comparison is exact, so this must be rejected.
    let allow = vec!["https://app.example.com/cb".to_string()];
    assert!(!is_redirect_uri_allowed("https://app.example.com.evil.com/cb", &allow));
}

#[test]
fn matcher_rejects_path_prefix_without_segment_boundary() {
    let allow = vec!["https://app.example.com/cb".to_string()];
    assert!(!is_redirect_uri_allowed("https://app.example.com/cbEVIL", &allow));
}

#[test]
fn matcher_rejects_scheme_downgrade() {
    let allow = vec!["https://app.example.com/cb".to_string()];
    assert!(!is_redirect_uri_allowed("http://app.example.com/cb", &allow));
}

#[test]
fn matcher_rejects_port_mismatch() {
    let allow = vec!["https://app.example.com/cb".to_string()];
    assert!(!is_redirect_uri_allowed("https://app.example.com:8443/cb", &allow));
}

#[test]
fn matcher_rejects_foreign_host() {
    let allow = vec!["https://app.example.com/cb".to_string()];
    assert!(!is_redirect_uri_allowed("https://evil.example.com/cb", &allow));
}

#[test]
fn matcher_empty_allowlist_rejects_everything() {
    assert!(!is_redirect_uri_allowed("https://app.example.com/cb", &[]));
}

#[test]
fn matcher_rejects_unparseable_candidate() {
    let allow = vec!["https://app.example.com/cb".to_string()];
    assert!(!is_redirect_uri_allowed("\\\\not a url", &allow));
}

#[test]
fn matcher_multiple_entries_any_match() {
    let allow = vec![
        "https://a.example.com/cb".to_string(),
        "https://b.example.com/cb".to_string(),
    ];
    assert!(is_redirect_uri_allowed("https://b.example.com/cb", &allow));
    assert!(!is_redirect_uri_allowed("https://c.example.com/cb", &allow));
}

// ── CSRF-state value codec (provider ± bound redirect_uri) ──────────────────────────

#[test]
fn codec_round_trips_with_redirect() {
    let encoded = encode_state_value("github", Some("https://app.example.com/cb"));
    assert_eq!(
        decode_state_value(&encoded),
        ("github".to_string(), Some("https://app.example.com/cb".to_string()))
    );
}

#[test]
fn codec_round_trips_without_redirect() {
    let encoded = encode_state_value("github", None);
    assert_eq!(encoded, "github");
    assert_eq!(decode_state_value(&encoded), ("github".to_string(), None));
}

#[test]
fn codec_decodes_legacy_provider_only_value() {
    // A value written before #427 is a bare provider name → no bound redirect.
    assert_eq!(decode_state_value("google"), ("google".to_string(), None));
}

// ── fragment-delivery redirect builder ──────────────────────────────────────────────

#[test]
fn redirect_builder_places_url_encoded_tokens_in_fragment() {
    let url = build_redirect_with_tokens(
        "https://app.example.com/cb",
        "acc tok",
        "ref/tok",
        3600,
        "mock",
    );
    assert!(url.starts_with("https://app.example.com/cb#"));
    assert!(url.contains("access_token=acc%20tok"));
    assert!(url.contains("refresh_token=ref%2Ftok"));
    assert!(url.contains("token_type=Bearer"));
    assert!(url.contains("expires_in=3600"));
    assert!(url.contains("provider=mock"));
}

// ── handler-level flow ──────────────────────────────────────────────────────────────

#[derive(Debug)]
struct MockProvider {
    name: String,
}

// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl OAuthProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn authorization_url(&self, state: &str) -> String {
        format!("https://provider.example/authorize?state={state}")
    }

    async fn exchange_code(&self, _code: &str) -> Result<TokenResponse> {
        Ok(TokenResponse {
            access_token:  "provider-access".to_string(),
            refresh_token: Some("provider-refresh".to_string()),
            expires_in:    3600,
            token_type:    "Bearer".to_string(),
        })
    }

    async fn user_info(&self, _access_token: &str) -> Result<UserInfo> {
        Ok(UserInfo {
            id:             "user-123".to_string(),
            email:          Some("u@example.com".to_string()),
            email_verified: true,
            name:           None,
            picture:        None,
            raw_claims:     serde_json::Value::Null,
        })
    }
}

fn state_with_allowlist(allowlist: Vec<String>) -> (Arc<MultiProviderAuthState>, Arc<InMemoryStateStore>) {
    let state_store = Arc::new(InMemoryStateStore::new());
    let mut state = MultiProviderAuthState::new(
        state_store.clone(),
        Arc::new(InMemorySessionStore::new()),
    )
    .with_redirect_uri_allowlist(allowlist);
    state.register_provider("mock", Arc::new(MockProvider { name: "mock".to_string() }));
    (Arc::new(state), state_store)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

#[tokio::test]
async fn authorize_rejects_non_allowlisted_redirect_uri() {
    // The open-redirect regression: a redirect_uri pointing at an attacker host is rejected.
    let (state, _store) = state_with_allowlist(vec!["https://app.example.com/cb".to_string()]);
    let response = authorize(
        State(state),
        Query(AuthorizeQuery {
            provider:     "mock".to_string(),
            redirect_uri: "https://evil.example.com/steal".to_string(),
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn authorize_accepts_allowlisted_redirect_uri() {
    let (state, _store) = state_with_allowlist(vec!["https://app.example.com/cb".to_string()]);
    let response = authorize(
        State(state),
        Query(AuthorizeQuery {
            provider:     "mock".to_string(),
            redirect_uri: "https://app.example.com/cb".to_string(),
        }),
    )
    .await;
    // Passes the allow-list and redirects to the provider's authorization endpoint.
    assert!(response.status().is_redirection());
}

#[tokio::test]
async fn authorize_without_allowlist_is_backward_compatible() {
    // No allow-list configured: redirect_uri is validated for presence/length only and the
    // flow proceeds to the provider redirect (legacy JSON-token behavior in callback).
    let (state, _store) = state_with_allowlist(vec![]);
    let response = authorize(
        State(state),
        Query(AuthorizeQuery {
            provider:     "mock".to_string(),
            redirect_uri: "https://anything.example/whatever".to_string(),
        }),
    )
    .await;
    assert!(response.status().is_redirection());
}

#[tokio::test]
async fn callback_redirects_to_bound_uri_with_token_fragment() {
    let (state, store) = state_with_allowlist(vec!["https://app.example.com/cb".to_string()]);

    // Bind a validated redirect_uri to a known state token, as `authorize` would have.
    store
        .store(
            "state-token-1".to_string(),
            encode_state_value("mock", Some("https://app.example.com/cb")),
            now_secs() + 600,
        )
        .await
        .unwrap();

    let response = callback(
        State(state),
        Query(CallbackQuery {
            code:              Some("auth-code".to_string()),
            state:             Some("state-token-1".to_string()),
            error:             None,
            error_description: None,
        }),
    )
    .await;

    assert!(response.status().is_redirection());
    let location = response.headers().get(LOCATION).unwrap().to_str().unwrap();
    assert!(location.starts_with("https://app.example.com/cb#"), "got: {location}");
    assert!(location.contains("access_token="));
    assert!(location.contains("token_type=Bearer"));
    assert!(location.contains("provider=mock"));
}

#[tokio::test]
async fn callback_without_bound_redirect_returns_json() {
    // Legacy path: state stored with no bound redirect → JSON token response, not a redirect.
    let (state, store) = state_with_allowlist(vec![]);
    store
        .store("state-token-2".to_string(), encode_state_value("mock", None), now_secs() + 600)
        .await
        .unwrap();

    let response = callback(
        State(state),
        Query(CallbackQuery {
            code:              Some("auth-code".to_string()),
            state:             Some("state-token-2".to_string()),
            error:             None,
            error_description: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get(LOCATION).is_none());
}
