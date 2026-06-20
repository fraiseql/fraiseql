//! Unit tests for the pure parts of [`LocalPasswordAuthenticator`]: input validation,
//! timing-equalization dummy-hash construction, and rehash detection. The DB-backed
//! signup / login / disabled / RLS behaviour is exercised by the live-PostgreSQL suite
//! in `tests/local_password.rs`.

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

use super::{compute_dummy_hash, needs_rehash, validate_credentials};
use crate::error::AuthError;

/// Fast Argon2 parameters for tests (8 KiB, 1 pass). Never use these in production.
fn weak_argon2() -> Argon2<'static> {
    let params = Params::new(8, 1, 1, None).unwrap();
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

fn hash_with(argon2: &Argon2<'_>, password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    argon2.hash_password(password.as_bytes(), &salt).unwrap().to_string()
}

#[test]
fn validate_accepts_a_well_formed_signup() {
    assert!(validate_credentials("alice@example.com", "correct horse battery").is_ok());
}

#[test]
fn validate_rejects_empty_or_malformed_email() {
    for email in ["", "   ", "no-at-sign"] {
        assert!(
            matches!(
                validate_credentials(email, "correct horse battery"),
                Err(AuthError::InvalidRegistration { .. })
            ),
            "email {email:?} should be rejected"
        );
    }
}

#[test]
fn validate_rejects_short_password() {
    assert!(matches!(
        validate_credentials("alice@example.com", "short"),
        Err(AuthError::InvalidRegistration { .. })
    ));
}

#[test]
fn validate_rejects_oversize_password() {
    let huge = "x".repeat(5000);
    assert!(matches!(
        validate_credentials("alice@example.com", &huge),
        Err(AuthError::InvalidRegistration { .. })
    ));
}

#[test]
fn dummy_hash_is_a_valid_argon2id_hash_that_rejects_passwords() {
    let argon2 = weak_argon2();
    let dummy = compute_dummy_hash(&argon2);
    assert!(dummy.starts_with("$argon2id$"), "dummy hash is an Argon2id PHC string: {dummy}");
    let parsed = PasswordHash::new(&dummy).expect("dummy hash parses");
    // Verifying an arbitrary password against the dummy returns a clean failure, not an
    // error — this is the path an unknown-user login takes.
    assert!(argon2.verify_password(b"anything", &parsed).is_err());
}

#[test]
fn needs_rehash_is_false_when_params_match() {
    let argon2 = weak_argon2();
    let stored = hash_with(&argon2, "correct horse battery");
    let parsed = PasswordHash::new(&stored).unwrap();
    assert!(!needs_rehash(&parsed, argon2.params()));
}

#[test]
fn needs_rehash_is_true_when_current_policy_is_stronger() {
    let weak = weak_argon2();
    let stored = hash_with(&weak, "correct horse battery");
    let parsed = PasswordHash::new(&stored).unwrap();
    let strong = Params::new(64, 3, 1, None).unwrap();
    assert!(needs_rehash(&parsed, &strong));
}
