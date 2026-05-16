#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use super::*;

#[test]
fn test_scram_client_creation() {
    let client = ScramClient::new("user".to_string(), "password".to_string());
    assert_eq!(client.username, "user");
    assert_eq!(client.password.as_str(), "password");
    assert!(!client.nonce.is_empty());
}

#[test]
fn test_client_first_message_format() {
    let client = ScramClient::new("alice".to_string(), "secret".to_string());
    let first = client.client_first();

    // RFC 5802 format: "n,,n=<username>,r=<nonce>"
    assert!(first.starts_with("n,,n=alice,r="));
    assert!(first.len() > 20);
}

#[test]
fn test_parse_server_first_valid() {
    let server_first = "r=client_nonce_server_nonce,s=aW1hZ2luYXJ5c2FsdA==,i=4096";
    let (nonce, salt, iterations) = parse_server_first(server_first).unwrap();

    assert_eq!(nonce, "client_nonce_server_nonce");
    assert_eq!(salt, "aW1hZ2luYXJ5c2FsdA==");
    assert_eq!(iterations, "4096");
}

#[test]
fn test_parse_server_first_invalid() {
    let server_first = "r=nonce,s=salt"; // missing iterations
    let result = parse_server_first(server_first);
    assert!(
        matches!(result, Err(ScramError::InvalidServerMessage(_))),
        "expected InvalidServerMessage error, got: {result:?}"
    );
}

#[test]
fn test_constant_time_compare_equal() {
    let a = b"test_value";
    let b_arr = b"test_value";
    assert!(constant_time_compare(a, b_arr));
}

#[test]
fn test_constant_time_compare_different() {
    let a = b"test_value";
    let b_arr = b"test_wrong";
    assert!(!constant_time_compare(a, b_arr));
}

#[test]
fn test_constant_time_compare_different_length() {
    let a = b"test";
    let b_arr = b"test_longer";
    assert!(!constant_time_compare(a, b_arr));
}

#[test]
fn test_scram_client_final_flow() {
    let mut client = ScramClient::new("user".to_string(), "password".to_string());
    let _client_first = client.client_first();

    // Simulate server response
    let server_nonce = format!("{}server_nonce_part", client.nonce);
    let server_first = format!("r={},s={},i=4096", server_nonce, BASE64.encode(b"salty"));

    // Should succeed with valid format
    let result = client.client_final(&server_first);
    let (client_final, state) = result
        .unwrap_or_else(|e| panic!("expected Ok for client_final with valid server message: {e}"));
    assert!(client_final.starts_with("c="));
    assert!(!state.auth_message.is_empty());
}

#[test]
fn test_scram_iteration_count_too_high_is_rejected() {
    // H3: A server-supplied i= value above MAX_SCRAM_ITERATIONS must be rejected
    // to prevent PBKDF2-based denial-of-service.
    let mut client = ScramClient::new("user".to_string(), "password".to_string());
    let _client_first = client.client_first();

    let server_nonce = format!("{}server_nonce_part", client.nonce);
    let excessive_iterations = MAX_SCRAM_ITERATIONS + 1;
    let server_first = format!(
        "r={},s={},i={}",
        server_nonce,
        BASE64.encode(b"salty"),
        excessive_iterations
    );

    let result = client.client_final(&server_first);
    assert!(
        matches!(result, Err(ScramError::InvalidServerMessage(_))),
        "expected InvalidServerMessage for excessive iterations, got: {result:?}"
    );
}

#[test]
fn test_scram_iteration_count_at_limit_is_accepted() {
    // Exactly MAX_SCRAM_ITERATIONS must be accepted.
    let mut client = ScramClient::new("user".to_string(), "password".to_string());
    let _client_first = client.client_first();

    let server_nonce = format!("{}server_nonce_part", client.nonce);
    let server_first = format!(
        "r={},s={},i={}",
        server_nonce,
        BASE64.encode(b"salty"),
        MAX_SCRAM_ITERATIONS
    );

    // Should not fail on the iteration count check (may fail for other reasons if any)
    let result = client.client_final(&server_first);
    // We only care that it didn't fail with an iteration-count error
    if let Err(ScramError::InvalidServerMessage(msg)) = &result {
        assert!(
            !msg.contains("iteration count"),
            "unexpected iteration-count rejection at limit: {msg}"
        );
    }
}

#[test]
fn test_scram_username_escaping_in_auth_message() {
    // H4: The auth message must use the RFC 5802-escaped username, not the raw one.
    // A username containing ',' or '=' must be escaped in client_first_bare.
    let mut client = ScramClient::new("user,admin=evil".to_string(), "password".to_string());
    let client_first = client.client_first();
    // client_first should have escaped username
    assert!(
        client_first.contains("user=2Cadmin=3Devil"),
        "client_first should escape ',' and '=' in username, got: {client_first}"
    );

    // client_final should use the same escaped username in the auth message
    let server_nonce = format!("{}server_nonce_part", client.nonce);
    let server_first = format!("r={},s={},i=4096", server_nonce, BASE64.encode(b"salty"));

    let result = client.client_final(&server_first);
    let (_client_final, state) =
        result.unwrap_or_else(|e| panic!("expected Ok for escaped-username client_final: {e}"));

    // The auth message must contain the escaped username, not the raw one
    let auth_message = String::from_utf8(state.auth_message).unwrap();
    assert!(
        auth_message.contains("user=2Cadmin=3Devil"),
        "auth_message should contain escaped username, got: {auth_message}"
    );
    assert!(
        !auth_message.contains("user,admin=evil"),
        "auth_message must NOT contain raw (unescaped) username, got: {auth_message}"
    );
}

#[test]
fn scram_password_is_zeroized_on_drop() {
    use zeroize::Zeroize as _;

    let mut pw = zeroize::Zeroizing::new("super-secret-pw-12345".to_string());
    assert!(
        !pw.is_empty(),
        "password should be non-empty before zeroize"
    );
    pw.zeroize();
    assert!(pw.is_empty(), "password bytes must be wiped after zeroize");

    let client = ScramClient::new("user".to_string(), "secret".to_string());
    let _: &zeroize::Zeroizing<String> = &client.password;
    drop(client);
}
