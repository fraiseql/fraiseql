#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

use super::*;

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};

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
    let provider = ExternalAuthProvider::new(
        ProviderType::OIDC,
        "auth0",
        "client_id",
        "vault/path/to/secret",
    );
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
    OAuth2Client::new(
        "test_client",
        "test_secret",
        "https://example.com/authorize",
        token_endpoint,
    )
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
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("error"));
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

    let client = OIDCClient::new(config, "client_id", "secret");
    let user = client.get_userinfo("access_token_xyz").await.unwrap();
    assert_eq!(user.sub, "user_789");
    assert_eq!(user.email, Some("real@example.com".to_string()));
    assert_eq!(user.name, Some("Real User".to_string()));
}

#[tokio::test]
async fn test_get_userinfo_no_endpoint() {
    let mut config = test_oidc_config();
    config.userinfo_endpoint = None;

    let client = OIDCClient::new(config, "client_id", "secret");
    let result = client.get_userinfo("token").await;
    assert!(result.is_err());
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

    let client = OIDCClient::new(config, "client_id", "secret");
    let result = client.get_userinfo("token").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("500"));
}

#[tokio::test]
async fn test_verify_id_token_rejects_missing_kid() {
    let config = test_oidc_config();
    let client = OIDCClient::new(config, "client_id", "secret");

    // A JWT without a kid in the header
    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
    let claims = IdTokenClaims::new(
        "https://example.com".into(),
        "user_1".into(),
        "client_id".into(),
        (Utc::now() + Duration::hours(1)).timestamp(),
        Utc::now().timestamp(),
    );
    let token = jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap();

    let result = client.verify_id_token(&token, None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("kid"));
}

// --- TokenRefreshWorker tests ---

#[tokio::test]
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

    // Wait for worker to process the due refresh
    tokio::time::sleep(StdDuration::from_millis(200)).await;
    let _ = cancel_tx.send(true);
    handle.await.unwrap();

    assert!(refresher.call_count.load(std::sync::atomic::Ordering::Relaxed) >= 1);
}

#[tokio::test]
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
    tokio::time::sleep(StdDuration::from_millis(200)).await;
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
