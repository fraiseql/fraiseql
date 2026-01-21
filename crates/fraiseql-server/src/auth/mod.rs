// Authentication module
// Handles JWT validation, OAuth/OIDC flows, session management

pub mod error;
pub mod jwt;
pub mod session;
pub mod session_postgres;
pub mod provider;
pub mod oidc_provider;
pub mod middleware;
pub mod handlers;

pub use error::{AuthError, Result};
pub use jwt::{Claims, JwtValidator};
pub use session::{SessionData, SessionStore, TokenPair};
pub use session_postgres::PostgresSessionStore;
pub use provider::{OAuthProvider, UserInfo, TokenResponse, PkceChallenge};
pub use oidc_provider::OidcProvider;
pub use middleware::{AuthMiddleware, AuthenticatedUser};
pub use handlers::{
    AuthState, auth_start, auth_callback, auth_refresh, auth_logout,
    AuthStartRequest, AuthCallbackQuery, AuthRefreshRequest, AuthLogoutRequest,
};
