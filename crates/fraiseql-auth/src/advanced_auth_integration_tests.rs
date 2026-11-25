//! Cross-cutting integration tests for advanced authentication flows.
//!
//! Tests the interactions between modules: TOTP MFA and account linking.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use crate::{
    account_linking::{AccountStore, InMemoryAccountStore},
    totp_mfa::{InMemoryMfaStore, MfaStore},
};

// ---------------------------------------------------------------------------
// Test: TOTP MFA enroll -> confirm -> challenge -> verify flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_mfa_enroll_then_challenge_verify_integration() {
    let mfa_store = Arc::new(InMemoryMfaStore::new());

    // Enroll and confirm via store API
    let enrollment = mfa_store
        .begin_enrollment("user-42", "FraiseQL-Test", "user42@example.com", None)
        .await
        .unwrap();

    assert!(!enrollment.secret_base32.is_empty());
    assert!(enrollment.otpauth_uri.starts_with("otpauth://"));
    assert_eq!(enrollment.recovery_codes.len(), 8);

    // Generate a valid TOTP to confirm enrollment
    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        totp_rs::Secret::Encoded(enrollment.secret_base32.clone()).to_bytes().unwrap(),
        None,
        String::new(),
    )
    .unwrap();
    let code = totp.generate_current().unwrap();
    mfa_store.confirm_enrollment("user-42", &code).await.unwrap();
    assert!(mfa_store.is_enrolled("user-42").await);

    // Create challenge and verify with TOTP code
    let challenge_token = mfa_store.create_challenge("user-42").await.unwrap();
    let verify_code = totp.generate_current().unwrap();
    let user_id = mfa_store.verify_challenge(&challenge_token, &verify_code).await.unwrap();
    assert_eq!(user_id, "user-42");

    // Verify with recovery code via a new challenge
    let challenge_token2 = mfa_store.create_challenge("user-42").await.unwrap();
    let result = mfa_store
        .verify_challenge(&challenge_token2, &enrollment.recovery_codes[0])
        .await;
    assert!(result.is_ok(), "recovery code should work");
}

// ---------------------------------------------------------------------------
// Test: account linking via InMemoryAccountStore
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_account_linking_same_email_across_providers() {
    let store = InMemoryAccountStore::new();

    let r1 = store
        .link_or_create_user(None, Some("alice@example.com"), true, "github", "gh-123")
        .await
        .unwrap();
    assert!(r1.is_new);

    let r2 = store
        .link_or_create_user(None, Some("alice@example.com"), true, "google", "gg-456")
        .await
        .unwrap();
    assert!(!r2.is_new);
    assert!(r2.linked);
    assert_eq!(r1.user_id, r2.user_id, "same email must link to same user");

    let account = store.get_account(&r1.user_id).await.unwrap();
    assert_eq!(account.providers.len(), 2);
}
