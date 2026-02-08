// Ory (Hydra/Kratos) OAuth provider implementation
use async_trait::async_trait;
use serde_json::json;

use crate::auth::{
    error::Result,
    oidc_provider::OidcProvider,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// Ory OAuth provider wrapper
///
/// Handles Ory-specific OAuth flows and group/role mapping.
/// Supports both Ory Cloud and self-hosted deployments.
///
/// # Examples
///
/// ```ignore
/// let provider = OryOAuth::new(
///     "client_id".to_string(),
///     "client_secret".to_string(),
///     "https://your-project.projects.oryapis.com".to_string(),
///     "http://localhost:8000/auth/callback".to_string(),
/// ).await?;
/// ```
#[derive(Debug)]
pub struct OryOAuth {
    oidc:       OidcProvider,
    issuer_url: String,
}

impl OryOAuth {
    /// Create a new Ory OAuth provider
    ///
    /// # Arguments
    /// * `client_id` - Ory client ID
    /// * `client_secret` - Ory client secret
    /// * `ory_issuer_url` - Ory issuer URL (e.g., <https://your-project.projects.oryapis.com> for Cloud,
    ///   or self-hosted instance URL)
    /// * `redirect_uri` - Redirect URI after authentication
    ///
    /// # Errors
    /// Returns error if OIDC discovery fails
    pub async fn new(
        client_id: String,
        client_secret: String,
        ory_issuer_url: String,
        redirect_uri: String,
    ) -> Result<Self> {
        let issuer_url = ory_issuer_url.clone();

        let oidc =
            OidcProvider::new("ory", &issuer_url, &client_id, &client_secret, &redirect_uri)
                .await?;

        Ok(Self { oidc, issuer_url })
    }

    /// Extract groups from JWT claims
    ///
    /// Looks for `groups` claim which may be a string or array.
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from token
    fn extract_groups(raw_claims: &serde_json::Value) -> Vec<String> {
        raw_claims
            .get("groups")
            .and_then(|groups| {
                if groups.is_array() {
                    Some(
                        groups
                            .as_array()
                            .unwrap_or(&vec![])
                            .iter()
                            .filter_map(|g| g.as_str().map(|s| s.to_string()))
                            .collect(),
                    )
                } else if let Some(s) = groups.as_str() {
                    // Single group as string
                    Some(vec![s.to_string()])
                } else {
                    None
                }
            })
            .unwrap_or_default()
    }

    /// Map Ory groups to FraiseQL role permissions
    ///
    /// Maps Ory groups and Keto permission patterns to FraiseQL role names.
    ///
    /// # Arguments
    /// * `ory_groups` - List of Ory group names or Keto permission patterns
    pub fn map_ory_groups_to_fraiseql(ory_groups: Vec<String>) -> Vec<String> {
        ory_groups
            .into_iter()
            .filter_map(|group| {
                let group_lower = group.to_lowercase();

                // Match direct group names
                match group_lower.as_str() {
                    "admin" | "ory-admin" | "administrators" => Some("admin".to_string()),
                    "operator" | "ory-operator" | "operators" => Some("operator".to_string()),
                    "viewer" | "ory-viewer" | "viewers" | "user" => Some("viewer".to_string()),
                    _ => {
                        // Match Keto permission patterns (e.g., "fraiseql:admin")
                        if group_lower.contains("fraiseql") {
                            if group_lower.contains("admin") {
                                Some("admin".to_string())
                            } else if group_lower.contains("operator") {
                                Some("operator".to_string())
                            } else if group_lower.contains("viewer") {
                                Some("viewer".to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    },
                }
            })
            .collect()
    }

    /// Extract organization ID from claims
    ///
    /// Tries to get `org_id` claim first, then falls back to extracting
    /// organization from email domain.
    ///
    /// # Arguments
    /// * `raw_claims` - Raw JWT claims from token
    /// * `email` - User email address
    fn extract_org_id(raw_claims: &serde_json::Value, email: &str) -> Option<String> {
        // Try to get org_id directly from claims
        if let Some(org_id) = raw_claims.get("org_id") {
            if let Some(org_id_str) = org_id.as_str() {
                return Some(org_id_str.to_string());
            }
        }

        // Fallback: extract domain from email if available
        if !email.is_empty() {
            if let Some(domain) = email.split('@').nth(1) {
                return Some(domain.to_string());
            }
        }

        None
    }
}

#[async_trait]
impl OAuthProvider for OryOAuth {
    fn name(&self) -> &'static str {
        "ory"
    }

    fn authorization_url(&self, state: &str) -> String {
        self.oidc.authorization_url(state)
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        self.oidc.exchange_code(code).await
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        let mut user_info = self.oidc.user_info(access_token).await?;

        // Extract Ory-specific claims
        let groups = Self::extract_groups(&user_info.raw_claims);
        let mapped_roles = Self::map_ory_groups_to_fraiseql(groups.clone());

        user_info.raw_claims["ory_groups"] = json!(groups);
        user_info.raw_claims["ory_roles"] = json!(mapped_roles);
        user_info.raw_claims["ory_issuer"] = json!(&self.issuer_url);

        // Extract org_id if present
        if let Some(org_id) =
            Self::extract_org_id(&user_info.raw_claims, &user_info.email)
        {
            user_info.raw_claims["org_id"] = json!(org_id);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_groups_from_array() {
        let claims = json!({
            "groups": ["admin", "operators", "viewers"]
        });

        let groups = OryOAuth::extract_groups(&claims);
        assert_eq!(groups.len(), 3);
        assert!(groups.contains(&"admin".to_string()));
        assert!(groups.contains(&"operators".to_string()));
    }

    #[test]
    fn test_extract_groups_from_string() {
        let claims = json!({
            "groups": "admin"
        });

        let groups = OryOAuth::extract_groups(&claims);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], "admin");
    }

    #[test]
    fn test_extract_groups_missing() {
        let claims = json!({});
        let groups = OryOAuth::extract_groups(&claims);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_map_ory_groups_to_fraiseql() {
        let groups = vec![
            "admin".to_string(),
            "ory-operator".to_string(),
            "user".to_string(),
            "unknown".to_string(),
        ];

        let fraiseql_roles = OryOAuth::map_ory_groups_to_fraiseql(groups);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_ory_groups_case_insensitive() {
        let groups = vec![
            "ADMIN".to_string(),
            "Operator".to_string(),
            "VIEWER".to_string(),
        ];

        let fraiseql_roles = OryOAuth::map_ory_groups_to_fraiseql(groups);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_ory_groups_keto_patterns() {
        let groups = vec![
            "fraiseql:admin".to_string(),
            "fraiseql:operator".to_string(),
            "fraiseql:viewer".to_string(),
            "other:role".to_string(),
        ];

        let fraiseql_roles = OryOAuth::map_ory_groups_to_fraiseql(groups);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_extract_org_id_from_claim() {
        let claims = json!({
            "org_id": "acme-corp"
        });

        let org_id = OryOAuth::extract_org_id(&claims, "user@example.com");
        assert_eq!(org_id, Some("acme-corp".to_string()));
    }

    #[test]
    fn test_extract_org_id_from_email_domain() {
        let claims = json!({});

        let org_id = OryOAuth::extract_org_id(&claims, "user@example.com");
        assert_eq!(org_id, Some("example.com".to_string()));
    }

    #[test]
    fn test_extract_org_id_missing() {
        let claims = json!({});

        let org_id = OryOAuth::extract_org_id(&claims, "");
        assert!(org_id.is_none());
    }

    #[test]
    fn test_extract_all_roles_and_org() {
        let claims = json!({
            "groups": ["admin", "operators"],
            "org_id": "my-org"
        });

        let groups = OryOAuth::extract_groups(&claims);
        let roles = OryOAuth::map_ory_groups_to_fraiseql(groups);
        let org_id = OryOAuth::extract_org_id(&claims, "user@example.com");

        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
        assert_eq!(org_id, Some("my-org".to_string()));
    }
}
