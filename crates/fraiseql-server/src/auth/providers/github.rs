// GitHub OAuth provider implementation
use async_trait::async_trait;
use serde::Deserialize;

use crate::auth::{
    error::{AuthError, Result},
    oidc_provider::OidcProvider,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// GitHub OAuth provider wrapper
///
/// Handles GitHub-specific OAuth flows and team mapping to FraiseQL roles.
#[derive(Debug)]
pub struct GitHubOAuth {
    oidc: OidcProvider,
}

/// GitHub user information with teams
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubUser {
    pub id: u64,
    pub login: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub company: Option<String>,
    pub location: Option<String>,
    pub public_repos: u32,
}

/// GitHub team from API response
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubTeam {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub organization: GitHubOrg,
}

/// GitHub organization
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubOrg {
    pub id: u64,
    pub login: String,
}

impl GitHubOAuth {
    /// Create a new GitHub OAuth provider
    ///
    /// # Arguments
    /// * `client_id` - GitHub OAuth app client ID
    /// * `client_secret` - GitHub OAuth app client secret
    /// * `redirect_uri` - Redirect URI after authentication (e.g., "http://localhost:8000/auth/callback")
    pub async fn new(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
    ) -> Result<Self> {
        let oidc = OidcProvider::new(
            "github",
            "https://github.com",
            &client_id,
            &client_secret,
            &redirect_uri,
        )
        .await?;

        Ok(Self { oidc })
    }

    /// Map GitHub teams to FraiseQL roles
    ///
    /// Maps organization:team slugs to role names.
    /// Example: "my-org:admin-team" -> "admin"
    ///
    /// # Arguments
    /// * `teams` - List of "org:team" strings from GitHub
    pub fn map_teams_to_roles(teams: Vec<String>) -> Vec<String> {
        teams
            .into_iter()
            .filter_map(|team| {
                let parts: Vec<&str> = team.split(':').collect();
                if parts.len() == 2 {
                    match parts[1] {
                        "admin" | "administrators" | "admin-team" => Some("admin".to_string()),
                        "operator" | "operators" | "operator-team" => Some("operator".to_string()),
                        "viewer" | "viewers" | "viewer-team" => Some("viewer".to_string()),
                        "maintainer" | "maintainers" => Some("operator".to_string()),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get user info including teams from GitHub API
    ///
    /// # Arguments
    /// * `access_token` - GitHub access token
    pub async fn get_user_with_teams(&self, access_token: &str) -> Result<(GitHubUser, Vec<String>)> {
        let client = reqwest::Client::new();

        // Get user info
        let user: GitHubUser = client
            .get("https://api.github.com/user")
            .header("Authorization", format!("token {}", access_token))
            .header("User-Agent", "FraiseQL")
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to fetch GitHub user: {}", e),
            })?
            .json()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse GitHub user: {}", e),
            })?;

        // Get teams (organizations membership)
        let teams: Vec<GitHubTeam> = client
            .get("https://api.github.com/user/teams")
            .header("Authorization", format!("token {}", access_token))
            .header("User-Agent", "FraiseQL")
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to fetch GitHub teams: {}", e),
            })?
            .json()
            .await
            .unwrap_or_default();

        let team_strings: Vec<String> = teams
            .iter()
            .map(|t| format!("{}:{}", t.organization.login, t.slug))
            .collect();

        Ok((user, team_strings))
    }

    /// Extract organization ID from GitHub teams (primary org)
    ///
    /// Returns the first organization the user belongs to as the org_id.
    /// In multi-org scenarios, this should be overridden with explicit org selection.
    pub fn extract_org_id_from_teams(teams: &[(GitHubUser, Vec<String>)]) -> Option<String> {
        teams
            .first()
            .and_then(|(_, team_strings)| team_strings.first())
            .and_then(|team_str| team_str.split(':').next())
            .map(|org| org.to_string())
    }
}

#[async_trait]
impl OAuthProvider for GitHubOAuth {
    fn name(&self) -> &str {
        "github"
    }

    fn authorization_url(&self, state: &str) -> String {
        self.oidc.authorization_url(state)
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        self.oidc.exchange_code(code).await
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        // Get basic user info from OIDC
        let user_info = self.oidc.user_info(access_token).await?;

        // Fetch additional GitHub-specific data
        let client = reqwest::Client::new();
        let github_user: GitHubUser = client
            .get("https://api.github.com/user")
            .header("Authorization", format!("token {}", access_token))
            .header("User-Agent", "FraiseQL")
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to fetch GitHub user: {}", e),
            })?
            .json()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to parse GitHub user: {}", e),
            })?;

        // Get teams
        let teams: Vec<GitHubTeam> = client
            .get("https://api.github.com/user/teams")
            .header("Authorization", format!("token {}", access_token))
            .header("User-Agent", "FraiseQL")
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to fetch GitHub teams: {}", e),
            })?
            .json()
            .await
            .unwrap_or_default();

        let team_strings: Vec<String> = teams
            .iter()
            .map(|t| format!("{}:{}", t.organization.login, t.slug))
            .collect();

        // Extract org_id from primary organization
        let org_id = team_strings
            .first()
            .and_then(|team| team.split(':').next())
            .map(|org| org.to_string());

        // Merge GitHub data into user info
        let mut user_info = user_info;
        user_info.raw_claims["github_id"] = serde_json::json!(github_user.id);
        user_info.raw_claims["github_login"] = serde_json::json!(github_user.login);
        user_info.raw_claims["github_teams"] = serde_json::json!(team_strings);
        user_info.raw_claims["github_company"] = serde_json::json!(github_user.company);
        user_info.raw_claims["github_location"] = serde_json::json!(github_user.location);
        user_info.raw_claims["github_public_repos"] = serde_json::json!(github_user.public_repos);

        // Add org_id if available (from primary organization)
        if let Some(org_id) = org_id {
            user_info.raw_claims["org_id"] = serde_json::json!(&org_id);
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
    fn test_map_github_teams_to_roles() {
        let teams = vec![
            "acme-corp:admin".to_string(),
            "acme-corp:operators".to_string(),
            "acme-corp:unknown".to_string(),
            "other-org:viewer".to_string(),
        ];

        let roles = GitHubOAuth::map_teams_to_roles(teams);

        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
        assert!(roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_teams_empty() {
        let roles = GitHubOAuth::map_teams_to_roles(vec![]);
        assert!(roles.is_empty());
    }

    #[test]
    fn test_map_teams_no_matches() {
        let teams = vec!["org:unknown-team".to_string(), "org:other".to_string()];
        let roles = GitHubOAuth::map_teams_to_roles(teams);
        assert!(roles.is_empty());
    }
}
