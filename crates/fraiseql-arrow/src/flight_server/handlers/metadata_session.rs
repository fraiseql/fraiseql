//! The session the Flight metadata calls require: the one `DoGet` requires, so every Flight
//! call has one access rule. A call without one is refused `Unauthenticated` before it reads
//! the registry.

use tonic::{Request, Status};

use super::super::{FraiseQLFlightService, extract_session_token};

/// Refuse `request` unless it carries a session token the service issued, that has not
/// expired, and whose source token the token revocation store still admits.
///
/// The token is read from `request` before the returned future starts, so the future holds
/// no reference to the request and is `Send` whatever the message type.
///
/// # Errors
///
/// `Unauthenticated` without a session token, or with one the service did not issue, that
/// expired or whose source token is revoked; `Internal` when the service has no session
/// secret, as `DoGet` answers.
pub(super) fn require_session<'a, T>(
    svc: &'a FraiseQLFlightService,
    request: &Request<T>,
) -> impl std::future::Future<Output = std::result::Result<(), Status>> + Send + 'a {
    let session_token = extract_session_token(request);
    async move {
        let session_token = session_token?;
        let secret = svc
            .session_secret
            .as_deref()
            .ok_or_else(|| Status::internal("FLIGHT_SESSION_SECRET not configured"))?;
        super::admit_session(svc, &session_token, secret).await.map(|_| ())
    }
}
