//! GitHub OAuth provider implementation.
//!
//! GitHub is a plain `OAuth2` provider, **not** an OIDC one: `github.com`
//! serves no `/.well-known/openid-configuration` (it is a 404), its token
//! endpoint answers form-encoded unless `Accept: application/json` is sent,
//! its token response carries no `expires_in` (tokens do not expire), and
//! `/user` has no `sub` claim. The previous implementation wrapped
//! [`OidcProvider`](crate::oidc_provider::OidcProvider) and therefore could
//! never have constructed against real GitHub — discovery failed at boot
//! (#368 made it reachable, which is what surfaced this).
//!
//! This implementation talks to the fixed well-known endpoints
//! (`{base}/login/oauth/authorize`, `{base}/login/oauth/access_token`,
//! `{api}/user`, `{api}/user/emails`, `{api}/user/teams`), overridable for
//! GitHub Enterprise Server. The `/user/emails` second hop resolves the
//! **primary verified** email so a GitHub identity can participate in
//! email-keyed account linking — without it, a private-email GitHub user
//! silently fell back to `(provider, id)` keying and GitHub stayed
//! untrustable for linking.

use std::{fmt::Write as _, time::Duration};

use async_trait::async_trait;
use serde::Deserialize;
use tracing::warn;
use zeroize::Zeroizing;

/// Timeout for all GitHub API HTTP requests.
pub(crate) const GITHUB_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum byte size for a GitHub API response.
///
/// GitHub user and team responses are small JSON documents (< 10 `KiB`).
/// 5 `MiB` is a generous cap that blocks allocation bombs from network
/// intermediaries while accommodating any legitimate response size.
pub(crate) const MAX_GITHUB_RESPONSE_BYTES: usize = 5 * 1024 * 1024; // 5 MiB

/// Default GitHub web base URL (authorize + token endpoints live under it).
const DEFAULT_BASE_URL: &str = "https://github.com";

/// Default GitHub REST API base URL.
const DEFAULT_API_BASE_URL: &str = "https://api.github.com";

/// OAuth scopes requested: `read:user` for `/user`, `user:email` for the
/// `/user/emails` second hop that resolves the primary verified email.
const GITHUB_SCOPES: &str = "read:user user:email";

use crate::{
    error::{AuthError, Result},
    oidc_provider::validate_oauth_endpoint_url,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// GitHub OAuth provider (plain `OAuth2` against the well-known endpoints).
pub struct GitHubOAuth {
    client_id:     String,
    /// Wiped from memory when the provider is dropped.
    client_secret: Zeroizing<String>,
    redirect_uri:  String,
    /// Web base URL (`https://github.com`, or a GitHub Enterprise host).
    base_url:      String,
    /// REST API base URL (`https://api.github.com`, or `https://HOST/api/v3`).
    api_base_url:  String,
    client:        reqwest::Client,
}

impl std::fmt::Debug for GitHubOAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHubOAuth")
            .field("client_id", &self.client_id)
            .field("redirect_uri", &self.redirect_uri)
            .field("base_url", &self.base_url)
            .field("api_base_url", &self.api_base_url)
            .finish_non_exhaustive() // client_secret omitted for security
    }
}

/// GitHub user information with teams
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubUser {
    /// GitHub numeric user ID (stable across username changes)
    pub id:           u64,
    /// GitHub username (login handle)
    pub login:        String,
    /// Primary email address (may be `None` if the user keeps it private)
    pub email:        Option<String>,
    /// User's display name
    pub name:         Option<String>,
    /// URL to the user's avatar image
    pub avatar_url:   Option<String>,
    /// Short biography text from the profile
    pub bio:          Option<String>,
    /// Company name from the profile
    pub company:      Option<String>,
    /// Location from the profile
    pub location:     Option<String>,
    /// Number of public repositories owned by the user
    #[serde(default)]
    pub public_repos: u32,
}

/// One entry of the `/user/emails` response (#368 second hop).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitHubEmail {
    /// The email address.
    pub email:    String,
    /// Whether this is the account's primary email.
    pub primary:  bool,
    /// Whether GitHub has verified this email.
    pub verified: bool,
}

/// GitHub team from API response
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubTeam {
    /// GitHub numeric team ID
    pub id:           u64,
    /// Human-readable team name
    pub name:         String,
    /// URL-safe team slug (used in API paths)
    pub slug:         String,
    /// Organization that owns this team
    pub organization: GitHubOrg,
}

/// GitHub organization
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubOrg {
    /// GitHub numeric organization ID
    pub id:    u64,
    /// Organization login (handle)
    pub login: String,
}

/// GitHub token response. Unlike OIDC, there is **no** `expires_in` — GitHub
/// OAuth app tokens do not expire.
#[derive(Debug, Deserialize)]
struct GitHubTokenResponse {
    access_token:  String,
    #[serde(default)]
    token_type:    Option<String>,
    /// Present when the OAuth app has expiring tokens enabled; absent otherwise.
    #[serde(default)]
    expires_in:    Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
}

/// Select the account's canonical linkable email from `/user/emails` (#368).
///
/// Returns the **primary** entry's address with its verified flag. Only the
/// primary email is the account identity — a verified secondary address is
/// deliberately not promoted, because GitHub allows removing and re-adding
/// secondaries freely. Returns `None` when no primary entry exists.
#[must_use]
pub fn select_linkable_email(emails: &[GitHubEmail]) -> Option<(String, bool)> {
    emails.iter().find(|e| e.primary).map(|e| (e.email.clone(), e.verified))
}

impl GitHubOAuth {
    /// Create a GitHub OAuth provider against the well-known `github.com`
    /// endpoints.
    ///
    /// Construction is network-free — GitHub serves no OIDC discovery
    /// document, so there is nothing to fetch at boot.
    ///
    /// # Arguments
    /// * `client_id` - GitHub OAuth app client ID
    /// * `client_secret` - GitHub OAuth app client secret
    /// * `redirect_uri` - Redirect URI after authentication (this server's callback)
    ///
    /// # Errors
    ///
    /// Returns `AuthError::ConfigError` if the HTTP client cannot be built.
    pub fn new(client_id: String, client_secret: String, redirect_uri: String) -> Result<Self> {
        Self::with_endpoints(
            client_id,
            client_secret,
            redirect_uri,
            DEFAULT_BASE_URL.to_string(),
            DEFAULT_API_BASE_URL.to_string(),
        )
    }

    /// Create a GitHub OAuth provider against explicit endpoints — GitHub
    /// Enterprise Server (`base_url = https://HOST`, `api_base_url =
    /// https://HOST/api/v3`) or a stub `IdP` in tests.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::OidcMetadataError` when either URL fails the shared
    /// SSRF guard (non-HTTPS scheme or a private/loopback/link-local address;
    /// the `FRAISEQL_OIDC_ALLOW_INSECURE` development bypass applies), or
    /// `AuthError::ConfigError` if the HTTP client cannot be built.
    pub fn with_endpoints(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        base_url: String,
        api_base_url: String,
    ) -> Result<Self> {
        // SECURITY: endpoint overrides face the same SSRF gate as OIDC issuer
        // URLs — a config-supplied base URL must not reach link-local metadata
        // services or internal hosts.
        validate_oauth_endpoint_url(&base_url)?;
        validate_oauth_endpoint_url(&api_base_url)?;
        let client =
            reqwest::Client::builder()
                .timeout(GITHUB_REQUEST_TIMEOUT)
                .build()
                .map_err(|e| AuthError::ConfigError {
                    message: format!("Failed to create HTTP client: {e}"),
                })?;
        Ok(Self {
            client_id,
            client_secret: Zeroizing::new(client_secret),
            redirect_uri,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_base_url: api_base_url.trim_end_matches('/').to_string(),
            client,
        })
    }

    /// Map GitHub teams to FraiseQL roles
    ///
    /// Maps organization:team slugs to role names.
    /// Example: "my-org:admin-team" -> "admin"
    ///
    /// # Arguments
    /// * `teams` - List of "org:team" strings from GitHub
    #[must_use]
    pub fn map_teams_to_roles(teams: Vec<String>) -> Vec<String> {
        teams
            .into_iter()
            .filter_map(|team| {
                let parts: Vec<&str> = team.split(':').collect();
                if parts.len() == 2 {
                    match parts[1] {
                        "admin" | "administrators" | "admin-team" => Some("admin".to_string()),
                        "operator" | "operators" | "operator-team" | "maintainer"
                        | "maintainers" => Some("operator".to_string()),
                        "viewer" | "viewers" | "viewer-team" => Some("viewer".to_string()),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    /// GET a GitHub API path with the user's access token, enforcing the
    /// response-size cap. Returns the raw bytes on HTTP success.
    async fn api_get(&self, path: &str, access_token: &str) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get(format!("{}{path}", self.api_base_url))
            .header("Authorization", format!("Bearer {access_token}"))
            .header("User-Agent", "FraiseQL")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to fetch GitHub {path}: {e}"),
            })?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| AuthError::OAuthError {
            message: format!("Failed to read GitHub {path} response: {e}"),
        })?;
        if !status.is_success() {
            return Err(AuthError::OAuthError {
                message: format!("GitHub {path} API returned HTTP {status}"),
            });
        }
        if bytes.len() > MAX_GITHUB_RESPONSE_BYTES {
            return Err(AuthError::OAuthError {
                message: format!("GitHub {path} response too large ({} bytes)", bytes.len()),
            });
        }
        Ok(bytes.to_vec())
    }

    /// Fetch `/user/teams` as `org:slug` strings. Best-effort: a missing
    /// scope, oversized response, or parse failure logs and yields an empty
    /// list — team-to-role mapping is an enrichment, not an identity claim.
    async fn fetch_team_strings(&self, access_token: &str) -> Vec<String> {
        let teams: Vec<GitHubTeam> = match self.api_get("/user/teams", access_token).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                warn!(error = %e, "Failed to parse GitHub teams response — treating as empty");
                Vec::new()
            }),
            Err(e) => {
                warn!(error = %e, "GitHub teams fetch failed — treating as empty");
                Vec::new()
            },
        };
        teams.iter().map(|t| format!("{}:{}", t.organization.login, t.slug)).collect()
    }

    /// Get user info including teams from GitHub API
    ///
    /// # Arguments
    /// * `access_token` - GitHub access token
    ///
    /// # Errors
    ///
    /// Returns `AuthError::OAuthError` if the GitHub `/user` request fails or
    /// returns a non-success status code.
    pub async fn get_user_with_teams(
        &self,
        access_token: &str,
    ) -> Result<(GitHubUser, Vec<String>)> {
        let user_bytes = self.api_get("/user", access_token).await?;
        let user: GitHubUser =
            serde_json::from_slice(&user_bytes).map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse GitHub user: {e}"),
            })?;
        let team_strings = self.fetch_team_strings(access_token).await;
        Ok((user, team_strings))
    }

    /// Extract organization ID from GitHub teams (primary org)
    ///
    /// Returns the first organization the user belongs to as the org_id.
    /// In multi-org scenarios, this should be overridden with explicit org selection.
    #[must_use]
    pub fn extract_org_id_from_teams(teams: &[(GitHubUser, Vec<String>)]) -> Option<String> {
        teams
            .first()
            .and_then(|(_, team_strings)| team_strings.first())
            .and_then(|team_str| team_str.split(':').next())
            .map(|org| org.to_string())
    }
}

// Reason: OAuthProvider is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl OAuthProvider for GitHubOAuth {
    fn name(&self) -> &'static str {
        "github"
    }

    fn authorization_url(&self, state: &str) -> String {
        let mut url = format!("{}/login/oauth/authorize", self.base_url);
        write!(
            url,
            "?client_id={}&redirect_uri={}&state={}&scope={}",
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&self.redirect_uri),
            urlencoding::encode(state),
            urlencoding::encode(GITHUB_SCOPES),
        )
        .expect("write to String is infallible");
        url
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", self.redirect_uri.as_str()),
        ];
        let resp = self
            .client
            .post(format!("{}/login/oauth/access_token", self.base_url))
            // GitHub answers application/x-www-form-urlencoded unless JSON is
            // explicitly requested.
            .header("Accept", "application/json")
            .form(&params)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to exchange code: {e}"),
            })?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| AuthError::OAuthError {
            message: format!("Failed to read token response: {e}"),
        })?;
        if !status.is_success() {
            return Err(AuthError::OAuthError {
                message: format!("GitHub token endpoint returned HTTP {status}"),
            });
        }
        if bytes.len() > MAX_GITHUB_RESPONSE_BYTES {
            return Err(AuthError::OAuthError {
                message: format!("GitHub token response too large ({} bytes)", bytes.len()),
            });
        }
        let response: GitHubTokenResponse =
            serde_json::from_slice(&bytes).map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse GitHub token response: {e}"),
            })?;
        Ok(TokenResponse {
            access_token:  response.access_token,
            refresh_token: response.refresh_token,
            // GitHub OAuth app tokens do not expire unless the app opts into
            // expiring tokens; 0 = "no expiry reported by the provider".
            expires_in:    response.expires_in.unwrap_or(0),
            token_type:    response.token_type.unwrap_or_else(|| "bearer".to_string()),
        })
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        // /user is the identity fetch — it must succeed.
        let user_bytes = self.api_get("/user", access_token).await?;
        let github_user: GitHubUser =
            serde_json::from_slice(&user_bytes).map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse GitHub user: {e}"),
            })?;

        // #368 second hop: /user/emails resolves the primary verified email —
        // /user omits the email entirely for private-email accounts, and never
        // carries a verified flag. Fail-closed: if the hop fails (missing
        // user:email scope on an old app authorization, network error), fall
        // back to the /user email with verified = false, so an unproven email
        // can never enter the email-keyed linking space.
        let (email, email_verified) = match self.api_get("/user/emails", access_token).await {
            Ok(bytes) => match serde_json::from_slice::<Vec<GitHubEmail>>(&bytes) {
                Ok(emails) => select_linkable_email(&emails)
                    .map_or_else(|| (github_user.email.clone(), false), |(e, v)| (Some(e), v)),
                Err(e) => {
                    warn!(error = %e, "Failed to parse GitHub /user/emails — treating email as unverified");
                    (github_user.email.clone(), false)
                },
            },
            Err(e) => {
                warn!(error = %e, "GitHub /user/emails fetch failed — treating email as unverified");
                (github_user.email.clone(), false)
            },
        };

        let team_strings = self.fetch_team_strings(access_token).await;
        let org_id = team_strings
            .first()
            .and_then(|team| team.split(':').next())
            .map(|org| org.to_string());

        let mut raw_claims = serde_json::Map::new();
        raw_claims.insert("github_id".to_string(), serde_json::json!(github_user.id));
        raw_claims.insert("github_login".to_string(), serde_json::json!(github_user.login));
        raw_claims.insert("github_teams".to_string(), serde_json::json!(team_strings));
        raw_claims.insert("github_company".to_string(), serde_json::json!(github_user.company));
        raw_claims.insert("github_location".to_string(), serde_json::json!(github_user.location));
        raw_claims
            .insert("github_public_repos".to_string(), serde_json::json!(github_user.public_repos));
        if let Some(ref email) = email {
            raw_claims.insert("email".to_string(), serde_json::json!(email));
        }
        raw_claims.insert("email_verified".to_string(), serde_json::json!(email_verified));
        if let Some(org_id) = org_id {
            raw_claims.insert("org_id".to_string(), serde_json::json!(&org_id));
        }

        Ok(UserInfo {
            id: github_user.id.to_string(),
            // Normalize an empty/whitespace-only email to `None` so it can
            // never serve as an account-linking key (H26).
            email: email.filter(|e| !e.trim().is_empty()),
            email_verified,
            name: github_user.name.clone().or(Some(github_user.login.clone())),
            picture: github_user.avatar_url.clone(),
            raw_claims: serde_json::Value::Object(raw_claims),
        })
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        // Only OAuth apps with expiring tokens enabled issue refresh tokens.
        let params = [
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ];
        let resp = self
            .client
            .post(format!("{}/login/oauth/access_token", self.base_url))
            .header("Accept", "application/json")
            .form(&params)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to refresh token: {e}"),
            })?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| AuthError::OAuthError {
            message: format!("Failed to read refresh response: {e}"),
        })?;
        if !status.is_success() {
            return Err(AuthError::OAuthError {
                message: format!("GitHub refresh endpoint returned HTTP {status}"),
            });
        }
        let response: GitHubTokenResponse =
            serde_json::from_slice(&bytes).map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse GitHub refresh response: {e}"),
            })?;
        Ok(TokenResponse {
            access_token:  response.access_token,
            refresh_token: response.refresh_token,
            expires_in:    response.expires_in.unwrap_or(0),
            token_type:    response.token_type.unwrap_or_else(|| "bearer".to_string()),
        })
    }
}
