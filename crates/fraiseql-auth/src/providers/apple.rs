//! Sign in with Apple (#943).
//!
//! Apple is not an ordinary OIDC client, and every difference below is a place
//! a copy of the Google provider would have failed:
//!
//! - **The client secret is a signed assertion, not a static string.** It is an ES256 JWT over
//!   `(team_id, key_id, client_id)`, signed with the `.p8` key Apple issues, valid for at most six
//!   months. There is no `client_secret_env` to read — [`AppleOAuth`] mints the assertion on demand
//!   and re-mints it before it expires.
//! - **There is no userinfo endpoint.** The identity lives in the `id_token` returned by the token
//!   endpoint, which is why the provider implements
//!   [`user_info_from_tokens`](crate::provider::OAuthProvider::user_info_from_tokens) and refuses
//!   the access-token-only [`user_info`](crate::provider::OAuthProvider::user_info).
//! - **`response_mode=form_post`.** Requesting the `name`/`email` scopes makes Apple deliver the
//!   callback as a `POST` with a form body, so the server mounts a POST variant of
//!   `/auth/v1/callback` whenever Apple is configured.
//! - **The name is returned exactly once**, in that first form POST's `user` field. It is
//!   **browser-supplied and therefore untrusted** — see [`AppleFirstAuthUser`].
//! - **Private Relay addresses** (`…@privaterelay.appleid.com`) are per-app aliases Apple owns and
//!   verifies, so they are safe to key on. They never match another provider's email, so a Private
//!   Relay user's identity simply does not participate in cross-provider linking — correct, not a
//!   bug.
//!
//! # Why the `id_token` signature is not re-verified
//!
//! The token this provider reads is the one the **token endpoint** returned over
//! TLS to a URL that passed the shared SSRF guard — direct client-to-issuer
//! communication. OIDC Core §3.1.3.7 rule 6 blesses exactly this case: TLS
//! server validation stands in for checking the signature. An attacker able to
//! forge that response could equally forge the JWKS document that would be used
//! to verify it, so a JWKS hop would add cost and no security. The claims that
//! *do* carry meaning — `iss`, `aud`, `exp` — are validated, fail-closed.
//!
//! The `id_token` a browser posts to the form-POST callback is a different
//! thing entirely and is never read; see [`crate::multi_provider::callback_form_post`].

use std::{
    fmt::Write as _,
    sync::RwLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{
    error::{AuthError, Result},
    oidc_provider::validate_oauth_endpoint_url,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// Timeout for all Apple ID HTTP requests.
const APPLE_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum byte size for an Apple ID token-endpoint response.
const MAX_APPLE_RESPONSE_BYTES: usize = 1024 * 1024; // 1 MiB

/// Default Apple ID base URL (authorize + token endpoints, and the `iss` every
/// `id_token` must carry).
const DEFAULT_BASE_URL: &str = "https://appleid.apple.com";

/// The `aud` Apple requires in the client-secret assertion. It names Apple
/// itself, **not** whichever host the token endpoint is reached on — so it stays
/// fixed even when [`AppleOAuth::with_base_url`] points elsewhere.
const CLIENT_SECRET_AUDIENCE: &str = "https://appleid.apple.com";

/// Scopes requested: `name` and `email` are what make Apple return the user
/// payload — and what make the callback a form POST.
const APPLE_SCOPES: &str = "name email";

/// Lifetime of a minted client-secret assertion. Apple's ceiling is six months;
/// minting a short-lived one per hour costs one ES256 signature and keeps a
/// leaked assertion worth minutes rather than half a year.
const CLIENT_SECRET_TTL_SECS: u64 = 3600;

/// Re-mint this long before expiry, so an assertion never expires in flight
/// between being taken from the cache and reaching Apple.
const CLIENT_SECRET_RENEW_SKEW_SECS: u64 = 300;

/// Domain of Apple's Private Relay aliases.
const PRIVATE_RELAY_DOMAIN: &str = "@privaterelay.appleid.com";

/// Claims of the ES256 client-secret assertion (Apple's documented shape).
#[derive(Debug, Serialize)]
struct ClientSecretClaims<'a> {
    /// Apple developer team ID.
    iss: &'a str,
    /// Issued-at, seconds since the epoch.
    iat: u64,
    /// Expiry, seconds since the epoch.
    exp: u64,
    /// Always [`CLIENT_SECRET_AUDIENCE`].
    aud: &'a str,
    /// The services ID this assertion authenticates.
    sub: &'a str,
}

/// A minted assertion and the moment it stops being usable.
#[derive(Debug, Clone)]
struct CachedClientSecret {
    assertion:  String,
    expires_at: u64,
}

/// Apple's `id_token` audience: a bare string in practice, an array by OIDC
/// spec. Accept both rather than fail on a conformant provider.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}

impl Audience {
    fn contains(&self, value: &str) -> bool {
        match self {
            Self::One(a) => a == value,
            Self::Many(all) => all.iter().any(|a| a == value),
        }
    }
}

/// Apple renders its boolean `id_token` claims as either a JSON bool or the
/// strings `"true"`/`"false"`, depending on the flow. A parser that accepts only
/// one of the two silently drops `email_verified` — which fails *open* into the
/// email-keyed linking space, so both are accepted explicitly.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AppleBool {
    Bool(bool),
    Str(String),
}

impl AppleBool {
    /// Fail-closed: anything that is not recognisably true is false.
    fn as_bool(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Str(s) => s.eq_ignore_ascii_case("true"),
        }
    }
}

/// The subset of Apple's `id_token` claims that carry identity.
#[derive(Debug, Deserialize)]
struct AppleIdTokenClaims {
    iss:              String,
    aud:              Audience,
    sub:              String,
    exp:              u64,
    #[serde(default)]
    email:            Option<String>,
    #[serde(default)]
    email_verified:   Option<AppleBool>,
    #[serde(default)]
    is_private_email: Option<AppleBool>,
}

/// Apple's token-endpoint response. `id_token` is the identity; `access_token`
/// opens nothing (Apple exposes no user API).
#[derive(Debug, Deserialize)]
struct AppleTokenResponse {
    access_token:  String,
    #[serde(default)]
    token_type:    Option<String>,
    #[serde(default)]
    expires_in:    Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token:      Option<String>,
}

/// The `user` field of Apple's first-authorization form POST.
///
/// # This is browser-supplied data
///
/// Apple does not sign it: it arrives as a form field in a POST the user's
/// browser makes, so any caller holding a valid `code`/`state` pair from *their
/// own* Apple account can put anything in it. Its `email` is therefore
/// deliberately **not** modelled here — an attacker-chosen address must never
/// reach the email-keyed linking space (that is account takeover). The linking
/// email comes only from the token endpoint's `id_token`, which Apple issues.
///
/// The name is display data, and is surfaced on the first callback's
/// [`UserInfo`] as such.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AppleFirstAuthUser {
    /// The user's name, present only on the very first authorization.
    #[serde(default)]
    pub name: Option<AppleFirstAuthName>,
}

/// The `name` object inside [`AppleFirstAuthUser`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppleFirstAuthName {
    /// Given name, as the user typed it into Apple's consent sheet.
    #[serde(default)]
    pub first_name: Option<String>,
    /// Family name, as the user typed it into Apple's consent sheet.
    #[serde(default)]
    pub last_name:  Option<String>,
}

impl AppleFirstAuthUser {
    /// Parse the raw `user` form field. A malformed payload is *not* an error:
    /// it is optional display data, and refusing a login over it would let a
    /// broken client lock a user out.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        serde_json::from_str(raw).ok()
    }

    /// The display name, or `None` when neither part was supplied.
    #[must_use]
    pub fn display_name(&self) -> Option<String> {
        let name = self.name.as_ref()?;
        let joined = [name.first_name.as_deref(), name.last_name.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        (!joined.is_empty()).then_some(joined)
    }
}

/// Return `true` when `email` is one of Apple's Private Relay aliases.
///
/// Relay addresses are verified — Apple owns the domain — so they are safe
/// linking keys. They are also per-app, so they never match another provider's
/// address and a Relay user's identity does not participate in cross-provider
/// linking.
#[must_use]
pub fn is_private_relay_email(email: &str) -> bool {
    email.trim().to_ascii_lowercase().ends_with(PRIVATE_RELAY_DOMAIN)
}

/// Sign in with Apple provider.
pub struct AppleOAuth {
    /// Services ID — the `client_id`, and the `aud` of every `id_token`.
    client_id:     String,
    /// Apple developer team ID (the assertion's `iss`).
    team_id:       String,
    /// Key ID of the `.p8` signing key (the assertion header's `kid`).
    key_id:        String,
    /// `.p8` private key, PEM-encoded. Wiped from memory on drop.
    private_key:   Zeroizing<String>,
    redirect_uri:  String,
    /// Base URL of the Apple ID service, and the `iss` every `id_token` must
    /// carry. Overridable for a stub `IdP`; the SSRF guard still applies.
    base_url:      String,
    client:        reqwest::Client,
    /// The current assertion. `RwLock` and not `OnceCell` because it is
    /// deliberately re-minted.
    cached_secret: RwLock<Option<CachedClientSecret>>,
}

impl std::fmt::Debug for AppleOAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppleOAuth")
            .field("client_id", &self.client_id)
            .field("team_id", &self.team_id)
            .field("key_id", &self.key_id)
            .field("redirect_uri", &self.redirect_uri)
            .field("base_url", &self.base_url)
            .finish_non_exhaustive() // private_key and minted assertions omitted
    }
}

fn now_secs() -> Result<u64> {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).map_err(|e| {
        AuthError::ConfigError {
            message: format!("system clock is before the Unix epoch: {e}"),
        }
    })
}

impl AppleOAuth {
    /// Create a provider against `https://appleid.apple.com`.
    ///
    /// Construction is network-free and does **not** sign anything: the key is
    /// validated here so a bad `.p8` fails at boot rather than on the first
    /// login attempt.
    ///
    /// # Arguments
    /// * `client_id` — the services ID registered with Apple
    /// * `team_id` — Apple developer team ID
    /// * `key_id` — key ID of the `.p8` signing key
    /// * `private_key_pem` — contents of the `.p8` file (PKCS#8 PEM)
    /// * `redirect_uri` — this server's `/auth/v1/callback` URL
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`] if the private key is not a usable
    /// ES256 key or the HTTP client cannot be built.
    pub fn new(
        client_id: String,
        team_id: String,
        key_id: String,
        private_key_pem: String,
        redirect_uri: String,
    ) -> Result<Self> {
        Self::with_base_url(
            client_id,
            team_id,
            key_id,
            private_key_pem,
            redirect_uri,
            DEFAULT_BASE_URL.to_string(),
        )
    }

    /// Create a provider against an explicit base URL — a stub `IdP` in tests.
    ///
    /// `base_url` is also the issuer every `id_token` must name, so pointing it
    /// elsewhere moves both halves together and cannot silently accept a token
    /// from a different issuer.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::OidcMetadataError`] when `base_url` fails the shared
    /// SSRF guard (non-HTTPS scheme, or a private/loopback/link-local address;
    /// the `FRAISEQL_OIDC_ALLOW_INSECURE` development bypass applies), or
    /// [`AuthError::ConfigError`] if the private key is unusable or the HTTP
    /// client cannot be built.
    pub fn with_base_url(
        client_id: String,
        team_id: String,
        key_id: String,
        private_key_pem: String,
        redirect_uri: String,
        base_url: String,
    ) -> Result<Self> {
        validate_oauth_endpoint_url(&base_url)?;
        // Fail at boot, not at the first login: a `.p8` that cannot sign is a
        // provider that can never exchange a code.
        jsonwebtoken::EncodingKey::from_ec_pem(private_key_pem.as_bytes()).map_err(|e| {
            AuthError::ConfigError {
                message: format!(
                    "[auth.social.apple] private key is not a usable ES256 key — Apple issues a \
                     PKCS#8 PEM `.p8` file: {e}"
                ),
            }
        })?;
        let client =
            reqwest::Client::builder().timeout(APPLE_REQUEST_TIMEOUT).build().map_err(|e| {
                AuthError::ConfigError {
                    message: format!("Failed to create HTTP client: {e}"),
                }
            })?;
        Ok(Self {
            client_id,
            team_id,
            key_id,
            private_key: Zeroizing::new(private_key_pem),
            redirect_uri,
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
            cached_secret: RwLock::new(None),
        })
    }

    /// Mint a fresh ES256 client-secret assertion valid from `now`.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`] if signing fails.
    fn mint_client_secret(&self, now: u64) -> Result<CachedClientSecret> {
        let expires_at = now + CLIENT_SECRET_TTL_SECS;
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        header.kid = Some(self.key_id.clone());
        let claims = ClientSecretClaims {
            iss: &self.team_id,
            iat: now,
            exp: expires_at,
            aud: CLIENT_SECRET_AUDIENCE,
            sub: &self.client_id,
        };
        let key =
            jsonwebtoken::EncodingKey::from_ec_pem(self.private_key.as_bytes()).map_err(|e| {
                AuthError::ConfigError {
                    message: format!(
                        "[auth.social.apple] private key is not a usable ES256 key: {e}"
                    ),
                }
            })?;
        let assertion =
            jsonwebtoken::encode(&header, &claims, &key).map_err(|e| AuthError::ConfigError {
                message: format!(
                    "[auth.social.apple] client-secret assertion could not be signed: {e}"
                ),
            })?;
        Ok(CachedClientSecret {
            assertion,
            expires_at,
        })
    }

    /// The current client-secret assertion, minting a new one when the cached
    /// one is absent or within five minutes of expiry, so an assertion never
    /// expires in flight between leaving the cache and reaching Apple.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`] if the clock cannot be read or
    /// signing fails.
    pub fn client_secret(&self) -> Result<String> {
        let now = now_secs()?;
        if let Ok(guard) = self.cached_secret.read() {
            if let Some(cached) = guard.as_ref() {
                if cached.expires_at > now + CLIENT_SECRET_RENEW_SKEW_SECS {
                    return Ok(cached.assertion.clone());
                }
            }
        }
        let minted = self.mint_client_secret(now)?;
        if let Ok(mut guard) = self.cached_secret.write() {
            *guard = Some(minted.clone());
        }
        Ok(minted.assertion)
    }

    /// POST a form to Apple's token endpoint and parse the response.
    async fn post_token(&self, params: &[(&str, &str)]) -> Result<TokenResponse> {
        let resp = self
            .client
            .post(format!("{}/auth/token", self.base_url))
            .form(params)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Apple token endpoint request failed: {e}"),
            })?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| AuthError::OAuthError {
            message: format!("Failed to read Apple token response: {e}"),
        })?;
        if !status.is_success() {
            return Err(AuthError::OAuthError {
                message: format!("Apple token endpoint returned HTTP {status}"),
            });
        }
        if bytes.len() > MAX_APPLE_RESPONSE_BYTES {
            return Err(AuthError::OAuthError {
                message: format!("Apple token response too large ({} bytes)", bytes.len()),
            });
        }
        let response: AppleTokenResponse =
            serde_json::from_slice(&bytes).map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse Apple token response: {e}"),
            })?;
        Ok(TokenResponse {
            access_token:  response.access_token,
            refresh_token: response.refresh_token,
            expires_in:    response.expires_in.unwrap_or(0),
            token_type:    response.token_type.unwrap_or_else(|| "Bearer".to_string()),
            id_token:      response.id_token,
        })
    }

    /// Decode and validate an `id_token` that came from the token endpoint.
    ///
    /// The signature is not re-verified (see the module docs); `iss`, `aud` and
    /// `exp` are, fail-closed.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::OAuthError`] if the token is malformed, names a
    /// different issuer or audience, or has expired.
    fn decode_id_token(&self, id_token: &str, now: u64) -> Result<AppleIdTokenClaims> {
        let payload = id_token.split('.').nth(1).ok_or_else(|| AuthError::OAuthError {
            message: "Apple id_token is not a JWT".to_string(),
        })?;
        let bytes =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).map_err(|e| {
                AuthError::OAuthError {
                    message: format!("Apple id_token payload is not base64url: {e}"),
                }
            })?;
        let claims: AppleIdTokenClaims =
            serde_json::from_slice(&bytes).map_err(|e| AuthError::OAuthError {
                message: format!("Apple id_token claims do not parse: {e}"),
            })?;
        if claims.iss.trim_end_matches('/') != self.base_url {
            return Err(AuthError::OAuthError {
                message: format!(
                    "Apple id_token issuer {} does not match the configured Apple ID service",
                    claims.iss
                ),
            });
        }
        if !claims.aud.contains(&self.client_id) {
            return Err(AuthError::OAuthError {
                message: "Apple id_token audience does not name this client".to_string(),
            });
        }
        if claims.exp <= now {
            return Err(AuthError::OAuthError {
                message: "Apple id_token has expired".to_string(),
            });
        }
        Ok(claims)
    }

    /// Build the [`UserInfo`] an `id_token` describes.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::OAuthError`] if the token fails validation.
    pub fn user_info_from_id_token(&self, id_token: &str) -> Result<UserInfo> {
        let now = now_secs()?;
        let claims = self.decode_id_token(id_token, now)?;
        // Apple only omits `email_verified` when it omitted `email` too; an
        // address with no flag is treated as unverified rather than assumed.
        let email_verified = claims.email_verified.as_ref().is_some_and(AppleBool::as_bool);
        let is_private_email = claims.is_private_email.as_ref().is_some_and(AppleBool::as_bool);
        // Normalize an empty claim to `None` so it can never key a link.
        let email = claims.email.filter(|e| !e.trim().is_empty());

        let mut raw_claims = serde_json::Map::new();
        raw_claims.insert("apple_sub".to_string(), serde_json::json!(claims.sub));
        if let Some(ref email) = email {
            raw_claims.insert("email".to_string(), serde_json::json!(email));
        }
        raw_claims.insert("email_verified".to_string(), serde_json::json!(email_verified));
        raw_claims.insert("is_private_email".to_string(), serde_json::json!(is_private_email));

        Ok(UserInfo {
            id: claims.sub,
            email,
            email_verified,
            name: None,
            picture: None,
            raw_claims: serde_json::Value::Object(raw_claims),
        })
    }
}

// Reason: OAuthProvider is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl OAuthProvider for AppleOAuth {
    fn name(&self) -> &'static str {
        "apple"
    }

    fn authorization_url(&self, state: &str) -> String {
        let mut url = format!("{}/auth/authorize", self.base_url);
        write!(
            url,
            "?client_id={}&redirect_uri={}&state={}&response_type=code&scope={}\
             &response_mode=form_post",
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&self.redirect_uri),
            urlencoding::encode(state),
            urlencoding::encode(APPLE_SCOPES),
        )
        .expect("write to String is infallible");
        url
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        let secret = self.client_secret()?;
        self.post_token(&[
            ("client_id", self.client_id.as_str()),
            ("client_secret", secret.as_str()),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", self.redirect_uri.as_str()),
        ])
        .await
    }

    /// Always an error: Apple publishes no userinfo endpoint, so an access
    /// token alone identifies nobody. Refusing loudly is the point — a caller
    /// that reaches here has skipped
    /// [`user_info_from_tokens`](OAuthProvider::user_info_from_tokens) and would
    /// otherwise get a silently identity-less login.
    async fn user_info(&self, _access_token: &str) -> Result<UserInfo> {
        Err(AuthError::OAuthError {
            message: "Apple exposes no userinfo endpoint: the identity is in the token \
                      endpoint's id_token — use OAuthProvider::user_info_from_tokens"
                .to_string(),
        })
    }

    async fn user_info_from_tokens(&self, tokens: &TokenResponse) -> Result<UserInfo> {
        let id_token = tokens.id_token.as_deref().ok_or_else(|| AuthError::OAuthError {
            message: "Apple token response carried no id_token — there is no other source of \
                      identity"
                .to_string(),
        })?;
        self.user_info_from_id_token(id_token)
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        let secret = self.client_secret()?;
        self.post_token(&[
            ("client_id", self.client_id.as_str()),
            ("client_secret", secret.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])
        .await
    }
}
