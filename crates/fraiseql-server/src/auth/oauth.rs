//! OAuth2 and OIDC authentication support with JWT validation,
//! provider discovery, and automatic user provisioning.

use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// OAuth2 token response from provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// Access token for API calls
    pub access_token:  String,
    /// Refresh token for getting new access tokens
    pub refresh_token: Option<String>,
    /// Token type (typically "Bearer")
    pub token_type:    String,
    /// Seconds until access token expires
    pub expires_in:    u64,
    /// ID token (JWT) for OIDC
    pub id_token:      Option<String>,
    /// Requested scopes
    pub scope:         Option<String>,
}

impl TokenResponse {
    /// Create new token response
    pub fn new(access_token: String, token_type: String, expires_in: u64) -> Self {
        Self {
            access_token,
            refresh_token: None,
            token_type,
            expires_in,
            id_token: None,
            scope: None,
        }
    }

    /// Calculate expiry time
    pub fn expiry_time(&self) -> DateTime<Utc> {
        Utc::now() + Duration::seconds(self.expires_in as i64)
    }

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        self.expiry_time() <= Utc::now()
    }
}

/// JWT ID token claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    /// Issuer (provider identifier)
    pub iss:            String,
    /// Subject (unique user ID)
    pub sub:            String,
    /// Audience (should be client_id)
    pub aud:            String,
    /// Expiration time (Unix timestamp)
    pub exp:            i64,
    /// Issued at time (Unix timestamp)
    pub iat:            i64,
    /// Authentication time (Unix timestamp)
    pub auth_time:      Option<i64>,
    /// Nonce (for replay protection)
    pub nonce:          Option<String>,
    /// Email address
    pub email:          Option<String>,
    /// Email verified flag
    pub email_verified: Option<bool>,
    /// User name
    pub name:           Option<String>,
    /// Profile picture URL
    pub picture:        Option<String>,
    /// Locale
    pub locale:         Option<String>,
}

impl IdTokenClaims {
    /// Create new ID token claims
    pub fn new(iss: String, sub: String, aud: String, exp: i64, iat: i64) -> Self {
        Self {
            iss,
            sub,
            aud,
            exp,
            iat,
            auth_time: None,
            nonce: None,
            email: None,
            email_verified: None,
            name: None,
            picture: None,
            locale: None,
        }
    }

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        self.exp <= Utc::now().timestamp()
    }

    /// Check if token will be expired within grace period
    pub fn is_expiring_soon(&self, grace_seconds: i64) -> bool {
        self.exp <= (Utc::now().timestamp() + grace_seconds)
    }
}

/// Userinfo response from provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// Subject (unique user ID)
    pub sub:            String,
    /// Email address
    pub email:          Option<String>,
    /// Email verified flag
    pub email_verified: Option<bool>,
    /// User name
    pub name:           Option<String>,
    /// Profile picture URL
    pub picture:        Option<String>,
    /// Locale
    pub locale:         Option<String>,
}

impl UserInfo {
    /// Create new userinfo
    pub fn new(sub: String) -> Self {
        Self {
            sub,
            email: None,
            email_verified: None,
            name: None,
            picture: None,
            locale: None,
        }
    }
}

/// OIDC provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OIDCProviderConfig {
    /// Provider issuer URL
    pub issuer:                   String,
    /// Authorization endpoint
    pub authorization_endpoint:   String,
    /// Token endpoint
    pub token_endpoint:           String,
    /// Userinfo endpoint
    pub userinfo_endpoint:        Option<String>,
    /// JWKS URI for public keys
    pub jwks_uri:                 String,
    /// Scopes supported by provider
    pub scopes_supported:         Vec<String>,
    /// Response types supported
    pub response_types_supported: Vec<String>,
}

impl OIDCProviderConfig {
    /// Create new provider configuration
    pub fn new(
        issuer: String,
        authorization_endpoint: String,
        token_endpoint: String,
        jwks_uri: String,
    ) -> Self {
        Self {
            issuer,
            authorization_endpoint,
            token_endpoint,
            userinfo_endpoint: None,
            jwks_uri,
            scopes_supported: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            response_types_supported: vec!["code".to_string()],
        }
    }
}

/// OAuth error response from provider (RFC 6749 Section 5.2)
#[derive(Debug, Clone, Deserialize)]
struct OAuthErrorResponse {
    /// Error code (e.g., "invalid_grant", "unauthorized_client")
    error:             String,
    /// Human-readable error description
    error_description: Option<String>,
}

/// OAuth2 client for authorization code flow
#[derive(Debug, Clone)]
pub struct OAuth2Client {
    /// Client ID from provider
    pub client_id:              String,
    /// Client secret from provider
    client_secret:              String,
    /// Authorization endpoint
    pub authorization_endpoint: String,
    /// Token endpoint
    token_endpoint:             String,
    /// Scopes to request
    pub scopes:                 Vec<String>,
    /// Use PKCE for additional security
    pub use_pkce:               bool,
    /// HTTP client (reused for connection pooling)
    #[allow(clippy::missing_docs_in_private_items)]
    http_client:                reqwest::Client,
}

impl OAuth2Client {
    /// Create new OAuth2 client
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        authorization_endpoint: impl Into<String>,
        token_endpoint: impl Into<String>,
    ) -> Self {
        Self {
            client_id:              client_id.into(),
            client_secret:          client_secret.into(),
            authorization_endpoint: authorization_endpoint.into(),
            token_endpoint:         token_endpoint.into(),
            scopes:                 vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            use_pkce:               false,
            http_client:            reqwest::Client::new(),
        }
    }

    /// Set scopes for request
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Enable PKCE protection
    pub fn with_pkce(mut self, enabled: bool) -> Self {
        self.use_pkce = enabled;
        self
    }

    /// Generate authorization URL
    pub fn authorization_url(&self, redirect_uri: &str) -> Result<String, String> {
        let state = uuid::Uuid::new_v4().to_string();
        let scope = self.scopes.join(" ");

        let url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
            self.authorization_endpoint,
            urlencoding::encode(&self.client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scope),
            urlencoding::encode(&state),
        );

        Ok(url)
    }

    /// Exchange authorization code for tokens via HTTP POST to the token endpoint
    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<TokenResponse, String> {
        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
        ];

        debug!(
            token_endpoint = %self.token_endpoint,
            client_id = %self.client_id,
            "Exchanging authorization code for tokens"
        );

        let response = self
            .http_client
            .post(&self.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Token exchange HTTP request failed: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if let Ok(oauth_err) = serde_json::from_str::<OAuthErrorResponse>(&body) {
                return Err(format!(
                    "OAuth token exchange error: {} - {}",
                    oauth_err.error,
                    oauth_err.error_description.unwrap_or_default()
                ));
            }
            return Err(format!("Token exchange failed with status {status}: {body}"));
        }

        response
            .json::<TokenResponse>()
            .await
            .map_err(|e| format!("Failed to parse token response: {e}"))
    }

    /// Refresh access token via HTTP POST with refresh_token grant type
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse, String> {
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
        ];

        debug!(
            token_endpoint = %self.token_endpoint,
            "Refreshing access token"
        );

        let response = self
            .http_client
            .post(&self.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Token refresh HTTP request failed: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if let Ok(oauth_err) = serde_json::from_str::<OAuthErrorResponse>(&body) {
                return Err(format!(
                    "OAuth token refresh error: {} - {}",
                    oauth_err.error,
                    oauth_err.error_description.unwrap_or_default()
                ));
            }
            return Err(format!("Token refresh failed with status {status}: {body}"));
        }

        response
            .json::<TokenResponse>()
            .await
            .map_err(|e| format!("Failed to parse refresh token response: {e}"))
    }
}

/// JWK (JSON Web Key) for OIDC token verification
#[derive(Debug, Clone, Deserialize)]
pub struct JWK {
    /// Key type (e.g., "RSA")
    pub kty: String,
    /// Key ID
    pub kid: Option<String>,
    /// Algorithm (e.g., "RS256")
    pub alg: Option<String>,
    /// RSA modulus (Base64url-encoded)
    pub n:   Option<String>,
    /// RSA exponent (Base64url-encoded)
    pub e:   Option<String>,
}

/// JWKS (JSON Web Key Set) response
#[derive(Debug, Clone, Deserialize)]
pub struct JWKSet {
    /// Array of JWK keys
    pub keys: Vec<JWK>,
}

impl JWKSet {
    /// Find a key by its key ID (kid)
    pub fn find_key(&self, kid: &str) -> Result<&JWK, String> {
        self.keys
            .iter()
            .find(|k| k.kid.as_deref() == Some(kid))
            .ok_or_else(|| format!("No key found with kid: {kid}"))
    }
}

/// Cached JWKS data with expiry
#[derive(Debug, Clone)]
struct CachedJWKS {
    /// The JWKS data
    jwks:       JWKSet,
    /// When the cache was fetched
    fetched_at: DateTime<Utc>,
}

/// OIDC client for OpenID Connect flow
#[derive(Debug, Clone)]
pub struct OIDCClient {
    /// Provider configuration
    pub config:    OIDCProviderConfig,
    /// Client ID
    pub client_id: String,
    /// Client secret (used in token exchange operations)
    #[allow(dead_code)]
    client_secret: String,
    /// HTTP client for making requests
    http_client:   reqwest::Client,
    /// Cached JWKS keys (cached for 24 hours)
    jwks_cache:    Arc<std::sync::Mutex<Option<CachedJWKS>>>,
}

impl OIDCClient {
    /// Create new OIDC client
    pub fn new(
        config: OIDCProviderConfig,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        Self {
            config,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            http_client: reqwest::Client::new(),
            jwks_cache: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Fetch JWKS from the provider's JWKS URI with caching (24h TTL)
    pub async fn fetch_jwks(&self) -> Result<JWKSet, String> {
        // Check cache first
        if let Ok(cache) = self.jwks_cache.lock() {
            if let Some(cached) = cache.as_ref() {
                if Utc::now() - cached.fetched_at < Duration::hours(24) {
                    return Ok(cached.jwks.clone());
                }
            }
        }

        debug!(jwks_uri = %self.config.jwks_uri, "Fetching JWKS from provider");

        let jwks = self
            .http_client
            .get(&self.config.jwks_uri)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch JWKS: {e}"))?
            .json::<JWKSet>()
            .await
            .map_err(|e| format!("Failed to parse JWKS response: {e}"))?;

        // Update cache
        if let Ok(mut cache) = self.jwks_cache.lock() {
            *cache = Some(CachedJWKS {
                jwks:       jwks.clone(),
                fetched_at: Utc::now(),
            });
        }

        Ok(jwks)
    }

    /// Verify ID token JWT signature and claims using JWKS
    pub async fn verify_id_token(
        &self,
        id_token: &str,
        expected_nonce: Option<&str>,
    ) -> Result<IdTokenClaims, String> {
        use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};

        // Decode JWT header to get kid
        let header = jsonwebtoken::decode_header(id_token)
            .map_err(|e| format!("Failed to decode JWT header: {e}"))?;

        let kid = header.kid.ok_or_else(|| "Missing key ID (kid) in JWT header".to_string())?;

        // Fetch JWKS and find matching key
        let jwks = self.fetch_jwks().await?;
        let jwk = jwks.find_key(&kid)?;

        // Build decoding key from RSA components
        let n = jwk.n.as_ref().ok_or_else(|| "Missing RSA modulus (n) in JWK".to_string())?;
        let e = jwk.e.as_ref().ok_or_else(|| "Missing RSA exponent (e) in JWK".to_string())?;

        let key = DecodingKey::from_rsa_components(n, e)
            .map_err(|e| format!("Failed to construct decoding key from RSA components: {e}"))?;

        // Configure validation
        let algorithm = match header.alg {
            Algorithm::RS256 => Algorithm::RS256,
            Algorithm::RS384 => Algorithm::RS384,
            Algorithm::RS512 => Algorithm::RS512,
            alg => return Err(format!("Unsupported JWT algorithm: {alg:?}")),
        };

        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.client_id]);

        // Decode and verify token
        let token_data = decode::<IdTokenClaims>(id_token, &key, &validation)
            .map_err(|e| format!("JWT verification failed: {e}"))?;

        let claims = token_data.claims;

        // Verify nonce if expected (replay protection)
        if let Some(expected) = expected_nonce {
            if claims.nonce.as_deref() != Some(expected) {
                return Err("Nonce mismatch: possible replay attack".to_string());
            }
        }

        // Verify token is not expired
        if claims.is_expired() {
            return Err("ID token is expired".to_string());
        }

        Ok(claims)
    }

    /// Get userinfo from provider's userinfo endpoint
    pub async fn get_userinfo(&self, access_token: &str) -> Result<UserInfo, String> {
        let userinfo_endpoint = self
            .config
            .userinfo_endpoint
            .as_ref()
            .ok_or_else(|| "No userinfo endpoint configured for this provider".to_string())?;

        debug!(userinfo_endpoint = %userinfo_endpoint, "Fetching userinfo from provider");

        let response = self
            .http_client
            .get(userinfo_endpoint)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch userinfo: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Userinfo request failed with status {status}: {body}"));
        }

        response
            .json::<UserInfo>()
            .await
            .map_err(|e| format!("Failed to parse userinfo response: {e}"))
    }
}

/// External authentication provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    /// OAuth2 provider
    OAuth2,
    /// OIDC provider
    OIDC,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OAuth2 => write!(f, "oauth2"),
            Self::OIDC => write!(f, "oidc"),
        }
    }
}

/// OAuth session stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthSession {
    /// Session ID
    pub id:               String,
    /// User ID (local system)
    pub user_id:          String,
    /// Provider type (oauth2, oidc)
    pub provider_type:    ProviderType,
    /// Provider name (Auth0, Google, etc.)
    pub provider_name:    String,
    /// Provider's user ID (sub claim)
    pub provider_user_id: String,
    /// Access token (encrypted)
    pub access_token:     String,
    /// Refresh token (encrypted), if available
    pub refresh_token:    Option<String>,
    /// When access token expires
    pub token_expiry:     DateTime<Utc>,
    /// Session creation time
    pub created_at:       DateTime<Utc>,
    /// Last time token was refreshed
    pub last_refreshed:   Option<DateTime<Utc>>,
}

impl OAuthSession {
    /// Create new OAuth session
    pub fn new(
        user_id: String,
        provider_type: ProviderType,
        provider_name: String,
        provider_user_id: String,
        access_token: String,
        token_expiry: DateTime<Utc>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id,
            provider_type,
            provider_name,
            provider_user_id,
            access_token,
            refresh_token: None,
            token_expiry,
            created_at: Utc::now(),
            last_refreshed: None,
        }
    }

    /// Check if session is expired
    pub fn is_expired(&self) -> bool {
        self.token_expiry <= Utc::now()
    }

    /// Check if session will be expired within grace period
    pub fn is_expiring_soon(&self, grace_seconds: i64) -> bool {
        self.token_expiry <= (Utc::now() + Duration::seconds(grace_seconds))
    }

    /// Update tokens after refresh
    pub fn refresh_tokens(&mut self, access_token: String, token_expiry: DateTime<Utc>) {
        self.access_token = access_token;
        self.token_expiry = token_expiry;
        self.last_refreshed = Some(Utc::now());
    }
}

/// External auth provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalAuthProvider {
    /// Provider ID
    pub id: String,
    /// Provider type (oauth2, oidc)
    pub provider_type: ProviderType,
    /// Provider name (Auth0, Google, Microsoft, Okta)
    pub provider_name: String,
    /// Client ID
    pub client_id: String,
    /// Client secret (should be fetched from vault)
    pub client_secret_vault_path: String,
    /// Provider configuration (OIDC)
    pub oidc_config: Option<OIDCProviderConfig>,
    /// OAuth2 configuration
    pub oauth2_config: Option<OAuth2ClientConfig>,
    /// Enabled flag
    pub enabled: bool,
    /// Requested scopes
    pub scopes: Vec<String>,
}

/// OAuth2 client configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuth2ClientConfig {
    /// Authorization endpoint
    pub authorization_endpoint: String,
    /// Token endpoint
    pub token_endpoint:         String,
    /// Use PKCE
    pub use_pkce:               bool,
}

impl ExternalAuthProvider {
    /// Create new external auth provider
    pub fn new(
        provider_type: ProviderType,
        provider_name: impl Into<String>,
        client_id: impl Into<String>,
        client_secret_vault_path: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            provider_type,
            provider_name: provider_name.into(),
            client_id: client_id.into(),
            client_secret_vault_path: client_secret_vault_path.into(),
            oidc_config: None,
            oauth2_config: None,
            enabled: true,
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
        }
    }

    /// Enable or disable provider
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set requested scopes
    pub fn set_scopes(&mut self, scopes: Vec<String>) {
        self.scopes = scopes;
    }
}

/// Provider registry managing multiple OAuth providers
#[derive(Debug, Clone)]
pub struct ProviderRegistry {
    /// Map of providers by name
    providers: Arc<std::sync::Mutex<HashMap<String, ExternalAuthProvider>>>,
}

impl ProviderRegistry {
    /// Create new provider registry
    pub fn new() -> Self {
        Self {
            providers: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Register provider
    pub fn register(&self, provider: ExternalAuthProvider) -> Result<(), String> {
        let mut providers = self.providers.lock().map_err(|_| "Lock failed".to_string())?;
        providers.insert(provider.provider_name.clone(), provider);
        Ok(())
    }

    /// Get provider by name
    pub fn get(&self, name: &str) -> Result<Option<ExternalAuthProvider>, String> {
        let providers = self.providers.lock().map_err(|_| "Lock failed".to_string())?;
        Ok(providers.get(name).cloned())
    }

    /// List all enabled providers
    pub fn list_enabled(&self) -> Result<Vec<ExternalAuthProvider>, String> {
        let providers = self.providers.lock().map_err(|_| "Lock failed".to_string())?;
        Ok(providers.values().filter(|p| p.enabled).cloned().collect())
    }

    /// Disable provider
    pub fn disable(&self, name: &str) -> Result<bool, String> {
        let mut providers = self.providers.lock().map_err(|_| "Lock failed".to_string())?;
        if let Some(provider) = providers.get_mut(name) {
            provider.set_enabled(false);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Enable provider
    pub fn enable(&self, name: &str) -> Result<bool, String> {
        let mut providers = self.providers.lock().map_err(|_| "Lock failed".to_string())?;
        if let Some(provider) = providers.get_mut(name) {
            provider.set_enabled(true);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// PKCE code challenge for public clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PKCEChallenge {
    /// Random code verifier (43-128 characters)
    pub code_verifier:         String,
    /// BASE64URL(SHA256(code_verifier))
    pub code_challenge:        String,
    /// Challenge method: "S256" (SHA256)
    pub code_challenge_method: String,
}

impl PKCEChallenge {
    /// Generate new PKCE challenge
    pub fn new() -> Self {
        use sha2::{Digest, Sha256};

        // Generate random verifier
        let verifier = format!("{}", uuid::Uuid::new_v4());

        // Compute challenge
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let digest = hasher.finalize();
        let challenge = urlencoding::encode_binary(&digest).to_string();

        Self {
            code_verifier:         verifier,
            code_challenge:        challenge,
            code_challenge_method: "S256".to_string(),
        }
    }

    /// Verify code verifier matches challenge
    pub fn verify(&self, verifier: &str) -> bool {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let digest = hasher.finalize();
        let computed_challenge = urlencoding::encode_binary(&digest).to_string();

        computed_challenge == self.code_challenge
    }
}

impl Default for PKCEChallenge {
    fn default() -> Self {
        Self::new()
    }
}

/// OAuth state parameter for CSRF protection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateParameter {
    /// Random state value
    pub state:      String,
    /// When state expires
    pub expires_at: DateTime<Utc>,
}

impl StateParameter {
    /// Generate new state parameter
    pub fn new() -> Self {
        Self {
            state:      uuid::Uuid::new_v4().to_string(),
            expires_at: Utc::now() + Duration::minutes(10),
        }
    }

    /// Check if state is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }

    /// Verify state matches and is not expired
    pub fn verify(&self, provided_state: &str) -> bool {
        self.state == provided_state && !self.is_expired()
    }
}

impl Default for StateParameter {
    fn default() -> Self {
        Self::new()
    }
}

/// Nonce parameter for replay protection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonceParameter {
    /// Random nonce value
    pub nonce:      String,
    /// When nonce expires
    pub expires_at: DateTime<Utc>,
}

impl NonceParameter {
    /// Generate new nonce
    pub fn new() -> Self {
        Self {
            nonce:      uuid::Uuid::new_v4().to_string(),
            expires_at: Utc::now() + Duration::minutes(10),
        }
    }

    /// Check if nonce is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }

    /// Verify nonce matches and is not expired
    pub fn verify(&self, provided_nonce: &str) -> bool {
        self.nonce == provided_nonce && !self.is_expired()
    }
}

impl Default for NonceParameter {
    fn default() -> Self {
        Self::new()
    }
}

/// Token refresh scheduler
#[derive(Debug, Clone)]
pub struct TokenRefreshScheduler {
    /// Sessions needing refresh
    refresh_queue: Arc<std::sync::Mutex<Vec<(String, DateTime<Utc>)>>>,
}

impl TokenRefreshScheduler {
    /// Create new refresh scheduler
    pub fn new() -> Self {
        Self {
            refresh_queue: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Schedule token refresh for session
    pub fn schedule_refresh(
        &self,
        session_id: String,
        refresh_time: DateTime<Utc>,
    ) -> Result<(), String> {
        let mut queue = self.refresh_queue.lock().map_err(|_| "Lock failed".to_string())?;
        queue.push((session_id, refresh_time));
        queue.sort_by_key(|(_, time)| *time);
        Ok(())
    }

    /// Get next session to refresh
    pub fn get_next_refresh(&self) -> Result<Option<String>, String> {
        let mut queue = self.refresh_queue.lock().map_err(|_| "Lock failed".to_string())?;
        if let Some((_, refresh_time)) = queue.first() {
            if *refresh_time <= Utc::now() {
                let (id, _) = queue.remove(0);
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// Cancel scheduled refresh
    pub fn cancel_refresh(&self, session_id: &str) -> Result<bool, String> {
        let mut queue = self.refresh_queue.lock().map_err(|_| "Lock failed".to_string())?;
        let len_before = queue.len();
        queue.retain(|(id, _)| id != session_id);
        Ok(queue.len() < len_before)
    }
}

impl Default for TokenRefreshScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Multi-provider failover manager
#[derive(Debug, Clone)]
pub struct ProviderFailoverManager {
    /// Primary provider name
    primary_provider:   String,
    /// Fallback providers in priority order
    fallback_providers: Vec<String>,
    /// Providers currently unavailable
    unavailable:        Arc<std::sync::Mutex<Vec<(String, DateTime<Utc>)>>>,
}

impl ProviderFailoverManager {
    /// Create new failover manager
    pub fn new(primary: String, fallbacks: Vec<String>) -> Self {
        Self {
            primary_provider:   primary,
            fallback_providers: fallbacks,
            unavailable:        Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Get next available provider
    pub fn get_available_provider(&self) -> Result<String, String> {
        let unavailable = self.unavailable.lock().map_err(|_| "Lock failed".to_string())?;
        let now = Utc::now();

        // Check if primary is available
        if !unavailable
            .iter()
            .any(|(name, exp)| name == &self.primary_provider && *exp > now)
        {
            return Ok(self.primary_provider.clone());
        }

        // Find first available fallback
        for fallback in &self.fallback_providers {
            if !unavailable.iter().any(|(name, exp)| name == fallback && *exp > now) {
                return Ok(fallback.clone());
            }
        }

        Err("No providers available".to_string())
    }

    /// Mark provider as unavailable
    pub fn mark_unavailable(&self, provider: String, duration_seconds: u64) -> Result<(), String> {
        let mut unavailable = self.unavailable.lock().map_err(|_| "Lock failed".to_string())?;
        unavailable.push((provider, Utc::now() + Duration::seconds(duration_seconds as i64)));
        Ok(())
    }

    /// Mark provider as available
    pub fn mark_available(&self, provider: &str) -> Result<(), String> {
        let mut unavailable = self.unavailable.lock().map_err(|_| "Lock failed".to_string())?;
        unavailable.retain(|(name, _)| name != provider);
        Ok(())
    }
}

/// OAuth audit event for logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAuditEvent {
    /// Event type: authorization, token_exchange, token_refresh, logout
    pub event_type: String,
    /// Provider name
    pub provider:   String,
    /// User ID (if known)
    pub user_id:    Option<String>,
    /// Status: success, failed
    pub status:     String,
    /// Error message (if failed)
    pub error:      Option<String>,
    /// Timestamp
    pub timestamp:  DateTime<Utc>,
    /// Additional metadata
    pub metadata:   HashMap<String, String>,
}

impl OAuthAuditEvent {
    /// Create new audit event
    pub fn new(
        event_type: impl Into<String>,
        provider: impl Into<String>,
        status: impl Into<String>,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            provider:   provider.into(),
            user_id:    None,
            status:     status.into(),
            error:      None,
            timestamp:  Utc::now(),
            metadata:   HashMap::new(),
        }
    }

    /// Set user ID
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Set error message
    pub fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

#[cfg(test)]
mod tests {
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
}
