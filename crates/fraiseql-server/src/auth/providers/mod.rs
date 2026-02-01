// OAuth provider implementations
// Provides provider-specific wrappers for Auth0, GitHub, Google, Keycloak, Okta, and Azure AD

pub mod auth0;
pub mod azure_ad;
pub mod github;
pub mod google;
pub mod keycloak;
pub mod okta;

pub use auth0::Auth0OAuth;
pub use azure_ad::AzureADOAuth;
pub use github::GitHubOAuth;
pub use google::GoogleOAuth;
pub use keycloak::KeycloakOAuth;
pub use okta::OktaOAuth;

use crate::auth::{error::Result, provider::OAuthProvider};

/// Factory for creating OAuth providers from configuration
///
/// # Arguments
/// * `provider_type` - Provider type: "auth0", "github", "google", "keycloak", "okta", "azure_ad"
/// * `client_id` - OAuth client ID
/// * `client_secret` - OAuth client secret
/// * `config` - Provider-specific configuration (JSON value)
///
/// # Returns
/// A boxed OAuthProvider implementation
pub async fn create_provider(
    provider_type: &str,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    config: Option<serde_json::Value>,
) -> Result<Box<dyn OAuthProvider>> {
    match provider_type {
        "auth0" => {
            let config = config.ok_or_else(|| crate::auth::AuthError::ConfigError {
                message: "Auth0 provider requires config with auth0_domain".to_string(),
            })?;

            let auth0_domain = config
                .get("auth0_domain")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::auth::AuthError::ConfigError {
                    message: "Missing auth0_domain in config".to_string(),
                })?
                .to_string();

            let provider = Auth0OAuth::new(client_id, client_secret, auth0_domain, redirect_uri)
                .await?;
            Ok(Box::new(provider))
        },
        "github" => {
            let provider = GitHubOAuth::new(client_id, client_secret, redirect_uri).await?;
            Ok(Box::new(provider))
        },
        "google" => {
            let provider = GoogleOAuth::new(client_id, client_secret, redirect_uri).await?;
            Ok(Box::new(provider))
        },
        "keycloak" => {
            let config = config.ok_or_else(|| crate::auth::AuthError::ConfigError {
                message: "Keycloak provider requires config with keycloak_url and realm"
                    .to_string(),
            })?;

            let keycloak_url = config
                .get("keycloak_url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::auth::AuthError::ConfigError {
                    message: "Missing keycloak_url in config".to_string(),
                })?
                .to_string();

            let realm = config
                .get("realm")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::auth::AuthError::ConfigError {
                    message: "Missing realm in config".to_string(),
                })?
                .to_string();

            let provider =
                KeycloakOAuth::new(client_id, client_secret, keycloak_url, realm, redirect_uri)
                    .await?;
            Ok(Box::new(provider))
        },
        "okta" => {
            let config = config.ok_or_else(|| crate::auth::AuthError::ConfigError {
                message: "Okta provider requires config with okta_domain".to_string(),
            })?;

            let okta_domain = config
                .get("okta_domain")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::auth::AuthError::ConfigError {
                    message: "Missing okta_domain in config".to_string(),
                })?
                .to_string();

            let provider = OktaOAuth::new(client_id, client_secret, okta_domain, redirect_uri)
                .await?;
            Ok(Box::new(provider))
        },
        "azure_ad" => {
            let config = config.ok_or_else(|| crate::auth::AuthError::ConfigError {
                message: "Azure AD provider requires config with tenant".to_string(),
            })?;

            let tenant = config
                .get("tenant")
                .and_then(|v| v.as_str())
                .ok_or_else(|| crate::auth::AuthError::ConfigError {
                    message: "Missing tenant in config".to_string(),
                })?
                .to_string();

            let provider =
                AzureADOAuth::new(client_id, client_secret, tenant, redirect_uri).await?;
            Ok(Box::new(provider))
        },
        _ => Err(crate::auth::AuthError::ConfigError {
            message: format!("Unknown provider type: {}", provider_type),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth0_role_mapping() {
        let roles =
            auth0::Auth0OAuth::map_auth0_roles_to_fraiseql(vec!["admin".to_string()]);
        assert!(roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_github_role_mapping() {
        let roles = github::GitHubOAuth::map_teams_to_roles(vec![
            "org:admin".to_string(),
            "org:operator".to_string(),
        ]);
        assert_eq!(roles.len(), 2);
    }

    #[test]
    fn test_google_role_mapping() {
        let roles = google::GoogleOAuth::map_groups_to_roles(vec![
            "fraiseql-admins@company.com".to_string(),
        ]);
        assert!(roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_keycloak_role_mapping() {
        let roles =
            keycloak::KeycloakOAuth::map_keycloak_roles_to_fraiseql(vec!["admin".to_string()]);
        assert!(roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_okta_group_mapping() {
        let groups = okta::OktaOAuth::map_okta_groups_to_fraiseql(vec![
            "fraiseql-admin".to_string(),
            "everyone".to_string(),
        ]);
        assert_eq!(groups.len(), 2);
        assert!(groups.contains(&"admin".to_string()));
        assert!(groups.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_azure_ad_role_mapping() {
        let roles =
            azure_ad::AzureADOAuth::map_azure_roles_to_fraiseql(vec!["fraiseql.admin".to_string()]);
        assert!(roles.contains(&"admin".to_string()));
    }
}
