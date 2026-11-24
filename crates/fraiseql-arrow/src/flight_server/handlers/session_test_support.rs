//! A Flight session for the library's own tests: the service's secret, and a request
//! carrying a token signed with it, as the handshake issues one.

#![allow(clippy::unwrap_used)] // Reason: test support, panics acceptable

use tonic::Request;

use crate::flight_server::FraiseQLFlightService;

/// The session secret every test service holds.
pub const SECRET: &str = "flight-test-session-secret";

/// A service holding [`SECRET`], with the library's default views.
pub fn service() -> FraiseQLFlightService {
    FraiseQLFlightService::new().with_session_secret(SECRET)
}

/// A session token signed with [`SECRET`], valid for five minutes.
pub fn session_token() -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    let now = chrono::Utc::now().timestamp();
    encode(
        &Header::new(Algorithm::HS256),
        &serde_json::json!({
            "sub": "flight-test",
            "exp": now + 300,
            "iat": now,
            "scopes": ["user"],
            "session_type": "flight",
        }),
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

/// `message` with a session.
pub fn authorized<T>(message: T) -> Request<T> {
    let mut request = Request::new(message);
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {}", session_token()).parse().unwrap());
    request
}
