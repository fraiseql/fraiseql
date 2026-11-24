//! An authenticated user's `Display` text, which log lines carry: its expiry in UTC to the
//! whole second, with a space and no fraction.

use std::collections::HashMap;

use super::AuthenticatedUser;

#[test]
fn a_user_displays_its_expiry_in_utc_to_the_whole_second() {
    let expires_at = serde_json::from_value(serde_json::json!("2024-01-15T10:30:00.999999Z"))
        .expect("an RFC 3339 instant");
    let user = AuthenticatedUser {
        user_id: "user123".into(),
        scopes: vec![],
        expires_at,
        email: None,
        display_name: None,
        extra_claims: HashMap::new(),
        provenance: crate::security::Provenance::BearerToken,
    };

    assert_eq!(user.to_string(), "User(user123, expires=2024-01-15 10:30:00 UTC)");
}

/// Before 1970 a fraction belongs to the second below it, not the one toward zero.
#[test]
fn a_fraction_before_1970_displays_the_second_below_it() {
    let expires_at = serde_json::from_value(serde_json::json!("1969-12-31T23:59:59.500Z"))
        .expect("an RFC 3339 instant");
    let user = AuthenticatedUser {
        user_id: "user123".into(),
        scopes: vec![],
        expires_at,
        email: None,
        display_name: None,
        extra_claims: HashMap::new(),
        provenance: crate::security::Provenance::BearerToken,
    };

    assert_eq!(user.to_string(), "User(user123, expires=1969-12-31 23:59:59 UTC)");
}
