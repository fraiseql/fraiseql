//! `[security.rate_limiting] auth_mfa_verify_max_requests` sizes the MFA verify budget.
//!
//! The MFA route spends its own per-client budget before it reads the challenge, so
//! verification attempts with no valid challenge spend it as real ones do.

use fraiseql_core::schema::{RateLimitingSecurityConfig, SecurityConfig};

use super::{
    LocalAuthConfig, boot_schema, database_url_or_skip, drop_scratch, local_schema, post_from,
    scratch_pool, set_test_env, with_database,
};

#[tokio::test]
async fn the_configured_mfa_verify_budget_is_the_one_spent() {
    // Below the 50 attempts a minute the route allowed before the key existed.
    const BUDGET: u32 = 3;

    let Some(url) = database_url_or_skip("configured_mfa_verify_budget") else {
        return;
    };
    set_test_env();
    let db = "fraiseql_local_mfa_verify_budget";
    let pool = scratch_pool(&url, db).await;
    let scratch_url = with_database(&url, db);

    let mut schema = local_schema(LocalAuthConfig {
        mfa: true,
        mfa_issuer: Some("Acme".to_string()),
        ..LocalAuthConfig::default()
    });
    schema.security = Some(SecurityConfig {
        rate_limiting: Some(RateLimitingSecurityConfig {
            auth_mfa_verify_max_requests: BUDGET,
            auth_mfa_verify_window_secs: 3_600,
            ..RateLimitingSecurityConfig::default()
        }),
        ..SecurityConfig::default()
    });
    let server = boot_schema(schema, pool.clone(), &scratch_url).await;
    let one: std::net::IpAddr = [127, 0, 0, 1].into();
    let attempt =
        || serde_json::json!({ "challenge_token": "no-such-challenge", "code": "000000" });

    for n in 0..BUDGET {
        assert_ne!(
            post_from(one, &server.base, "/auth/v1/mfa/verify", attempt()).await,
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            "attempt {n} of a configured budget of {BUDGET} was refused"
        );
    }
    assert_eq!(
        post_from(one, &server.base, "/auth/v1/mfa/verify", attempt()).await,
        reqwest::StatusCode::TOO_MANY_REQUESTS,
        "the attempt past the configured budget must be refused"
    );

    server.stop().await;
    drop_scratch(&url, db).await;
}
