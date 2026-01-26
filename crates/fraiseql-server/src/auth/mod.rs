// Authentication module
// Handles JWT validation, OAuth/OIDC flows, session management, and authorization

pub mod error;
pub mod handlers;
pub mod jwt;
pub mod middleware;
pub mod monitoring;
pub mod oidc_provider;
pub mod operation_rbac;
pub mod provider;
pub mod providers;
pub mod session;
pub mod session_postgres;
pub mod state_store;

pub use error::{AuthError, Result};
pub use handlers::{
    AuthCallbackQuery, AuthLogoutRequest, AuthRefreshRequest, AuthStartRequest, AuthState,
    auth_callback, auth_logout, auth_refresh, auth_start,
};
pub use jwt::{Claims, JwtValidator};
pub use middleware::{AuthMiddleware, AuthenticatedUser};
pub use monitoring::{AuthEvent, AuthMetrics, OperationTimer};
pub use oidc_provider::OidcProvider;
pub use operation_rbac::{OperationPermission, RBACPolicy, Role};
pub use provider::{OAuthProvider, PkceChallenge, TokenResponse, UserInfo};
pub use providers::{AzureADOAuth, GitHubOAuth, GoogleOAuth, KeycloakOAuth, create_provider};
pub use session::{SessionData, SessionStore, TokenPair};
pub use session_postgres::PostgresSessionStore;
pub use state_store::{InMemoryStateStore, StateStore};
