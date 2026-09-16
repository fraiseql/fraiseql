//! The single bounded JWKS client for the FraiseQL workspace.
//!
//! Every crate that verifies a JWT against a publisher's key set fetches that set
//! here: the `[auth]` OIDC path (`fraiseql-core`), the OAuth client's ID-token
//! check (`fraiseql-auth`), and the inbound webhook schemes whose sender is an
//! identity provider (`fraiseql-webhooks`). Named rather than linked, because
//! those crates depend on this one — the link would have to go the wrong way.
//!
//! # Why this crate exists
//!
//! Two JWKS caches existed and had drifted (#1335). One accepted RSA keys only
//! and did not pin DNS; the other accepted RSA and EC and pinned against
//! rebinding. **Neither bounded a refetch**, and the consequence was not a
//! performance problem:
//!
//! > A miss always fetched. So any anonymous client could make the server issue
//! > one outbound HTTPS request per inbound request, simply by naming a `kid` the
//! > cache did not hold. IdPs rate-limit their JWKS endpoints; once the `IdP`
//! > throttles the server, a *genuine* key rotation can no longer be fetched and
//! > every user holding a new-`kid` token is refused. The amplification converts
//! > into an authentication outage, caused by traffic that never authenticated.
//!
//! Adding a third consumer would have repeated the drift that produced it, so the
//! rule lives here once: [`JwksSource`] is the only implementation, and
//! [`JwksKeys`] is the seam a caller substitutes in a test.
//!
//! # What is bounded, and how
//!
//! * **A miss is remembered.** A `kid` absent from a freshly-fetched set does not make the next
//!   request fetch again; [`REFETCH_COOLDOWN`] has to elapse first, however many distinct `kid`s
//!   arrive in the meantime.
//! * **Concurrent misses single-flight.** They serialise on one lock and the losers re-read the
//!   state the winner published, rather than each opening its own request. A cooldown alone does
//!   not give this: it is only consulted once a fetch has *finished*, and concurrent misses all
//!   start before that.
//! * **An expired set is never served.** A key the publisher has rotated out stops validating when
//!   the TTL lapses (#361) — the cooldown must not extend that window, so a stale set is refused
//!   rather than read.
//! * **An operator-forced refresh ignores the cooldown.** It is the response to a known key
//!   compromise, and it is not sender-triggered; see [`JwksSource::refresh`].
//!
//! The order of checks is the caller's responsibility in one respect: **refuse a
//! token's algorithm before asking for its key.** A token this server would
//! refuse on its header alone must not cost an outbound request, and only the
//! caller knows its own allow-list.

use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use jsonwebtoken::DecodingKey;
use serde::{Deserialize, Serialize};

mod fetch;
mod source;

// #1198: a published crate whose public signature names a third-party type must
// re-export it, or the caller has to add the dependency themselves and guess the
// major version this workspace built against — a mismatch there is a type error in
// code they did not touch. `Jwk::decoding_key` returns a `jsonwebtoken::DecodingKey`
// and `JwksError::InvalidUrl` carries a `url::ParseError`.
pub use jsonwebtoken;
pub use source::{JwksKeys, JwksSource};
pub use url;

/// How long a fetch of the publisher's key set is deferred after the previous
/// attempt, whatever the outcome and however many distinct `kid`s arrive.
///
/// Thirty seconds is short enough that an unannounced rotation is picked up
/// promptly — a publisher that follows the usual practice publishes the new key
/// *before* signing with it, so the window is normally not felt at all — and long
/// enough that a burst of unauthenticated requests costs one outbound request
/// rather than one each.
///
/// # Not a configuration key, deliberately
///
/// An operator knob here would be config surface (a documented environment
/// variable, a drift check, a validation path) for a value nobody has asked to
/// tune, and testing both sides of the window would then mean sleeping for it.
/// The clock is injected instead ([`JwksSource::with_clock`]), which is what the
/// tests advance.
pub const REFETCH_COOLDOWN: Duration = Duration::from_secs(30);

/// Maximum byte length accepted from a JWKS endpoint.
///
/// A legitimate key set — a handful of RSA or EC public keys, a few hundred bytes
/// each — is far under this. The cap is what stops a compromised or malicious
/// publisher from answering with a response large enough to exhaust memory.
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024; // 1 MiB

/// Request timeout for one fetch of the publisher's key set.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Why a key could not be produced.
///
/// Every variant is the *publisher's* or the operator's fault. Nothing a token's
/// sender controls reaches here — a `kid` that is simply not published is
/// `Ok(None)`, not an error, because the caller's answer to it ("refuse this
/// token") is different from its answer to "the key set is unreachable".
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum JwksError {
    /// `jwks_uri` is not a URL.
    #[error("jwks_uri {uri:?} is not a URL: {source}")]
    InvalidUrl {
        /// The value that failed to parse.
        uri:    String,
        /// The underlying parse error.
        source: url::ParseError,
    },

    /// `jwks_uri` names a scheme a key set may not be fetched over.
    ///
    /// OIDC Core 1.0 §3 requires HTTPS. Plain HTTP is accepted only for a
    /// loopback host, which is what a local fixture or a development `IdP` is.
    #[error(
        "jwks_uri scheme {scheme:?} cannot be used to fetch a key set: it must be https, or \
         http on a loopback host for local development"
    )]
    InvalidScheme {
        /// The rejected scheme.
        scheme: String,
    },

    /// The HTTP client could not be built, or the host could not be resolved and
    /// pinned.
    #[error("the JWKS client for {uri:?} could not be prepared: {reason}")]
    Client {
        /// The key set this was for.
        uri:    String,
        /// What went wrong, for the operator's log.
        reason: String,
    },

    /// The key set could not be fetched, or the publisher answered with an error
    /// status.
    #[error("the key set at {uri:?} could not be fetched: {reason}")]
    Unreachable {
        /// The key set this was for.
        uri:    String,
        /// What went wrong, for the operator's log.
        reason: String,
    },

    /// No key set is held and the cooldown has not yet allowed another attempt.
    ///
    /// Distinct from [`Unreachable`](Self::Unreachable) — which is *this* request's
    /// failed fetch — and, crucially, distinct from `Ok(None)`. Once a fetch has
    /// failed, the bound means later lookups do not retry for a while; answering
    /// them "that key is not published" would be a different claim from "I cannot
    /// say", and the two have opposite consequences for a caller that maps them
    /// to HTTP: an inbound webhook route would report the operator's own broken
    /// key fetch to the sender as a 401, and providers disable endpoints that
    /// return sustained authentication failures (#1045).
    ///
    /// A key set that *is* held and simply does not name the `kid` stays
    /// `Ok(None)`. That is a real answer: the publisher's set as of the last
    /// successful fetch does not contain it.
    #[error(
        "no key set is held for {uri:?} and the next attempt is not due yet; the last one \
         failed: {reason}"
    )]
    Unavailable {
        /// The key set this was for.
        uri:    String,
        /// Why the last attempt failed, for the operator's log.
        reason: String,
    },

    /// The publisher's response exceeded [`MAX_RESPONSE_BYTES`].
    #[error("the key set at {uri:?} is {bytes} bytes, over the {MAX_RESPONSE_BYTES}-byte cap")]
    TooLarge {
        /// The key set this was for.
        uri:   String,
        /// How large the response was.
        bytes: usize,
    },

    /// The publisher's response is not a JWKS document.
    #[error("the key set at {uri:?} is not a JWKS document: {reason}")]
    Malformed {
        /// The key set this was for.
        uri:    String,
        /// The deserialisation error, for the operator's log.
        reason: String,
    },

    /// The key is of a kind this workspace does not verify with, or its
    /// components do not form a key.
    ///
    /// Raised when a caller converts a [`Jwk`] it was given, not while fetching:
    /// a publisher may legitimately serve key types alongside the ones in use.
    #[error("the {kid:?} key is not usable for signature verification: {reason}")]
    UnusableKey {
        /// The key's id, as published.
        kid:    String,
        /// Why it cannot be used.
        reason: String,
    },
}

/// One JSON Web Key, as its publisher serialized it.
///
/// Deserialized permissively on purpose: a publisher serves every key it has,
/// including kinds this workspace does not verify with (an `oct` HMAC key, an
/// `OKP` `EdDSA` key) and fields it has no use for. Refusing the whole document
/// because one entry is of an unfamiliar kind would make an unrelated key
/// rotation break authentication, so an unusable entry is only refused when a
/// token actually names it — see [`Jwk::decoding_key`].
///
/// `Serialize` is part of the contract: a caller that hands key material across
/// a seam taking `&str` — the inbound webhook schemes do, because that is how
/// every other scheme in that crate takes its key — round-trips the key through
/// JSON rather than through a second definition of this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Jwk {
    /// Key type: `RSA`, `EC`, `OKP`, `oct`.
    pub kty: String,

    /// Key id, as named by a token's `kid` header.
    ///
    /// Optional in the JWK spec, and an entry without one can never be selected
    /// by `kid`, so [`JwksSource`] keeps it and simply never matches it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kid: Option<String>,

    /// The algorithm the publisher intends this key for, when stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alg: Option<String>,

    /// Intended use (`sig` for signature verification).
    #[serde(default, rename = "use", skip_serializing_if = "Option::is_none")]
    pub key_use: Option<String>,

    /// RSA modulus, base64url.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<String>,

    /// RSA public exponent, base64url.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub e: Option<String>,

    /// EC curve x coordinate, base64url.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,

    /// EC curve y coordinate, base64url.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<String>,

    /// The curve an `EC` or `OKP` key is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crv: Option<String>,

    /// X.509 certificate chain, when the publisher serves one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub x5c: Vec<String>,
}

impl Jwk {
    /// The verification key this JWK is, ready for `jsonwebtoken`.
    ///
    /// # Which key types are accepted, and why that is one list
    ///
    /// `RSA` and `EC`. Before this crate the two callers disagreed — the `[auth]`
    /// OIDC path accepted RSA alone, so an operator whose `IdP` had rotated onto an
    /// EC key saw every token refused, while the OAuth client accepted both
    /// (#1335). One list ends that.
    ///
    /// `OKP` (`EdDSA`) and `oct` (HMAC) are refused **by name**. `oct` is refused
    /// because a shared secret published in a key set is not a signature the
    /// server can attribute to the publisher; `OKP` because nothing in this
    /// workspace verifies Ed25519 JWTs yet, and a key type accepted with no
    /// caller is a surface with no test. Both refusals name the type, so an
    /// operator reading the log learns which it is rather than "invalid token".
    ///
    /// # Errors
    ///
    /// [`JwksError::UnusableKey`] naming the key and what is wrong with it.
    pub fn decoding_key(&self) -> Result<DecodingKey, JwksError> {
        let unusable = |reason: String| JwksError::UnusableKey {
            kid: self.kid.clone().unwrap_or_default(),
            reason,
        };
        match self.kty.as_str() {
            "RSA" => {
                let (Some(n), Some(e)) = (self.n.as_ref(), self.e.as_ref()) else {
                    return Err(unusable(
                        "an RSA key needs both a modulus `n` and an exponent `e`".to_string(),
                    ));
                };
                DecodingKey::from_rsa_components(n, e).map_err(|error| {
                    unusable(format!("its RSA components do not form a key: {error}"))
                })
            },
            "EC" => {
                let (Some(x), Some(y)) = (self.x.as_ref(), self.y.as_ref()) else {
                    return Err(unusable(
                        "an EC key needs both coordinates `x` and `y`".to_string(),
                    ));
                };
                DecodingKey::from_ec_components(x, y).map_err(|error| {
                    unusable(format!("its EC coordinates do not form a key: {error}"))
                })
            },
            "oct" => Err(unusable(
                "`oct` is a shared secret. A secret served in a public key set cannot attribute \
                 a signature to its publisher, so it is refused rather than used"
                    .to_string(),
            )),
            other => Err(unusable(format!(
                "`{other}` is not a key type this server verifies with; it verifies with RSA and \
                 EC keys"
            ))),
        }
    }
}

/// A JWKS document: what a publisher serves at its `jwks_uri`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwkSet {
    /// The published keys, in the publisher's order.
    #[serde(default)]
    pub keys: Vec<Jwk>,
}

impl JwkSet {
    /// The key named by `kid`, if this set publishes one.
    #[must_use]
    pub fn find(&self, kid: &str) -> Option<&Jwk> {
        self.keys.iter().find(|key| key.kid.as_deref() == Some(kid))
    }

    /// Every key id in this set.
    pub fn kids(&self) -> impl Iterator<Item = &str> {
        self.keys.iter().filter_map(|key| key.kid.as_deref())
    }

    /// The key ids in this set that `newer` no longer publishes.
    ///
    /// A non-empty answer is a **rotation**: the publisher has withdrawn keys this
    /// server held. Replacing the held set is what makes tokens signed by them
    /// stop validating (#361); this is what lets the event be reported, because a
    /// rotation an operator did not initiate is worth seeing in a log.
    #[must_use]
    pub fn kids_missing_from<'a>(&'a self, newer: &Self) -> Vec<&'a str> {
        let published: Vec<&str> = newer.kids().collect();
        self.kids().filter(|kid| !published.contains(kid)).collect()
    }
}

/// The monotonic clock a [`JwksSource`] measures its TTL and cooldown against.
///
/// A seam rather than a direct [`Instant::now`] so that both sides of the
/// cooldown are testable without sleeping through it — and so that a test which
/// means to exercise the *expiry* path cannot accidentally pass because the
/// machine was slow.
pub trait MonotonicClock: Send + Sync + std::fmt::Debug {
    /// The current instant on a clock that cannot go backwards.
    fn now(&self) -> Instant;
}

/// The production clock: [`Instant::now`].
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemMonotonicClock;

impl MonotonicClock for SystemMonotonicClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// A future returned from a `dyn`-dispatched method of this crate.
///
/// Written out rather than reached through `#[async_trait]`: there are two
/// implementations of [`JwksKeys`] in the workspace, the boxing is the whole cost
/// of `dyn` dispatch and is better visible than generated.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The clock a source uses when the caller does not supply one.
#[must_use]
pub fn system_clock() -> Arc<dyn MonotonicClock> {
    Arc::new(SystemMonotonicClock)
}

#[cfg(test)]
mod tests;
