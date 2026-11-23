//! A delivery token is not accepted before its `nbf`, within the scheme's clock-skew
//! leeway, and a refusal says the clock is the problem rather than the key.

use serde_json::json;

use super::{no_headers, now, receive, scheme_for, sign, with_audience};
use crate::signature::SignatureError;

/// A Hanko delivery whose token carries `nbf`.
async fn deliver_with_nbf(nbf: i64) -> Result<(), SignatureError> {
    let issued = now();
    let token = sign(&json!({
        "sub": "hanko webhooks", "aud": ["my-app"], "iat": issued, "nbf": nbf,
        "exp": issued + 3600, "evt": "user.create", "data": {},
    }));
    let scheme = scheme_for("hanko", &with_audience("my-app"));
    let body = json!({ "token": token }).to_string();
    receive(&*scheme, &no_headers(), body.as_bytes()).await.map(|_| ())
}

#[tokio::test]
async fn a_token_delivered_before_its_nbf_is_refused_as_outside_its_window() {
    let error = deliver_with_nbf(now() + 3600).await.expect_err("refused");
    assert!(
        matches!(error, SignatureError::TimestampExpired),
        "a token not yet valid is a clock refusal, not `Mismatch`: {error}"
    );
}

#[tokio::test]
async fn a_token_whose_nbf_is_within_the_leeway_is_accepted() {
    deliver_with_nbf(now() + 60).await.unwrap();
}

#[tokio::test]
async fn a_token_whose_nbf_has_passed_is_accepted() {
    deliver_with_nbf(now() - 60).await.unwrap();
}
