//! PyO3 bindings for authentication module (Phase 10).

use pyo3::prelude::*;

use crate::auth::provider::{Auth0Provider, CustomJWTProvider};
use crate::pipeline::unified::UserContext;

/// Python wrapper for UserContext (exposed from Rust to Python)
#[pyclass]
#[derive(Clone)]
pub struct PyUserContext {
    #[pyo3(get)]
    pub user_id: Option<String>,
    #[pyo3(get)]
    pub roles: Vec<String>,
    #[pyo3(get)]
    pub permissions: Vec<String>,
    #[pyo3(get)]
    pub exp: u64,
}

impl From<UserContext> for PyUserContext {
    fn from(ctx: UserContext) -> Self {
        Self {
            user_id: ctx.user_id,
            roles: ctx.roles,
            permissions: ctx.permissions,
            exp: ctx.exp,
        }
    }
}

/// Python wrapper for authentication providers
///
/// This is a synchronous wrapper that provides factory methods for creating
/// Auth0 and custom JWT providers. Token validation is currently synchronous
/// and should be called from async Python code using asyncio.to_thread() or similar.
#[pyclass]
#[allow(dead_code)]
pub struct PyAuthProvider {
    // Store provider configuration for lazy initialization
    // (Some fields stored for future validation/debug features)
    provider_type: String,
    domain_or_issuer: String,
    audience: Vec<String>,
    jwks_url: Option<String>,
    roles_claim: Option<String>,
    permissions_claim: Option<String>,
}

#[pymethods]
impl PyAuthProvider {
    /// Create an Auth0 provider.
    ///
    /// Args:
    ///     domain: Auth0 domain (e.g., "example.auth0.com")
    ///     audience: List of allowed audiences
    ///
    /// Returns:
    ///     PyAuthProvider instance
    ///
    /// Raises:
    ///     ValueError: If domain or audience is invalid
    #[staticmethod]
    pub fn auth0(domain: String, audience: Vec<String>) -> PyResult<Self> {
        // Validate by creating a temporary provider
        let _provider = Auth0Provider::new(&domain, audience.clone()).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Failed to create Auth0 provider: {}",
                e
            ))
        })?;

        Ok(Self {
            provider_type: "auth0".to_string(),
            domain_or_issuer: domain,
            audience,
            jwks_url: None,
            roles_claim: None,
            permissions_claim: None,
        })
    }

    /// Create a custom JWT provider.
    ///
    /// Args:
    ///     issuer: JWT issuer URL (must be HTTPS)
    ///     audience: List of allowed audiences
    ///     jwks_url: URL to fetch JWK set (must be HTTPS)
    ///     roles_claim: Custom claim name for roles (default: "roles")
    ///     permissions_claim: Custom claim name for permissions (default: "permissions")
    ///
    /// Returns:
    ///     PyAuthProvider instance
    ///
    /// Raises:
    ///     ValueError: If URLs are invalid or other parameters are malformed
    #[staticmethod]
    #[pyo3(signature = (issuer, audience, jwks_url, roles_claim="roles", permissions_claim="permissions"))]
    pub fn jwt(
        issuer: String,
        audience: Vec<String>,
        jwks_url: String,
        roles_claim: &str,
        permissions_claim: &str,
    ) -> PyResult<Self> {
        // Validate by creating a temporary provider
        let _provider = CustomJWTProvider::new(
            issuer.clone(),
            audience.clone(),
            jwks_url.clone(),
            roles_claim.to_string(),
            permissions_claim.to_string(),
        )
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Failed to create custom JWT provider: {}",
                e
            ))
        })?;

        Ok(Self {
            provider_type: "jwt".to_string(),
            domain_or_issuer: issuer,
            audience,
            jwks_url: Some(jwks_url),
            roles_claim: Some(roles_claim.to_string()),
            permissions_claim: Some(permissions_claim.to_string()),
        })
    }

    /// Get provider type (for debugging)
    pub fn provider_type(&self) -> String {
        self.provider_type.clone()
    }

    /// Get configured audience list
    pub fn audience(&self) -> Vec<String> {
        self.audience.clone()
    }
}
