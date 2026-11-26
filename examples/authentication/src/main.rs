//! Minting a token, validating it, and reading the identity back out.
//!
//! FraiseQL's server accepts a bearer token, validates it, and turns the claims into
//! the identity that row-level security and field-level authorization see. This
//! example does that half in isolation: no server, no database — just the validator
//! the server uses, [`OidcValidator`], against tokens from an ES256 issuer this
//! process runs on loopback.
//!
//! The interesting part is not the happy path. It is the five rejections at the end:
//! a validator that accepts any of them is a validator that lets one service's tokens
//! be replayed against another, or lets an expired session keep working.
//!
//! The issuer here generates its key at startup and publishes the public half as a
//! JWKS, which is what an OIDC provider does — and what the server's own issuer
//! (`[auth_issuer]`) does for the sessions it mints. The validator holds only public
//! keys.
//!
//! Run it:
//!
//! ```text
//! ./run.sh
//! ```

#![allow(clippy::print_stdout)] // Reason: an example shows its results by printing them

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use fraiseql_core::security::{OidcConfig, OidcValidator};
use fraiseql_test_support::issuer::{Claims, TestIssuer};
use serde_json::json;

const OTHER_AUDIENCE: &str = "some-other-service";

#[tokio::main]
async fn main() -> Result<()> {
    let issuer = TestIssuer::start().await;

    // A validator is pinned to one issuer AND one audience. Pinning only the issuer
    // accepts every token that issuer ever minted, including those addressed to a
    // different service.
    let config = OidcConfig {
        issuer: Some(issuer.issuer().to_string()),
        audience: Some(issuer.audience().to_string()),
        required: true,
        allowed_algorithms: vec!["ES256".to_string()],
        ..OidcConfig::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, &issuer.jwks_uri())?;

    // ── the happy path ─────────────────────────────────────────────────────
    let token = issuer.mint(&claims_for(&issuer));
    let user = validator.validate_token(&token).await?;

    println!("── a valid token");
    println!("  sub:     {}", user.user_id.as_str());
    println!("  email:   {:?}", user.email);
    println!("  name:    {:?}", user.display_name);
    println!("  roles:   {:?}", user.extra_claims.get("roles"));
    println!("  expires: {}", user.expires_at);

    // Custom claims are where authorization data lives. The server puts them into the
    // session variables a row-level-security policy reads, so `roles` here is what
    // decides which rows a query can see.
    let roles = user
        .extra_claims
        .get("roles")
        .and_then(serde_json::Value::as_array)
        .map_or_default(|roles| {
            roles.iter().filter_map(serde_json::Value::as_str).collect::<Vec<_>>()
        });
    println!(
        "  → this request is authorized for: {}",
        if roles.is_empty() {
            "nothing".to_string()
        } else {
            roles.join(", ")
        }
    );

    // ── the rejections ─────────────────────────────────────────────────────
    //
    // Each of these must fail. A `validated` line here would be a finding.
    println!("\n── tokens that must be rejected");
    let now = now();
    let rejections = [
        (
            "expired an hour ago",
            issuer.mint(
                &claims_for(&issuer)
                    .set("iat", now - 7200)
                    .set("nbf", now - 7200)
                    .set("exp", now - 3600),
            ),
        ),
        (
            "addressed to another service",
            issuer.mint(&claims_for(&issuer).set("aud", OTHER_AUDIENCE)),
        ),
        ("signed by someone else", issuer.foreign_token(&claims_for(&issuer))),
        (
            "minted by a different issuer",
            issuer.mint(&claims_for(&issuer).set("iss", "https://attacker.example.com")),
        ),
        ("not a JWT at all", "Bearer nope".to_string()),
    ];

    let mut accepted = 0;
    for (label, token) in rejections {
        match validator.validate_token(&token).await {
            Ok(user) => {
                accepted += 1;
                println!("  ACCEPTED {label} (sub={}) — this is a defect", user.user_id.as_str());
            },
            Err(err) => println!("  rejected {label}: {err}"),
        }
    }

    if accepted > 0 {
        return Err(anyhow!("{accepted} token(s) that must be rejected were accepted"));
    }
    println!("\nAll five rejected.");
    Ok(())
}

/// The issuer's default claims for `user-42`, plus the profile and roles a provider
/// would add.
fn claims_for(issuer: &TestIssuer) -> Claims {
    issuer
        .claims()
        .set("sub", "user-42")
        .set("email", "ada@example.com")
        .set("name", "Ada Lovelace")
        .set("roles", json!(["reader", "author"]))
}

fn now() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock is before the Unix epoch")
        .as_secs();
    i64::try_from(secs).expect("epoch seconds fit an i64")
}
