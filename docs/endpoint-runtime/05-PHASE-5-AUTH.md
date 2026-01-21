# Phase 5: Auth Runtime

## Objective

Implement OAuth 2.0 / OpenID Connect authentication with 12+ providers, JWT session management, token refresh, and user entity integration.

---

## 5.0 Testing Seams & Security Model

### Security Considerations

Authentication is critical infrastructure. Key protections:

```
┌─────────────────────────────────────────────────────────────┐
│ Authentication Security Layers                              │
├─────────────────────────────────────────────────────────────┤
│ 1. CSRF Protection - State parameter in OAuth flows         │
│ 2. Token Rotation - Refresh tokens are single-use           │
│ 3. Secure Storage - Tokens hashed before DB storage         │
│ 4. Rate Limiting - Prevent brute force on refresh           │
│ 5. Audit Logging - Track all auth events                    │
│ 6. IP/UA Binding - Optional session binding                 │
└─────────────────────────────────────────────────────────────┘
```

### Task: Define testing seams for auth operations

```rust
// crates/fraiseql-auth/src/traits.rs

use async_trait::async_trait;

/// OAuth provider abstraction for testing
#[async_trait]
pub trait OAuthProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn authorization_url(&self, state: &str, redirect_uri: &str) -> String;
    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<TokenResponse, AuthError>;
    async fn user_info(&self, access_token: &str) -> Result<UserInfo, AuthError>;
    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse, AuthError> {
        Err(AuthError::RefreshNotSupported)
    }
}

/// JWT manager abstraction for testing
pub trait JwtManager: Send + Sync {
    fn generate_access_token(&self, user_id: &str, user: &UserInfo) -> Result<String, AuthError>;
    fn verify_access_token(&self, token: &str) -> Result<Claims, AuthError>;
    fn access_expiry(&self) -> chrono::Duration;
    fn refresh_expiry(&self) -> chrono::Duration;
}

/// Session store abstraction for testing
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create_session(
        &self,
        user_id: uuid::Uuid,
        user_info: &UserInfo,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<TokenPair, AuthError>;

    async fn refresh_tokens(
        &self,
        refresh_token: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<TokenPair, AuthError>;

    async fn revoke_all(&self, user_id: uuid::Uuid) -> Result<(), AuthError>;
    async fn revoke_token(&self, refresh_token: &str) -> Result<(), AuthError>;
}

/// State store abstraction for testing (CSRF protection)
#[async_trait]
pub trait StateStore: Send + Sync {
    async fn create_state(&self, redirect_uri: Option<&str>) -> Result<String, AuthError>;
    async fn validate_state(&self, state: &str) -> Result<SavedState, AuthError>;
}

/// User repository abstraction for testing
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_provider(&self, provider: &str, provider_user_id: &str) -> Result<Option<uuid::Uuid>, AuthError>;
    async fn find_by_email(&self, email: &str) -> Result<Option<uuid::Uuid>, AuthError>;
    async fn create_user(&self, user_info: &UserInfo) -> Result<uuid::Uuid, AuthError>;
    async fn link_provider(&self, user_id: uuid::Uuid, provider: &str, user_info: &UserInfo) -> Result<(), AuthError>;
    async fn get_user(&self, user_id: uuid::Uuid) -> Result<UserRecord, AuthError>;
}

/// HTTP client abstraction for testing provider calls
#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn post_form(&self, url: &str, params: &[(&str, &str)]) -> Result<serde_json::Value, AuthError>;
    async fn get_json(&self, url: &str, bearer_token: Option<&str>) -> Result<serde_json::Value, AuthError>;
}

#[derive(Debug, Clone)]
pub struct SavedState {
    pub redirect_uri: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

### Task: Mock implementations for testing

```rust
// crates/fraiseql-auth/src/testing.rs

#[cfg(any(test, feature = "testing"))]
pub mod mocks {
    use super::*;
    use std::sync::Mutex;
    use std::collections::HashMap;

    /// Mock OAuth provider for testing
    pub struct MockOAuthProvider {
        pub name: &'static str,
        pub user_info: UserInfo,
        pub should_fail: bool,
        pub exchange_calls: Mutex<Vec<ExchangeCall>>,
    }

    #[derive(Debug, Clone)]
    pub struct ExchangeCall {
        pub code: String,
        pub redirect_uri: String,
    }

    impl MockOAuthProvider {
        pub fn new(name: &'static str, user_info: UserInfo) -> Self {
            Self {
                name,
                user_info,
                should_fail: false,
                exchange_calls: Mutex::new(Vec::new()),
            }
        }

        pub fn failing(name: &'static str) -> Self {
            Self {
                name,
                user_info: UserInfo::default(),
                should_fail: true,
                exchange_calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl OAuthProvider for MockOAuthProvider {
        fn name(&self) -> &'static str { self.name }

        fn authorization_url(&self, state: &str, redirect_uri: &str) -> String {
            format!("https://mock-provider.test/auth?state={}&redirect_uri={}", state, redirect_uri)
        }

        async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<TokenResponse, AuthError> {
            self.exchange_calls.lock().unwrap().push(ExchangeCall {
                code: code.to_string(),
                redirect_uri: redirect_uri.to_string(),
            });

            if self.should_fail {
                return Err(AuthError::ProviderError {
                    provider: self.name.to_string(),
                    message: "Simulated failure".into(),
                });
            }

            Ok(TokenResponse {
                access_token: "mock_access_token".into(),
                token_type: "Bearer".into(),
                expires_in: Some(3600),
                refresh_token: Some("mock_refresh_token".into()),
                scope: Some("openid email profile".into()),
                id_token: None,
            })
        }

        async fn user_info(&self, _access_token: &str) -> Result<UserInfo, AuthError> {
            if self.should_fail {
                return Err(AuthError::ProviderError {
                    provider: self.name.to_string(),
                    message: "Simulated failure".into(),
                });
            }

            Ok(self.user_info.clone())
        }
    }

    /// Mock JWT manager for testing
    pub struct MockJwtManager {
        pub valid_tokens: Mutex<HashMap<String, Claims>>,
        pub generated_tokens: Mutex<Vec<String>>,
    }

    impl MockJwtManager {
        pub fn new() -> Self {
            Self {
                valid_tokens: Mutex::new(HashMap::new()),
                generated_tokens: Mutex::new(Vec::new()),
            }
        }

        pub fn add_valid_token(&self, token: &str, claims: Claims) {
            self.valid_tokens.lock().unwrap().insert(token.to_string(), claims);
        }
    }

    impl JwtManager for MockJwtManager {
        fn generate_access_token(&self, user_id: &str, _user: &UserInfo) -> Result<String, AuthError> {
            let token = format!("mock_jwt_{}", user_id);
            self.generated_tokens.lock().unwrap().push(token.clone());
            Ok(token)
        }

        fn verify_access_token(&self, token: &str) -> Result<Claims, AuthError> {
            self.valid_tokens.lock().unwrap()
                .get(token)
                .cloned()
                .ok_or(AuthError::InvalidToken)
        }

        fn access_expiry(&self) -> chrono::Duration {
            chrono::Duration::minutes(15)
        }

        fn refresh_expiry(&self) -> chrono::Duration {
            chrono::Duration::days(7)
        }
    }

    /// Mock session store for testing
    pub struct MockSessionStore {
        pub sessions: Mutex<HashMap<uuid::Uuid, Vec<TokenPair>>>,
        pub refresh_tokens: Mutex<HashMap<String, (uuid::Uuid, bool)>>, // (user_id, revoked)
    }

    impl MockSessionStore {
        pub fn new() -> Self {
            Self {
                sessions: Mutex::new(HashMap::new()),
                refresh_tokens: Mutex::new(HashMap::new()),
            }
        }

        pub fn add_refresh_token(&self, token: &str, user_id: uuid::Uuid) {
            self.refresh_tokens.lock().unwrap().insert(token.to_string(), (user_id, false));
        }
    }

    #[async_trait]
    impl SessionStore for MockSessionStore {
        async fn create_session(
            &self,
            user_id: uuid::Uuid,
            _user_info: &UserInfo,
            _ip_address: Option<&str>,
            _user_agent: Option<&str>,
        ) -> Result<TokenPair, AuthError> {
            let pair = TokenPair {
                access_token: format!("access_{}", uuid::Uuid::new_v4()),
                refresh_token: format!("refresh_{}", uuid::Uuid::new_v4()),
                token_type: "Bearer".into(),
                expires_in: 900,
            };

            self.sessions.lock().unwrap()
                .entry(user_id)
                .or_insert_with(Vec::new)
                .push(pair.clone());

            self.refresh_tokens.lock().unwrap()
                .insert(pair.refresh_token.clone(), (user_id, false));

            Ok(pair)
        }

        async fn refresh_tokens(
            &self,
            refresh_token: &str,
            _ip_address: Option<&str>,
            _user_agent: Option<&str>,
        ) -> Result<TokenPair, AuthError> {
            let mut tokens = self.refresh_tokens.lock().unwrap();

            let (user_id, revoked) = tokens.get(refresh_token)
                .ok_or(AuthError::InvalidRefreshToken)?
                .clone();

            if revoked {
                return Err(AuthError::InvalidRefreshToken);
            }

            // Revoke old token
            tokens.insert(refresh_token.to_string(), (user_id, true));

            // Create new pair
            let new_pair = TokenPair {
                access_token: format!("access_{}", uuid::Uuid::new_v4()),
                refresh_token: format!("refresh_{}", uuid::Uuid::new_v4()),
                token_type: "Bearer".into(),
                expires_in: 900,
            };

            tokens.insert(new_pair.refresh_token.clone(), (user_id, false));

            Ok(new_pair)
        }

        async fn revoke_all(&self, user_id: uuid::Uuid) -> Result<(), AuthError> {
            let mut tokens = self.refresh_tokens.lock().unwrap();
            for (_, (uid, revoked)) in tokens.iter_mut() {
                if *uid == user_id {
                    *revoked = true;
                }
            }
            Ok(())
        }

        async fn revoke_token(&self, refresh_token: &str) -> Result<(), AuthError> {
            let mut tokens = self.refresh_tokens.lock().unwrap();
            if let Some((_, revoked)) = tokens.get_mut(refresh_token) {
                *revoked = true;
            }
            Ok(())
        }
    }

    /// Mock state store for testing
    pub struct MockStateStore {
        pub states: Mutex<HashMap<String, SavedState>>,
    }

    impl MockStateStore {
        pub fn new() -> Self {
            Self {
                states: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl StateStore for MockStateStore {
        async fn create_state(&self, redirect_uri: Option<&str>) -> Result<String, AuthError> {
            let state = uuid::Uuid::new_v4().to_string();
            self.states.lock().unwrap().insert(
                state.clone(),
                SavedState {
                    redirect_uri: redirect_uri.map(|s| s.to_string()),
                    created_at: chrono::Utc::now(),
                },
            );
            Ok(state)
        }

        async fn validate_state(&self, state: &str) -> Result<SavedState, AuthError> {
            self.states.lock().unwrap()
                .remove(state)
                .ok_or(AuthError::InvalidState)
        }
    }

    /// Mock HTTP client for testing provider API calls
    pub struct MockHttpClient {
        pub responses: Mutex<HashMap<String, serde_json::Value>>,
        pub calls: Mutex<Vec<HttpCall>>,
    }

    #[derive(Debug, Clone)]
    pub struct HttpCall {
        pub method: String,
        pub url: String,
    }

    impl MockHttpClient {
        pub fn new() -> Self {
            Self {
                responses: Mutex::new(HashMap::new()),
                calls: Mutex::new(Vec::new()),
            }
        }

        pub fn with_response(self, url: &str, response: serde_json::Value) -> Self {
            self.responses.lock().unwrap().insert(url.to_string(), response);
            self
        }
    }

    #[async_trait]
    impl HttpClient for MockHttpClient {
        async fn post_form(&self, url: &str, _params: &[(&str, &str)]) -> Result<serde_json::Value, AuthError> {
            self.calls.lock().unwrap().push(HttpCall {
                method: "POST".into(),
                url: url.to_string(),
            });

            self.responses.lock().unwrap()
                .get(url)
                .cloned()
                .ok_or(AuthError::ProviderError {
                    provider: "mock".into(),
                    message: format!("No mock response for {}", url),
                })
        }

        async fn get_json(&self, url: &str, _bearer_token: Option<&str>) -> Result<serde_json::Value, AuthError> {
            self.calls.lock().unwrap().push(HttpCall {
                method: "GET".into(),
                url: url.to_string(),
            });

            self.responses.lock().unwrap()
                .get(url)
                .cloned()
                .ok_or(AuthError::ProviderError {
                    provider: "mock".into(),
                    message: format!("No mock response for {}", url),
                })
        }
    }
}
```

---

## 5.1 Auth Configuration

```rust
// crates/fraiseql-auth/src/config.rs

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct AuthConfig {
    /// Session type: jwt or cookie
    #[serde(default = "default_session_type")]
    pub session_type: String,

    /// Access token expiry (e.g., "15m")
    #[serde(default = "default_access_expiry")]
    pub access_token_expiry: String,

    /// Refresh token expiry (e.g., "7d")
    #[serde(default = "default_refresh_expiry")]
    pub refresh_token_expiry: String,

    /// JWT secret (env var)
    pub jwt_secret_env: Option<String>,

    /// OAuth providers
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,

    /// User entity mapping
    pub user_mapping: Option<UserMappingConfig>,

    /// Callback URLs
    pub redirect_url: Option<String>,
    pub allowed_redirects: Option<Vec<String>>,
}

fn default_session_type() -> String { "jwt".to_string() }
fn default_access_expiry() -> String { "15m".to_string() }
fn default_refresh_expiry() -> String { "7d".to_string() }

#[derive(Debug, Deserialize)]
pub struct ProviderConfig {
    /// Provider type (if different from key)
    pub provider: Option<String>,

    /// Client ID (env var)
    pub client_id_env: String,

    /// Client secret (env var)
    pub client_secret_env: String,

    /// OAuth scopes
    pub scopes: Option<Vec<String>>,

    /// Tenant ID (for Microsoft/Azure)
    pub tenant: Option<String>,

    /// Team ID (for Apple)
    pub team_id: Option<String>,

    /// Key ID (for Apple)
    pub key_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserMappingConfig {
    /// Entity name (e.g., "User")
    pub entity: String,

    /// Field mappings from provider to entity
    #[serde(default)]
    pub email: String,

    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub avatar: Option<String>,

    /// Custom claim mappings
    #[serde(default)]
    pub custom_claims: HashMap<String, String>,
}
```

---

## 5.2 OAuth Provider Trait

```rust
// crates/fraiseql-auth/src/providers/mod.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[async_trait]
pub trait OAuthProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &'static str;

    /// Generate authorization URL
    fn authorization_url(&self, state: &str, redirect_uri: &str) -> String;

    /// Exchange authorization code for tokens
    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<TokenResponse, AuthError>;

    /// Get user info from provider
    async fn user_info(&self, access_token: &str) -> Result<UserInfo, AuthError>;

    /// Refresh access token (if supported)
    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse, AuthError> {
        Err(AuthError::RefreshNotSupported)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub id_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub picture: Option<String>,
    pub locale: Option<String>,

    /// Raw claims from provider
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
```

---

## 5.3 Provider Implementations

### Google Provider

```rust
// crates/fraiseql-auth/src/providers/google.rs

pub struct GoogleProvider {
    client_id: String,
    client_secret: String,
    scopes: Vec<String>,
}

impl GoogleProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, AuthError> {
        let client_id = std::env::var(&config.client_id_env)?;
        let client_secret = std::env::var(&config.client_secret_env)?;

        let scopes = config.scopes.clone()
            .unwrap_or_else(|| vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ]);

        Ok(Self { client_id, client_secret, scopes })
    }
}

#[async_trait]
impl OAuthProvider for GoogleProvider {
    fn name(&self) -> &'static str { "google" }

    fn authorization_url(&self, state: &str, redirect_uri: &str) -> String {
        let params = [
            ("client_id", &self.client_id),
            ("redirect_uri", &redirect_uri.to_string()),
            ("response_type", &"code".to_string()),
            ("scope", &self.scopes.join(" ")),
            ("state", &state.to_string()),
            ("access_type", &"offline".to_string()),
            ("prompt", &"consent".to_string()),
        ];

        let query = serde_urlencoded::to_string(&params).unwrap();
        format!("https://accounts.google.com/o/oauth2/v2/auth?{}", query)
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<TokenResponse, AuthError> {
        let client = reqwest::Client::new();

        let response = client.post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("code", &code.to_string()),
                ("redirect_uri", &redirect_uri.to_string()),
                ("grant_type", &"authorization_code".to_string()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let error: serde_json::Value = response.json().await?;
            return Err(AuthError::ProviderError {
                provider: "google".to_string(),
                message: error.to_string(),
            });
        }

        let tokens: TokenResponse = response.json().await?;
        Ok(tokens)
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo, AuthError> {
        let client = reqwest::Client::new();

        let response = client.get("https://www.googleapis.com/oauth2/v2/userinfo")
            .bearer_auth(access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AuthError::ProviderError {
                provider: "google".to_string(),
                message: "Failed to get user info".to_string(),
            });
        }

        let data: serde_json::Value = response.json().await?;

        Ok(UserInfo {
            id: data["id"].as_str().unwrap_or_default().to_string(),
            email: data["email"].as_str().map(|s| s.to_string()),
            email_verified: data["verified_email"].as_bool(),
            name: data["name"].as_str().map(|s| s.to_string()),
            given_name: data["given_name"].as_str().map(|s| s.to_string()),
            family_name: data["family_name"].as_str().map(|s| s.to_string()),
            picture: data["picture"].as_str().map(|s| s.to_string()),
            locale: data["locale"].as_str().map(|s| s.to_string()),
            extra: HashMap::new(),
        })
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse, AuthError> {
        let client = reqwest::Client::new();

        let response = client.post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("refresh_token", &refresh_token.to_string()),
                ("grant_type", &"refresh_token".to_string()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AuthError::RefreshFailed);
        }

        let tokens: TokenResponse = response.json().await?;
        Ok(tokens)
    }
}
```

### GitHub Provider

```rust
// crates/fraiseql-auth/src/providers/github.rs

pub struct GitHubProvider {
    client_id: String,
    client_secret: String,
    scopes: Vec<String>,
}

#[async_trait]
impl OAuthProvider for GitHubProvider {
    fn name(&self) -> &'static str { "github" }

    fn authorization_url(&self, state: &str, redirect_uri: &str) -> String {
        let params = [
            ("client_id", &self.client_id),
            ("redirect_uri", &redirect_uri.to_string()),
            ("scope", &self.scopes.join(" ")),
            ("state", &state.to_string()),
        ];

        let query = serde_urlencoded::to_string(&params).unwrap();
        format!("https://github.com/login/oauth/authorize?{}", query)
    }

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<TokenResponse, AuthError> {
        let client = reqwest::Client::new();

        let response = client.post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("code", &code.to_string()),
                ("redirect_uri", &redirect_uri.to_string()),
            ])
            .send()
            .await?;

        let tokens: TokenResponse = response.json().await?;
        Ok(tokens)
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo, AuthError> {
        let client = reqwest::Client::new();

        // Get user profile
        let user_response = client.get("https://api.github.com/user")
            .header("User-Agent", "FraiseQL")
            .bearer_auth(access_token)
            .send()
            .await?;

        let user: serde_json::Value = user_response.json().await?;

        // Get user emails (if email not public)
        let email = if user["email"].is_null() {
            let emails_response = client.get("https://api.github.com/user/emails")
                .header("User-Agent", "FraiseQL")
                .bearer_auth(access_token)
                .send()
                .await?;

            let emails: Vec<serde_json::Value> = emails_response.json().await?;

            emails.iter()
                .find(|e| e["primary"].as_bool() == Some(true))
                .and_then(|e| e["email"].as_str())
                .map(|s| s.to_string())
        } else {
            user["email"].as_str().map(|s| s.to_string())
        };

        Ok(UserInfo {
            id: user["id"].to_string(),
            email,
            email_verified: Some(true), // GitHub verifies emails
            name: user["name"].as_str().map(|s| s.to_string()),
            given_name: None,
            family_name: None,
            picture: user["avatar_url"].as_str().map(|s| s.to_string()),
            locale: None,
            extra: HashMap::new(),
        })
    }
}
```

### Generic OIDC Provider

```rust
// crates/fraiseql-auth/src/providers/oidc.rs

pub struct OidcProvider {
    client_id: String,
    client_secret: String,
    issuer: String,
    scopes: Vec<String>,

    // Discovered endpoints
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
}

impl OidcProvider {
    pub async fn new(
        config: &ProviderConfig,
        issuer: &str,
    ) -> Result<Self, AuthError> {
        let client_id = std::env::var(&config.client_id_env)?;
        let client_secret = std::env::var(&config.client_secret_env)?;

        // Discover OIDC endpoints
        let discovery_url = format!("{}/.well-known/openid-configuration", issuer.trim_end_matches('/'));

        let client = reqwest::Client::new();
        let discovery: serde_json::Value = client.get(&discovery_url)
            .send()
            .await?
            .json()
            .await?;

        Ok(Self {
            client_id,
            client_secret,
            issuer: issuer.to_string(),
            scopes: config.scopes.clone().unwrap_or_else(|| vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ]),
            authorization_endpoint: discovery["authorization_endpoint"].as_str().unwrap().to_string(),
            token_endpoint: discovery["token_endpoint"].as_str().unwrap().to_string(),
            userinfo_endpoint: discovery["userinfo_endpoint"].as_str().unwrap().to_string(),
        })
    }
}

#[async_trait]
impl OAuthProvider for OidcProvider {
    fn name(&self) -> &'static str { "oidc" }

    fn authorization_url(&self, state: &str, redirect_uri: &str) -> String {
        let params = [
            ("client_id", &self.client_id),
            ("redirect_uri", &redirect_uri.to_string()),
            ("response_type", &"code".to_string()),
            ("scope", &self.scopes.join(" ")),
            ("state", &state.to_string()),
        ];

        let query = serde_urlencoded::to_string(&params).unwrap();
        format!("{}?{}", self.authorization_endpoint, query)
    }

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<TokenResponse, AuthError> {
        let client = reqwest::Client::new();

        let response = client.post(&self.token_endpoint)
            .form(&[
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("code", &code.to_string()),
                ("redirect_uri", &redirect_uri.to_string()),
                ("grant_type", &"authorization_code".to_string()),
            ])
            .send()
            .await?;

        let tokens: TokenResponse = response.json().await?;
        Ok(tokens)
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo, AuthError> {
        let client = reqwest::Client::new();

        let response = client.get(&self.userinfo_endpoint)
            .bearer_auth(access_token)
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;

        Ok(UserInfo {
            id: data["sub"].as_str().unwrap_or_default().to_string(),
            email: data["email"].as_str().map(|s| s.to_string()),
            email_verified: data["email_verified"].as_bool(),
            name: data["name"].as_str().map(|s| s.to_string()),
            given_name: data["given_name"].as_str().map(|s| s.to_string()),
            family_name: data["family_name"].as_str().map(|s| s.to_string()),
            picture: data["picture"].as_str().map(|s| s.to_string()),
            locale: data["locale"].as_str().map(|s| s.to_string()),
            extra: serde_json::from_value(data).unwrap_or_default(),
        })
    }
}
```

---

## 5.4 JWT Management

```rust
// crates/fraiseql-auth/src/jwt.rs

use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use serde::{Serialize, Deserialize};
use chrono::{Utc, Duration};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,          // User ID
    pub email: Option<String>,
    pub name: Option<String>,
    pub iat: i64,             // Issued at
    pub exp: i64,             // Expiration
    pub iss: String,          // Issuer
    pub aud: String,          // Audience

    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

pub struct JwtManager {
    secret: Vec<u8>,
    issuer: String,
    audience: String,
    access_expiry: Duration,
    refresh_expiry: Duration,
}

impl JwtManager {
    pub fn new(config: &AuthConfig) -> Result<Self, AuthError> {
        let secret = config.jwt_secret_env.as_ref()
            .and_then(|env| std::env::var(env).ok())
            .unwrap_or_else(|| {
                // Auto-generate if not provided (dev mode)
                tracing::warn!("JWT_SECRET not set, generating random secret. Do not use in production!");
                uuid::Uuid::new_v4().to_string()
            });

        let access_expiry = parse_duration(&config.access_token_expiry)
            .map(|d| Duration::from_std(d).unwrap())
            .unwrap_or_else(|_| Duration::minutes(15));

        let refresh_expiry = parse_duration(&config.refresh_token_expiry)
            .map(|d| Duration::from_std(d).unwrap())
            .unwrap_or_else(|_| Duration::days(7));

        Ok(Self {
            secret: secret.into_bytes(),
            issuer: "fraiseql".to_string(),
            audience: "fraiseql".to_string(),
            access_expiry,
            refresh_expiry,
        })
    }

    pub fn generate_access_token(&self, user_id: &str, user: &UserInfo) -> Result<String, AuthError> {
        let now = Utc::now();

        let claims = Claims {
            sub: user_id.to_string(),
            email: user.email.clone(),
            name: user.name.clone(),
            iat: now.timestamp(),
            exp: (now + self.access_expiry).timestamp(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            custom: HashMap::new(),
        };

        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(&self.secret),
        )?;

        Ok(token)
    }

    pub fn verify_access_token(&self, token: &str) -> Result<Claims, AuthError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);

        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.secret),
            &validation,
        ).map_err(|e| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
            _ => AuthError::InvalidToken,
        })?;

        Ok(token_data.claims)
    }

    pub fn access_expiry(&self) -> Duration {
        self.access_expiry
    }

    pub fn refresh_expiry(&self) -> Duration {
        self.refresh_expiry
    }
}
```

---

## 5.5 Session Management

```rust
// crates/fraiseql-auth/src/session.rs

use sqlx::PgPool;
use uuid::Uuid;
use sha2::{Sha256, Digest};
use rand::Rng;

pub struct SessionManager {
    db: PgPool,
    jwt: JwtManager,
}

impl SessionManager {
    pub fn new(db: PgPool, jwt: JwtManager) -> Self {
        Self { db, jwt }
    }

    /// Create a new session with access and refresh tokens
    pub async fn create_session(
        &self,
        user_id: Uuid,
        user_info: &UserInfo,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<TokenPair, AuthError> {
        // Generate access token
        let access_token = self.jwt.generate_access_token(&user_id.to_string(), user_info)?;

        // Generate refresh token
        let refresh_token = self.generate_refresh_token();
        let refresh_hash = self.hash_token(&refresh_token);

        // Store refresh token
        let expires_at = chrono::Utc::now() + self.jwt.refresh_expiry();

        sqlx::query!(
            r#"
            INSERT INTO _system.refresh_tokens (user_id, token_hash, expires_at, ip_address, user_agent)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            user_id,
            refresh_hash,
            expires_at,
            ip_address,
            user_agent
        )
        .execute(&self.db)
        .await?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: self.jwt.access_expiry().num_seconds() as u64,
        })
    }

    /// Refresh tokens using a refresh token
    pub async fn refresh_tokens(
        &self,
        refresh_token: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<TokenPair, AuthError> {
        let refresh_hash = self.hash_token(refresh_token);

        // Find and validate refresh token
        let record = sqlx::query!(
            r#"
            SELECT id, user_id
            FROM _system.refresh_tokens
            WHERE token_hash = $1
              AND revoked_at IS NULL
              AND expires_at > NOW()
            "#,
            refresh_hash
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or(AuthError::InvalidRefreshToken)?;

        // Revoke old token (rotation)
        sqlx::query!(
            "UPDATE _system.refresh_tokens SET revoked_at = NOW() WHERE id = $1",
            record.id
        )
        .execute(&self.db)
        .await?;

        // Get user info
        let user = sqlx::query!(
            r#"
            SELECT id, email, name
            FROM auth.tb_user
            WHERE id = $1
            "#,
            record.user_id
        )
        .fetch_one(&self.db)
        .await?;

        let user_info = UserInfo {
            id: user.id.to_string(),
            email: Some(user.email),
            name: user.name,
            ..Default::default()
        };

        // Create new session
        self.create_session(
            record.user_id,
            &user_info,
            ip_address,
            user_agent
        ).await
    }

    /// Revoke all sessions for a user
    pub async fn revoke_all(&self, user_id: Uuid) -> Result<(), AuthError> {
        sqlx::query!(
            "UPDATE _system.refresh_tokens SET revoked_at = NOW() WHERE user_id = $1",
            user_id
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    fn generate_refresh_token(&self) -> String {
        let bytes: [u8; 32] = rand::thread_rng().gen();
        format!("rt_{}", base64::encode(&bytes))
    }

    fn hash_token(&self, token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hex::encode(hasher.finalize())
    }
}

#[derive(Debug, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
}
```

---

## 5.6 Auth Handler

```rust
// crates/fraiseql-auth/src/handler.rs

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Redirect,
    Json,
};
use std::sync::Arc;

pub struct AuthHandler {
    config: AuthConfig,
    providers: HashMap<String, Arc<dyn OAuthProvider>>,
    sessions: SessionManager,
    db: PgPool,
}

impl AuthHandler {
    /// Initiate OAuth flow
    pub async fn initiate(
        &self,
        provider_name: &str,
        redirect_uri: Option<String>,
    ) -> Result<Redirect, RuntimeError> {
        let provider = self.providers.get(provider_name)
            .ok_or_else(|| RuntimeError::Auth(AuthError::ProviderNotConfigured {
                provider: provider_name.to_string()
            }))?;

        // Generate state token for CSRF protection
        let state = self.generate_state(redirect_uri.as_deref()).await?;

        // Build callback URL
        let callback_uri = format!(
            "{}/auth/{}/callback",
            self.config.redirect_url.as_deref().unwrap_or("http://localhost:4000"),
            provider_name
        );

        let auth_url = provider.authorization_url(&state, &callback_uri);

        Ok(Redirect::temporary(&auth_url))
    }

    /// Handle OAuth callback
    pub async fn callback(
        &self,
        provider_name: &str,
        code: &str,
        state: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<AuthResponse, RuntimeError> {
        // Validate state
        let saved_state = self.validate_state(state).await?;

        let provider = self.providers.get(provider_name)
            .ok_or_else(|| RuntimeError::Auth(AuthError::ProviderNotConfigured {
                provider: provider_name.to_string()
            }))?;

        // Build callback URL
        let callback_uri = format!(
            "{}/auth/{}/callback",
            self.config.redirect_url.as_deref().unwrap_or("http://localhost:4000"),
            provider_name
        );

        // Exchange code for tokens
        let tokens = provider.exchange_code(code, &callback_uri).await
            .map_err(|e| RuntimeError::Auth(e))?;

        // Get user info
        let user_info = provider.user_info(&tokens.access_token).await
            .map_err(|e| RuntimeError::Auth(e))?;

        // Find or create user
        let user_id = self.find_or_create_user(provider_name, &user_info).await?;

        // Store provider tokens
        self.store_provider_tokens(
            user_id,
            provider_name,
            &user_info.id,
            &tokens
        ).await?;

        // Create session
        let session_tokens = self.sessions.create_session(
            user_id,
            &user_info,
            ip_address,
            user_agent
        ).await?;

        // Log auth event
        self.log_auth_event(user_id, "login_success", provider_name, ip_address).await;

        // Record metrics
        record_auth_attempt(provider_name, "success");

        Ok(AuthResponse {
            access_token: session_tokens.access_token,
            refresh_token: session_tokens.refresh_token,
            token_type: session_tokens.token_type,
            expires_in: session_tokens.expires_in,
            user: UserResponse {
                id: user_id.to_string(),
                email: user_info.email,
                name: user_info.name,
                avatar: user_info.picture,
            },
            redirect_uri: saved_state.redirect_uri,
        })
    }

    async fn find_or_create_user(
        &self,
        provider: &str,
        user_info: &UserInfo,
    ) -> Result<Uuid, RuntimeError> {
        // Check if user exists by provider ID
        let existing = sqlx::query_scalar!(
            r#"
            SELECT user_id FROM _system.user_providers
            WHERE provider = $1 AND provider_user_id = $2
            "#,
            provider,
            user_info.id
        )
        .fetch_optional(&self.db)
        .await?;

        if let Some(user_id) = existing {
            return Ok(user_id);
        }

        // Check if user exists by email
        if let Some(email) = &user_info.email {
            let existing_by_email = sqlx::query_scalar!(
                r#"SELECT id FROM auth.tb_user WHERE email = $1"#,
                email
            )
            .fetch_optional(&self.db)
            .await?;

            if let Some(user_id) = existing_by_email {
                // Link provider to existing user
                self.link_provider(user_id, provider, user_info).await?;
                return Ok(user_id);
            }
        }

        // Create new user
        let user_id = self.create_user(user_info).await?;

        // Link provider
        self.link_provider(user_id, provider, user_info).await?;

        Ok(user_id)
    }

    async fn create_user(&self, user_info: &UserInfo) -> Result<Uuid, RuntimeError> {
        let mapping = self.config.user_mapping.as_ref();

        let id = Uuid::new_v4();

        // Dynamic SQL based on mapping config
        let email = user_info.email.as_deref().unwrap_or("");
        let name = user_info.name.as_deref();
        let avatar = user_info.picture.as_deref();

        sqlx::query!(
            r#"
            INSERT INTO auth.tb_user (id, email, name, avatar_url, created_at)
            VALUES ($1, $2, $3, $4, NOW())
            "#,
            id,
            email,
            name,
            avatar
        )
        .execute(&self.db)
        .await?;

        Ok(id)
    }

    async fn link_provider(
        &self,
        user_id: Uuid,
        provider: &str,
        user_info: &UserInfo,
    ) -> Result<(), RuntimeError> {
        sqlx::query!(
            r#"
            INSERT INTO _system.user_providers (user_id, provider, provider_user_id, provider_email, created_at)
            VALUES ($1, $2, $3, $4, NOW())
            ON CONFLICT (provider, provider_user_id) DO UPDATE SET
                provider_email = EXCLUDED.provider_email
            "#,
            user_id,
            provider,
            user_info.id,
            user_info.email
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Refresh tokens endpoint
    pub async fn refresh(
        &self,
        refresh_token: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<RefreshResponse, RuntimeError> {
        let tokens = self.sessions.refresh_tokens(
            refresh_token,
            ip_address,
            user_agent
        ).await?;

        Ok(RefreshResponse {
            access_token: tokens.access_token,
            expires_in: tokens.expires_in,
        })
    }

    /// Logout endpoint
    pub async fn logout(&self, user_id: Uuid) -> Result<(), RuntimeError> {
        self.sessions.revoke_all(user_id).await?;
        Ok(())
    }

    /// Get current user
    pub async fn me(&self, user_id: Uuid) -> Result<MeResponse, RuntimeError> {
        let user = sqlx::query!(
            r#"
            SELECT id, email, name, avatar_url, created_at
            FROM auth.tb_user
            WHERE id = $1
            "#,
            user_id
        )
        .fetch_one(&self.db)
        .await?;

        let providers = sqlx::query!(
            r#"
            SELECT provider, provider_email, created_at
            FROM _system.user_providers
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_all(&self.db)
        .await?;

        Ok(MeResponse {
            id: user.id.to_string(),
            email: user.email,
            name: user.name,
            avatar: user.avatar_url,
            providers: providers.iter().map(|p| ProviderInfo {
                name: p.provider.clone(),
                email: p.provider_email.clone(),
                linked_at: p.created_at,
            }).collect(),
            created_at: user.created_at,
        })
    }
}
```

---

## 5.7 Database Schema

```sql
-- migrations/003_auth_system_tables.sql

-- User providers table (multi-provider support)
CREATE TABLE IF NOT EXISTS _system.user_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    provider TEXT NOT NULL,
    provider_user_id TEXT NOT NULL,
    provider_email TEXT,
    access_token TEXT,           -- Encrypted
    refresh_token TEXT,          -- Encrypted
    token_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    UNIQUE(provider, provider_user_id)
);

CREATE INDEX IF NOT EXISTS idx_user_providers_user
ON _system.user_providers(user_id);

-- Refresh tokens table
CREATE TABLE IF NOT EXISTS _system.refresh_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,
    user_agent TEXT,
    ip_address INET
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user
ON _system.refresh_tokens(user_id);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_hash
ON _system.refresh_tokens(token_hash);

-- OAuth state tokens (CSRF protection)
CREATE TABLE IF NOT EXISTS _system.oauth_states (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    state TEXT NOT NULL UNIQUE,
    redirect_uri TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Auth events audit log
CREATE TABLE IF NOT EXISTS _system.auth_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID,
    event_type TEXT NOT NULL,
    provider TEXT,
    ip_address INET,
    user_agent TEXT,
    success BOOLEAN NOT NULL,
    failure_reason TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_auth_events_user
ON _system.auth_events(user_id);

CREATE INDEX IF NOT EXISTS idx_auth_events_created
ON _system.auth_events(created_at);
```

---

## Acceptance Criteria

- [ ] Google OAuth flow works end-to-end
- [ ] GitHub OAuth flow works
- [ ] Generic OIDC provider works
- [ ] JWT access tokens are generated correctly
- [ ] Refresh token rotation works
- [ ] User creation/linking works correctly
- [ ] CSRF state validation works
- [ ] Sessions can be revoked
- [ ] /auth/me returns correct user info
- [ ] Auth events are logged
- [ ] Metrics are recorded

---

## Providers to Implement

```
Phase 5a (Core):
- google ✅
- github ✅
- oidc (generic) ✅

Phase 5b (Popular):
- microsoft
- apple
- discord

Phase 5c (Extended):
- twitter
- linkedin
- facebook
- slack
- gitlab
- bitbucket
- twitch
- spotify
```

---

## Files to Create

```
crates/fraiseql-auth/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── config.rs
│   ├── handler.rs
│   ├── jwt.rs
│   ├── session.rs
│   ├── error.rs
│   ├── traits.rs
│   ├── testing.rs
│   ├── providers/
│   │   ├── mod.rs
│   │   ├── google.rs
│   │   ├── github.rs
│   │   ├── microsoft.rs
│   │   ├── apple.rs
│   │   ├── discord.rs
│   │   └── oidc.rs
│   └── axum.rs
└── tests/
    ├── jwt_test.rs
    ├── session_test.rs
    └── oauth_test.rs
```

---

## 5.8 Comprehensive Error Handling

### Task: Define auth-specific errors with error codes

```rust
// crates/fraiseql-auth/src/error.rs

use thiserror::Error;

/// Auth error codes for consistent error responses
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthErrorCode {
    /// AU001: Invalid access token
    InvalidToken,
    /// AU002: Access token expired
    TokenExpired,
    /// AU003: Invalid refresh token
    InvalidRefreshToken,
    /// AU004: Refresh token expired
    RefreshTokenExpired,
    /// AU005: Provider not configured
    ProviderNotConfigured,
    /// AU006: OAuth provider error
    ProviderError,
    /// AU007: Invalid OAuth state (CSRF)
    InvalidState,
    /// AU008: OAuth state expired
    StateExpired,
    /// AU009: User not found
    UserNotFound,
    /// AU010: Email not verified
    EmailNotVerified,
    /// AU011: Account disabled
    AccountDisabled,
    /// AU012: Rate limited
    RateLimited,
    /// AU013: Invalid credentials
    InvalidCredentials,
    /// AU014: Session revoked
    SessionRevoked,
}

impl AuthErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidToken => "AU001",
            Self::TokenExpired => "AU002",
            Self::InvalidRefreshToken => "AU003",
            Self::RefreshTokenExpired => "AU004",
            Self::ProviderNotConfigured => "AU005",
            Self::ProviderError => "AU006",
            Self::InvalidState => "AU007",
            Self::StateExpired => "AU008",
            Self::UserNotFound => "AU009",
            Self::EmailNotVerified => "AU010",
            Self::AccountDisabled => "AU011",
            Self::RateLimited => "AU012",
            Self::InvalidCredentials => "AU013",
            Self::SessionRevoked => "AU014",
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::InvalidToken
            | Self::TokenExpired
            | Self::InvalidRefreshToken
            | Self::RefreshTokenExpired
            | Self::InvalidState
            | Self::StateExpired
            | Self::InvalidCredentials
            | Self::SessionRevoked => StatusCode::UNAUTHORIZED,

            Self::UserNotFound => StatusCode::NOT_FOUND,
            Self::EmailNotVerified | Self::AccountDisabled => StatusCode::FORBIDDEN,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::ProviderNotConfigured | Self::ProviderError => StatusCode::BAD_GATEWAY,
        }
    }

    pub fn docs_url(&self) -> &'static str {
        match self {
            Self::InvalidToken | Self::TokenExpired
                => "https://docs.fraiseql.dev/auth/tokens#access-tokens",
            Self::InvalidRefreshToken | Self::RefreshTokenExpired
                => "https://docs.fraiseql.dev/auth/tokens#refresh-tokens",
            Self::ProviderNotConfigured
                => "https://docs.fraiseql.dev/auth/providers",
            Self::ProviderError
                => "https://docs.fraiseql.dev/auth/troubleshooting#provider-errors",
            Self::InvalidState | Self::StateExpired
                => "https://docs.fraiseql.dev/auth/oauth-flow#csrf-protection",
            Self::UserNotFound
                => "https://docs.fraiseql.dev/auth/users",
            Self::EmailNotVerified
                => "https://docs.fraiseql.dev/auth/email-verification",
            Self::AccountDisabled
                => "https://docs.fraiseql.dev/auth/account-management",
            Self::RateLimited
                => "https://docs.fraiseql.dev/auth/rate-limiting",
            Self::InvalidCredentials | Self::SessionRevoked
                => "https://docs.fraiseql.dev/auth/sessions",
        }
    }

    /// Whether this error should be logged at warn level
    pub fn is_suspicious(&self) -> bool {
        matches!(
            self,
            Self::InvalidToken
            | Self::InvalidRefreshToken
            | Self::InvalidState
            | Self::InvalidCredentials
        )
    }
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Invalid access token")]
    InvalidToken,

    #[error("Access token expired")]
    TokenExpired,

    #[error("Invalid refresh token")]
    InvalidRefreshToken,

    #[error("Refresh token expired")]
    RefreshTokenExpired,

    #[error("OAuth provider not configured: {provider}")]
    ProviderNotConfigured { provider: String },

    #[error("OAuth provider error: {provider}: {message}")]
    ProviderError { provider: String, message: String },

    #[error("Invalid OAuth state")]
    InvalidState,

    #[error("OAuth state expired")]
    StateExpired,

    #[error("User not found: {user_id}")]
    UserNotFound { user_id: String },

    #[error("Email not verified")]
    EmailNotVerified,

    #[error("Account disabled")]
    AccountDisabled,

    #[error("Rate limited, retry after {retry_after} seconds")]
    RateLimited { retry_after: u64 },

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Session revoked")]
    SessionRevoked,

    #[error("Refresh not supported by provider")]
    RefreshNotSupported,

    #[error("Database error: {0}")]
    Database(String),

    #[error("JWT error: {0}")]
    Jwt(String),
}

impl AuthError {
    pub fn error_code(&self) -> AuthErrorCode {
        match self {
            Self::InvalidToken => AuthErrorCode::InvalidToken,
            Self::TokenExpired => AuthErrorCode::TokenExpired,
            Self::InvalidRefreshToken | Self::RefreshNotSupported => AuthErrorCode::InvalidRefreshToken,
            Self::RefreshTokenExpired => AuthErrorCode::RefreshTokenExpired,
            Self::ProviderNotConfigured { .. } => AuthErrorCode::ProviderNotConfigured,
            Self::ProviderError { .. } => AuthErrorCode::ProviderError,
            Self::InvalidState => AuthErrorCode::InvalidState,
            Self::StateExpired => AuthErrorCode::StateExpired,
            Self::UserNotFound { .. } => AuthErrorCode::UserNotFound,
            Self::EmailNotVerified => AuthErrorCode::EmailNotVerified,
            Self::AccountDisabled => AuthErrorCode::AccountDisabled,
            Self::RateLimited { .. } => AuthErrorCode::RateLimited,
            Self::InvalidCredentials => AuthErrorCode::InvalidCredentials,
            Self::SessionRevoked => AuthErrorCode::SessionRevoked,
            Self::Database(_) | Self::Jwt(_) => AuthErrorCode::ProviderError,
        }
    }

    pub fn to_response(&self) -> (StatusCode, Json<Value>) {
        let code = self.error_code();

        // Log suspicious activity
        if code.is_suspicious() {
            tracing::warn!(
                error_code = %code.as_str(),
                error = %self,
                "Suspicious auth activity"
            );
        }

        let mut response = json!({
            "error": {
                "code": code.as_str(),
                "message": self.to_string(),
                "docs": code.docs_url(),
            }
        });

        // Add retry-after header info for rate limiting
        if let Self::RateLimited { retry_after } = self {
            response["error"]["retry_after"] = json!(retry_after);
        }

        (code.http_status(), Json(response))
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, body) = self.to_response();
        (status, body).into_response()
    }
}

impl From<jsonwebtoken::errors::Error> for AuthError {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => Self::TokenExpired,
            jsonwebtoken::errors::ErrorKind::InvalidToken => Self::InvalidToken,
            _ => Self::Jwt(e.to_string()),
        }
    }
}

impl From<sqlx::Error> for AuthError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.to_string())
    }
}
```

---

## 5.9 Unit Tests

### Task: Comprehensive unit tests for auth

```rust
// crates/fraiseql-auth/tests/jwt_test.rs

use fraiseql_auth::{jwt::*, testing::mocks::*};

#[test]
fn test_generate_and_verify_token() {
    let manager = JwtManagerImpl::new_for_test("test_secret");

    let user_info = UserInfo {
        id: "user_123".into(),
        email: Some("test@example.com".into()),
        name: Some("Test User".into()),
        ..Default::default()
    };

    let token = manager.generate_access_token("user_123", &user_info).unwrap();

    let claims = manager.verify_access_token(&token).unwrap();
    assert_eq!(claims.sub, "user_123");
    assert_eq!(claims.email, Some("test@example.com".into()));
}

#[test]
fn test_expired_token() {
    let manager = JwtManagerImpl::new_for_test("test_secret")
        .with_access_expiry(chrono::Duration::seconds(-1)); // Already expired

    let user_info = UserInfo::default();
    let token = manager.generate_access_token("user_123", &user_info).unwrap();

    let result = manager.verify_access_token(&token);
    assert!(matches!(result, Err(AuthError::TokenExpired)));
}

#[test]
fn test_invalid_token() {
    let manager = JwtManagerImpl::new_for_test("test_secret");

    let result = manager.verify_access_token("invalid.token.here");
    assert!(matches!(result, Err(AuthError::InvalidToken)));
}

#[test]
fn test_wrong_secret() {
    let manager1 = JwtManagerImpl::new_for_test("secret1");
    let manager2 = JwtManagerImpl::new_for_test("secret2");

    let user_info = UserInfo::default();
    let token = manager1.generate_access_token("user_123", &user_info).unwrap();

    let result = manager2.verify_access_token(&token);
    assert!(matches!(result, Err(AuthError::InvalidToken)));
}
```

```rust
// crates/fraiseql-auth/tests/session_test.rs

use fraiseql_auth::{session::*, testing::mocks::*};

#[tokio::test]
async fn test_create_session() {
    let store = MockSessionStore::new();

    let user_id = uuid::Uuid::new_v4();
    let user_info = UserInfo {
        id: user_id.to_string(),
        email: Some("test@example.com".into()),
        ..Default::default()
    };

    let tokens = store.create_session(
        user_id,
        &user_info,
        Some("127.0.0.1"),
        Some("Test Agent"),
    ).await.unwrap();

    assert!(!tokens.access_token.is_empty());
    assert!(!tokens.refresh_token.is_empty());
    assert_eq!(tokens.token_type, "Bearer");
}

#[tokio::test]
async fn test_refresh_tokens() {
    let store = MockSessionStore::new();

    let user_id = uuid::Uuid::new_v4();
    let user_info = UserInfo::default();

    let initial = store.create_session(user_id, &user_info, None, None)
        .await.unwrap();

    let refreshed = store.refresh_tokens(&initial.refresh_token, None, None)
        .await.unwrap();

    // New tokens should be different
    assert_ne!(refreshed.access_token, initial.access_token);
    assert_ne!(refreshed.refresh_token, initial.refresh_token);
}

#[tokio::test]
async fn test_refresh_token_rotation() {
    let store = MockSessionStore::new();

    let user_id = uuid::Uuid::new_v4();
    let user_info = UserInfo::default();

    let initial = store.create_session(user_id, &user_info, None, None)
        .await.unwrap();

    // First refresh should work
    let _ = store.refresh_tokens(&initial.refresh_token, None, None)
        .await.unwrap();

    // Second refresh with same token should fail (token was rotated)
    let result = store.refresh_tokens(&initial.refresh_token, None, None).await;
    assert!(matches!(result, Err(AuthError::InvalidRefreshToken)));
}

#[tokio::test]
async fn test_revoke_all_sessions() {
    let store = MockSessionStore::new();

    let user_id = uuid::Uuid::new_v4();
    let user_info = UserInfo::default();

    let session1 = store.create_session(user_id, &user_info, None, None).await.unwrap();
    let session2 = store.create_session(user_id, &user_info, None, None).await.unwrap();

    store.revoke_all(user_id).await.unwrap();

    // Both refresh tokens should be invalid
    assert!(matches!(
        store.refresh_tokens(&session1.refresh_token, None, None).await,
        Err(AuthError::InvalidRefreshToken)
    ));
    assert!(matches!(
        store.refresh_tokens(&session2.refresh_token, None, None).await,
        Err(AuthError::InvalidRefreshToken)
    ));
}

#[tokio::test]
async fn test_invalid_refresh_token() {
    let store = MockSessionStore::new();

    let result = store.refresh_tokens("nonexistent_token", None, None).await;
    assert!(matches!(result, Err(AuthError::InvalidRefreshToken)));
}
```

```rust
// crates/fraiseql-auth/tests/oauth_test.rs

use fraiseql_auth::{handler::*, testing::mocks::*};
use axum::http::StatusCode;
use axum_test::TestServer;
use std::sync::Arc;

async fn setup_test_server() -> TestServer {
    let user_info = UserInfo {
        id: "provider_user_123".into(),
        email: Some("test@example.com".into()),
        name: Some("Test User".into()),
        email_verified: Some(true),
        ..Default::default()
    };

    let provider = Arc::new(MockOAuthProvider::new("google", user_info));
    let session_store = Arc::new(MockSessionStore::new());
    let state_store = Arc::new(MockStateStore::new());
    let jwt_manager = Arc::new(MockJwtManager::new());
    let user_repo = Arc::new(MockUserRepository::new());

    let handler = AuthHandler::new_with_deps(
        AuthConfig::default(),
        vec![("google".into(), provider as Arc<dyn OAuthProvider>)].into_iter().collect(),
        session_store,
        state_store,
        jwt_manager,
        user_repo,
    );

    let app = axum::Router::new()
        .route("/auth/:provider", axum::routing::get(initiate_handler))
        .route("/auth/:provider/callback", axum::routing::get(callback_handler))
        .route("/auth/refresh", axum::routing::post(refresh_handler))
        .route("/auth/logout", axum::routing::post(logout_handler))
        .with_state(Arc::new(handler));

    TestServer::new(app).unwrap()
}

#[tokio::test]
async fn test_oauth_initiate() {
    let server = setup_test_server().await;

    let response = server.get("/auth/google").await;

    // Should redirect to provider
    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);

    let location = response.header("Location");
    assert!(location.contains("mock-provider.test"));
    assert!(location.contains("state="));
}

#[tokio::test]
async fn test_oauth_callback_success() {
    let server = setup_test_server().await;

    // First initiate to create state
    let init_response = server.get("/auth/google").await;
    let location = init_response.header("Location");

    // Extract state from redirect URL
    let state = location.split("state=").nth(1).unwrap()
        .split('&').next().unwrap();

    // Call callback with state and code
    let response = server
        .get(&format!("/auth/google/callback?code=auth_code&state={}", state))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
    assert!(body["user"]["email"].as_str() == Some("test@example.com"));
}

#[tokio::test]
async fn test_oauth_callback_invalid_state() {
    let server = setup_test_server().await;

    let response = server
        .get("/auth/google/callback?code=auth_code&state=invalid_state")
        .await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["error"]["code"], "AU007");
}

#[tokio::test]
async fn test_unknown_provider() {
    let server = setup_test_server().await;

    let response = server.get("/auth/unknown_provider").await;

    assert_eq!(response.status_code(), StatusCode::BAD_GATEWAY);

    let body: serde_json::Value = response.json();
    assert_eq!(body["error"]["code"], "AU005");
}

#[tokio::test]
async fn test_token_refresh() {
    let server = setup_test_server().await;

    // First, complete OAuth flow to get tokens
    let init_response = server.get("/auth/google").await;
    let location = init_response.header("Location");
    let state = location.split("state=").nth(1).unwrap()
        .split('&').next().unwrap();

    let auth_response = server
        .get(&format!("/auth/google/callback?code=auth_code&state={}", state))
        .await;

    let auth_body: serde_json::Value = auth_response.json();
    let refresh_token = auth_body["refresh_token"].as_str().unwrap();

    // Now refresh the token
    let refresh_response = server
        .post("/auth/refresh")
        .json(&json!({"refresh_token": refresh_token}))
        .await;

    assert_eq!(refresh_response.status_code(), StatusCode::OK);

    let refresh_body: serde_json::Value = refresh_response.json();
    assert!(refresh_body["access_token"].is_string());
}

#[tokio::test]
async fn test_logout() {
    let server = setup_test_server().await;

    // Complete OAuth flow
    let init_response = server.get("/auth/google").await;
    let location = init_response.header("Location");
    let state = location.split("state=").nth(1).unwrap()
        .split('&').next().unwrap();

    let auth_response = server
        .get(&format!("/auth/google/callback?code=auth_code&state={}", state))
        .await;

    let auth_body: serde_json::Value = auth_response.json();
    let access_token = auth_body["access_token"].as_str().unwrap();
    let refresh_token = auth_body["refresh_token"].as_str().unwrap();

    // Logout
    let logout_response = server
        .post("/auth/logout")
        .add_header("Authorization", format!("Bearer {}", access_token))
        .await;

    assert_eq!(logout_response.status_code(), StatusCode::OK);

    // Refresh should now fail
    let refresh_response = server
        .post("/auth/refresh")
        .json(&json!({"refresh_token": refresh_token}))
        .await;

    assert_eq!(refresh_response.status_code(), StatusCode::UNAUTHORIZED);
}
```

---

## DO NOT

- Do not store plaintext tokens in database - always hash
- Do not skip CSRF state validation
- Do not implement password auth in first iteration (OAuth only)
- Do not expose provider tokens to frontend
- Do not skip rate limiting on auth endpoints
