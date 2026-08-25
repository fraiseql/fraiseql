//! Minting a JWT, validating it, and reading the identity back out.
//!
//! FraiseQL's server accepts a bearer token, validates it, and turns the claims into
//! the identity that row-level security and field-level authorization see. This
//! example does that half in isolation: no server, no database, no network — just
//! [`fraiseql_auth`]'s validator against tokens this process signs itself.
//!
//! The interesting part is not the happy path. It is the five rejections at the end:
//! a validator that accepts any of them is a validator that lets one service's tokens
//! be replayed against another, or lets an expired session keep working.
//!
//! An HS256 shared secret is used here because it fits in one file. Production uses
//! RS256 against the provider's JWKS, where the server holds only a public key —
//! see `docs/` for the OIDC configuration; the validation posture below is the same.
//!
//! Run it:
//!
//! ```text
//! ./run.sh
//! ```

use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, anyhow};
use fraiseql_auth::{Claims, JwtValidator, generate_hs256_token};
use jsonwebtoken::Algorithm;
use serde_json::json;

const ISSUER: &str = "https://issuer.example.com";
const AUDIENCE: &str = "fraiseql-api";
const OTHER_AUDIENCE: &str = "some-other-service";

/// The signing secret. In a real deployment this is a secret manager reference, and
/// with RS256 the server never holds a signing key at all.
const SECRET: &[u8] = b"a-shared-secret-at-least-32-bytes-long!!";

fn main() -> Result<()> {
    // A validator is pinned to one issuer AND one audience. Pinning only the issuer
    // accepts every token that issuer ever minted, including those addressed to a
    // different service.
    let validator = JwtValidator::new(ISSUER, Algorithm::HS256)?.with_audiences(&[AUDIENCE])?;

    // ── the happy path ─────────────────────────────────────────────────────
    let token = generate_hs256_token(&claims_for("user-42", AUDIENCE, 3600, 0), SECRET)?;
    let validated = validator.validate_hmac(&token, SECRET)?;

    println!("── a valid token");
    println!("  sub:    {}", validated.sub);
    println!("  iss:    {}", validated.iss);
    println!("  aud:    {:?}", validated.aud);
    println!("  email:  {:?}", validated.email());
    println!("  name:   {:?}", validated.name());
    println!("  roles:  {:?}", validated.get_custom("roles"));
    println!("  expired: {}", validated.is_expired());

    // Custom claims are where authorization data lives. The server puts them into the
    // session variables a row-level-security policy reads, so `roles` here is what
    // decides which rows a query can see.
    let roles = validated
        .get_custom("roles")
        .and_then(serde_json::Value::as_array)
        .map(|roles| roles.iter().filter_map(serde_json::Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
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
    let rejections: [(&str, Result<String>); 5] = [
        (
            "expired an hour ago",
            generate_hs256_token(&claims_for("user-42", AUDIENCE, 0, 3600), SECRET)
                .map_err(Into::into),
        ),
        (
            "addressed to another service",
            generate_hs256_token(&claims_for("user-42", OTHER_AUDIENCE, 3600, 0), SECRET)
                .map_err(Into::into),
        ),
        (
            "signed by someone else",
            generate_hs256_token(&claims_for("user-42", AUDIENCE, 3600, 0), b"the-wrong-secret")
                .map_err(Into::into),
        ),
        (
            "minted by a different issuer",
            generate_hs256_token(
                &Claims {
                    iss: "https://attacker.example.com".to_string(),
                    ..claims_for("user-42", AUDIENCE, 3600, 0)
                },
                SECRET,
            )
            .map_err(Into::into),
        ),
        ("not a JWT at all", Ok("Bearer nope".to_string())),
    ];

    let mut accepted = 0;
    for (label, token) in rejections {
        let token = token?;
        match validator.validate_hmac(&token, SECRET) {
            Ok(claims) => {
                accepted += 1;
                println!("  ACCEPTED {label} (sub={}) — this is a defect", claims.sub);
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

/// Build claims for `subject`, valid `ttl_secs` from now, issued `age_secs` ago.
fn claims_for(subject: &str, audience: &str, ttl_secs: u64, age_secs: u64) -> Claims {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock is before the Unix epoch")
        .as_secs();
    let issued_at = now - age_secs;

    let mut extra = HashMap::new();
    extra.insert("email".to_string(), json!("ada@example.com"));
    extra.insert("name".to_string(), json!("Ada Lovelace"));
    extra.insert("roles".to_string(), json!(["reader", "author"]));

    Claims {
        sub: subject.to_string(),
        iat: issued_at,
        exp: issued_at + ttl_secs,
        nbf: None,
        iss: ISSUER.to_string(),
        aud: vec![audience.to_string()],
        extra,
    }
}
