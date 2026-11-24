//! `fraiseql-auth`'s public API is nameable with `fraiseql-auth` as the only dependency
//! (#1198).
//!
//! This file imports no third-party crate. Every third-party type it names is reached
//! through `fraiseql_auth::`, which is precisely what a downstream can do — and could
//! not before: the crate's first documented constructor took a `jsonwebtoken` type that
//! this crate re-exported nowhere, so it did not compile unless the caller added
//! `jsonwebtoken` itself and guessed the major this workspace builds against. A mismatch
//! there is a type error, in code the caller never wrote.
//!
//! `tools/check-public-api-reexports.py` is the ratchet for the other published crates;
//! this is the one the issue names, so it is proved by compiling rather than by reading
//! source. That distinction earned its keep: the gate's first revision could not read a
//! type spelled by the name it was imported under, and reported this crate clean while
//! `sqlx::PgPool` — its widest public type, across fourteen signatures — was reachable
//! from nowhere. A compiling proof cannot be blind in that way, which is why the list
//! below names every dependency the public API touches rather than a sample.

/// The session store over a pool named through this crate: the `sqlx::PgPool` the
/// widest part of the public API takes. Compile-only, because a pool needs a runtime and a
/// database; what it proves is that the signature is nameable.
#[allow(dead_code)]
fn a_session_store_is_built_over_the_reexported_pool(
    pool: fraiseql_auth::sqlx::PgPool,
) -> fraiseql_auth::PostgresSessionStore {
    fraiseql_auth::PostgresSessionStore::new(pool)
        .with_token_claims("https://issuer.example.com", "my-api")
}

/// A session's custom claims are `serde_json` values, built and read through this crate.
#[test]
fn a_custom_claim_round_trips_through_the_reexported_json() {
    let mut extra = std::collections::HashMap::new();
    extra.insert("tenant".to_string(), fraiseql_auth::serde_json::json!("acme"));
    let claims = fraiseql_auth::Claims {
        sub: "user-1".to_string(),
        iat: 0,
        exp: 0,
        nbf: None,
        iss: "https://issuer.example.com".to_string(),
        aud: vec!["my-api".to_string()],
        extra,
    };

    assert_eq!(
        claims.get_custom("tenant").and_then(fraiseql_auth::serde_json::Value::as_str),
        Some("acme")
    );
}

/// Compile-only. Every third-party type `fraiseql-auth`'s public API names, reached
/// through this crate. Deleting a re-export breaks the build here, in the same run as
/// the deletion, which is louder than the gate and does not depend on it.
#[allow(dead_code)]
type EveryThirdPartyTypeInThePublicApi = (
    fraiseql_auth::anyhow::Error,
    fraiseql_auth::axum::Router,
    fraiseql_auth::axum::extract::State<()>,
    fraiseql_auth::axum::http::StatusCode,
    fraiseql_auth::fraiseql_time::Timestamp,
    fraiseql_auth::reqwest::Client,
    fraiseql_auth::serde_json::Value,
    fraiseql_auth::sqlx::PgPool,
    fraiseql_auth::tokio::sync::RwLock<()>,
    fraiseql_auth::url::ParseError,
    fraiseql_auth::uuid::Uuid,
    fraiseql_auth::zeroize::Zeroizing<String>,
);

/// The same for the feature-gated half: `redis` is compiled only under the two features
/// that use it, and its re-export carries the same `cfg`. A re-export gated on the wrong
/// feature is invisible to a source-reading gate and fails here instead.
#[allow(dead_code)]
#[cfg(any(feature = "redis-pkce", feature = "redis-rate-limiting"))]
type RedisTypesInThePublicApi = fraiseql_auth::redis::RedisError;
