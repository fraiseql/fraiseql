//! Unit tests for the per-user outbound-send policy. These run in the fast lib
//! leg — the policy is pure and always compiled.

use serde_json::json;

use super::{SenderIdentity, resolve_sender_identity};

#[test]
fn resolves_the_connected_users_verified_address() {
    let auth = json!({
        "user_id": "u123",
        "email": "rep@outreach.example",
        "display_name": "Sales Rep",
    });
    assert_eq!(
        resolve_sender_identity(&auth),
        Ok(SenderIdentity {
            address:      "rep@outreach.example".to_string(),
            display_name: Some("Sales Rep".to_string()),
        })
    );
}

#[test]
fn display_name_is_optional() {
    let auth = json!({ "email": "rep@outreach.example" });
    let identity = resolve_sender_identity(&auth).expect("address present");
    assert_eq!(identity.address, "rep@outreach.example");
    assert_eq!(identity.display_name, None);
}

#[test]
fn a_missing_address_is_a_refusal_never_a_shared_mailbox() {
    // No `email` at all — a function must not fall back to any default sender.
    let auth = json!({ "user_id": "u123", "roles": ["rep"] });
    let error = resolve_sender_identity(&auth).expect_err("must refuse");
    assert!(error.message.contains("no verified sending address"));
    assert!(error.message.contains("never a"));
}

#[test]
fn a_blank_address_is_a_refusal() {
    for blank in ["", "   "] {
        let auth = json!({ "email": blank });
        assert!(resolve_sender_identity(&auth).is_err(), "blank email must refuse: {blank:?}");
    }
}

#[test]
fn a_malformed_address_is_a_refusal() {
    // No `@`, or embedded whitespace: not a plausible single sending address.
    for bad in ["not-an-email", "two addrs@a.example b@c.example"] {
        let auth = json!({ "email": bad });
        assert!(resolve_sender_identity(&auth).is_err(), "malformed email must refuse: {bad:?}");
    }
}

#[test]
fn a_non_string_address_is_a_refusal() {
    let auth = json!({ "email": 42 });
    assert!(resolve_sender_identity(&auth).is_err());
}

#[test]
fn surrounding_whitespace_is_trimmed() {
    let auth = json!({ "email": "  rep@outreach.example  ", "display_name": "  Rep  " });
    let identity = resolve_sender_identity(&auth).expect("address present");
    assert_eq!(identity.address, "rep@outreach.example");
    assert_eq!(identity.display_name, Some("Rep".to_string()));
}

#[tokio::test]
async fn login_email_sender_delegates_to_the_pure_policy() {
    use super::{LoginEmailSender, SenderIdentityResolver};

    // The degenerate resolver reframes the pure login-email policy (DESIGN §4.1):
    // a valid verified address resolves, a missing one refuses — identically.
    let auth = json!({ "email": "rep@outreach.example", "display_name": "Rep" });
    let identity = LoginEmailSender.resolve_sender(&auth).await.expect("address present");
    assert_eq!(identity.address, "rep@outreach.example");
    assert_eq!(identity.display_name.as_deref(), Some("Rep"));

    let missing = json!({ "sub": "u1" });
    assert!(LoginEmailSender.resolve_sender(&missing).await.is_err());
}

#[test]
fn a_guest_supplied_from_is_dropped_on_deserialization() {
    use super::SendEmailRequest;

    // The op's `from` is host-owned. `SendEmailRequest` has no `from` field, so a
    // guest that tries to set one has it silently dropped — the enforcement is at
    // the type level, before the op even runs.
    let request: SendEmailRequest = serde_json::from_str(
        r#"{"from":"attacker@evil.example","to":"bob@example.com","subject":"hi","text":"body"}"#,
    )
    .expect("valid request JSON");
    assert_eq!(request.to, "bob@example.com");
    assert_eq!(request.subject, "hi");
    assert_eq!(request.text.as_deref(), Some("body"));
    // Round-trip carries no `from` key.
    let serialized = serde_json::to_string(&request).expect("serializes");
    assert!(!serialized.contains("from"), "serialized request must not carry a `from`");
}

#[test]
fn send_policy_error_permanence() {
    use super::SendPolicyError;

    // `new` is a permanent refusal (default); `transient` is retryable.
    assert!(!SendPolicyError::new("no identity").retryable);
    assert!(SendPolicyError::transient("store down").retryable);
}

#[test]
fn send_email_response_omits_absent_message_id() {
    use super::SendEmailResponse;

    let response = SendEmailResponse {
        message_id: None,
        accepted:   true,
    };
    let serialized = serde_json::to_string(&response).expect("serializes");
    assert_eq!(serialized, r#"{"accepted":true}"#);
}
