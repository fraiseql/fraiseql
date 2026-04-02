//! Authentication helpers for Flight service session tokens.

use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use tonic::{Request, Status};
use tracing::warn;

use super::SessionTokenClaims;

/// Map security error to gRPC status.
pub fn map_security_error_to_status(error: fraiseql_core::security::SecurityError) -> Status {
    use fraiseql_core::security::SecurityError;

    match error {
        SecurityError::TokenExpired { expired_at } => {
            Status::unauthenticated(format!("Token expired at {expired_at}"))
        },
        SecurityError::InvalidToken => Status::unauthenticated("Invalid token"),
        SecurityError::TokenMissingClaim { claim } => {
            Status::unauthenticated(format!("Token missing claim: {claim}"))
        },
        SecurityError::InvalidTokenAlgorithm { algorithm } => {
            Status::unauthenticated(format!("Invalid token algorithm: {algorithm}"))
        },
        SecurityError::AuthRequired => Status::unauthenticated("Authentication required"),
        _ => Status::unauthenticated(format!("Authentication failed: {error}")),
    }
}

/// Create a short-lived session token (5 minutes).
///
/// `secret` is the HMAC-SHA256 key, read once at service startup from
/// `FLIGHT_SESSION_SECRET` and cached in [`FraiseQLFlightService::session_secret`].
#[allow(clippy::result_large_err)] // Reason: tonic::Status is inherently large; boxing would add indirection in hot path
pub fn create_session_token(
    user: &fraiseql_core::security::auth_middleware::AuthenticatedUser,
    secret: &str,
) -> std::result::Result<String, Status> {
    let now = Utc::now();
    let exp = now + chrono::Duration::minutes(5);

    let claims = SessionTokenClaims {
        sub: user.user_id.clone(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        scopes: user.scopes.clone(),
        session_type: "flight".to_string(),
    };

    let key = EncodingKey::from_secret(secret.as_bytes());
    let header = Header::new(Algorithm::HS256);

    encode(&header, &claims, &key)
        .map_err(|e| Status::internal(format!("Failed to create session token: {e}")))
}

/// Validate session token from gRPC request.
///
/// Decodes and verifies HMAC-SHA256 session token, checking:
/// - Signature validity
/// - Expiration timestamp
/// - Session type ("flight")
///
/// # Arguments
/// * `token` - Session token string from Authorization header
/// * `secret` - HMAC-SHA256 key, cached at service startup in `FraiseQLFlightService`
///
/// # Returns
/// * `Ok(AuthenticatedUser)` - Valid token with user identity
/// * `Err(Status)` - Invalid token, expired, or malformed
#[allow(clippy::result_large_err)] // Reason: tonic::Status is inherently large; boxing would add indirection in hot path
pub fn validate_session_token(
    token: &str,
    secret: &str,
) -> std::result::Result<fraiseql_core::security::auth_middleware::AuthenticatedUser, Status> {
    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true; // Check expiration

    // Decode and verify token
    let token_data = decode::<SessionTokenClaims>(token, &key, &validation).map_err(|e| {
        warn!(error = %e, "Session token validation failed");
        match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                Status::unauthenticated("Session token expired - perform handshake again")
            },
            jsonwebtoken::errors::ErrorKind::InvalidSignature => {
                Status::unauthenticated("Invalid session token signature")
            },
            _ => Status::unauthenticated(format!("Invalid session token: {e}")),
        }
    })?;

    let claims = token_data.claims;

    // Verify session type
    if claims.session_type != "flight" {
        return Err(Status::unauthenticated("Invalid session type"));
    }

    // Convert claims back to AuthenticatedUser
    let expires_at = chrono::DateTime::<chrono::Utc>::from_timestamp(claims.exp, 0)
        .ok_or_else(|| Status::internal("Invalid expiration timestamp"))?;

    Ok(fraiseql_core::security::auth_middleware::AuthenticatedUser {
        user_id: claims.sub,
        scopes: claims.scopes,
        expires_at,
    })
}

/// Extract session token from gRPC request metadata.
///
/// Looks for "authorization" header in format: "Bearer <`session_token`>"
///
/// # Arguments
/// * `request` - Tonic gRPC request with metadata
///
/// # Returns
/// * `Ok(String)` - Session token extracted from header
/// * `Err(Status)` - Missing or malformed authorization header
#[allow(clippy::result_large_err)] // Reason: tonic::Status is inherently large; boxing would add indirection in hot path
pub fn extract_session_token<T>(request: &Request<T>) -> std::result::Result<String, Status> {
    let metadata = request.metadata();

    let auth_header = metadata.get("authorization").ok_or_else(|| {
        Status::unauthenticated("Missing authorization header - perform handshake first")
    })?;

    let auth_str = auth_header
        .to_str()
        .map_err(|_| Status::unauthenticated("Invalid authorization header encoding"))?;

    auth_str
        .strip_prefix("Bearer ")
        .ok_or_else(|| {
            Status::unauthenticated("Invalid authorization format, expected 'Bearer <token>'")
        })
        .map(|s| s.to_string())
}
