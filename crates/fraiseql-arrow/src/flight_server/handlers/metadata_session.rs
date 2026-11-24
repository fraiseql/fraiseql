//! The session the Flight metadata calls require: the one `DoGet` requires, so every Flight
//! call has one access rule. A call without one is refused `Unauthenticated` before it reads
//! the registry.

use tonic::{Request, Status};

use super::super::{FraiseQLFlightService, extract_session_token, validate_session_token};

/// Refuse `request` unless it carries a session token the service issued and that has not
/// expired.
///
/// # Errors
///
/// `Unauthenticated` without a session token, or with one the service did not issue or that
/// expired; `Internal` when the service has no session secret, as `DoGet` answers.
pub(super) fn require_session<T>(
    svc: &FraiseQLFlightService,
    request: &Request<T>,
) -> std::result::Result<(), Status> {
    let session_token = extract_session_token(request)?;
    let secret = svc
        .session_secret
        .as_deref()
        .ok_or_else(|| Status::internal("FLIGHT_SESSION_SECRET not configured"))?;
    validate_session_token(&session_token, secret).map(|_| ())
}
