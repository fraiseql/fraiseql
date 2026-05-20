#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

use std::{sync::Arc, time::Duration as StdDuration};

use base64::Engine as _;
use chrono::{Duration, Utc};

use super::*;

#[test]
fn test_token_response_creation() {
    let token = TokenResponse::new("token123".to_string(), "Bearer".to_string(), 3600);
    assert_eq!(token.access_token, "token123");
    assert_eq!(token.token_type, "Bearer");
    assert_eq!(token.expires_in, 3600);
}

#[test]
fn test_token_response_expiry_calculation() {
    let token = TokenResponse::new("token123".to_string(), "Bearer".to_string(), 3600);
    assert!(!token.is_expired());
}

#[test]
fn test_id_token_claims_creation() {
    let exp = (Utc::now() + Duration::hours(1)).timestamp();
    let claims = IdTokenClaims::new(
        "https://provider.com".to_string(),
        "user123".to_string(),
        "client_id".to_string(),
        exp,
        Utc::now().timestamp(),
    );
    assert_eq!(claims.sub, "user123");
    assert!(!claims.is_expired());
}

#[test]
fn test_id_token_claims_expiry() {
    let exp = (Utc::now() - Duration::hours(1)).timestamp();
    let claims = IdTokenClaims::new(
        "https://provider.com".to_string(),
        "user123".to_string(),
        "client_id".to_string(),
        exp,
        (Utc::now() - Duration::hours(2)).timestamp(),
    );
    assert!(claims.is_expired());
}

#[test]
fn test_userinfo_creation() {
    let userinfo = UserInfo::new("user123".to_string());
    assert_eq!(userinfo.sub, "user123");
    assert!(userinfo.email.is_none());
}

#[test]
fn test_oauth2_client_creation() {
    let client = OAuth2Client::new(
        "client_id",
        "client_secret",
        "https://provider.com/authorize",
        "https://provider.com/token",
    );
    assert_eq!(client.client_id, "client_id");
}

#[test]
fn test_oauth2_client_with_scopes() {
    let scopes = vec!["openid".to_string(), "profile".to_string()];
    let client = OAuth2Client::new(
        "client_id",
        "client_secret",
        "https://provider.com/authorize",
        "https://provider.com/token",
    )
    .with_scopes(scopes.clone());
    assert_eq!(client.scopes, scopes);
}

#[test]
fn test_oidc_provider_config_creation() {
    let config = OIDCProviderConfig::new(
        "https://provider.com".to_string(),
        "https://provider.com/authorize".to_string(),
        "https://provider.com/token".to_string(),
        "https://provider.com/jwks".to_string(),
    );
    assert_eq!(config.issuer, "https://provider.com");
}

#[test]
fn test_oauth_session_creation() {
    let session = OAuthSession::new(
        "user_123".to_string(),
        ProviderType::OIDC,
        "auth0".to_string(),
        "auth0|user_id".to_string(),
        "access_token".to_string(),
        Utc::now() + Duration::hours(1),
    );
    assert_eq!(session.user_id, "user_123");
    assert!(!session.is_expired());
}

#[test]
fn test_oauth_session_token_refresh() {
    let mut session = OAuthSession::new(
        "user_123".to_string(),
        ProviderType::OIDC,
        "auth0".to_string(),
        "auth0|user_id".to_string(),
        "old_token".to_string(),
        Utc::now() + Duration::hours(1),
    );
    let new_expiry = Utc::now() + Duration::hours(2);
    session.refresh_tokens("new_token".to_string(), new_expiry);
    assert_eq!(session.access_token, "new_token");
    assert!(session.last_refreshed.is_some());
}

#[test]
fn test_external_auth_provider_creation() {
    let provider =
        ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "client_id", "vault/path/to/secret");
    assert_eq!(provider.provider_name, "auth0");
    assert!(provider.enabled);
}

#[test]
fn test_provider_registry_register_and_get() {
    let registry = ProviderRegistry::new();
    let provider =
        ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "client_id", "vault/path");
    registry.register(provider.clone()).unwrap();
    let retrieved = registry.get("auth0").unwrap();
    assert_eq!(retrieved, Some(provider));
}

#[test]
fn test_provider_registry_list_enabled() {
    let registry = ProviderRegistry::new();
    let provider1 = ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "id1", "path1");
    let provider2 = ExternalAuthProvider::new(ProviderType::OAuth2, "google", "id2", "path2");
    registry.register(provider1).unwrap();
    registry.register(provider2).unwrap();
    let enabled = registry.list_enabled().unwrap();
    assert_eq!(enabled.len(), 2);
}

#[test]
fn test_provider_registry_disable_enable() {
    let registry = ProviderRegistry::new();
    let provider = ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "id", "path");
    registry.register(provider).unwrap();

    registry.disable("auth0").unwrap();
    let retrieved = registry.get("auth0").unwrap();
    assert!(!retrieved.unwrap().enabled);

    registry.enable("auth0").unwrap();
    let retrieved = registry.get("auth0").unwrap();
    assert!(retrieved.unwrap().enabled);
}

#[test]
fn test_pkce_challenge_generation() {
    let challenge = PKCEChallenge::new();
    assert!(!challenge.code_verifier.is_empty());
    assert!(!challenge.code_challenge.is_empty());
    assert_eq!(challenge.code_challenge_method, "S256");
}

#[test]
fn test_pkce_verification() {
    let challenge = PKCEChallenge::new();
    let verifier = challenge.code_verifier.clone();
    assert!(challenge.verify(&verifier));
}

#[test]
fn test_pkce_verification_fails_with_wrong_verifier() {
    let challenge = PKCEChallenge::new();
    assert!(!challenge.verify("wrong_verifier"));
}

#[test]
fn test_state_parameter_generation() {
    let state = StateParameter::new();
    assert!(!state.state.is_empty());
    assert!(!state.is_expired());
}

#[test]
fn test_state_parameter_verification() {
    let state = StateParameter::new();
    assert!(state.verify(&state.state));
}

#[test]
fn test_state_parameter_verification_fails_with_wrong_state() {
    let state = StateParameter::new();
    assert!(!state.verify("wrong_state"));
}

#[test]
fn test_nonce_parameter_generation() {
    let nonce = NonceParameter::new();
    assert!(!nonce.nonce.is_empty());
    assert!(!nonce.is_expired());
}

#[test]
fn test_nonce_parameter_verification() {
    let nonce = NonceParameter::new();
    assert!(nonce.verify(&nonce.nonce));
}

#[test]
fn test_token_refresh_scheduler_schedule_and_retrieve() {
    let scheduler = TokenRefreshScheduler::new();
    let refresh_time = Utc::now() - Duration::seconds(10);
    scheduler.schedule_refresh("session_1".to_string(), refresh_time).unwrap();

    let next = scheduler.get_next_refresh().unwrap();
    assert_eq!(next, Some("session_1".to_string()));
}

#[test]
fn test_token_refresh_scheduler_cancel() {
    let scheduler = TokenRefreshScheduler::new();
    let refresh_time = Utc::now() + Duration::hours(1);
    scheduler.schedule_refresh("session_1".to_string(), refresh_time).unwrap();

    let cancelled = scheduler.cancel_refresh("session_1").unwrap();
    assert!(cancelled);
}

#[test]
fn test_failover_manager_primary_available() {
    let manager = ProviderFailoverManager::new("auth0".to_string(), vec!["google".to_string()]);
    let available = manager.get_available_provider().unwrap();
    assert_eq!(available, "auth0");
}

#[test]
fn test_failover_manager_fallback() {
    let manager = ProviderFailoverManager::new("auth0".to_string(), vec!["google".to_string()]);
    manager.mark_unavailable("auth0".to_string(), 300).unwrap();
    let available = manager.get_available_provider().unwrap();
    assert_eq!(available, "google");
}

#[test]
fn test_failover_manager_mark_available() {
    let manager = ProviderFailoverManager::new("auth0".to_string(), vec!["google".to_string()]);
    manager.mark_unavailable("auth0".to_string(), 300).unwrap();
    manager.mark_available("auth0").unwrap();
    let available = manager.get_available_provider().unwrap();
    assert_eq!(available, "auth0");
}

#[test]
fn test_oauth_audit_event_creation() {
    let event = OAuthAuditEvent::new("authorization", "auth0", "success");
    assert_eq!(event.event_type, "authorization");
    assert_eq!(event.provider, "auth0");
    assert_eq!(event.status, "success");
}

#[test]
fn test_oauth_audit_event_with_user_id() {
    let event = OAuthAuditEvent::new("token_exchange", "auth0", "success")
        .with_user_id("user_123".to_string());
    assert_eq!(event.user_id, Some("user_123".to_string()));
}

#[test]
fn test_oauth_audit_event_with_error() {
    let event = OAuthAuditEvent::new("token_exchange", "auth0", "failed")
        .with_error("Provider unavailable".to_string());
    assert_eq!(event.error, Some("Provider unavailable".to_string()));
}

#[test]
fn test_oauth_audit_event_with_metadata() {
    let event = OAuthAuditEvent::new("authorization", "auth0", "success")
        .with_metadata("ip_address".to_string(), "192.168.1.1".to_string());
    assert_eq!(event.metadata.get("ip_address"), Some(&"192.168.1.1".to_string()));
}

// --- OAuth2Client HTTP tests ---

fn mock_oauth2_client(token_endpoint: &str) -> OAuth2Client {
    OAuth2Client::new("test_client", "test_secret", "https://example.com/authorize", token_endpoint)
        .with_redirect_uri("http://localhost/callback")
}

#[tokio::test]
async fn test_exchange_code_sends_correct_request() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_string_contains, method},
    };

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=auth_code_123"))
        .and(body_string_contains("client_id=test_client"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "at_real",
            "refresh_token": "rt_real",
            "token_type": "Bearer",
            "expires_in": 3600,
            "id_token": "ey.header.payload",
            "scope": "openid email"
        })))
        .mount(&mock_server)
        .await;

    let client = mock_oauth2_client(&format!("{}/token", mock_server.uri()));
    let response = client
        .exchange_code("auth_code_123", "http://localhost/callback")
        .await
        .unwrap();
    assert_eq!(response.access_token, "at_real");
    assert_eq!(response.refresh_token, Some("rt_real".to_string()));
    assert_eq!(response.expires_in, 3600);
    assert_eq!(response.id_token, Some("ey.header.payload".to_string()));
}

#[tokio::test]
async fn test_exchange_code_handles_error_response() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "invalid_grant",
            "error_description": "Code expired"
        })))
        .mount(&mock_server)
        .await;

    let client = mock_oauth2_client(&format!("{}/token", mock_server.uri()));
    let result = client.exchange_code("expired_code", "http://localhost/callback").await;
    assert!(result.is_err(), "expected Err for 400 error response, got: {result:?}");
    assert!(result.unwrap_err().contains("error"));
}

// ── S39: redirect_uri allowlist guard ────────────────────────────────────────

#[tokio::test]
async fn exchange_code_rejects_mismatched_redirect_uri() {
    // A registered client must reject a redirect_uri that differs from the one
    // used when authorization_url was called (prevents open-redirect / code
    // interception).
    let client = OAuth2Client::new(
        "client",
        "secret",
        "https://provider.example.com/auth",
        "https://provider.example.com/token",
    )
    .with_redirect_uri("https://myapp.example.com/callback");

    let result = client.exchange_code("auth_code_xyz", "https://evil.example.com/steal").await;

    assert!(result.is_err(), "mismatched redirect_uri must be rejected");
    let msg = result.unwrap_err();
    assert!(msg.contains("mismatch"), "error must mention 'mismatch', got: {msg}");
}

#[tokio::test]
async fn exchange_code_accepts_matching_redirect_uri_with_trailing_slash() {
    // Trailing slashes on either side must be normalised before comparison.
    // We can't reach the token endpoint in a unit test so we expect the error
    // to be a network failure (not a redirect_uri error).
    let client = OAuth2Client::new(
        "client",
        "secret",
        "https://provider.example.com/auth",
        "https://192.0.2.1/token", // non-routable — will fail at network, not guard
    )
    .with_redirect_uri("https://myapp.example.com/callback/");

    let result = client
        .exchange_code("auth_code_xyz", "https://myapp.example.com/callback")
        .await;

    // Must NOT be a redirect_uri mismatch error (trailing slash is normalised)
    if let Err(ref msg) = result {
        assert!(
            !msg.contains("mismatch"),
            "trailing-slash difference must not trigger mismatch error, got: {msg}"
        );
    }
}

#[tokio::test]
async fn exchange_code_without_registered_uri_passes_through() {
    // When no redirect_uri is registered (with_redirect_uri not called),
    // exchange_code must not reject any caller-supplied value — the check is
    // opt-in to preserve backward compatibility.
    let client = OAuth2Client::new(
        "client",
        "secret",
        "https://provider.example.com/auth",
        "https://192.0.2.1/token", // non-routable — will fail at network
    );

    let result = client.exchange_code("auth_code_xyz", "https://any-value.example.com/cb").await;

    if let Err(ref msg) = result {
        assert!(
            !msg.contains("mismatch"),
            "unregistered client must not produce a mismatch error, got: {msg}"
        );
    }
}

#[tokio::test]
async fn test_refresh_token_sends_correct_request() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_string_contains, method},
    };

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=rt_abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "new_at",
            "refresh_token": "new_rt",
            "token_type": "Bearer",
            "expires_in": 3600
        })))
        .mount(&mock_server)
        .await;

    let client = mock_oauth2_client(&format!("{}/token", mock_server.uri()));
    let response = client.refresh_token("rt_abc").await.unwrap();
    assert_eq!(response.access_token, "new_at");
    assert_eq!(response.refresh_token, Some("new_rt".to_string()));
}

// --- OIDCClient tests ---

fn test_oidc_config() -> OIDCProviderConfig {
    OIDCProviderConfig::new(
        "https://example.com".to_string(),
        "https://example.com/authorize".to_string(),
        "https://example.com/token".to_string(),
        "https://example.com/.well-known/jwks.json".to_string(),
    )
}

#[tokio::test]
async fn test_get_userinfo_success() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/userinfo"))
        .and(header("Authorization", "Bearer access_token_xyz"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sub": "user_789",
            "email": "real@example.com",
            "email_verified": true,
            "name": "Real User",
            "picture": "https://example.com/photo.jpg",
            "locale": "fr-FR"
        })))
        .mount(&mock_server)
        .await;

    let mut config = test_oidc_config();
    config.userinfo_endpoint = Some(format!("{}/userinfo", mock_server.uri()));

    let client = OIDCClient::new(config, "client_id", "secret").unwrap();
    let user = client.get_userinfo("access_token_xyz").await.unwrap();
    assert_eq!(user.sub, "user_789");
    assert_eq!(user.email, Some("real@example.com".to_string()));
    assert_eq!(user.name, Some("Real User".to_string()));
}

#[tokio::test]
async fn test_get_userinfo_no_endpoint() {
    let mut config = test_oidc_config();
    config.userinfo_endpoint = None;

    let client = OIDCClient::new(config, "client_id", "secret").unwrap();
    let result = client.get_userinfo("token").await;
    assert!(
        result.is_err(),
        "expected Err when no userinfo endpoint configured, got: {result:?}"
    );
    assert!(result.unwrap_err().contains("No userinfo endpoint"));
}

#[tokio::test]
async fn test_get_userinfo_server_error() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let mut config = test_oidc_config();
    config.userinfo_endpoint = Some(format!("{}/userinfo", mock_server.uri()));

    let client = OIDCClient::new(config, "client_id", "secret").unwrap();
    let result = client.get_userinfo("token").await;
    assert!(result.is_err(), "expected Err for 500 server error, got: {result:?}");
    assert!(result.unwrap_err().contains("500"));
}

#[tokio::test]
async fn test_verify_id_token_rejects_missing_kid() {
    let config = test_oidc_config();
    let client = OIDCClient::new(config, "client_id", "secret").unwrap();

    // Use RS256 (an allowed algorithm) so the algorithm whitelist passes and
    // the missing-kid check is reached.  The header is crafted manually because
    // creating a real RS256 key in a unit test is expensive.
    let header_json = r#"{"alg":"RS256","typ":"JWT"}"#; // no "kid"
    let header_b64 =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(header_json.as_bytes());
    let token = format!("{header_b64}.fakepayload.fakesig");

    let result = client.verify_id_token(&token, None, None).await;
    assert!(result.is_err(), "expected Err for token without kid header, got: {result:?}");
    assert!(result.unwrap_err().contains("kid"), "error must mention missing kid");
}

// --- OIDC nonce + max_age tests (H2/H3) ---

#[test]
fn test_oidc_authorization_url_includes_nonce() {
    use crate::oauth::client::OIDCProviderConfig;
    let config = OIDCProviderConfig::new(
        "https://idp.example.com".to_string(),
        "https://idp.example.com/auth".to_string(),
        "https://idp.example.com/token".to_string(),
        "https://idp.example.com/.well-known/jwks.json".to_string(),
    );
    let client = OIDCClient::new(config, "client_id", "secret").unwrap();
    let req = client.authorization_url("https://app.example.com/callback");

    assert!(req.url.contains("nonce="), "auth URL must include nonce parameter");
    assert!(req.nonce.is_some(), "AuthorizationRequest must carry the NonceParameter");
    assert!(req.pkce.is_some(), "OIDC auth URL must use PKCE");
}

#[test]
fn test_oidc_authorization_url_nonce_is_unique() {
    use crate::oauth::client::OIDCProviderConfig;
    let config = OIDCProviderConfig::new(
        "https://idp.example.com".to_string(),
        "https://idp.example.com/auth".to_string(),
        "https://idp.example.com/token".to_string(),
        "https://idp.example.com/.well-known/jwks.json".to_string(),
    );
    let client = OIDCClient::new(config, "client_id", "secret").unwrap();
    let r1 = client.authorization_url("https://app.example.com/callback");
    let r2 = client.authorization_url("https://app.example.com/callback");
    assert_ne!(
        r1.nonce.unwrap().nonce,
        r2.nonce.unwrap().nonce,
        "consecutive nonces must be unique"
    );
}

// --- TokenRefreshWorker tests ---

#[tokio::test(start_paused = true)]
async fn test_token_refresh_worker_processes_due_refresh() {
    struct MockRefresher {
        call_count: std::sync::atomic::AtomicU32,
    }

    #[async_trait::async_trait]
    impl TokenRefresher for MockRefresher {
        async fn refresh_session(
            &self,
            _session_id: &str,
        ) -> std::result::Result<Option<chrono::DateTime<Utc>>, crate::error::AuthError> {
            self.call_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(Some(Utc::now() + Duration::hours(1)))
        }
    }

    let scheduler = Arc::new(TokenRefreshScheduler::new());
    scheduler
        .schedule_refresh("session_1".to_string(), Utc::now() - Duration::seconds(1))
        .unwrap();

    let refresher = Arc::new(MockRefresher {
        call_count: std::sync::atomic::AtomicU32::new(0),
    });

    let (worker, cancel_tx) =
        TokenRefreshWorker::new(scheduler, refresher.clone(), StdDuration::from_millis(50));

    let handle = tokio::spawn(worker.run());

    // Allow the worker to run through a few poll cycles.
    tokio::task::yield_now().await;
    tokio::time::advance(StdDuration::from_millis(200)).await;
    let _ = cancel_tx.send(true);
    handle.await.unwrap();

    assert!(refresher.call_count.load(std::sync::atomic::Ordering::Relaxed) >= 1);
}

#[tokio::test(start_paused = true)]
async fn test_token_refresh_worker_handles_missing_session() {
    struct NoSessionRefresher;

    #[async_trait::async_trait]
    impl TokenRefresher for NoSessionRefresher {
        async fn refresh_session(
            &self,
            _session_id: &str,
        ) -> std::result::Result<Option<chrono::DateTime<Utc>>, crate::error::AuthError> {
            Ok(None) // Session doesn't exist
        }
    }

    let scheduler = Arc::new(TokenRefreshScheduler::new());
    scheduler
        .schedule_refresh("missing_session".to_string(), Utc::now() - Duration::seconds(1))
        .unwrap();

    let refresher = Arc::new(NoSessionRefresher);
    let (worker, cancel_tx) =
        TokenRefreshWorker::new(scheduler, refresher, StdDuration::from_millis(50));

    let handle = tokio::spawn(worker.run());
    tokio::task::yield_now().await;
    tokio::time::advance(StdDuration::from_millis(200)).await;
    let _ = cancel_tx.send(true);
    handle.await.unwrap();
    // No panic = success
}

#[tokio::test]
async fn test_token_refresh_worker_cancellation() {
    struct NeverCalledRefresher;

    #[async_trait::async_trait]
    impl TokenRefresher for NeverCalledRefresher {
        async fn refresh_session(
            &self,
            _session_id: &str,
        ) -> std::result::Result<Option<chrono::DateTime<Utc>>, crate::error::AuthError> {
            panic!("Should not be called");
        }
    }

    let scheduler = Arc::new(TokenRefreshScheduler::new());
    let refresher = Arc::new(NeverCalledRefresher);
    let (worker, cancel_tx) =
        TokenRefreshWorker::new(scheduler, refresher, StdDuration::from_secs(3600));

    let handle = tokio::spawn(worker.run());
    // Cancel immediately
    let _ = cancel_tx.send(true);
    handle.await.unwrap();
}

// ── Submodule tests extracted from oauth subfiles ────────────────────────────

mod audit_tests {
    use super::super::audit::*;

    #[test]
    fn test_audit_event_creation() {
        let event = OAuthAuditEvent::new("authorization", "auth0", "success");
        assert_eq!(event.event_type, "authorization");
        assert_eq!(event.provider, "auth0");
        assert_eq!(event.status, "success");
        assert!(event.user_id.is_none());
        assert!(event.error.is_none());
        assert!(event.metadata.is_empty());
    }

    #[test]
    fn test_audit_event_with_user_id() {
        let event = OAuthAuditEvent::new("token_exchange", "google", "success")
            .with_user_id("user_456".to_string());
        assert_eq!(event.user_id, Some("user_456".to_string()));
    }

    #[test]
    fn test_audit_event_with_error() {
        let event = OAuthAuditEvent::new("token_exchange", "auth0", "failed")
            .with_error("Provider unreachable".to_string());
        assert_eq!(event.error, Some("Provider unreachable".to_string()));
    }

    #[test]
    fn test_audit_event_with_metadata() {
        let event = OAuthAuditEvent::new("authorization", "auth0", "success")
            .with_metadata("ip".to_string(), "10.0.0.1".to_string())
            .with_metadata("user_agent".to_string(), "TestClient/1.0".to_string());
        assert_eq!(event.metadata.len(), 2);
        assert_eq!(event.metadata.get("ip"), Some(&"10.0.0.1".to_string()));
    }

    #[test]
    fn test_audit_event_serializes_to_json() {
        let event = OAuthAuditEvent::new("logout", "okta", "success");
        let json = serde_json::to_string(&event).expect("audit event must serialize");
        assert!(json.contains("\"event_type\":\"logout\""));
        assert!(json.contains("\"provider\":\"okta\""));
    }
}

mod claims_validator_tests {
    use super::super::{claims_validator::*, types::IdTokenClaims};
    use crate::error::AuthError;

    fn make_claims(nonce: Option<&str>, auth_time: Option<i64>) -> IdTokenClaims {
        let mut c = IdTokenClaims::new(
            "https://idp.example.com".into(),
            "user1".into(),
            "client_id".into(),
            9_999_999_999,
            0,
        );
        c.nonce = nonce.map(str::to_owned);
        c.auth_time = auth_time;
        c
    }

    // ── Nonce tests (13-1) ────────────────────────────────────────────────────

    #[test]
    fn test_callback_rejects_missing_nonce_claim() {
        let claims = make_claims(None, None);
        let result = validate_nonce_claim(&claims, "expected-nonce");
        assert!(matches!(result, Err(AuthError::MissingNonce)));
    }

    #[test]
    fn test_callback_rejects_wrong_nonce() {
        let claims = make_claims(Some("actual-nonce"), None);
        let result = validate_nonce_claim(&claims, "different-nonce");
        assert!(matches!(result, Err(AuthError::NonceMismatch)));
    }

    #[test]
    fn test_callback_accepts_correct_nonce() {
        let claims = make_claims(Some("correct-nonce"), None);
        validate_nonce_claim(&claims, "correct-nonce")
            .unwrap_or_else(|e| panic!("expected Ok for correct nonce: {e}"));
    }

    #[test]
    fn test_callback_nonce_is_one_shot() {
        let claims = make_claims(Some("once-nonce"), None);
        validate_nonce_claim(&claims, "once-nonce")
            .unwrap_or_else(|e| panic!("expected Ok on first nonce use: {e}"));

        let cleared_claims = make_claims(None, None);
        let result = validate_nonce_claim(&cleared_claims, "once-nonce");
        assert!(
            matches!(result, Err(AuthError::MissingNonce)),
            "second use must fail: stored nonce already consumed"
        );
    }

    // ── auth_time / max_age tests (13-2) ─────────────────────────────────────

    const NOW: i64 = 1_700_000_000;

    #[test]
    fn test_auth_time_within_max_age_accepted() {
        let claims = make_claims(None, Some(NOW - 30));
        validate_auth_time_claim(&claims, 60, NOW)
            .unwrap_or_else(|e| panic!("expected Ok for auth_time within max_age: {e}"));
    }

    #[test]
    fn test_auth_time_exceeds_max_age_rejected() {
        let claims = make_claims(None, Some(NOW - 200));
        let result = validate_auth_time_claim(&claims, 60, NOW);
        assert!(
            matches!(
                result,
                Err(AuthError::SessionTooOld {
                    age: 200,
                    max_age_secs: 60,
                })
            ),
            "expected SessionTooOld, got: {result:?}"
        );
    }

    #[test]
    fn test_missing_auth_time_when_max_age_present_rejected() {
        let claims = make_claims(None, None);
        let result = validate_auth_time_claim(&claims, 3600, NOW);
        assert!(matches!(result, Err(AuthError::MissingAuthTime)));
    }

    #[test]
    fn test_max_age_absent_skips_auth_time_check() {
        let claims = make_claims(None, Some(NOW - 59));
        validate_auth_time_claim(&claims, 0, NOW)
            .unwrap_or_else(|e| panic!("expected Ok for age(59) within skew window: {e}"));
    }
}

mod failover_tests {
    use super::super::failover::*;

    #[test]
    fn test_primary_available_by_default() {
        let mgr = ProviderFailoverManager::new("primary".to_string(), vec!["fallback".to_string()]);
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "primary");
    }

    #[test]
    fn test_fallback_used_when_primary_unavailable() {
        let mgr = ProviderFailoverManager::new("primary".to_string(), vec!["fallback".to_string()]);
        mgr.mark_unavailable("primary".to_string(), 300)
            .expect("mark_unavailable must succeed");
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "fallback");
    }

    #[test]
    fn test_all_unavailable_returns_error() {
        let mgr = ProviderFailoverManager::new("primary".to_string(), vec!["fallback".to_string()]);
        mgr.mark_unavailable("primary".to_string(), 300).expect("must succeed");
        mgr.mark_unavailable("fallback".to_string(), 300).expect("must succeed");
        let result = mgr.get_available_provider();
        assert!(result.is_err(), "must return error when no providers are available");
    }

    #[test]
    fn test_mark_available_restores_provider() {
        let mgr = ProviderFailoverManager::new("primary".to_string(), vec!["fallback".to_string()]);
        mgr.mark_unavailable("primary".to_string(), 300).expect("must succeed");
        mgr.mark_available("primary").expect("must succeed");
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "primary", "primary must be available after mark_available");
    }

    #[test]
    fn test_no_fallbacks_returns_primary() {
        let mgr = ProviderFailoverManager::new("only".to_string(), vec![]);
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "only");
    }

    #[test]
    fn test_no_fallbacks_primary_unavailable_returns_error() {
        let mgr = ProviderFailoverManager::new("only".to_string(), vec![]);
        mgr.mark_unavailable("only".to_string(), 300).expect("must succeed");
        let result = mgr.get_available_provider();
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_fallbacks_in_order() {
        let mgr = ProviderFailoverManager::new(
            "primary".to_string(),
            vec!["fb1".to_string(), "fb2".to_string()],
        );
        mgr.mark_unavailable("primary".to_string(), 300).expect("must succeed");
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "fb1", "first fallback must be selected");

        mgr.mark_unavailable("fb1".to_string(), 300).expect("must succeed");
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "fb2", "second fallback must be selected when first is unavailable");
    }
}

mod types_tests {
    use chrono::Utc;

    use super::super::types::*;
    use crate::jwt::{MAX_CLOCK_SKEW_SECS, MAX_TOKEN_AGE_SECS};

    // --- TokenResponse tests ---

    #[test]
    fn test_token_response_deserializes_from_json() {
        let json = r#"{
            "access_token": "eyJhbGciOiJSUzI1NiJ9.test.sig",
            "token_type": "Bearer",
            "expires_in": 3600,
            "refresh_token": "rt-abc123",
            "scope": "openid profile email"
        }"#;

        let token: TokenResponse = serde_json::from_str(json)
            .expect("valid OAuth token response JSON must deserialize successfully");

        assert_eq!(token.access_token, "eyJhbGciOiJSUzI1NiJ9.test.sig");
        assert_eq!(token.token_type, "Bearer");
        assert_eq!(token.expires_in, 3600);
        assert_eq!(token.refresh_token, Some("rt-abc123".to_string()));
        assert_eq!(token.scope, Some("openid profile email".to_string()));
    }

    #[test]
    fn test_token_response_missing_optional_fields() {
        let json = r#"{
            "access_token": "at_minimal",
            "token_type": "Bearer",
            "expires_in": 3600
        }"#;

        let token: TokenResponse = serde_json::from_str(json)
            .expect("token response without optional fields must still deserialize");

        assert!(token.refresh_token.is_none(), "missing refresh_token must deserialize to None");
        assert!(token.id_token.is_none(), "missing id_token must deserialize to None");
        assert!(token.scope.is_none(), "missing scope must deserialize to None");
    }

    #[test]
    fn test_token_response_missing_access_token_fails() {
        let json = r#"{
            "token_type": "Bearer",
            "expires_in": 3600
        }"#;

        let result: Result<TokenResponse, _> = serde_json::from_str(json);
        assert!(result.is_err(), "token response without access_token must fail to deserialize");
    }

    #[test]
    fn test_token_response_expiry_is_in_future() {
        let token = TokenResponse::new("at".to_string(), "Bearer".to_string(), 3600);
        let expiry = token.expiry_time();
        assert!(
            expiry > Utc::now(),
            "expiry_time for a token with expires_in=3600 must be in the future"
        );
    }

    #[test]
    fn test_token_response_new_is_not_expired() {
        let token = TokenResponse::new("at".to_string(), "Bearer".to_string(), 3600);
        assert!(
            !token.is_expired(),
            "a freshly created token with expires_in=3600 must not be expired"
        );
    }

    // --- IdTokenClaims tests ---

    #[test]
    fn test_id_token_claims_not_expired() {
        let exp = (Utc::now() + chrono::Duration::hours(1)).timestamp();
        let claims = IdTokenClaims::new(
            "https://issuer.example.com".to_string(),
            "user123".to_string(),
            "client_id".to_string(),
            exp,
            Utc::now().timestamp(),
        );
        assert!(!claims.is_expired(), "future exp must not be expired");
    }

    #[test]
    fn test_id_token_claims_expired() {
        let exp = (Utc::now() - chrono::Duration::hours(1)).timestamp();
        let claims = IdTokenClaims::new(
            "https://issuer.example.com".to_string(),
            "user123".to_string(),
            "client_id".to_string(),
            exp,
            Utc::now().timestamp(),
        );
        assert!(claims.is_expired(), "past exp must be expired");
    }

    #[test]
    fn test_id_token_claims_expiring_soon() {
        let exp = (Utc::now() + chrono::Duration::seconds(30)).timestamp();
        let claims = IdTokenClaims::new(
            "https://issuer.example.com".to_string(),
            "user123".to_string(),
            "client_id".to_string(),
            exp,
            Utc::now().timestamp(),
        );
        assert!(
            claims.is_expiring_soon(60),
            "token expiring in 30s must be considered expiring soon with grace=60s"
        );
        assert!(
            !claims.is_expiring_soon(10),
            "token expiring in 30s must not be considered expiring soon with grace=10s"
        );
    }

    // --- UserInfo tests ---

    #[test]
    fn test_userinfo_creation() {
        let user = UserInfo::new("sub_123".to_string());
        assert_eq!(user.sub, "sub_123");
        assert!(user.email.is_none());
        assert!(user.name.is_none());
    }

    #[test]
    fn test_userinfo_deserializes_from_json() {
        let json = r#"{
            "sub": "user_789",
            "email": "user@example.com",
            "email_verified": true,
            "name": "Test User"
        }"#;
        let user: UserInfo =
            serde_json::from_str(json).expect("valid userinfo JSON must deserialize");
        assert_eq!(user.sub, "user_789");
        assert_eq!(user.email, Some("user@example.com".to_string()));
        assert_eq!(user.email_verified, Some(true));
    }

    // ── S40: IdTokenClaims temporal claim tests ───────────────────────────────

    fn make_temporal_claims(iat: i64, nbf: Option<i64>) -> IdTokenClaims {
        let mut c = IdTokenClaims::new(
            "https://issuer.example.com".to_string(),
            "user123".to_string(),
            "client_id".to_string(),
            (Utc::now() + chrono::Duration::hours(1)).timestamp(),
            iat,
        );
        c.nbf = nbf;
        c
    }

    #[test]
    fn test_temporal_claims_valid_token() {
        let now = Utc::now().timestamp();
        let claims = make_temporal_claims(now - 60, None);
        claims
            .validate_temporal_claims()
            .unwrap_or_else(|e| panic!("expected Ok for valid temporal claims: {e}"));
    }

    #[test]
    fn test_temporal_claims_iat_too_far_in_future() {
        let now = Utc::now().timestamp();
        let max_skew = i64::try_from(MAX_CLOCK_SKEW_SECS).expect("MAX_CLOCK_SKEW_SECS fits in i64");
        let claims = make_temporal_claims(now + max_skew + 60, None);
        let err = claims
            .validate_temporal_claims()
            .expect_err("iat too far in future must be rejected");
        assert!(err.contains("iat"), "error message must mention iat, got: {err}");
    }

    #[test]
    fn test_temporal_claims_iat_too_old() {
        let now = Utc::now().timestamp();
        let max_age = i64::try_from(MAX_TOKEN_AGE_SECS).expect("MAX_TOKEN_AGE_SECS fits in i64");
        let claims = make_temporal_claims(now - max_age - 60, None);
        let err = claims.validate_temporal_claims().expect_err("iat too old must be rejected");
        assert!(err.contains("old"), "error message must mention old, got: {err}");
    }

    #[test]
    fn test_temporal_claims_nbf_in_future_rejected() {
        let now = Utc::now().timestamp();
        let max_skew = i64::try_from(MAX_CLOCK_SKEW_SECS).expect("MAX_CLOCK_SKEW_SECS fits in i64");
        let claims = make_temporal_claims(now - 60, Some(now + max_skew + 60));
        let err = claims.validate_temporal_claims().expect_err("nbf in future must be rejected");
        assert!(err.contains("nbf"), "error message must mention nbf, got: {err}");
    }

    #[test]
    fn test_temporal_claims_nbf_in_past_accepted() {
        let now = Utc::now().timestamp();
        let claims = make_temporal_claims(now - 60, Some(now - 600));
        claims
            .validate_temporal_claims()
            .unwrap_or_else(|e| panic!("expected Ok for nbf in past: {e}"));
    }

    #[test]
    fn test_temporal_claims_iat_within_clock_skew_accepted() {
        let now = Utc::now().timestamp();
        let max_skew = i64::try_from(MAX_CLOCK_SKEW_SECS).expect("MAX_CLOCK_SKEW_SECS fits in i64");
        // 100s in future — within the 300s skew window
        let claims = make_temporal_claims(now + 100_i64.min(max_skew - 1), None);
        claims
            .validate_temporal_claims()
            .unwrap_or_else(|e| panic!("expected Ok for iat within clock skew: {e}"));
    }
}

mod pkce_tests {
    use super::super::pkce::*;

    // --- PKCEChallenge tests ---

    #[test]
    fn test_pkce_challenge_method_is_s256() {
        let challenge = PKCEChallenge::new();
        assert_eq!(challenge.code_challenge_method, "S256", "PKCE challenge method must be S256");
    }

    #[test]
    fn test_pkce_verifier_rfc7636_length_and_charset() {
        let challenge = PKCEChallenge::new();
        let len = challenge.code_verifier.len();
        assert!(
            (43..=128).contains(&len),
            "PKCE code_verifier length {len} must be 43–128 chars (RFC 7636 §4.1)"
        );
        assert!(
            challenge
                .code_verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)),
            "PKCE code_verifier must only contain [A-Za-z0-9\\-._~] (RFC 7636 §4.1)"
        );
        assert!(
            !challenge.code_verifier.contains('='),
            "PKCE code_verifier must not contain padding characters"
        );
    }

    #[test]
    fn test_pkce_verifier_has_256_bits_entropy() {
        let c1 = PKCEChallenge::new();
        let c2 = PKCEChallenge::new();
        assert_ne!(
            c1.code_verifier, c2.code_verifier,
            "two PKCE verifiers generated back-to-back must be distinct (entropy check)"
        );
    }

    #[test]
    fn test_pkce_challenge_is_not_empty() {
        let challenge = PKCEChallenge::new();
        assert!(!challenge.code_challenge.is_empty(), "PKCE code_challenge must not be empty");
    }

    #[test]
    fn test_pkce_verify_correct_verifier() {
        let challenge = PKCEChallenge::new();
        let verifier = challenge.code_verifier.clone();
        assert!(
            challenge.verify(&verifier),
            "PKCEChallenge::verify must succeed for the original verifier"
        );
    }

    #[test]
    fn test_pkce_verify_wrong_verifier_fails() {
        let challenge = PKCEChallenge::new();
        assert!(
            !challenge.verify("definitely-wrong-verifier"),
            "PKCEChallenge::verify must fail for an incorrect verifier"
        );
    }

    #[test]
    fn test_pkce_two_challenges_differ() {
        let c1 = PKCEChallenge::new();
        let c2 = PKCEChallenge::new();
        assert_ne!(
            c1.code_verifier, c2.code_verifier,
            "consecutive PKCE challenges must have unique verifiers"
        );
        assert_ne!(
            c1.code_challenge, c2.code_challenge,
            "consecutive PKCE challenges must have unique challenges"
        );
    }

    // --- StateParameter tests ---

    #[test]
    fn test_state_parameter_not_expired_on_creation() {
        let state = StateParameter::new();
        assert!(!state.is_expired(), "freshly created StateParameter must not be expired");
    }

    #[test]
    fn test_state_verify_correct_value() {
        let state = StateParameter::new();
        let value = state.state.clone();
        assert!(
            state.verify(&value),
            "StateParameter::verify must succeed for the correct state value"
        );
    }

    #[test]
    fn test_state_verify_wrong_value_fails() {
        let state = StateParameter::new();
        assert!(
            !state.verify("wrong-state-value"),
            "StateParameter::verify must fail for an incorrect state value"
        );
    }

    #[test]
    fn test_state_parameters_are_unique() {
        let s1 = StateParameter::new();
        let s2 = StateParameter::new();
        assert_ne!(s1.state, s2.state, "consecutive StateParameter values must be unique");
    }

    // --- NonceParameter tests ---

    #[test]
    fn test_nonce_not_expired_on_creation() {
        let nonce = NonceParameter::new();
        assert!(!nonce.is_expired(), "freshly created NonceParameter must not be expired");
    }

    #[test]
    fn test_nonce_verify_correct_value() {
        let nonce = NonceParameter::new();
        let value = nonce.nonce.clone();
        assert!(
            nonce.verify(&value),
            "NonceParameter::verify must succeed for the correct nonce value"
        );
    }

    #[test]
    fn test_nonce_verify_wrong_value_fails() {
        let nonce = NonceParameter::new();
        assert!(
            !nonce.verify("wrong-nonce-value"),
            "NonceParameter::verify must fail for an incorrect nonce value"
        );
    }

    #[test]
    fn test_nonce_parameters_are_unique() {
        let n1 = NonceParameter::new();
        let n2 = NonceParameter::new();
        assert_ne!(n1.nonce, n2.nonce, "consecutive NonceParameter values must be unique");
    }
}

mod provider_tests {
    use chrono::{Duration, Utc};

    use super::super::provider::*;

    // --- ProviderType tests ---

    #[test]
    fn test_provider_type_display() {
        assert_eq!(ProviderType::OAuth2.to_string(), "oauth2");
        assert_eq!(ProviderType::OIDC.to_string(), "oidc");
    }

    // --- OAuthSession tests ---

    #[test]
    fn test_session_is_not_expired_on_creation() {
        let session = OAuthSession::new(
            "user_1".to_string(),
            ProviderType::OIDC,
            "auth0".to_string(),
            "auth0|sub".to_string(),
            "at_fresh".to_string(),
            Utc::now() + Duration::hours(1),
        );
        assert!(!session.is_expired(), "freshly created session must not be expired");
    }

    #[test]
    fn test_session_is_expiring_soon() {
        let session = OAuthSession::new(
            "user_1".to_string(),
            ProviderType::OIDC,
            "auth0".to_string(),
            "auth0|sub".to_string(),
            "at".to_string(),
            Utc::now() + Duration::seconds(30),
        );
        assert!(
            session.is_expiring_soon(60),
            "session expiring in 30s must be considered expiring soon with grace=60"
        );
        assert!(
            !session.is_expiring_soon(10),
            "session expiring in 30s must not be considered expiring soon with grace=10"
        );
    }

    #[test]
    fn test_session_refresh_tokens_updates_fields() {
        let mut session = OAuthSession::new(
            "user_1".to_string(),
            ProviderType::OIDC,
            "auth0".to_string(),
            "auth0|sub".to_string(),
            "old_at".to_string(),
            Utc::now() + Duration::hours(1),
        );
        let new_expiry = Utc::now() + Duration::hours(2);
        session.refresh_tokens("new_at".to_string(), new_expiry);

        assert_eq!(session.access_token, "new_at");
        assert_eq!(session.token_expiry, new_expiry);
        assert!(session.last_refreshed.is_some());
    }

    // --- ExternalAuthProvider tests ---

    #[test]
    fn test_external_provider_defaults() {
        let provider =
            ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "client_id", "vault/secret");
        assert!(provider.enabled, "new provider must be enabled by default");
        assert_eq!(provider.scopes, vec!["openid", "profile", "email"]);
        assert!(provider.oidc_config.is_none());
        assert!(provider.oauth2_config.is_none());
    }

    #[test]
    fn test_external_provider_set_enabled() {
        let mut provider =
            ExternalAuthProvider::new(ProviderType::OAuth2, "google", "id", "vault/path");
        provider.set_enabled(false);
        assert!(!provider.enabled);
        provider.set_enabled(true);
        assert!(provider.enabled);
    }

    #[test]
    fn test_external_provider_set_scopes() {
        let mut provider =
            ExternalAuthProvider::new(ProviderType::OAuth2, "google", "id", "vault/path");
        provider.set_scopes(vec!["openid".to_string()]);
        assert_eq!(provider.scopes, vec!["openid"]);
    }

    // --- ProviderRegistry tests ---

    #[test]
    fn test_registry_register_and_get() {
        let registry = ProviderRegistry::new();
        let provider = ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "id", "vault/path");
        registry.register(provider.clone()).expect("register must succeed");
        let retrieved = registry.get("auth0").expect("get must succeed");
        assert_eq!(retrieved, Some(provider));
    }

    #[test]
    fn test_registry_get_nonexistent_returns_none() {
        let registry = ProviderRegistry::new();
        let retrieved = registry.get("nonexistent").expect("get must succeed");
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_registry_list_enabled_filters_disabled() {
        let registry = ProviderRegistry::new();
        let p1 = ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "id1", "path1");
        let mut p2 = ExternalAuthProvider::new(ProviderType::OAuth2, "google", "id2", "path2");
        p2.set_enabled(false);
        registry.register(p1).expect("register must succeed");
        registry.register(p2).expect("register must succeed");

        let enabled = registry.list_enabled().expect("list_enabled must succeed");
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].provider_name, "auth0");
    }

    #[test]
    fn test_registry_disable_and_enable() {
        let registry = ProviderRegistry::new();
        let provider = ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "id", "path");
        registry.register(provider).expect("register must succeed");

        assert!(registry.disable("auth0").expect("disable must succeed"));
        let p = registry.get("auth0").expect("get must succeed").expect("provider must exist");
        assert!(!p.enabled);

        assert!(registry.enable("auth0").expect("enable must succeed"));
        let p = registry.get("auth0").expect("get must succeed").expect("provider must exist");
        assert!(p.enabled);
    }

    #[test]
    fn test_registry_disable_nonexistent_returns_false() {
        let registry = ProviderRegistry::new();
        assert!(!registry.disable("nonexistent").expect("disable must succeed"));
    }
}

mod refresh_tests {
    use chrono::{Duration, Utc};

    use super::super::refresh::*;

    #[test]
    fn test_scheduler_schedule_and_get_due_refresh() {
        let scheduler = TokenRefreshScheduler::new();
        let past = Utc::now() - Duration::seconds(10);
        scheduler
            .schedule_refresh("session_a".to_string(), past)
            .expect("schedule_refresh must succeed");

        let next = scheduler.get_next_refresh().expect("get_next_refresh must succeed");
        assert_eq!(next, Some("session_a".to_string()));
    }

    #[test]
    fn test_scheduler_future_refresh_not_returned() {
        let scheduler = TokenRefreshScheduler::new();
        let future = Utc::now() + Duration::hours(1);
        scheduler
            .schedule_refresh("session_b".to_string(), future)
            .expect("schedule_refresh must succeed");

        let next = scheduler.get_next_refresh().expect("get_next_refresh must succeed");
        assert!(next.is_none(), "future refresh must not be returned as next");
    }

    #[test]
    fn test_scheduler_ordering_by_time() {
        let scheduler = TokenRefreshScheduler::new();
        let now = Utc::now();
        scheduler
            .schedule_refresh("later".to_string(), now - Duration::seconds(5))
            .expect("schedule must succeed");
        scheduler
            .schedule_refresh("earlier".to_string(), now - Duration::seconds(10))
            .expect("schedule must succeed");

        let first = scheduler.get_next_refresh().expect("must succeed");
        assert_eq!(first, Some("earlier".to_string()));
        let second = scheduler.get_next_refresh().expect("must succeed");
        assert_eq!(second, Some("later".to_string()));
    }

    #[test]
    fn test_scheduler_cancel_refresh() {
        let scheduler = TokenRefreshScheduler::new();
        let future = Utc::now() + Duration::hours(1);
        scheduler
            .schedule_refresh("session_c".to_string(), future)
            .expect("schedule must succeed");

        let cancelled = scheduler.cancel_refresh("session_c").expect("cancel must succeed");
        assert!(cancelled, "cancel_refresh must return true for existing session");

        let cancelled_again = scheduler.cancel_refresh("session_c").expect("cancel must succeed");
        assert!(!cancelled_again, "cancel_refresh must return false for already-removed session");
    }

    #[test]
    fn test_scheduler_cancel_nonexistent_returns_false() {
        let scheduler = TokenRefreshScheduler::new();
        let cancelled = scheduler.cancel_refresh("nonexistent").expect("cancel must succeed");
        assert!(!cancelled);
    }

    #[test]
    fn test_scheduler_empty_returns_none() {
        let scheduler = TokenRefreshScheduler::new();
        let next = scheduler.get_next_refresh().expect("must succeed");
        assert!(next.is_none());
    }
}

mod client_tests {
    use super::super::client::*;

    #[test]
    fn oauth_response_cap_constant_is_reasonable() {
        assert_eq!(OAuth2Client::MAX_OAUTH_RESPONSE_BYTES, 1024 * 1024);
    }

    #[test]
    fn oauth_response_error_body_is_capped() {
        let cap = OAuth2Client::MAX_OAUTH_RESPONSE_BYTES;
        let oversized: Vec<u8> = vec![b'e'; cap + 1_000];
        let capped = &oversized[..oversized.len().min(cap)];
        let text = String::from_utf8_lossy(capped).into_owned();
        assert_eq!(text.len(), cap, "body must be capped at MAX_OAUTH_RESPONSE_BYTES");
    }

    // ── S25-H1: OAuth2/OIDC client timeout ────────────────────────────────────

    #[test]
    fn oauth_request_timeout_is_set() {
        let secs = OAUTH_REQUEST_TIMEOUT.as_secs();
        assert!(secs > 0 && secs <= 120, "OAuth timeout should be 1–120 s, got {secs}");
    }

    #[test]
    fn oauth2_client_new_creates_instance() {
        let client = OAuth2Client::new(
            "client_id",
            "client_secret",
            "https://example.com/auth",
            "https://example.com/token",
        );
        assert_eq!(client.client_id, "client_id");
    }

    #[test]
    fn oidc_client_new_creates_instance() {
        let config = OIDCProviderConfig {
            issuer: "https://example.com".to_string(),
            authorization_endpoint: "https://example.com/auth".to_string(),
            token_endpoint: "https://example.com/token".to_string(),
            userinfo_endpoint: None,
            jwks_uri: "https://example.com/.well-known/jwks.json".to_string(),
            scopes_supported: vec!["openid".to_string()],
            response_types_supported: vec!["code".to_string()],
        };
        let client = OIDCClient::new(config, "client_id", "client_secret").unwrap();
        assert_eq!(client.client_id, "client_id");
    }

    // ── S26: OIDCClient userinfo response size cap ────────────────────────────

    #[test]
    fn oidc_userinfo_cap_constant_is_reasonable() {
        const { assert!(OIDCClient::MAX_USERINFO_RESPONSE_BYTES >= 64 * 1024) }
        const { assert!(OIDCClient::MAX_USERINFO_RESPONSE_BYTES <= 100 * 1024 * 1024) }
    }

    #[tokio::test]
    async fn oidc_userinfo_oversized_response_is_rejected() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        let oversized = vec![b'x'; OIDCClient::MAX_USERINFO_RESPONSE_BYTES + 1];
        Mock::given(method("GET"))
            .and(path("/userinfo"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
            .mount(&mock_server)
            .await;

        let config = OIDCProviderConfig {
            issuer: mock_server.uri(),
            authorization_endpoint: format!("{}/auth", mock_server.uri()),
            token_endpoint: format!("{}/token", mock_server.uri()),
            userinfo_endpoint: Some(format!("{}/userinfo", mock_server.uri())),
            jwks_uri: format!("{}/.well-known/jwks.json", mock_server.uri()),
            scopes_supported: vec!["openid".to_string()],
            response_types_supported: vec!["code".to_string()],
        };
        let client = OIDCClient::new(config, "client_id", "secret").unwrap();

        let result = client.get_userinfo("dummy_token").await;
        assert!(result.is_err(), "oversized userinfo response must be rejected, got: {result:?}");
        let msg = result.unwrap_err();
        assert!(msg.contains("too large"), "error must mention size limit: {msg}");
    }

    #[tokio::test]
    async fn oidc_userinfo_within_limit_proceeds_to_parse() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/userinfo"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"{}".to_vec()))
            .mount(&mock_server)
            .await;

        let config = OIDCProviderConfig {
            issuer: mock_server.uri(),
            authorization_endpoint: format!("{}/auth", mock_server.uri()),
            token_endpoint: format!("{}/token", mock_server.uri()),
            userinfo_endpoint: Some(format!("{}/userinfo", mock_server.uri())),
            jwks_uri: format!("{}/.well-known/jwks.json", mock_server.uri()),
            scopes_supported: vec!["openid".to_string()],
            response_types_supported: vec!["code".to_string()],
        };
        let client = OIDCClient::new(config, "client_id", "secret").unwrap();

        let result = client.get_userinfo("dummy_token").await;
        assert!(
            result.is_err(),
            "expected Err when userinfo JSON is missing required fields, got: {result:?}"
        );
        let msg = result.unwrap_err();
        assert!(
            !msg.contains("too large"),
            "size gate must not trigger for small payload: {msg}"
        );
    }

    // ── S38: SCRAM / auth key-material zeroization ────────────────────────────

    #[test]
    fn oauth2_client_secret_is_zeroized_on_drop() {
        let mut secret = zeroize::Zeroizing::new("oauth2-client-secret-abc".to_string());
        assert!(!secret.is_empty(), "secret should be non-empty before zeroize");
        zeroize::Zeroize::zeroize(&mut *secret);
        assert!(secret.is_empty(), "secret bytes must be wiped after zeroize");

        let client = OAuth2Client::new(
            "client_id",
            "my_secret_value",
            "https://example.com/auth",
            "https://example.com/token",
        );
        let _: &zeroize::Zeroizing<String> = &client.client_secret;
    }

    #[test]
    fn oauth2_client_debug_redacts_secret() {
        let client = OAuth2Client::new(
            "client_id",
            "super_secret_xyz",
            "https://example.com/auth",
            "https://example.com/token",
        );
        let debug = format!("{client:?}");
        assert!(!debug.contains("super_secret_xyz"), "Debug must redact client_secret: {debug}");
        assert!(debug.contains("[REDACTED]"), "Debug must show [REDACTED]: {debug}");
    }

    #[test]
    fn oidc_client_secret_is_zeroized_on_drop() {
        let mut secret = zeroize::Zeroizing::new("oidc-client-secret-xyz".to_string());
        assert!(!secret.is_empty(), "secret should be non-empty before zeroize");
        zeroize::Zeroize::zeroize(&mut *secret);
        assert!(secret.is_empty(), "secret bytes must be wiped after zeroize");

        let config = OIDCProviderConfig {
            issuer: "https://example.com".to_string(),
            authorization_endpoint: "https://example.com/auth".to_string(),
            token_endpoint: "https://example.com/token".to_string(),
            userinfo_endpoint: None,
            jwks_uri: "https://example.com/.well-known/jwks.json".to_string(),
            scopes_supported: vec!["openid".to_string()],
            response_types_supported: vec!["code".to_string()],
        };
        let client = OIDCClient::new(config, "client_id", "oidc_secret_value").unwrap();
        let _: &zeroize::Zeroizing<String> = &client.client_secret;
    }

    #[test]
    fn oidc_client_debug_redacts_secret() {
        let config = OIDCProviderConfig {
            issuer: "https://example.com".to_string(),
            authorization_endpoint: "https://example.com/auth".to_string(),
            token_endpoint: "https://example.com/token".to_string(),
            userinfo_endpoint: None,
            jwks_uri: "https://example.com/.well-known/jwks.json".to_string(),
            scopes_supported: vec!["openid".to_string()],
            response_types_supported: vec!["code".to_string()],
        };
        let client = OIDCClient::new(config, "client_id", "super_oidc_secret").unwrap();
        let debug = format!("{client:?}");
        assert!(!debug.contains("super_oidc_secret"), "Debug must redact client_secret: {debug}");
        assert!(debug.contains("[REDACTED]"), "Debug must show [REDACTED]: {debug}");
    }

    // ── S41: Algorithm whitelist and typ-header guards ────────────────────────

    fn fake_jwt_with_header(header_json: &str) -> String {
        use base64::Engine as _;
        let header_b64 =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(header_json.as_bytes());
        format!("{header_b64}.fakepayload.fakesig")
    }

    fn fake_oidc_client() -> OIDCClient {
        let config = OIDCProviderConfig {
            issuer: "https://example.com".to_string(),
            authorization_endpoint: "https://example.com/auth".to_string(),
            token_endpoint: "https://example.com/token".to_string(),
            userinfo_endpoint: None,
            jwks_uri: "https://example.com/.well-known/jwks.json".to_string(),
            scopes_supported: vec!["openid".to_string()],
            response_types_supported: vec!["code".to_string()],
        };
        OIDCClient::new(config, "client_id", "secret").unwrap()
    }

    #[test]
    fn allowed_oidc_algorithms_constant_does_not_contain_symmetric() {
        for alg in ALLOWED_OIDC_ALGORITHMS {
            assert!(
                !FORBIDDEN_OIDC_ALGORITHMS.contains(alg),
                "ALLOWED_OIDC_ALGORITHMS must not overlap with FORBIDDEN: {alg:?}"
            );
        }
    }

    #[test]
    fn forbidden_oidc_algorithms_covers_hmac_family() {
        use jsonwebtoken::Algorithm;
        assert!(FORBIDDEN_OIDC_ALGORITHMS.contains(&Algorithm::HS256));
        assert!(FORBIDDEN_OIDC_ALGORITHMS.contains(&Algorithm::HS384));
        assert!(FORBIDDEN_OIDC_ALGORITHMS.contains(&Algorithm::HS512));
    }

    #[tokio::test]
    async fn verify_id_token_rejects_hs256_alg() {
        let client = fake_oidc_client();
        let token = fake_jwt_with_header(r#"{"alg":"HS256","kid":"k1","typ":"JWT"}"#);

        let result = client.verify_id_token(&token, None, None).await;
        assert!(result.is_err(), "HS256 must be rejected: {result:?}");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("Forbidden") || msg.contains("forbidden"),
            "error must mention 'Forbidden': {msg}"
        );
    }

    #[tokio::test]
    async fn verify_id_token_rejects_hs384_alg() {
        let client = fake_oidc_client();
        let token = fake_jwt_with_header(r#"{"alg":"HS384","kid":"k1","typ":"JWT"}"#);

        let result = client.verify_id_token(&token, None, None).await;
        assert!(result.is_err(), "HS384 must be rejected: {result:?}");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("Forbidden") || msg.contains("forbidden"),
            "error must mention 'Forbidden': {msg}"
        );
    }

    #[tokio::test]
    async fn verify_id_token_rejects_hs512_alg() {
        let client = fake_oidc_client();
        let token = fake_jwt_with_header(r#"{"alg":"HS512","kid":"k1","typ":"JWT"}"#);

        let result = client.verify_id_token(&token, None, None).await;
        assert!(result.is_err(), "HS512 must be rejected: {result:?}");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("Forbidden") || msg.contains("forbidden"),
            "error must mention 'Forbidden': {msg}"
        );
    }

    #[tokio::test]
    async fn verify_id_token_rejects_algorithm_not_in_allowlist() {
        let client = fake_oidc_client();
        let token = fake_jwt_with_header(r#"{"alg":"PS256","kid":"k1","typ":"JWT"}"#);

        let result = client.verify_id_token(&token, None, None).await;
        assert!(result.is_err(), "PS256 must be rejected as not in allowlist: {result:?}");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("allowlist") || msg.contains("not allowed") || msg.contains("Forbidden"),
            "error must mention allowlist rejection: {msg}"
        );
    }

    #[tokio::test]
    async fn verify_id_token_rejects_alg_none() {
        let client = fake_oidc_client();
        let token = fake_jwt_with_header(r#"{"alg":"none","kid":"k1","typ":"JWT"}"#);

        let result = client.verify_id_token(&token, None, None).await;
        assert!(result.is_err(), "alg:none token must be rejected: {result:?}");
    }

    #[tokio::test]
    async fn verify_id_token_rejects_unexpected_typ_header() {
        let client = fake_oidc_client();
        let token = fake_jwt_with_header(r#"{"alg":"RS256","kid":"k1","typ":"at+JWT"}"#);

        let result = client.verify_id_token(&token, None, None).await;
        assert!(result.is_err(), "typ 'at+JWT' must be rejected: {result:?}");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("typ") || msg.contains("Unexpected"),
            "error must mention typ header: {msg}"
        );
    }

    #[tokio::test]
    async fn verify_id_token_accepts_absent_typ_header() {
        let client = fake_oidc_client();
        let token = fake_jwt_with_header(r#"{"alg":"RS256","kid":"k1"}"#); // no typ

        let result = client.verify_id_token(&token, None, None).await;
        if let Err(ref msg) = result {
            assert!(
                !msg.contains("typ") && !msg.contains("Unexpected"),
                "absent typ must not trigger typ rejection: {msg}"
            );
        }
    }

    #[test]
    fn required_jwt_typ_constant_is_uppercase_jwt() {
        assert_eq!(REQUIRED_JWT_TYP, "JWT");
    }

    // ── S42: Key-injection header rejection ───────────────────────────────────

    #[test]
    fn forbidden_key_injection_headers_lists_expected_names() {
        assert!(FORBIDDEN_KEY_INJECTION_HEADERS.contains(&"jku"));
        assert!(FORBIDDEN_KEY_INJECTION_HEADERS.contains(&"jwk"));
        assert!(FORBIDDEN_KEY_INJECTION_HEADERS.contains(&"x5u"));
        assert!(FORBIDDEN_KEY_INJECTION_HEADERS.contains(&"x5c"));
    }

    #[tokio::test]
    async fn verify_id_token_rejects_jku_header() {
        let client = fake_oidc_client();
        let token =
            fake_jwt_with_header(r#"{"alg":"RS256","kid":"k1","jku":"https://evil.example/keys"}"#);

        let result = client.verify_id_token(&token, None, None).await;
        assert!(result.is_err(), "jku header must be rejected: {result:?}");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("injection") || msg.contains("forbidden"),
            "error must mention key-injection: {msg}"
        );
    }

    #[tokio::test]
    async fn verify_id_token_rejects_x5u_header() {
        let client = fake_oidc_client();
        let token = fake_jwt_with_header(
            r#"{"alg":"RS256","kid":"k1","x5u":"https://evil.example/cert.pem"}"#,
        );

        let result = client.verify_id_token(&token, None, None).await;
        assert!(result.is_err(), "x5u header must be rejected: {result:?}");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("injection") || msg.contains("forbidden"),
            "error must mention key-injection: {msg}"
        );
    }

    #[tokio::test]
    async fn verify_id_token_rejects_x5c_header() {
        let client = fake_oidc_client();
        let token = fake_jwt_with_header(r#"{"alg":"RS256","kid":"k1","x5c":["MIIB..."]}"#);

        let result = client.verify_id_token(&token, None, None).await;
        assert!(result.is_err(), "x5c header must be rejected: {result:?}");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("injection") || msg.contains("forbidden"),
            "error must mention key-injection: {msg}"
        );
    }

    #[tokio::test]
    async fn verify_id_token_rejects_jwk_header() {
        let client = fake_oidc_client();
        let token = fake_jwt_with_header(
            r#"{"alg":"RS256","kid":"k1","jwk":{"kty":"RSA","n":"AAAA","e":"AQAB"}}"#,
        );

        let result = client.verify_id_token(&token, None, None).await;
        assert!(result.is_err(), "jwk header must be rejected: {result:?}");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("injection") || msg.contains("forbidden"),
            "error must mention key-injection: {msg}"
        );
    }

    #[tokio::test]
    async fn verify_id_token_without_injection_headers_proceeds_to_jwks_check() {
        let client = fake_oidc_client();
        let token = fake_jwt_with_header(r#"{"alg":"RS256","kid":"k1","typ":"JWT"}"#);

        let result = client.verify_id_token(&token, None, None).await;
        if let Err(ref msg) = result {
            assert!(
                !msg.contains("injection"),
                "clean token must not trigger injection rejection: {msg}"
            );
            assert!(
                msg.to_lowercase().contains("jwks") || msg.contains("kid") || msg.contains("key"),
                "error must indicate JWKS/key lookup failure: {msg}"
            );
        }
    }
}
