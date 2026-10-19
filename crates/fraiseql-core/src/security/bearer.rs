//! The one parser of a `Bearer` credential (RFC 6750 §2.1).
//!
//! Every transport reads its token through [`bearer_credential`]: the HTTP auth
//! middlewares, MCP, storage, gRPC, the admin bearer gate, the rate limiter and the Flight
//! handshake. Each used to strip the prefix itself, and the copies disagreed on an empty
//! credential and on surrounding whitespace.

/// The credential of an `Authorization` value of the form `Bearer <token>`.
///
/// `None` for another scheme or for an empty credential. Surrounding whitespace is not
/// part of the credential. The scheme is matched exactly (`Bearer`), as every transport
/// matched it before this function existed.
#[must_use]
pub fn bearer_credential(value: &str) -> Option<&str> {
    let token = value.strip_prefix("Bearer ")?.trim();
    (!token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests;
