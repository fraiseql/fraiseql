//! Well-known endpoints (RFC 5785).
//!
//! Currently provides `/.well-known/security.txt` (RFC 9116).

use axum::{
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};

/// Handler for `/.well-known/security.txt`.
///
/// Returns an RFC 9116 security.txt with the configured contact email.
pub async fn security_txt_handler(
    State(contact): State<String>,
) -> impl IntoResponse {
    let body = format!(
        "Contact: mailto:{contact}\nPreferred-Languages: en\n"
    );
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
}
