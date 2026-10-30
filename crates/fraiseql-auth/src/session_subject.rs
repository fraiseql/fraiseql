//! The account an authenticated session was minted for (#945).
//!
//! Routes that act **on the caller's own account**, as email verification and `MFA`
//! enrollment do, need that account, and must not accept it from the request body: a
//! handler that trusts a body-supplied `user_id` is not authenticated at all, it is a
//! parameter.
//!
//! The server's authentication layer resolves the session, its revocation included, and
//! puts the account on the request as a [`SessionSubject`]. A handler reads it from there
//! and nowhere else, and refuses a request that carries none.

/// The account an authenticated session was minted for, which the server's
/// authentication layer puts on the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSubject(pub String);
