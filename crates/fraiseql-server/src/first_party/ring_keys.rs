//! The live key ring as a key source the OIDC validator trusts the first-party issuer by.

use std::sync::Arc;

use fraiseql_auth::LiveKeyRing;
use fraiseql_jwks::{BoxFuture, Jwk, JwksError, JwksKeys};

/// Where the keys come from, for the operator's log.
const SOURCE: &str = "auth.signing_key";

/// The first-party issuer's keys, read from the ring this process holds: no request is
/// ever made for them.
///
/// The operator's JWKS invalidate and refresh reach the ring: invalidate makes the next
/// lookup re-read the table, refresh re-reads it now.
#[derive(Debug, Clone)]
pub struct RingKeys {
    ring: Arc<LiveKeyRing>,
}

impl RingKeys {
    /// The keys `ring` publishes.
    #[must_use]
    pub const fn new(ring: Arc<LiveKeyRing>) -> Self {
        Self { ring }
    }
}

fn unavailable(reason: impl std::fmt::Display) -> JwksError {
    JwksError::Unavailable {
        uri:    SOURCE.to_string(),
        reason: reason.to_string(),
    }
}

/// Unix seconds now.
fn now() -> Result<u64, JwksError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .map_err(|e| unavailable(format!("the system clock is before 1970: {e}")))
}

impl JwksKeys for RingKeys {
    fn key<'a>(&'a self, kid: &'a str) -> BoxFuture<'a, Result<Option<Jwk>, JwksError>> {
        Box::pin(async move {
            let Some(jwk) = self.ring.published_key(kid, now()?).await.map_err(unavailable)? else {
                return Ok(None);
            };
            serde_json::from_value(jwk).map(Some).map_err(unavailable)
        })
    }

    fn invalidate(&self) {
        self.ring.invalidate();
    }

    fn refresh(&self) -> BoxFuture<'_, Result<usize, JwksError>> {
        Box::pin(async move { self.ring.reload(now()?).await.map_err(unavailable) })
    }
}
