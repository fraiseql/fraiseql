//! `fraiseql-auth`'s public API is nameable with `fraiseql-auth` as the only dependency
//! (#1198).
//!
//! This file imports no third-party crate. Every third-party type it names is reached
//! through `fraiseql_auth::`, which is precisely what a downstream can do — and could
//! not before: `JwtValidator::new(issuer, algorithm)` takes a `jsonwebtoken::Algorithm`
//! that this crate re-exported nowhere, so the documented first line did not compile
//! unless the caller added `jsonwebtoken` itself and guessed the major this workspace
//! builds against. A mismatch there is a type error, in code the caller never wrote.
//!
//! `tools/check-public-api-reexports.py` is the ratchet for the other twelve published
//! crates; this is the one the issue names, so it is proved by compiling rather than by
//! reading source.

/// The example from the crate's own documentation, with nothing else in scope.
#[test]
fn the_documented_first_line_compiles() {
    use fraiseql_auth::{Algorithm, JwtValidator};

    let validator = JwtValidator::new("https://issuer.example.com", Algorithm::HS256)
        .expect("HS256 validator for a well-formed issuer");

    // Used rather than dropped, so this stays a test of a working validator and not
    // merely of a constructor that returns something.
    assert!(validator.validate_hmac("not-a-token", b"secret").is_err());
}

/// Compile-only. Every third-party type `fraiseql-auth`'s public API names, reached
/// through this crate. Deleting a re-export breaks the build here, in the same run as
/// the deletion, which is louder than the gate and does not depend on it.
#[allow(dead_code)]
type EveryThirdPartyTypeInThePublicApi = (
    fraiseql_auth::anyhow::Error,
    fraiseql_auth::jsonwebtoken::Algorithm,
    fraiseql_auth::jsonwebtoken::DecodingKey,
    fraiseql_auth::reqwest::Client,
    fraiseql_auth::serde_json::Value,
);

/// The same for the feature-gated half: `redis` is compiled only under the two features
/// that use it, and its re-export carries the same `cfg`. A re-export gated on the wrong
/// feature is invisible to a source-reading gate and fails here instead.
#[allow(dead_code)]
#[cfg(any(feature = "redis-pkce", feature = "redis-rate-limiting"))]
type RedisTypesInThePublicApi = fraiseql_auth::redis::RedisError;
