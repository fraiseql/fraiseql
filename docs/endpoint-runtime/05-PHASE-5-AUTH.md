# Phase 5: Authentication Runtime (Extended fraiseql-server)

## Objective

Implement OAuth 2.0 / OpenID Connect authentication with 12+ providers, JWT session management, token refresh, and user entity integration **as a module within the unified fraiseql-server crate**.

---

## Architecture Note

This phase extends the existing `fraiseql-server` crate (after Phase 4B restructuring). Auth becomes `crate::auth/`, using shared infrastructure:

- Configuration system (`crate::config`)
- Error handling (`crate::error`, `fraiseql-error`)
- Middleware pipeline (`crate::middleware`)
- Database connections
- Tracing and metrics (existing in server)

---

## 5.1 Auth Module Structure

```
crates/fraiseql-server/src/auth/
├── mod.rs                  # Module root, public exports
├── config.rs               # AuthConfig struct
├── error.rs                # AuthError enum with error codes
├── traits.rs               # OAuthProvider, SessionStore, etc.
├── testing.rs              # Mock implementations
├── jwt.rs                  # JWT generation and verification
├── session.rs              # Session management (create, refresh, revoke)
├── handler.rs              # Main auth orchestration
├── providers/
│   ├── mod.rs              # Provider registry
│   ├── google.rs           # Google OAuth
│   ├── github.rs           # GitHub OAuth
│   ├── microsoft.rs        # Microsoft Entra ID
│   ├── apple.rs            # Apple Sign In
│   ├── discord.rs          # Discord OAuth
│   ├── oidc.rs             # Generic OIDC
│   └── [more providers]
└── routes.rs               # Axum HTTP handlers
```

---

## 5.2 Configuration Integration

Auth config merges with RuntimeConfig:

```rust
// crates/fraiseql-server/src/config.rs

#[derive(Debug, Deserialize)]
pub struct RuntimeConfig {
    // Existing fields...
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub lifecycle: LifecycleConfig,

    // NEW: Auth configuration
    #[serde(default)]
    pub auth: Option<AuthConfig>,
}

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
```

---

## 5.3 Error Handling

Auth errors integrate with existing RuntimeError:

```rust
// crates/fraiseql-server/src/auth/error.rs

use crate::error::{RuntimeError, HttpError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthErrorCode {
    InvalidToken,           // AU001
    TokenExpired,           // AU002
    InvalidRefreshToken,    // AU003
    RefreshTokenExpired,    // AU004
    ProviderNotConfigured,  // AU005
    ProviderError,          // AU006
    InvalidState,           // AU007
    StateExpired,           // AU008
    UserNotFound,           // AU009
    EmailNotVerified,       // AU010
    AccountDisabled,        // AU011
    RateLimited,            // AU012
    InvalidCredentials,     // AU013
    SessionRevoked,         // AU014
}

impl AuthErrorCode {
    pub fn as_str(&self) -> &'static str { /* ... */ }
    pub fn http_status(&self) -> StatusCode { /* ... */ }
    pub fn docs_url(&self) -> &'static str { /* ... */ }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid access token")]
    InvalidToken,

    #[error("Access token expired")]
    TokenExpired,

    // ... other variants
}

impl From<AuthError> for RuntimeError {
    fn from(e: AuthError) -> Self {
        RuntimeError::Auth(e)
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let code = match self {
            Self::InvalidToken => AuthErrorCode::InvalidToken,
            // ...
        };

        let status = code.http_status();
        let body = json!({
            "error": {
                "code": code.as_str(),
                "message": self.to_string(),
                "docs": code.docs_url(),
            }
        });

        (status, Json(body)).into_response()
    }
}
```

---

## 5.4 OAuth Provider Trait

```rust
// crates/fraiseql-server/src/auth/traits.rs

use async_trait::async_trait;

#[async_trait]
pub trait OAuthProvider: Send + Sync {
    fn name(&self) -> &'static str;

    fn authorization_url(&self, state: &str, redirect_uri: &str) -> String;

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<TokenResponse, AuthError>;

    async fn user_info(&self, access_token: &str) -> Result<UserInfo, AuthError>;

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

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

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

pub trait JwtManager: Send + Sync {
    fn generate_access_token(&self, user_id: &str, user: &UserInfo) -> Result<String, AuthError>;
    fn verify_access_token(&self, token: &str) -> Result<Claims, AuthError>;
    fn access_expiry(&self) -> chrono::Duration;
    fn refresh_expiry(&self) -> chrono::Duration;
}
```

---

## 5.5 Provider Implementations

### Google Provider

```rust
// crates/fraiseql-server/src/auth/providers/google.rs

pub struct GoogleProvider {
    client_id: String,
    client_secret: String,
    scopes: Vec<String>,
}

impl GoogleProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, AuthError> {
        let client_id = std::env::var(&config.client_id_env)?;
        let client_secret = std::env::var(&config.client_secret_env)?;

        let scopes = config.scopes.clone().unwrap_or_else(|| vec![
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

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<TokenResponse, AuthError> {
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
// crates/fraiseql-server/src/auth/providers/github.rs

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
            email_verified: Some(true),
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

---

## 5.6 Session Management

```rust
// crates/fraiseql-server/src/auth/session.rs

use sqlx::PgPool;
use uuid::Uuid;
use sha2::{Sha256, Digest};

pub struct SessionManager {
    db: PgPool,
    jwt: Arc<dyn JwtManager>,
}

impl SessionManager {
    pub fn new(db: PgPool, jwt: Arc<dyn JwtManager>) -> Self {
        Self { db, jwt }
    }

    pub async fn create_session(
        &self,
        user_id: Uuid,
        user_info: &UserInfo,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<TokenPair, AuthError> {
        let access_token = self.jwt.generate_access_token(&user_id.to_string(), user_info)?;

        let refresh_token = self.generate_refresh_token();
        let refresh_hash = self.hash_token(&refresh_token);

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

    pub async fn refresh_tokens(
        &self,
        refresh_token: &str,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<TokenPair, AuthError> {
        let refresh_hash = self.hash_token(refresh_token);

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

        let user = sqlx::query!(
            r#"SELECT id, email, name FROM auth.tb_user WHERE id = $1"#,
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

        self.create_session(record.user_id, &user_info, ip_address, user_agent).await
    }

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
        use rand::Rng;
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

## 5.7 HTTP Routes

```rust
// crates/fraiseql-server/src/auth/routes.rs

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Redirect,
    routing::{get, post},
    Router,
    Json,
};
use std::sync::Arc;

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/auth/:provider", get(initiate))
        .route("/auth/:provider/callback", get(callback))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .with_state(state)
}

async fn initiate(
    State(state): State<Arc<AppState>>,
    Path(provider_name): Path<String>,
    Query(params): Query<serde_json::Value>,
) -> Result<Redirect, RuntimeError> {
    let redirect_uri = params.get("redirect_uri").and_then(|v| v.as_str());

    let auth = state.auth.as_ref()
        .ok_or_else(|| RuntimeError::Auth(AuthError::ProviderNotConfigured {
            provider: "auth_not_configured".to_string(),
        }))?;

    let provider = auth.providers.get(&provider_name)
        .ok_or_else(|| RuntimeError::Auth(AuthError::ProviderNotConfigured {
            provider: provider_name.clone(),
        }))?;

    let state_token = auth.generate_state(redirect_uri).await?;
    let callback_url = format!("{}/auth/{}/callback", auth.config.redirect_url.as_deref().unwrap_or(""), provider_name);

    let auth_url = provider.authorization_url(&state_token, &callback_url);

    Ok(Redirect::temporary(&auth_url))
}

async fn callback(
    State(state): State<Arc<AppState>>,
    Path(provider_name): Path<String>,
    Query(params): Query<CallbackParams>,
    headers: HeaderMap,
) -> Result<Json<AuthResponse>, RuntimeError> {
    let auth = state.auth.as_ref()
        .ok_or_else(|| RuntimeError::Auth(AuthError::ProviderNotConfigured {
            provider: provider_name.clone(),
        }))?;

    let saved_state = auth.validate_state(&params.state).await?;

    let provider = auth.providers.get(&provider_name)
        .ok_or_else(|| RuntimeError::Auth(AuthError::ProviderNotConfigured {
            provider: provider_name.clone(),
        }))?;

    let callback_url = format!("{}/auth/{}/callback", auth.config.redirect_url.as_deref().unwrap_or(""), provider_name);
    let tokens = provider.exchange_code(&params.code, &callback_url).await?;
    let user_info = provider.user_info(&tokens.access_token).await?;

    let user_id = auth.find_or_create_user(&provider_name, &user_info).await?;

    let ip_address = headers.get("x-forwarded-for")
        .and_then(|v| v.to_str().ok());
    let user_agent = headers.get("user-agent")
        .and_then(|v| v.to_str().ok());

    let session_tokens = auth.sessions.create_session(
        user_id,
        &user_info,
        ip_address,
        user_agent
    ).await?;

    auth.log_auth_event(user_id, "login_success", &provider_name, ip_address).await;

    Ok(Json(AuthResponse {
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
    }))
}

async fn refresh(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RefreshRequest>,
) -> Result<Json<RefreshResponse>, RuntimeError> {
    let auth = state.auth.as_ref()
        .ok_or_else(|| RuntimeError::Auth(AuthError::ProviderNotConfigured {
            provider: "auth_not_configured".to_string(),
        }))?;

    let ip_address = headers.get("x-forwarded-for")
        .and_then(|v| v.to_str().ok());
    let user_agent = headers.get("user-agent")
        .and_then(|v| v.to_str().ok());

    let tokens = auth.sessions.refresh_tokens(
        &payload.refresh_token,
        ip_address,
        user_agent
    ).await?;

    Ok(Json(RefreshResponse {
        access_token: tokens.access_token,
        token_type: tokens.token_type,
        expires_in: tokens.expires_in,
    }))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<StatusCode, RuntimeError> {
    let user_id = extract_user_id(&headers)?;

    let auth = state.auth.as_ref()
        .ok_or_else(|| RuntimeError::Auth(AuthError::ProviderNotConfigured {
            provider: "auth_not_configured".to_string(),
        }))?;

    auth.sessions.revoke_all(user_id).await?;

    Ok(StatusCode::OK)
}

async fn me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, RuntimeError> {
    let user_id = extract_user_id(&headers)?;

    let auth = state.auth.as_ref()
        .ok_or_else(|| RuntimeError::Auth(AuthError::ProviderNotConfigured {
            provider: "auth_not_configured".to_string(),
        }))?;

    let user = sqlx::query!(
        r#"SELECT id, email, name, avatar_url, created_at FROM auth.tb_user WHERE id = $1"#,
        user_id
    )
    .fetch_one(&state.db)
    .await?;

    let providers = sqlx::query!(
        r#"SELECT provider, provider_email, created_at FROM _system.user_providers WHERE user_id = $1"#,
        user_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(MeResponse {
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
    }))
}

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

#[derive(Debug, Serialize)]
struct AuthResponse {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_in: u64,
    user: UserResponse,
    redirect_uri: Option<String>,
}

#[derive(Debug, Serialize)]
struct UserResponse {
    id: String,
    email: Option<String>,
    name: Option<String>,
    avatar: Option<String>,
}

#[derive(Debug, Serialize)]
struct RefreshResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
}

#[derive(Debug, Serialize)]
struct MeResponse {
    id: String,
    email: String,
    name: Option<String>,
    avatar: Option<String>,
    providers: Vec<ProviderInfo>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct ProviderInfo {
    name: String,
    email: Option<String>,
    linked_at: chrono::DateTime<chrono::Utc>,
}

fn extract_user_id(headers: &HeaderMap) -> Result<Uuid, RuntimeError> {
    let auth_header = headers.get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(RuntimeError::Auth(AuthError::InvalidToken))?;

    let token = auth_header.strip_prefix("Bearer ")
        .ok_or(RuntimeError::Auth(AuthError::InvalidToken))?;

    // Extract user_id from JWT claims
    // This would normally use the JwtManager to verify the token
    // For now, simplified placeholder

    todo!("Extract user_id from token")
}
```

---

## 5.8 Database Schema

```sql
-- migrations/003_auth_system_tables.sql

CREATE TABLE IF NOT EXISTS _system.user_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    provider TEXT NOT NULL,
    provider_user_id TEXT NOT NULL,
    provider_email TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    UNIQUE(provider, provider_user_id)
);

CREATE INDEX idx_user_providers_user ON _system.user_providers(user_id);

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

CREATE INDEX idx_refresh_tokens_user ON _system.refresh_tokens(user_id);
CREATE INDEX idx_refresh_tokens_hash ON _system.refresh_tokens(token_hash);

CREATE TABLE IF NOT EXISTS _system.oauth_states (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    state TEXT NOT NULL UNIQUE,
    redirect_uri TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

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

CREATE INDEX idx_auth_events_user ON _system.auth_events(user_id);
CREATE INDEX idx_auth_events_created ON _system.auth_events(created_at);
```

---

## 5.9 Testing

See original Phase 5 documentation for comprehensive unit tests (`tests/jwt_test.rs`, `tests/session_test.rs`, `tests/oauth_test.rs`).

All tests import from `fraiseql_server::auth::*` instead of separate crates.

---

## 5.10 Integration with AppState

Auth is integrated into AppState:

```rust
// crates/fraiseql-server/src/state.rs

pub struct AppState {
    pub db: PgPool,
    pub config: RuntimeConfig,
    pub lifecycle: Arc<ShutdownCoordinator>,

    // Phase 3: Webhooks
    pub webhooks: Option<Arc<WebhookHandler>>,

    // Phase 4: Files
    pub files: Option<Arc<FileManager>>,

    // Phase 5: Auth
    pub auth: Option<Arc<AuthManager>>,

    // Phase 6+: More features
}
```

---

## 5.11 Configuration Example

```toml
# fraiseql.toml

[auth]
session_type = "jwt"
access_token_expiry = "15m"
refresh_token_expiry = "7d"
jwt_secret_env = "JWT_SECRET"
redirect_url = "https://api.example.com"

[auth.providers.google]
client_id_env = "GOOGLE_CLIENT_ID"
client_secret_env = "GOOGLE_CLIENT_SECRET"
scopes = ["openid", "email", "profile"]

[auth.providers.github]
client_id_env = "GITHUB_CLIENT_ID"
client_secret_env = "GITHUB_CLIENT_SECRET"
scopes = ["user:email"]

[auth.providers.oidc]
client_id_env = "OIDC_CLIENT_ID"
client_secret_env = "OIDC_CLIENT_SECRET"
```

---

## Acceptance Criteria

- [ ] OAuth flows work for Google, GitHub, generic OIDC
- [ ] JWT access tokens generated and verified correctly
- [ ] Refresh token rotation works
- [ ] User creation/linking works
- [ ] CSRF state validation works
- [ ] Sessions can be revoked
- [ ] /auth/me returns correct user info
- [ ] Auth events logged to database
- [ ] Metrics recorded for auth attempts
- [ ] All tests pass

---

## Files to Create (within fraiseql-server)

```
crates/fraiseql-server/src/auth/
├── mod.rs
├── config.rs
├── error.rs
├── traits.rs
├── testing.rs
├── jwt.rs
├── session.rs
├── handler.rs
├── providers/
│   ├── mod.rs
│   ├── google.rs
│   ├── github.rs
│   ├── microsoft.rs
│   ├── apple.rs
│   ├── discord.rs
│   └── oidc.rs
└── routes.rs

crates/fraiseql-server/tests/
├── auth_jwt_test.rs
├── auth_session_test.rs
└── auth_oauth_test.rs
```

---

## Notes

- After Phase 4B, auth is added directly to fraiseql-server
- No new crates created
- Reuses existing middleware, config, error handling
- Single source of truth for all server features
- Phases 6-10 follow the same pattern

---

## Design Enhancements (Phase 5 Refined)

See `05-PHASE-5-AUTH-DESIGN.md` for detailed architecture focusing on:

- **Performance**: Token caching (<100µs latency), JWKS caching (99% hit rate), connection pooling
- **DX**: Environment-based config, one-command setup scripts, clear error messages with docs links
- **Pluggability**: Trait-based provider system, middleware hooks, custom session stores (Postgres/Redis/In-Memory)

The refined design improves upon this specification with concrete performance targets and practical developer experience patterns.

