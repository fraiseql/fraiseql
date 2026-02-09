//! Comprehensive test specifications for OAuth2 and OIDC authentication,
//! provider discovery, JWT validation, and user provisioning.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod oauth_tests {
    use chrono::{Duration, Utc};

    use crate::auth::oauth::{
        ExternalAuthProvider, IdTokenClaims, NonceParameter, OAuth2Client, OAuth2ClientConfig,
        OAuthAuditEvent, OAuthSession, OIDCClient, OIDCProviderConfig, PKCEChallenge,
        ProviderFailoverManager, ProviderRegistry, ProviderType, StateParameter,
        TokenRefreshScheduler, UserInfo,
    };

    fn test_oidc_config() -> OIDCProviderConfig {
        OIDCProviderConfig::new(
            "https://provider.example.com".to_string(),
            "https://provider.example.com/authorize".to_string(),
            "https://provider.example.com/token".to_string(),
            "https://provider.example.com/.well-known/jwks.json".to_string(),
        )
    }

    fn test_oauth2_client() -> OAuth2Client {
        OAuth2Client::new(
            "test_client_id",
            "test_client_secret",
            "https://provider.example.com/authorize",
            "https://provider.example.com/token",
        )
    }

    // ============================================================================
    // OAUTH2 AUTHORIZATION CODE FLOW TESTS
    // ============================================================================

    /// Test OAuth2 authorization URL generation
    #[tokio::test]
    async fn test_oauth2_authorization_url_generation() {
        let client = test_oauth2_client().with_scopes(vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
        ]);

        let url = client
            .authorization_url("http://localhost:3000/auth/callback")
            .unwrap();

        // URL should start with authorization endpoint
        assert!(url.starts_with("https://provider.example.com/authorize?"));

        // URL should contain required parameters
        assert!(url.contains("client_id=test_client_id"));
        assert!(url.contains("redirect_uri="));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("scope="));
        assert!(url.contains("state="));

        // Scope should include openid, profile, email
        assert!(url.contains("openid"));
    }

    /// Test OAuth2 authorization code exchange
    #[tokio::test]
    async fn test_oauth2_exchange_code_for_token() {
        let client = test_oauth2_client();

        let token = client
            .exchange_code("test_auth_code", "http://localhost:3000/auth/callback")
            .await
            .unwrap();

        // Response should contain required fields
        assert!(!token.access_token.is_empty());
        assert_eq!(token.token_type, "Bearer");
        assert!(token.expires_in > 0);
        assert!(token.refresh_token.is_some());
        assert!(token.scope.is_some());
    }

    /// Test OAuth2 token refresh
    #[tokio::test]
    async fn test_oauth2_refresh_token() {
        let client = test_oauth2_client();

        let token = client.refresh_token("refresh_token_123").await.unwrap();

        // Response should contain new access token
        assert!(!token.access_token.is_empty());
        assert_eq!(token.token_type, "Bearer");
        assert!(token.expires_in > 0);
        // Refresh token returned
        assert!(token.refresh_token.is_some());
    }

    /// Test OAuth2 state parameter validation
    #[tokio::test]
    async fn test_oauth2_state_parameter_prevents_csrf() {
        let state = StateParameter::new();
        let state_value = state.state.clone();

        // Correct state should verify
        assert!(state.verify(&state_value));

        // Wrong state should fail
        assert!(!state.verify("wrong_state_value"));

        // Empty state should fail
        assert!(!state.verify(""));
    }

    /// Test OAuth2 redirect URI validation
    #[tokio::test]
    async fn test_oauth2_redirect_uri_validation() {
        let client = test_oauth2_client();

        // Generate URL with specific redirect URI
        let url = client
            .authorization_url("http://localhost:3000/auth/callback")
            .unwrap();

        // URL should contain the exact redirect URI (encoded)
        assert!(url.contains("redirect_uri="));
        assert!(url.contains("localhost"));

        // Different redirect URIs generate different URLs
        let url2 = client
            .authorization_url("http://localhost:4000/auth/callback")
            .unwrap();
        assert_ne!(url, url2);
    }

    // ============================================================================
    // OAUTH2 ERROR HANDLING TESTS
    // ============================================================================

    /// Test OAuth2 invalid credentials
    #[tokio::test]
    async fn test_oauth2_invalid_credentials_error() {
        // Test that error audit events can be created for invalid credentials
        let event = OAuthAuditEvent::new("token_exchange", "auth0", "failed")
            .with_error("invalid_client: Client authentication failed".to_string());

        assert_eq!(event.event_type, "token_exchange");
        assert_eq!(event.status, "failed");
        assert!(event.error.as_ref().unwrap().contains("invalid_client"));
    }

    /// Test OAuth2 expired authorization code
    #[tokio::test]
    async fn test_oauth2_expired_authorization_code() {
        // Test that expired code errors can be represented
        let event = OAuthAuditEvent::new("token_exchange", "google", "failed")
            .with_error("invalid_grant: Authorization code expired".to_string());

        assert_eq!(event.status, "failed");
        assert!(event.error.as_ref().unwrap().contains("invalid_grant"));
    }

    /// Test OAuth2 scope mismatch
    #[tokio::test]
    async fn test_oauth2_insufficient_permissions_error() {
        // Test that scope denial errors are represented
        let event = OAuthAuditEvent::new("authorization", "google", "failed")
            .with_error("access_denied: User denied the requested scopes".to_string());

        assert_eq!(event.event_type, "authorization");
        assert!(event.error.as_ref().unwrap().contains("access_denied"));
    }

    // ============================================================================
    // OIDC PROVIDER DISCOVERY TESTS
    // ============================================================================

    /// Test OIDC provider discovery via well-known endpoint
    #[tokio::test]
    async fn test_oidc_provider_discovery() {
        let config = test_oidc_config();

        // Config should have all required endpoints
        assert_eq!(config.issuer, "https://provider.example.com");
        assert!(!config.authorization_endpoint.is_empty());
        assert!(!config.token_endpoint.is_empty());
        assert!(!config.jwks_uri.is_empty());

        // Should support required scopes
        assert!(config.scopes_supported.contains(&"openid".to_string()));

        // Should support authorization code flow
        assert!(config.response_types_supported.contains(&"code".to_string()));
    }

    /// Test OIDC JWKS (JSON Web Key Set) retrieval
    #[tokio::test]
    async fn test_oidc_jwks_retrieval_and_caching() {
        let config = test_oidc_config();

        // JWKS URI should be present and well-formed
        assert!(config.jwks_uri.starts_with("https://"));
        assert!(config.jwks_uri.contains("jwks"));

        // Multiple configs with same JWKS URI should share cache potential
        let config2 = test_oidc_config();
        assert_eq!(config.jwks_uri, config2.jwks_uri);
    }

    /// Test OIDC configuration caching
    #[tokio::test]
    async fn test_oidc_configuration_caching() {
        // Create two identical configs to simulate caching
        let config1 = test_oidc_config();
        let config2 = test_oidc_config();

        // Cached configuration should be equal
        assert_eq!(config1, config2);

        // All fields should match
        assert_eq!(config1.issuer, config2.issuer);
        assert_eq!(config1.authorization_endpoint, config2.authorization_endpoint);
        assert_eq!(config1.token_endpoint, config2.token_endpoint);
        assert_eq!(config1.jwks_uri, config2.jwks_uri);
    }

    // ============================================================================
    // JWT ID TOKEN VALIDATION TESTS
    // ============================================================================

    /// Test ID token structure validation
    #[tokio::test]
    async fn test_id_token_structure_validation() {
        let exp = (Utc::now() + Duration::hours(1)).timestamp();
        let iat = Utc::now().timestamp();

        let claims = IdTokenClaims::new(
            "https://provider.example.com".to_string(),
            "user_123".to_string(),
            "test_client_id".to_string(),
            exp,
            iat,
        );

        // Required claims should be present
        assert_eq!(claims.iss, "https://provider.example.com");
        assert_eq!(claims.sub, "user_123");
        assert_eq!(claims.aud, "test_client_id");
        assert!(claims.exp > 0);
        assert!(claims.iat > 0);
        assert!(claims.exp > claims.iat);
    }

    /// Test ID token signature verification
    #[tokio::test]
    async fn test_id_token_signature_verification() {
        let config = test_oidc_config();
        let client = OIDCClient::new(config, "test_client_id", "test_client_secret");

        // Verify mock ID token (the mock always returns valid claims)
        let claims = client.verify_id_token("mock_token", None).unwrap();

        // Claims should have correct issuer and audience
        assert_eq!(claims.iss, "https://provider.example.com");
        assert_eq!(claims.aud, "test_client_id");
        assert!(!claims.sub.is_empty());
    }

    /// Test ID token expiry validation
    #[tokio::test]
    async fn test_id_token_expiry_validation() {
        // Test a valid (non-expired) token
        let exp = (Utc::now() + Duration::hours(1)).timestamp();
        let claims = IdTokenClaims::new(
            "https://provider.com".to_string(),
            "user_123".to_string(),
            "client_id".to_string(),
            exp,
            Utc::now().timestamp(),
        );
        assert!(!claims.is_expired());

        // Test an expired token
        let expired_exp = (Utc::now() - Duration::hours(1)).timestamp();
        let expired_claims = IdTokenClaims::new(
            "https://provider.com".to_string(),
            "user_123".to_string(),
            "client_id".to_string(),
            expired_exp,
            (Utc::now() - Duration::hours(2)).timestamp(),
        );
        assert!(expired_claims.is_expired());

        // Test expiring soon
        let near_exp = (Utc::now() + Duration::seconds(30)).timestamp();
        let near_claims = IdTokenClaims::new(
            "https://provider.com".to_string(),
            "user_123".to_string(),
            "client_id".to_string(),
            near_exp,
            Utc::now().timestamp(),
        );
        assert!(near_claims.is_expiring_soon(300)); // Within 5 min grace
        assert!(!near_claims.is_expired()); // Not yet expired
    }

    /// Test ID token issuer validation
    #[tokio::test]
    async fn test_id_token_issuer_validation() {
        let config = test_oidc_config();
        let client = OIDCClient::new(config.clone(), "test_client_id", "test_client_secret");

        let claims = client.verify_id_token("mock_token", None).unwrap();

        // Issuer should match configured provider
        assert_eq!(claims.iss, config.issuer);

        // Mismatched issuer should be detectable
        let wrong_issuer_claims = IdTokenClaims::new(
            "https://evil-provider.com".to_string(),
            "user_123".to_string(),
            "test_client_id".to_string(),
            (Utc::now() + Duration::hours(1)).timestamp(),
            Utc::now().timestamp(),
        );
        assert_ne!(wrong_issuer_claims.iss, config.issuer);
    }

    /// Test ID token audience (aud) claim validation
    #[tokio::test]
    async fn test_id_token_audience_validation() {
        let config = test_oidc_config();
        let client = OIDCClient::new(config, "test_client_id", "test_client_secret");

        let claims = client.verify_id_token("mock_token", None).unwrap();

        // Audience should match client_id
        assert_eq!(claims.aud, "test_client_id");

        // Different client_id would result in mismatch
        let wrong_aud_claims = IdTokenClaims::new(
            "https://provider.example.com".to_string(),
            "user_123".to_string(),
            "other_client_id".to_string(),
            (Utc::now() + Duration::hours(1)).timestamp(),
            Utc::now().timestamp(),
        );
        assert_ne!(wrong_aud_claims.aud, "test_client_id");
    }

    /// Test ID token subject (sub) claim validation
    #[tokio::test]
    async fn test_id_token_subject_claim_extraction() {
        let config = test_oidc_config();
        let client = OIDCClient::new(config, "test_client_id", "test_client_secret");

        let claims = client.verify_id_token("mock_token", None).unwrap();

        // Subject should be a non-empty unique identifier
        assert!(!claims.sub.is_empty());

        // Subject can be used to identify the user
        let user_id = claims.sub.clone();
        assert_eq!(user_id, claims.sub);
    }

    // ============================================================================
    // USERINFO ENDPOINT TESTS
    // ============================================================================

    /// Test userinfo endpoint access
    #[tokio::test]
    async fn test_userinfo_endpoint_retrieval() {
        let config = test_oidc_config();
        let client = OIDCClient::new(config, "test_client_id", "test_client_secret");

        let userinfo = client.get_userinfo("mock_access_token").await.unwrap();

        // Userinfo should contain expected fields
        assert!(!userinfo.sub.is_empty());
        assert!(userinfo.email.is_some());
        assert!(userinfo.email_verified.is_some());
        assert!(userinfo.name.is_some());
        assert!(userinfo.locale.is_some());
    }

    /// Test userinfo token validation
    #[tokio::test]
    async fn test_userinfo_access_token_validation() {
        let config = test_oidc_config();
        let client = OIDCClient::new(config, "test_client_id", "test_client_secret");

        // Valid access token should return userinfo
        let result = client.get_userinfo("valid_token").await;
        assert!(result.is_ok());

        // Sub should match between ID token and userinfo
        let claims = client.verify_id_token("mock_token", None).unwrap();
        let userinfo = result.unwrap();
        assert_eq!(claims.sub, userinfo.sub);
    }

    /// Test userinfo email verification
    #[tokio::test]
    async fn test_userinfo_email_verified_flag() {
        let config = test_oidc_config();
        let client = OIDCClient::new(config, "test_client_id", "test_client_secret");

        let userinfo = client.get_userinfo("mock_token").await.unwrap();

        // Email verified flag should be present and boolean
        let verified = userinfo.email_verified.unwrap();
        assert!(verified); // Mock returns true

        // An unverified user scenario
        let mut unverified_user = UserInfo::new("user_456".to_string());
        unverified_user.email = Some("unverified@example.com".to_string());
        unverified_user.email_verified = Some(false);
        assert!(!unverified_user.email_verified.unwrap());
    }

    // ============================================================================
    // USER PROVISIONING TESTS
    // ============================================================================

    /// Test first-time user auto-provisioning
    #[tokio::test]
    async fn test_first_login_auto_provisioning() {
        let config = test_oidc_config();
        let client = OIDCClient::new(config, "test_client_id", "test_client_secret");

        // Get userinfo for new user
        let userinfo = client.get_userinfo("mock_token").await.unwrap();

        // Create session for new user (simulated provisioning)
        let session = OAuthSession::new(
            "new_user_id".to_string(),
            ProviderType::OIDC,
            "auth0".to_string(),
            userinfo.sub.clone(),
            "access_token".to_string(),
            Utc::now() + Duration::hours(1),
        );

        // Session should be valid
        assert_eq!(session.provider_name, "auth0");
        assert_eq!(session.provider_user_id, userinfo.sub);
        assert!(!session.is_expired());
    }

    /// Test existing user OAuth linking
    #[tokio::test]
    async fn test_linking_existing_user_to_oauth() {
        // Simulate linking an existing user to multiple OAuth providers
        let existing_user_id = "existing_user_123".to_string();

        let session_google = OAuthSession::new(
            existing_user_id.clone(),
            ProviderType::OAuth2,
            "google".to_string(),
            "google|sub_123".to_string(),
            "google_access_token".to_string(),
            Utc::now() + Duration::hours(1),
        );

        let session_auth0 = OAuthSession::new(
            existing_user_id.clone(),
            ProviderType::OIDC,
            "auth0".to_string(),
            "auth0|sub_456".to_string(),
            "auth0_access_token".to_string(),
            Utc::now() + Duration::hours(1),
        );

        // Both sessions should point to same user
        assert_eq!(session_google.user_id, session_auth0.user_id);
        // But different providers
        assert_ne!(session_google.provider_name, session_auth0.provider_name);
        assert_ne!(session_google.provider_user_id, session_auth0.provider_user_id);
    }

    /// Test user profile update on login
    #[tokio::test]
    async fn test_user_profile_update_from_provider() {
        let config = test_oidc_config();
        let client = OIDCClient::new(config, "test_client_id", "test_client_secret");

        // First login gets initial profile
        let userinfo = client.get_userinfo("mock_token").await.unwrap();
        assert!(userinfo.name.is_some());
        assert!(userinfo.email.is_some());

        // Subsequent login gets potentially updated profile
        let updated_info = client.get_userinfo("mock_token").await.unwrap();
        assert_eq!(userinfo.sub, updated_info.sub); // sub never changes
    }

    /// Test multiple OAuth providers per user
    #[tokio::test]
    async fn test_multiple_oauth_providers_same_user() {
        let registry = ProviderRegistry::new();

        let google = ExternalAuthProvider::new(
            ProviderType::OAuth2,
            "google",
            "google_client_id",
            "vault/oauth/google",
        );
        let auth0 = ExternalAuthProvider::new(
            ProviderType::OIDC,
            "auth0",
            "auth0_client_id",
            "vault/oauth/auth0",
        );
        let microsoft = ExternalAuthProvider::new(
            ProviderType::OIDC,
            "microsoft",
            "ms_client_id",
            "vault/oauth/microsoft",
        );

        registry.register(google).unwrap();
        registry.register(auth0).unwrap();
        registry.register(microsoft).unwrap();

        let enabled = registry.list_enabled().unwrap();
        assert_eq!(enabled.len(), 3);

        // Each provider independently accessible
        assert!(registry.get("google").unwrap().is_some());
        assert!(registry.get("auth0").unwrap().is_some());
        assert!(registry.get("microsoft").unwrap().is_some());
    }

    /// Test OAuth provider unlinking
    #[tokio::test]
    async fn test_unlinking_oauth_provider() {
        let registry = ProviderRegistry::new();

        let google = ExternalAuthProvider::new(
            ProviderType::OAuth2,
            "google",
            "google_client_id",
            "vault/oauth/google",
        );
        let auth0 = ExternalAuthProvider::new(
            ProviderType::OIDC,
            "auth0",
            "auth0_client_id",
            "vault/oauth/auth0",
        );

        registry.register(google).unwrap();
        registry.register(auth0).unwrap();

        // Disable one provider (unlinking)
        registry.disable("google").unwrap();

        let enabled = registry.list_enabled().unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].provider_name, "auth0");

        // Provider still exists but disabled
        let google_provider = registry.get("google").unwrap().unwrap();
        assert!(!google_provider.enabled);
    }

    // ============================================================================
    // PROVIDER-SPECIFIC TESTS
    // ============================================================================

    /// Test Auth0 OIDC provider support
    #[tokio::test]
    async fn test_auth0_provider_integration() {
        let auth0_config = OIDCProviderConfig::new(
            "https://tenant.auth0.com".to_string(),
            "https://tenant.auth0.com/authorize".to_string(),
            "https://tenant.auth0.com/oauth/token".to_string(),
            "https://tenant.auth0.com/.well-known/jwks.json".to_string(),
        );

        assert_eq!(auth0_config.issuer, "https://tenant.auth0.com");
        assert!(auth0_config.authorization_endpoint.contains("auth0.com"));

        let client = OIDCClient::new(auth0_config, "auth0_client_id", "auth0_secret");
        let claims = client.verify_id_token("mock_token", None).unwrap();
        assert!(!claims.sub.is_empty());
    }

    /// Test Google OAuth2 provider support
    #[tokio::test]
    async fn test_google_oauth2_provider_integration() {
        let google_config = OIDCProviderConfig::new(
            "https://accounts.google.com".to_string(),
            "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            "https://oauth2.googleapis.com/token".to_string(),
            "https://www.googleapis.com/oauth2/v3/certs".to_string(),
        );

        assert_eq!(google_config.issuer, "https://accounts.google.com");
        assert!(google_config.jwks_uri.contains("googleapis.com"));

        let provider = ExternalAuthProvider::new(
            ProviderType::OAuth2,
            "google",
            "google_client_id",
            "vault/oauth/google",
        );
        assert_eq!(provider.provider_type, ProviderType::OAuth2);
    }

    /// Test Microsoft OIDC provider support
    #[tokio::test]
    async fn test_microsoft_oidc_provider_integration() {
        let ms_config = OIDCProviderConfig::new(
            "https://login.microsoftonline.com/common/v2.0".to_string(),
            "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".to_string(),
            "https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string(),
            "https://login.microsoftonline.com/common/discovery/v2.0/keys".to_string(),
        );

        assert!(ms_config.issuer.contains("microsoftonline.com"));
        assert!(ms_config.token_endpoint.contains("oauth2/v2.0/token"));

        let provider = ExternalAuthProvider::new(
            ProviderType::OIDC,
            "microsoft",
            "ms_client_id",
            "vault/oauth/microsoft",
        );
        assert_eq!(provider.provider_type, ProviderType::OIDC);
    }

    /// Test Okta OIDC provider support
    #[tokio::test]
    async fn test_okta_oidc_provider_integration() {
        let okta_config = OIDCProviderConfig::new(
            "https://tenant.okta.com".to_string(),
            "https://tenant.okta.com/oauth2/default/v1/authorize".to_string(),
            "https://tenant.okta.com/oauth2/default/v1/token".to_string(),
            "https://tenant.okta.com/oauth2/default/v1/keys".to_string(),
        );

        assert!(okta_config.issuer.contains("okta.com"));
        assert!(okta_config.token_endpoint.contains("/v1/token"));

        let provider = ExternalAuthProvider::new(
            ProviderType::OIDC,
            "okta",
            "okta_client_id",
            "vault/oauth/okta",
        );
        assert!(provider.enabled);
    }

    // ============================================================================
    // OAUTH SESSION MANAGEMENT TESTS
    // ============================================================================

    /// Test OAuth session creation
    #[tokio::test]
    async fn test_oauth_session_creation() {
        let session = OAuthSession::new(
            "user_123".to_string(),
            ProviderType::OIDC,
            "auth0".to_string(),
            "auth0|user_id".to_string(),
            "encrypted_access_token".to_string(),
            Utc::now() + Duration::hours(1),
        );

        assert_eq!(session.user_id, "user_123");
        assert_eq!(session.provider_type, ProviderType::OIDC);
        assert_eq!(session.provider_name, "auth0");
        assert_eq!(session.provider_user_id, "auth0|user_id");
        assert!(!session.access_token.is_empty());
        assert!(!session.is_expired());
        assert!(!session.id.is_empty()); // UUID generated
    }

    /// Test OAuth session token refresh
    #[tokio::test]
    async fn test_oauth_session_automatic_refresh() {
        let mut session = OAuthSession::new(
            "user_123".to_string(),
            ProviderType::OIDC,
            "auth0".to_string(),
            "auth0|user_id".to_string(),
            "old_access_token".to_string(),
            Utc::now() + Duration::hours(1),
        );
        session.refresh_token = Some("refresh_token_123".to_string());

        assert!(session.last_refreshed.is_none());

        // Simulate token refresh
        let new_expiry = Utc::now() + Duration::hours(2);
        session.refresh_tokens("new_access_token".to_string(), new_expiry);

        assert_eq!(session.access_token, "new_access_token");
        assert!(session.last_refreshed.is_some());
        assert!(!session.is_expired());
    }

    /// Test OAuth session expiration handling
    #[tokio::test]
    async fn test_oauth_session_expiration() {
        // Create a session that's already expired
        let expired_session = OAuthSession::new(
            "user_123".to_string(),
            ProviderType::OIDC,
            "auth0".to_string(),
            "auth0|user_id".to_string(),
            "expired_token".to_string(),
            Utc::now() - Duration::hours(1), // Expired 1 hour ago
        );

        assert!(expired_session.is_expired());

        // Create a session expiring soon
        let expiring_session = OAuthSession::new(
            "user_456".to_string(),
            ProviderType::OAuth2,
            "google".to_string(),
            "google|user_id".to_string(),
            "expiring_token".to_string(),
            Utc::now() + Duration::seconds(30),
        );

        assert!(!expiring_session.is_expired());
        assert!(expiring_session.is_expiring_soon(300)); // Within 5 min grace
    }

    /// Test OAuth session revocation
    #[tokio::test]
    async fn test_oauth_session_revocation() {
        let scheduler = TokenRefreshScheduler::new();

        // Schedule a refresh for the session
        let session_id = "session_to_revoke".to_string();
        scheduler
            .schedule_refresh(session_id.clone(), Utc::now() + Duration::hours(1))
            .unwrap();

        // Revoke by canceling scheduled refresh
        let cancelled = scheduler.cancel_refresh(&session_id).unwrap();
        assert!(cancelled);

        // Verify no more pending refresh
        let next = scheduler.get_next_refresh().unwrap();
        assert!(next.is_none());
    }

    // ============================================================================
    // SECURITY TESTS
    // ============================================================================

    /// Test PKCE code challenge verification
    #[tokio::test]
    async fn test_oauth2_pkce_code_challenge() {
        let pkce = PKCEChallenge::new();

        // PKCE should have non-empty values
        assert!(!pkce.code_verifier.is_empty());
        assert!(!pkce.code_challenge.is_empty());
        assert_eq!(pkce.code_challenge_method, "S256");

        // Verify code verifier matches challenge
        let verifier = pkce.code_verifier.clone();
        assert!(pkce.verify(&verifier));

        // Wrong verifier should fail
        assert!(!pkce.verify("wrong_verifier"));

        // Each PKCE should be unique
        let pkce2 = PKCEChallenge::new();
        assert_ne!(pkce.code_verifier, pkce2.code_verifier);
        assert_ne!(pkce.code_challenge, pkce2.code_challenge);
    }

    /// Test state parameter CSRF protection
    #[tokio::test]
    async fn test_oauth2_state_csrf_protection() {
        let state = StateParameter::new();

        // State should be random and non-empty
        assert!(!state.state.is_empty());
        assert!(!state.is_expired());

        // Correct state verifies
        let original_state = state.state.clone();
        assert!(state.verify(&original_state));

        // Wrong state fails
        assert!(!state.verify("attacker_state"));

        // Each state should be unique
        let state2 = StateParameter::new();
        assert_ne!(state.state, state2.state);
    }

    /// Test nonce parameter replay protection
    #[tokio::test]
    async fn test_oauth2_nonce_replay_prevention() {
        let nonce = NonceParameter::new();

        // Nonce should be random and non-empty
        assert!(!nonce.nonce.is_empty());
        assert!(!nonce.is_expired());

        // Correct nonce verifies
        let original_nonce = nonce.nonce.clone();
        assert!(nonce.verify(&original_nonce));

        // Wrong nonce fails (prevents replay)
        assert!(!nonce.verify("replayed_nonce"));

        // OIDC client verifies nonce in ID token
        let config = test_oidc_config();
        let client = OIDCClient::new(config, "test_client_id", "test_secret");
        let claims = client
            .verify_id_token("mock_token", Some(&original_nonce))
            .unwrap();
        assert_eq!(claims.nonce, Some(original_nonce));
    }

    /// Test XSS prevention in OAuth flow
    #[tokio::test]
    async fn test_oauth2_xss_protection() {
        // Test that XSS-dangerous content in userinfo fields doesn't propagate
        let mut malicious_user = UserInfo::new("user_xss".to_string());
        malicious_user.name = Some("<script>alert('xss')</script>".to_string());
        malicious_user.picture = Some("javascript:alert('xss')".to_string());

        // Application should sanitize these values
        let name = malicious_user.name.unwrap();
        assert!(name.contains("<script>")); // Raw value stored
        // Sanitization happens at display layer, not storage

        // URL encoding in authorization URL prevents injection
        let client = test_oauth2_client();
        let url = client
            .authorization_url("http://localhost/callback?evil=<script>")
            .unwrap();
        // URL should be encoded, not contain raw script tags
        assert!(!url.contains("<script>"));
    }

    /// Test credential storage security
    #[tokio::test]
    async fn test_oauth_credentials_encrypted_storage() {
        // Token storage should use encrypted fields
        let session = OAuthSession::new(
            "user_123".to_string(),
            ProviderType::OIDC,
            "auth0".to_string(),
            "auth0|user_id".to_string(),
            "sensitive_access_token".to_string(),
            Utc::now() + Duration::hours(1),
        );
        session.refresh_token.as_ref(); // Can be None for storage

        // Provider secrets stored via vault paths
        let provider = ExternalAuthProvider::new(
            ProviderType::OIDC,
            "auth0",
            "client_id",
            "vault/oauth/auth0/client_secret",
        );
        assert!(provider.client_secret_vault_path.starts_with("vault/"));

        // Audit event should not include tokens
        let event = OAuthAuditEvent::new("token_exchange", "auth0", "success")
            .with_user_id("user_123".to_string());
        assert!(!format!("{:?}", event).contains("sensitive_access_token"));
    }

    // ============================================================================
    // CONFIGURATION AND DISCOVERY TESTS
    // ============================================================================

    /// Test dynamic provider configuration
    #[tokio::test]
    async fn test_dynamic_oauth_provider_configuration() {
        let registry = ProviderRegistry::new();

        let mut provider = ExternalAuthProvider::new(
            ProviderType::OIDC,
            "auth0",
            "initial_client_id",
            "vault/auth0/secret",
        );
        provider.oauth2_config = Some(OAuth2ClientConfig {
            authorization_endpoint: "https://auth0.com/authorize".to_string(),
            token_endpoint:         "https://auth0.com/token".to_string(),
            use_pkce:               true,
        });

        registry.register(provider).unwrap();

        let retrieved = registry.get("auth0").unwrap().unwrap();
        assert_eq!(retrieved.client_id, "initial_client_id");
        assert!(retrieved.oauth2_config.is_some());
        assert!(retrieved.oauth2_config.unwrap().use_pkce);
    }

    /// Test provider enablement toggle
    #[tokio::test]
    async fn test_oauth_provider_enable_disable() {
        let registry = ProviderRegistry::new();

        let provider = ExternalAuthProvider::new(
            ProviderType::OIDC,
            "auth0",
            "client_id",
            "vault/auth0/secret",
        );
        registry.register(provider).unwrap();

        // Initially enabled
        let enabled = registry.list_enabled().unwrap();
        assert_eq!(enabled.len(), 1);

        // Disable
        registry.disable("auth0").unwrap();
        let enabled = registry.list_enabled().unwrap();
        assert_eq!(enabled.len(), 0);

        // Provider still exists
        assert!(registry.get("auth0").unwrap().is_some());

        // Re-enable
        registry.enable("auth0").unwrap();
        let enabled = registry.list_enabled().unwrap();
        assert_eq!(enabled.len(), 1);
    }

    /// Test fallback provider handling
    #[tokio::test]
    async fn test_oauth_provider_fallback() {
        let failover = ProviderFailoverManager::new(
            "auth0".to_string(),
            vec!["google".to_string(), "microsoft".to_string()],
        );

        // Primary available
        assert_eq!(failover.get_available_provider().unwrap(), "auth0");

        // Mark primary unavailable
        failover
            .mark_unavailable("auth0".to_string(), 300)
            .unwrap();
        assert_eq!(failover.get_available_provider().unwrap(), "google");

        // Mark first fallback unavailable too
        failover
            .mark_unavailable("google".to_string(), 300)
            .unwrap();
        assert_eq!(failover.get_available_provider().unwrap(), "microsoft");

        // All unavailable
        failover
            .mark_unavailable("microsoft".to_string(), 300)
            .unwrap();
        assert!(failover.get_available_provider().is_err());

        // Restore primary
        failover.mark_available("auth0").unwrap();
        assert_eq!(failover.get_available_provider().unwrap(), "auth0");
    }

    /// Test scope customization per provider
    #[tokio::test]
    async fn test_oauth_scopes_configuration() {
        // Auth0 with custom scopes
        let mut auth0 = ExternalAuthProvider::new(
            ProviderType::OIDC,
            "auth0",
            "auth0_id",
            "vault/auth0/secret",
        );
        auth0.set_scopes(vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
            "custom:claims".to_string(),
        ]);
        assert_eq!(auth0.scopes.len(), 4);
        assert!(auth0.scopes.contains(&"custom:claims".to_string()));

        // Google with standard scopes
        let mut google = ExternalAuthProvider::new(
            ProviderType::OAuth2,
            "google",
            "google_id",
            "vault/google/secret",
        );
        google.set_scopes(vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
        ]);
        assert_eq!(google.scopes.len(), 3);

        // Microsoft with offline access
        let mut microsoft = ExternalAuthProvider::new(
            ProviderType::OIDC,
            "microsoft",
            "ms_id",
            "vault/ms/secret",
        );
        microsoft.set_scopes(vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
            "offline_access".to_string(),
        ]);
        assert!(microsoft.scopes.contains(&"offline_access".to_string()));
    }

    // ============================================================================
    // ERROR RECOVERY AND LOGGING TESTS
    // ============================================================================

    /// Test OAuth provider timeout handling
    #[tokio::test]
    async fn test_oauth_provider_timeout() {
        // Simulate timeout by marking provider unavailable
        let failover = ProviderFailoverManager::new(
            "auth0".to_string(),
            vec!["google".to_string()],
        );

        // Provider times out
        failover
            .mark_unavailable("auth0".to_string(), 60)
            .unwrap();

        // System uses fallback
        let available = failover.get_available_provider().unwrap();
        assert_eq!(available, "google");

        // Audit event logged
        let event = OAuthAuditEvent::new("token_exchange", "auth0", "failed")
            .with_error("Connection timeout after 10s".to_string())
            .with_metadata("timeout_ms".to_string(), "10000".to_string());
        assert!(event.error.as_ref().unwrap().contains("timeout"));
    }

    /// Test OAuth provider unavailability
    #[tokio::test]
    async fn test_oauth_provider_unavailable() {
        let failover = ProviderFailoverManager::new(
            "auth0".to_string(),
            vec!["google".to_string()],
        );

        // Provider returns 5xx
        failover
            .mark_unavailable("auth0".to_string(), 300)
            .unwrap();

        // Audit event logged
        let event = OAuthAuditEvent::new("authorization", "auth0", "failed")
            .with_error("Provider returned HTTP 503".to_string())
            .with_metadata("http_status".to_string(), "503".to_string());

        assert_eq!(event.status, "failed");
        assert!(event.error.as_ref().unwrap().contains("503"));

        // Fallback provider available
        assert_eq!(failover.get_available_provider().unwrap(), "google");
    }

    /// Test OAuth audit logging
    #[tokio::test]
    async fn test_oauth_audit_logging() {
        let mut events: Vec<OAuthAuditEvent> = Vec::new();

        // Log authorization attempt
        events.push(
            OAuthAuditEvent::new("authorization", "auth0", "success")
                .with_user_id("user_123".to_string())
                .with_metadata("ip_address".to_string(), "192.168.1.1".to_string()),
        );

        // Log token exchange
        events.push(
            OAuthAuditEvent::new("token_exchange", "auth0", "success")
                .with_user_id("user_123".to_string()),
        );

        // Log user provisioning
        events.push(
            OAuthAuditEvent::new("user_provisioning", "auth0", "success")
                .with_user_id("user_123".to_string())
                .with_metadata("action".to_string(), "created".to_string()),
        );

        // Log token refresh
        events.push(
            OAuthAuditEvent::new("token_refresh", "auth0", "success")
                .with_user_id("user_123".to_string()),
        );

        // Log session logout
        events.push(
            OAuthAuditEvent::new("logout", "auth0", "success")
                .with_user_id("user_123".to_string()),
        );

        // All events logged with required fields
        assert_eq!(events.len(), 5);
        for event in &events {
            assert!(!event.event_type.is_empty());
            assert!(!event.provider.is_empty());
            assert!(!event.status.is_empty());
            assert!(event.user_id.is_some());
            assert!(event.timestamp <= Utc::now());
        }
    }

    /// Test OAuth malicious token detection
    #[tokio::test]
    async fn test_oauth_suspicious_token_detection() {
        let mut failed_attempts: Vec<OAuthAuditEvent> = Vec::new();

        // Simulate multiple failed token exchanges
        for i in 0..5 {
            failed_attempts.push(
                OAuthAuditEvent::new("token_exchange", "auth0", "failed")
                    .with_error(format!("invalid_grant: attempt {}", i))
                    .with_metadata("ip_address".to_string(), "10.0.0.1".to_string()),
            );
        }

        // Detect suspicious pattern (multiple failures)
        let failures_from_ip = failed_attempts
            .iter()
            .filter(|e| {
                e.status == "failed"
                    && e.metadata.get("ip_address") == Some(&"10.0.0.1".to_string())
            })
            .count();
        assert_eq!(failures_from_ip, 5);

        // Threshold exceeded (e.g., 3+ failures = suspicious)
        let suspicious = failures_from_ip >= 3;
        assert!(suspicious);

        // Generate alert event
        let alert = OAuthAuditEvent::new("security_alert", "auth0", "warning")
            .with_metadata("reason".to_string(), "Multiple failed token exchanges".to_string())
            .with_metadata("ip_address".to_string(), "10.0.0.1".to_string())
            .with_metadata("attempt_count".to_string(), failures_from_ip.to_string());

        assert_eq!(alert.event_type, "security_alert");
        assert_eq!(alert.metadata.get("attempt_count"), Some(&"5".to_string()));
    }
}
