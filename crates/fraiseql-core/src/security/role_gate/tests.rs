//! Unit pins for [`enforce_requires_role`](super::enforce_requires_role).
//!
//! Each case is a way the five hand-written copies of this rule could have
//! disagreed, and the sixth (the REST resolver) did.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::enforce_requires_role;
use crate::{error::FraiseQLError, security::SecurityContext};

fn ctx_with(roles: &[&str], scopes: &[&str]) -> SecurityContext {
    let mut ctx = SecurityContext::system_job("t", "r", vec![], vec![], None);
    ctx.roles = roles.iter().map(|s| (*s).to_string()).collect();
    ctx.scopes = scopes.iter().map(|s| (*s).to_string()).collect();
    ctx
}

#[test]
fn an_ungated_operation_passes_without_a_context() {
    assert!(enforce_requires_role("Query", "openThing", None, None).is_ok());
}

#[test]
fn the_role_admits() {
    let ctx = ctx_with(&["reader"], &[]);
    assert!(enforce_requires_role("Query", "secrets", Some("reader"), Some(&ctx)).is_ok());
}

/// The defect this module exists for: a scope of the same name is not the role.
#[test]
fn a_same_named_scope_does_not_admit() {
    let ctx = ctx_with(&[], &["reader"]);
    let err = enforce_requires_role("Query", "secrets", Some("reader"), Some(&ctx)).unwrap_err();
    assert!(
        matches!(&err, FraiseQLError::Validation { message, .. } if message.contains("not found")),
        "a scope must not satisfy a role gate, and the refusal must hide the query: {err:?}"
    );
}

#[test]
fn an_unauthenticated_caller_is_refused() {
    let err = enforce_requires_role("Query", "secrets", Some("reader"), None).unwrap_err();
    assert!(matches!(err, FraiseQLError::Validation { .. }));
}

/// The refusal must never be `Authorization`: `FORBIDDEN` confirms the operation
/// exists and that some role reaches it, which is what enumeration needs.
#[test]
fn the_refusal_hides_the_operation_rather_than_forbidding_it() {
    let ctx = ctx_with(&["writer"], &[]);
    let err =
        enforce_requires_role("Mutation", "createSecret", Some("admin"), Some(&ctx)).unwrap_err();
    match err {
        FraiseQLError::Validation { message, .. } => {
            assert_eq!(message, "Mutation 'createSecret' not found in schema");
        },
        other => panic!("a role refusal must read as absence, got: {other:?}"),
    }
}

#[test]
fn the_operation_kind_reaches_the_message() {
    let err = enforce_requires_role("Query", "secrets", Some("reader"), None).unwrap_err();
    let FraiseQLError::Validation { message, .. } = err else {
        panic!("wrong variant")
    };
    assert_eq!(message, "Query 'secrets' not found in schema");
}
