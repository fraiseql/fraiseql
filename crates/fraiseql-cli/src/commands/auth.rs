//! `fraiseql auth`: first-party auth operations on the server's database.
//!
//! `rotate-signing-key` reads exactly what the server reads, and takes no URL or secret on
//! its command line, where `ps` would show it: `[auth_issuer]` from the server configuration
//! file, `[auth.database]` from the project, the auth database URL and the key-encryption
//! key from the variables those name.

use anyhow::{Context, Result};
use fraiseql_server::{ServerConfig, first_party};

use super::run::{compile_schema, resolve_input};

/// `fraiseql auth rotate-signing-key`: retire the active signing key and activate a new one.
///
/// # Errors
///
/// Any input the server would refuse, a key-encryption key that does not open the active
/// key, a ring the server has not created, and a database failure.
pub async fn rotate_signing_key(input: Option<&str>, config: Option<&str>) -> Result<()> {
    let path = config
        .map(str::to_owned)
        .or_else(|| std::env::var("FRAISEQL_CONFIG").ok())
        .context(
            "rotating the signing key reads [auth_issuer] from the server configuration file: \
         name it with --config or FRAISEQL_CONFIG",
        )?;
    let server = ServerConfig::from_file(&path)
        .map_err(|e| anyhow::anyhow!("failed to parse config file `{path}`: {e}"))?;
    let issuer = server.auth_issuer.with_context(|| {
        format!("`{path}` has no [auth_issuer]: there is no first-party signing key to rotate")
    })?;

    let compiled = compile_schema(&resolve_input(input)?).await?;
    let database = compiled.schema.auth.as_ref().and_then(|auth| auth.database.as_ref());
    let store = first_party::signing_key_store(&issuer, database)?;

    let rotation = store.rotate(first_party::unix_now()?).await?;
    println!(
        "Retired signing key {}: it stays published until the sessions it signed have expired.",
        rotation.retired
    );
    println!("Active signing key {}.", rotation.active);
    println!(
        "Running servers sign with it after their next hourly re-read, or at once after \
         POST /admin/v1/auth/refresh-jwks."
    );
    Ok(())
}
