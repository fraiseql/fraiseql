//! Admin scope guard for JWT-authenticated admin requests.
//!
//! When admin routes receive a JWT (rather than a raw `admin_token`), the guard
//! verifies that the JWT's `scope` claim contains `fraiseql:admin`. Bearer-token
//! authenticated requests (using `admin_token`) bypass this check entirely for
//! backwards compatibility.
//!
//! The guard is additive: it does not replace bearer-token auth. Routes that use
//! the guard should apply it **after** the bearer-auth middleware.

/// The required scope claim value for admin API access via JWT.
pub const ADMIN_SCOPE: &str = "fraiseql:admin";

/// Check whether a space-delimited scope string contains the `fraiseql:admin` scope.
///
/// Scope claims in JWTs are typically a space-separated list of scope values
/// (RFC 8693 / `OpenID` Connect).
///
/// # Examples
///
/// ```
/// use fraiseql_server::middleware::admin_scope::has_admin_scope;
///
/// assert!(has_admin_scope("fraiseql:admin"));
/// assert!(has_admin_scope("read write fraiseql:admin"));
/// assert!(!has_admin_scope("read write"));
/// assert!(!has_admin_scope(""));
/// ```
#[must_use]
pub fn has_admin_scope(scope_claim: &str) -> bool {
    scope_claim.split_whitespace().any(|s| s == ADMIN_SCOPE)
}

/// Validate that a JWT scope claim authorizes admin access.
///
/// Returns `Ok(())` if the scope claim contains `fraiseql:admin`,
/// `Err` with a 403 message otherwise.
///
/// # Errors
///
/// Returns `FraiseQLError::Authorization` if the scope claim does not
/// contain `fraiseql:admin`.
pub fn require_admin_scope(scope_claim: &str) -> fraiseql_error::Result<()> {
    if has_admin_scope(scope_claim) {
        Ok(())
    } else {
        Err(fraiseql_error::FraiseQLError::unauthorized(format!(
            "Admin API requires '{ADMIN_SCOPE}' scope. \
             Found: '{scope_claim}'"
        )))
    }
}
