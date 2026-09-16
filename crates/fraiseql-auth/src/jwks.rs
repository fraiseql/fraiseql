//! The OAuth client's view of the shared JWKS client.
//!
//! This crate used to carry its own: a `JwksCache` with a TTL, a `kid` lookup, a
//! DNS-resolve-and-pin fetch, a 1 `MiB` response cap, and a JWK → `DecodingKey`
//! conversion accepting RSA and EC keys. `fraiseql-core` carried a near-twin for
//! the `[auth]` OIDC path, and the two had drifted: that one accepted RSA only
//! and did not pin DNS, so an operator whose IdP rotated onto an EC key had one
//! path work and the other refuse every token, and an issuer whose name rebound
//! mid-fetch was guarded on one path only.
//!
//! **Neither bounded a refetch.** That was the half they agreed on, and it was
//! the half that mattered: a `kid` the cache did not hold fetched again, every
//! time, so any caller could set the server's outbound request rate (#1335).
//!
//! There is now one implementation, in [`fraiseql_jwks`] — see that crate for
//! what "bounded" means and why the bound is a constant rather than a knob.

/// The one bounded JWKS client, and its errors.
///
/// Re-exported under this crate's root (see `lib.rs`) so a caller naming the type
/// in an `OidcClient`'s public field can reach it without guessing which version
/// of a third crate this one was built against (#1198).
pub use fraiseql_jwks::{JwksError, JwksSource};
