//! Okta OAuth / OIDC provider implementation with group-based role mapping.
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::Result,
    oidc_provider::OidcProvider,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// Okta OAuth provider wrapper
///
/// Handles Okta-specific OAuth flows and role/group mapping.
/// Supports Okta custom claims and group-based authorization.
#[derive(Debug)]
pub struct OktaOAuth {
    oidc:   OidcProvider,
    domain: String,
}

/// Okta user information
#[derive(Debug, Clone, Deserialize)]
pub struct OktaUser {
    /// Subject — Okta's stable user ID
    pub sub:            String,
    /// User's email address
    pub email:          String,
    /// Whether Okta has verified the email address
    pub email_verified: Option<bool>,
    /// User's full display name
    pub name:           Option<String>,
    /// Given (first) name
    pub given_name:     Option<String>,
    /// Family (last) name
    pub family_name:    Option<String>,
    /// URL of the user's profile picture
    pub picture:        Option<String>,
    /// User's locale (e.g., `"en_US"`)
    pub locale:         Option<String>,
}

/// Okta groups claim
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OktaGroups {
    /// List of Okta group names the user belongs to (mapped to FraiseQL roles)
    pub groups: Option<Vec<String>>,
}

impl OktaOAuth {
    /// Create a new Okta OAuth provider
    ///
    /// # Arguments
    /// * `client_id` - Okta application client ID
    /// * `client_secret` - Okta application client secret
    /// * `okta_domain` - Okta tenant domain (e.g., "company.okta.com")
    /// * `redirect_uri` - Redirect URI after authentication (e.g., "http://localhost:8000/auth/callback")
    ///
    /// # Errors
    ///
    /// Returns `AuthError` if OIDC discovery against the Okta domain fails.
    pub async fn new(
        client_id: String,
        client_secret: String,
        okta_domain: String,
        redirect_uri: String,
    ) -> Result<Self> {
        let issuer_url = format!("https://{}", okta_domain);

        let oidc =
            OidcProvider::new("okta", &issuer_url, &client_id, &client_secret, &redirect_uri)
                .await?;

        Ok(Self {
            oidc,
            domain: okta_domain,
        })
    }

    /// Extract groups from Okta claims
    ///
    /// Okta can provide groups in the token if configured with appropriate scopes.
    /// Groups are typically in the "groups" claim or custom namespaced claims.
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from Okta token
    #[must_use]
    pub fn extract_groups(raw_claims: &serde_json::Value) -> Vec<String> {
        // Try standard Okta groups claim first
        if let Some(groups_val) = raw_claims.get("groups") {
            if let Ok(groups) = serde_json::from_value::<Vec<String>>(groups_val.clone()) {
                return groups;
            }
        }

        // Fallback: check for custom roles claim
        if let Some(roles_val) = raw_claims.get("roles") {
            if let Ok(roles) = serde_json::from_value::<Vec<String>>(roles_val.clone()) {
                return roles;
            }
        }

        Vec::new()
    }

    /// Map Okta groups to FraiseQL role permissions
    ///
    /// Maps Okta group names to FraiseQL role names.
    /// Supports flexible naming conventions for Okta groups.
    ///
    /// # Arguments
    /// * `okta_groups` - List of Okta group names
    #[must_use]
    pub fn map_okta_groups_to_fraiseql(okta_groups: Vec<String>) -> Vec<String> {
        okta_groups
            .into_iter()
            .filter_map(|group| {
                let group_lower = group.to_lowercase();

                match group_lower.as_str() {
                    // Direct group matches
                    "fraiseql-admin" | "fraiseql_admin" | "admin" | "administrators" => {
                        Some("admin".to_string())
                    },
                    "fraiseql-operator" | "fraiseql_operator" | "operator" | "operators" => {
                        Some("operator".to_string())
                    },
                    "fraiseql-viewer" | "fraiseql_viewer" | "viewer" | "viewers" | "user"
                    | "fraiseql-user" | "read_only" | "okta_viewer" | "everyone" => {
                        Some("viewer".to_string())
                    },
                    // Common Okta patterns
                    "okta_admin" => Some("admin".to_string()),
                    "okta_operator" => Some("operator".to_string()),
                    _ => {
                        // Check for partial matches (admin/operator/viewer substrings)
                        if group_lower.contains("admin") {
                            Some("admin".to_string())
                        } else if group_lower.contains("operator") {
                            Some("operator".to_string())
                        } else if group_lower.contains("viewer")
                            || group_lower.contains("user")
                            || group_lower.contains("read")
                        {
                            Some("viewer".to_string())
                        } else {
                            None
                        }
                    },
                }
            })
            .collect()
    }

    /// Extract organization information from Okta claims
    ///
    /// Okta provides org information in claims or via the org_id custom claim.
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims
    /// * `email` - User email as fallback
    #[must_use]
    pub fn extract_org_id(raw_claims: &serde_json::Value, email: &str) -> Option<String> {
        // Check for explicit org_id claim
        if let Some(org_id_val) = raw_claims.get("org_id") {
            if let Some(org_id_str) = org_id_val.as_str() {
                return Some(org_id_str.to_string());
            }
        }

        // Check for Okta-specific org claim
        if let Some(org_val) = raw_claims.get("org") {
            if let Some(org_str) = org_val.as_str() {
                return Some(org_str.to_string());
            }
        }

        // Fallback: extract from email domain
        email
            .split('@')
            .nth(1)
            .and_then(|domain| domain.split('.').next())
            .map(|domain_part| domain_part.to_string())
    }

    /// Get user's Okta ID
    ///
    /// Okta provides the user ID in the 'sub' (subject) claim
    #[must_use]
    pub fn get_okta_id(raw_claims: &serde_json::Value) -> Option<String> {
        raw_claims.get("sub").and_then(|sub| sub.as_str()).map(|s| s.to_string())
    }
}

// Reason: OAuthProvider is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl OAuthProvider for OktaOAuth {
    fn name(&self) -> &'static str {
        "okta"
    }

    fn authorization_url(&self, state: &str) -> String {
        self.oidc.authorization_url(state)
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        self.oidc.exchange_code(code).await
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        let mut user_info = self.oidc.user_info(access_token).await?;

        // Extract Okta-specific claims
        let groups = Self::extract_groups(&user_info.raw_claims);
        user_info.raw_claims["okta_groups"] = json!(groups);

        // Extract Okta user ID
        if let Some(okta_id) = Self::get_okta_id(&user_info.raw_claims) {
            user_info.raw_claims["okta_id"] = json!(&okta_id);
        }

        // Extract organization ID
        let email = user_info.email.clone().unwrap_or_default();
        if let Some(org_id) = Self::extract_org_id(&user_info.raw_claims, &email) {
            user_info.raw_claims["org_id"] = json!(&org_id);
        }

        // Store Okta domain for reference
        user_info.raw_claims["okta_domain"] = json!(&self.domain);

        // Add email verification status
        if let Some(email_verified) = user_info.raw_claims.get("email_verified") {
            user_info.raw_claims["okta_email_verified"] = email_verified.clone();
        }

        // Extract user type if present
        if let Some(user_type) = user_info.raw_claims.get("user_type") {
            user_info.raw_claims["okta_user_type"] = user_type.clone();
        }

        Ok(user_info)
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        self.oidc.refresh_token(refresh_token).await
    }

    async fn revoke_token(&self, token: &str) -> Result<()> {
        self.oidc.revoke_token(token).await
    }
}
