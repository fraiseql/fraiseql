//! Unit tests for the storage-route auth helper (`storage_admin_user`).

use super::storage_admin_user;

#[test]
fn admin_user_is_none_when_no_token_configured() {
    assert!(
        storage_admin_user("any-presented-token", None).is_none(),
        "no configured storage_token must never grant admin",
    );
}

#[test]
fn admin_user_is_none_when_configured_token_is_empty() {
    // A misconfigured empty `storage_token` must not grant admin to a bare
    // `Authorization: Bearer ` (empty presented token) or anything else.
    assert!(storage_admin_user("", Some("")).is_none());
    assert!(storage_admin_user("whatever", Some("")).is_none());
}

#[test]
fn admin_user_is_some_on_exact_match() {
    let user = storage_admin_user("s3cr3t-admin-token", Some("s3cr3t-admin-token"))
        .expect("an exact token match should yield an admin user");
    // The static-token admin grant must carry the explicit storage-admin role
    // (NOT the generic `"admin"`) so it stays in lockstep with the storage RLS
    // evaluator after the M-storage-scope decollision.
    assert_eq!(
        user.roles,
        vec![fraiseql_storage::STORAGE_ADMIN_ROLE.to_string()],
        "admin user carries the explicit storage-admin role",
    );
    assert_ne!(user.roles, vec!["admin".to_string()], "must NOT carry the generic admin role");
    assert!(user.user_id.is_some(), "admin user has a stable identifier");
}

#[test]
fn admin_user_is_none_on_mismatch() {
    assert!(storage_admin_user("wrong-token", Some("right-token")).is_none());
    // A prefix of the configured token must not match.
    assert!(storage_admin_user("right", Some("right-token")).is_none());
    // A superstring of the configured token must not match.
    assert!(storage_admin_user("right-token-plus", Some("right-token")).is_none());
}
