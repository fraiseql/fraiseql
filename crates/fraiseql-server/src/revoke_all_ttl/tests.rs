//! The retention a `revoke-all` gets, and the configurations that are refused.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code, panics acceptable

#[cfg(feature = "auth")]
use fraiseql_core::schema::{CompiledSchema, SecurityConfig};
#[cfg(feature = "auth")]
use serde_json::json;

#[cfg(feature = "auth")]
use super::for_server;
use super::{DEFAULT_SECS, resolve};
use crate::ServerError;
#[cfg(feature = "auth")]
use crate::server_config::IssuerConfig;

pub const DAY: u64 = 86_400;
#[cfg(feature = "auth")]
const WEEK: u64 = 7 * DAY;

fn refusal(result: Result<u64, ServerError>) -> String {
    match result {
        Err(ServerError::ConfigError(message)) => message,
        other => panic!("expected a configuration refusal, got {other:?}"),
    }
}

#[test]
fn the_default_is_a_day() {
    assert_eq!(DEFAULT_SECS, DAY);
}

#[test]
fn without_a_first_party_issuer_the_configured_value_is_used_as_is() {
    assert_eq!(resolve(Some(60), None).unwrap(), 60);
    assert_eq!(resolve(None, None).unwrap(), DAY);
}

#[test]
fn unset_it_follows_the_sessions_this_server_signs_and_never_drops_below_a_day() {
    assert_eq!(resolve(None, Some(961)).unwrap(), DAY);
    assert_eq!(resolve(None, Some(WEEK + 61)).unwrap(), WEEK + 61);
}

#[test]
fn set_to_exactly_the_accepted_span_it_is_used() {
    assert_eq!(resolve(Some(961), Some(961)).unwrap(), 961);
}

#[test]
fn set_one_second_short_of_the_accepted_span_it_is_refused_naming_both() {
    let message = refusal(resolve(Some(960), Some(961)));
    assert!(message.contains("revoke_all_ttl_secs = 960"), "{message}");
    assert!(message.contains("961"), "{message}");
}

#[cfg(feature = "auth")]
pub fn issuer() -> IssuerConfig {
    IssuerConfig {
        url:                    "https://api.example.com".to_string(),
        audience:               "my-api".to_string(),
        key_encryption_key_env: "FRAISEQL_TEST_REVOKE_ALL_KEK_NEVER_SET".to_string(),
    }
}

#[cfg(feature = "auth")]
/// A schema with `[security.token_revocation]` (`enabled`, `revoke_all_ttl_secs`) and an
/// `[auth]` block with `[auth.session] ttl_secs` and anonymous sign-in on or off.
pub fn schema(
    enabled: bool,
    configured: Option<u64>,
    ttl_secs: Option<u64>,
    anonymous: bool,
) -> CompiledSchema {
    let mut revocation = json!({ "enabled": enabled, "backend": "memory" });
    if let Some(configured) = configured {
        revocation["revoke_all_ttl_secs"] = json!(configured);
    }
    let mut session = json!({});
    if let Some(ttl_secs) = ttl_secs {
        session["ttl_secs"] = json!(ttl_secs);
    }
    CompiledSchema {
        security: Some(SecurityConfig {
            token_revocation: Some(serde_json::from_value(revocation).expect("token_revocation")),
            ..SecurityConfig::default()
        }),
        auth: Some(
            serde_json::from_value(json!({
                "session": session,
                "local": { "password": true, "anonymous": anonymous },
            }))
            .expect("auth"),
        ),
        ..CompiledSchema::default()
    }
}

#[cfg(feature = "auth")]
#[test]
fn the_accepted_span_is_the_session_ttl_its_grace_and_the_second_it_ends_on() {
    let issuer = issuer();
    assert_eq!(
        for_server(Some(&issuer), &schema(true, Some(961), Some(900), false)).unwrap(),
        961
    );
    refusal(for_server(Some(&issuer), &schema(true, Some(960), Some(900), false)));
}

#[cfg(feature = "auth")]
#[test]
fn with_anonymous_sign_in_off_the_span_does_not_include_a_week() {
    let issuer = issuer();
    assert_eq!(
        for_server(Some(&issuer), &schema(true, None, Some(2 * DAY), false)).unwrap(),
        2 * DAY + 61
    );
}

#[cfg(feature = "auth")]
#[test]
fn with_anonymous_sign_in_on_the_span_is_at_least_a_week() {
    let issuer = issuer();
    assert_eq!(
        for_server(Some(&issuer), &schema(true, None, Some(900), true)).unwrap(),
        WEEK + 61
    );
    let message =
        refusal(for_server(Some(&issuer), &schema(true, Some(WEEK + 60), Some(900), true)));
    assert!(message.contains(&(WEEK + 61).to_string()), "{message}");
}

#[cfg(feature = "auth")]
#[test]
fn a_session_longer_than_a_week_widens_the_span_past_anonymous_sign_in() {
    let issuer = issuer();
    assert_eq!(
        for_server(Some(&issuer), &schema(true, None, Some(2 * WEEK), true)).unwrap(),
        2 * WEEK + 61
    );
}

#[test]
fn without_a_first_party_issuer_nothing_is_checked() {
    assert_eq!(for_server(None, &schema(true, Some(60), Some(900), false)).unwrap(), 60);
    assert_eq!(for_server(None, &schema(true, None, Some(2 * DAY), false)).unwrap(), DAY);
}

#[cfg(feature = "auth")]
#[test]
fn with_revocation_disabled_nothing_is_checked() {
    let issuer = issuer();
    for_server(Some(&issuer), &schema(false, Some(60), Some(900), false)).unwrap();
    for_server(Some(&issuer), &CompiledSchema::default()).unwrap();
}

#[cfg(feature = "auth")]
#[test]
fn an_issuer_without_a_session_ttl_is_refused_as_the_issuer_refuses_it() {
    let message = refusal(for_server(Some(&issuer()), &schema(true, None, None, false)));
    assert!(message.contains("[auth.session] ttl_secs"), "{message}");
}
