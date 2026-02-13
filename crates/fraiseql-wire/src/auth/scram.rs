//! SCRAM-SHA-256 authentication implementation
//!
//! Implements the SCRAM-SHA-256 (Salted Challenge Response Authentication Mechanism)
//! as defined in RFC 5802 for PostgreSQL authentication (Postgres 10+).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::fmt;

type HmacSha256 = Hmac<Sha256>;

/// SCRAM authentication error types
#[derive(Debug, Clone)]
pub enum ScramError {
    /// Invalid proof from server
    InvalidServerProof(String),
    /// Invalid server message format
    InvalidServerMessage(String),
    /// UTF-8 encoding/decoding error
    Utf8Error(String),
    /// Base64 decoding error
    Base64Error(String),
}

impl fmt::Display for ScramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScramError::InvalidServerProof(msg) => write!(f, "invalid server proof: {}", msg),
            ScramError::InvalidServerMessage(msg) => write!(f, "invalid server message: {}", msg),
            ScramError::Utf8Error(msg) => write!(f, "UTF-8 error: {}", msg),
            ScramError::Base64Error(msg) => write!(f, "Base64 error: {}", msg),
        }
    }
}

impl std::error::Error for ScramError {}

/// Internal state needed for SCRAM authentication
#[derive(Clone, Debug)]
pub struct ScramState {
    /// Combined authentication message (for verification)
    auth_message: Vec<u8>,
    /// Server key (for verification calculation)
    server_key: Vec<u8>,
}

/// SCRAM-SHA-256 client implementation
pub struct ScramClient {
    username: String,
    password: String,
    nonce: String,
}

impl ScramClient {
    /// Create a new SCRAM client
    pub fn new(username: String, password: String) -> Self {
        // Generate random client nonce (24 bytes, base64 encoded = 32 chars)
        let mut rng = rand::thread_rng();
        let nonce_bytes: Vec<u8> = (0..24).map(|_| rng.gen()).collect();
        let nonce = BASE64.encode(&nonce_bytes);

        Self {
            username,
            password,
            nonce,
        }
    }

    /// Generate client first message (no proof)
    pub fn client_first(&self) -> String {
        // RFC 5802 format: gs2-header client-first-message-bare
        // gs2-header = "n,," (n = no channel binding, empty authorization identity)
        // client-first-message-bare = "n=<username>,r=<nonce>"
        // Note: "n=" is the username, "a=" would be authorization identity (not supported by PostgreSQL)
        format!("n,,n={},r={}", self.username, self.nonce)
    }

    /// Process server first message and generate client final message
    ///
    /// Returns (client_final_message, internal_state)
    pub fn client_final(&mut self, server_first: &str) -> Result<(String, ScramState), ScramError> {
        // Parse server first message: r=<client_nonce><server_nonce>,s=<salt>,i=<iterations>
        let (server_nonce, salt, iterations) = parse_server_first(server_first)?;

        // Verify server nonce starts with our client nonce
        if !server_nonce.starts_with(&self.nonce) {
            return Err(ScramError::InvalidServerMessage(
                "server nonce doesn't contain client nonce".to_string(),
            ));
        }

        // Decode salt and iterations
        let salt_bytes = BASE64
            .decode(&salt)
            .map_err(|_| ScramError::Base64Error("invalid salt encoding".to_string()))?;
        let iterations = iterations
            .parse::<u32>()
            .map_err(|_| ScramError::InvalidServerMessage("invalid iteration count".to_string()))?;

        // Build channel binding (no channel binding for SCRAM-SHA-256)
        let channel_binding = BASE64.encode(b"n,,");

        // Build client final without proof
        let client_final_without_proof = format!("c={},r={}", channel_binding, server_nonce);

        // Build auth message for signature calculation
        // client-first-message-bare is "n=<username>,r=<nonce>" (without gs2-header)
        let client_first_bare = format!("n={},r={}", self.username, self.nonce);
        let auth_message = format!(
            "{},{},{}",
            client_first_bare, server_first, client_final_without_proof
        );

        // Calculate proof
        let proof = calculate_client_proof(
            &self.password,
            &salt_bytes,
            iterations,
            auth_message.as_bytes(),
        )?;

        // Calculate server signature for later verification
        let server_key = calculate_server_key(&self.password, &salt_bytes, iterations)?;

        // Build client final message
        let client_final = format!("{},p={}", client_final_without_proof, BASE64.encode(&proof));

        let state = ScramState {
            auth_message: auth_message.into_bytes(),
            server_key,
        };

        Ok((client_final, state))
    }

    /// Verify server final message and confirm authentication
    pub fn verify_server_final(
        &self,
        server_final: &str,
        state: &ScramState,
    ) -> Result<(), ScramError> {
        // Parse server final: v=<server_signature>
        let server_sig_encoded = server_final
            .strip_prefix("v=")
            .ok_or_else(|| ScramError::InvalidServerMessage("missing 'v=' prefix".to_string()))?;

        let server_signature = BASE64.decode(server_sig_encoded).map_err(|_| {
            ScramError::Base64Error("invalid server signature encoding".to_string())
        })?;

        // Calculate expected server signature
        let expected_signature = calculate_server_signature(&state.server_key, &state.auth_message);

        // Constant-time comparison
        if constant_time_compare(&server_signature, &expected_signature) {
            Ok(())
        } else {
            Err(ScramError::InvalidServerProof(
                "server signature verification failed".to_string(),
            ))
        }
    }
}

/// Parse server first message format: r=<nonce>,s=<salt>,i=<iterations>
fn parse_server_first(msg: &str) -> Result<(String, String, String), ScramError> {
    let mut nonce = String::new();
    let mut salt = String::new();
    let mut iterations = String::new();

    for part in msg.split(',') {
        if let Some(value) = part.strip_prefix("r=") {
            nonce = value.to_string();
        } else if let Some(value) = part.strip_prefix("s=") {
            salt = value.to_string();
        } else if let Some(value) = part.strip_prefix("i=") {
            iterations = value.to_string();
        }
    }

    if nonce.is_empty() || salt.is_empty() || iterations.is_empty() {
        return Err(ScramError::InvalidServerMessage(
            "missing required fields in server first message".to_string(),
        ));
    }

    Ok((nonce, salt, iterations))
}

/// Calculate SCRAM client proof
fn calculate_client_proof(
    password: &str,
    salt: &[u8],
    iterations: u32,
    auth_message: &[u8],
) -> Result<Vec<u8>, ScramError> {
    // SaltedPassword := PBKDF2(password, salt, iterations, HMAC-SHA256)
    let password_bytes = password.as_bytes();
    let mut salted_password = vec![0u8; 32]; // SHA256 produces 32 bytes
    let _ = pbkdf2::<HmacSha256>(password_bytes, salt, iterations, &mut salted_password);

    // ClientKey := HMAC(SaltedPassword, "Client Key")
    let mut client_key_hmac = HmacSha256::new_from_slice(&salted_password)
        .map_err(|_| ScramError::Utf8Error("HMAC key error".to_string()))?;
    client_key_hmac.update(b"Client Key");
    let client_key = client_key_hmac.finalize().into_bytes();

    // StoredKey := SHA256(ClientKey)
    let stored_key = Sha256::digest(client_key.to_vec().as_slice());

    // ClientSignature := HMAC(StoredKey, AuthMessage)
    let mut client_sig_hmac = HmacSha256::new_from_slice(&stored_key)
        .map_err(|_| ScramError::Utf8Error("HMAC key error".to_string()))?;
    client_sig_hmac.update(auth_message);
    let client_signature = client_sig_hmac.finalize().into_bytes();

    // ClientProof := ClientKey XOR ClientSignature
    let mut proof = client_key.to_vec();
    for (proof_byte, sig_byte) in proof.iter_mut().zip(client_signature.iter()) {
        *proof_byte ^= sig_byte;
    }

    Ok(proof.to_vec())
}

/// Calculate server key for server signature verification
fn calculate_server_key(
    password: &str,
    salt: &[u8],
    iterations: u32,
) -> Result<Vec<u8>, ScramError> {
    // SaltedPassword := PBKDF2(password, salt, iterations, HMAC-SHA256)
    let password_bytes = password.as_bytes();
    let mut salted_password = vec![0u8; 32];
    let _ = pbkdf2::<HmacSha256>(password_bytes, salt, iterations, &mut salted_password);

    // ServerKey := HMAC(SaltedPassword, "Server Key")
    let mut server_key_hmac = HmacSha256::new_from_slice(&salted_password)
        .map_err(|_| ScramError::Utf8Error("HMAC key error".to_string()))?;
    server_key_hmac.update(b"Server Key");

    Ok(server_key_hmac.finalize().into_bytes().to_vec())
}

/// Calculate server signature for verification
fn calculate_server_signature(server_key: &[u8], auth_message: &[u8]) -> Vec<u8> {
    let mut hmac = HmacSha256::new_from_slice(server_key).expect("HMAC key should be valid");
    hmac.update(auth_message);
    hmac.finalize().into_bytes().to_vec()
}

/// Constant-time comparison to prevent timing attacks
fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scram_client_creation() {
        let client = ScramClient::new("user".to_string(), "password".to_string());
        assert_eq!(client.username, "user");
        assert_eq!(client.password, "password");
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
        assert!(parse_server_first(server_first).is_err());
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
        assert!(result.is_ok());

        let (client_final, state) = result.unwrap();
        assert!(client_final.starts_with("c="));
        assert!(!state.auth_message.is_empty());
    }
}
