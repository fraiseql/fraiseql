//! How long a `revoke-all` is kept: `[security.token_revocation] revoke_all_ttl_secs`,
//! checked against how long a session this server signs is accepted.

use fraiseql_core::schema::CompiledSchema;

use crate::{ServerError, server_config::IssuerConfig};

/// How long a `revoke-all` is kept when nothing asks for longer: a day.
pub const DEFAULT_SECS: u64 = 86_400;

/// The retention the server's revocation manager uses.
///
/// `configured` is `revoke_all_ttl_secs`; `accepted_secs` is how long a session this
/// server signs is accepted, when it signs any. Unset, the retention is the longer of the
/// two defaults; set, it is refused when shorter than `accepted_secs`.
///
/// # Errors
///
/// [`ServerError::ConfigError`] when `configured` is shorter than `accepted_secs`.
pub fn resolve(configured: Option<u64>, accepted_secs: Option<u64>) -> Result<u64, ServerError> {
    match (configured, accepted_secs) {
        (Some(configured), Some(accepted)) if configured < accepted => {
            Err(ServerError::ConfigError(format!(
                "[security.token_revocation] revoke_all_ttl_secs = {configured} is shorter than \
                 the {accepted}s a session this server signs is accepted for: set it to at \
                 least {accepted}, or leave it unset to follow [auth.session]"
            )))
        },
        (Some(configured), _) => Ok(configured),
        (None, accepted) => {
            Ok(accepted.map_or(DEFAULT_SECS, |accepted| accepted.max(DEFAULT_SECS)))
        },
    }
}

/// [`resolve`] for the server `schema` and `auth_issuer` describe.
///
/// When `[security.token_revocation]` is absent or disabled no manager is built, the value
/// is unused and nothing is checked.
///
/// # Errors
///
/// [`ServerError::ConfigError`] when the configured retention is shorter than a session
/// this server signs is accepted, or when `[auth_issuer]` has no `[auth.session] ttl_secs`.
pub fn for_server(
    auth_issuer: Option<&IssuerConfig>,
    schema: &CompiledSchema,
) -> Result<u64, ServerError> {
    let Some(config) = schema
        .security
        .as_ref()
        .and_then(|security| security.token_revocation.as_ref())
        .filter(|config| config.enabled)
    else {
        return Ok(DEFAULT_SECS);
    };
    #[cfg(feature = "auth")]
    let accepted = auth_issuer.map(|_| crate::first_party::accepted_secs(schema)).transpose()?;
    // Without `auth` a server with `[auth_issuer]` refuses to build, and signs nothing.
    #[cfg(not(feature = "auth"))]
    let accepted = {
        let _ = auth_issuer;
        None
    };
    resolve(config.revoke_all_ttl_secs, accepted)
}

#[cfg(test)]
pub(crate) mod tests;
