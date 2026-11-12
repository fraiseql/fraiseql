//! An in-process ES256 OIDC issuer for tests.
//!
//! [`TestIssuer::start`] serves a JWKS and an OIDC discovery document on a
//! loopback port and signs tokens with a P-256 key generated at runtime, so no
//! private key is ever committed. A test points the real validator at
//! [`TestIssuer::jwks_uri`] (or at [`TestIssuer::issuer`], for discovery) and
//! hands it tokens from [`TestIssuer::mint`].
//!
//! The defaults are the strict shape: every token carries `iss`, `aud`, `sub`,
//! `iat`, `nbf`, `exp` and a unique `jti`, with an `at+jwt` header naming its
//! `kid`. A test that needs a claim missing or wrong says so with
//! [`Claims::without`] or [`Claims::set`]; nothing is permissive by omission.
//!
//! ```no_run
//! # async fn example() {
//! use fraiseql_test_support::issuer::TestIssuer;
//!
//! let issuer = TestIssuer::start().await;
//! let token = issuer.mint(&issuer.claims().set("sub", "alice"));
//! // Build the validator from `issuer.issuer()`, `issuer.audience()` and
//! // `issuer.jwks_uri()`, then validate `token` with it.
//! # let _ = token;
//! # }
//! ```

use std::{
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use p256::{
    SecretKey,
    elliptic_curve::sec1::ToEncodedPoint,
    pkcs8::{EncodePrivateKey, LineEnding},
};
use serde_json::{Map, Value, json};
use wiremock::{
    Mock, MockServer, Request, Respond, ResponseTemplate,
    matchers::{method, path},
};

/// The audience every token minted here carries by default.
pub const AUDIENCE: &str = "fraiseql-test-api";

/// The subject every token minted here carries by default.
pub const SUBJECT: &str = "test-user";

/// How long a default token is valid for, in seconds.
pub const LIFETIME_SECS: u64 = 300;

const JWKS_PATH: &str = "/jwks";
const DISCOVERY_PATH: &str = "/.well-known/openid-configuration";

/// One signing key: its private half and the JWK publishing its public half.
struct SigningKey {
    kid:       String,
    encoding:  EncodingKey,
    jwk:       Value,
    published: bool,
}

impl SigningKey {
    /// A fresh P-256 key named `kid`.
    fn generate(kid: String) -> Self {
        let secret = loop {
            // A random 32-byte string is a valid scalar unless it is zero or not
            // below the group order; both are vanishingly rare, but retry rather
            // than assume.
            if let Ok(secret) = SecretKey::from_slice(&rand::random::<[u8; 32]>()) {
                break secret;
            }
        };
        let pem = secret.to_pkcs8_pem(LineEnding::LF).expect("a P-256 key encodes as PKCS#8");
        let encoding =
            EncodingKey::from_ec_pem(pem.as_bytes()).expect("a PKCS#8 P-256 key is an ES256 key");
        let point = secret.public_key().to_encoded_point(false);
        let coordinate = |c: Option<&p256::FieldBytes>| {
            URL_SAFE_NO_PAD.encode(c.expect("an uncompressed point has both coordinates"))
        };
        let jwk = json!({
            "kty": "EC",
            "crv": "P-256",
            "alg": "ES256",
            "use": "sig",
            "kid": kid,
            "x": coordinate(point.x()),
            "y": coordinate(point.y()),
        });
        Self {
            kid,
            encoding,
            jwk,
            published: true,
        }
    }

    fn sign(&self, kid: &str, claims: &Claims) -> String {
        let mut header = Header::new(Algorithm::ES256);
        header.typ = Some("at+jwt".to_string());
        header.kid = Some(kid.to_string());
        jsonwebtoken::encode(&header, &claims.0, &self.encoding).expect("ES256 signing")
    }
}

/// The issuer's keys: every key it has made, and which one signs.
struct KeyRing {
    keys:   Vec<SigningKey>,
    active: usize,
}

impl KeyRing {
    fn active(&self) -> &SigningKey {
        &self.keys[self.active]
    }

    fn jwks(&self) -> Value {
        let keys: Vec<&Value> =
            self.keys.iter().filter(|key| key.published).map(|key| &key.jwk).collect();
        json!({ "keys": keys })
    }
}

type SharedRing = Arc<Mutex<KeyRing>>;

fn lock(ring: &SharedRing) -> std::sync::MutexGuard<'_, KeyRing> {
    // A test that panicked while holding the ring left it consistent: every
    // mutation is a single assignment or push.
    ring.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Serves the key set as it stands at each request, so a rotation or a
/// retirement is visible to the next fetch.
struct JwksResponder(SharedRing);

impl Respond for JwksResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(lock(&self.0).jwks())
    }
}

/// A running issuer. Dropping it stops the server.
pub struct TestIssuer {
    server:   MockServer,
    issuer:   String,
    ring:     SharedRing,
    next_kid: AtomicU64,
    next_jti: AtomicU64,
}

impl TestIssuer {
    /// Start the issuer with one signing key, serving its JWKS and discovery
    /// document on a loopback port.
    ///
    /// The issuer identifier is the server's own base URL, so discovery from
    /// [`issuer`](Self::issuer) finds [`jwks_uri`](Self::jwks_uri).
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let issuer = server.uri();
        let ring = Arc::new(Mutex::new(KeyRing {
            keys:   vec![SigningKey::generate(kid(0))],
            active: 0,
        }));
        Mock::given(method("GET"))
            .and(path(JWKS_PATH))
            .respond_with(JwksResponder(Arc::clone(&ring)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(DISCOVERY_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issuer": issuer,
                "jwks_uri": format!("{issuer}{JWKS_PATH}"),
                "id_token_signing_alg_values_supported": ["ES256"],
                "response_types_supported": ["id_token"],
                "subject_types_supported": ["public"],
            })))
            .mount(&server)
            .await;
        Self {
            server,
            issuer,
            ring,
            next_kid: AtomicU64::new(1),
            next_jti: AtomicU64::new(0),
        }
    }

    /// The issuer identifier: every default token's `iss`, and the base of its
    /// discovery document.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Every default token's `aud`: [`AUDIENCE`].
    #[must_use]
    pub const fn audience(&self) -> &'static str {
        AUDIENCE
    }

    /// The JWKS endpoint, a plain-`http` loopback URL.
    #[must_use]
    pub fn jwks_uri(&self) -> String {
        format!("{}{JWKS_PATH}", self.server.uri())
    }

    /// The key set this issuer publishes, exactly as its JWKS endpoint would
    /// serve it now: for a caller that holds the keys in process rather than
    /// fetching them.
    #[must_use]
    pub fn published_jwks(&self) -> Value {
        lock(&self.ring).jwks()
    }

    /// How many requests this issuer's endpoints have answered: the evidence that
    /// a caller holding the keys in process never asked for them.
    ///
    /// # Panics
    ///
    /// If the server was started without request recording, which
    /// [`start`](Self::start) never does.
    pub async fn requests_served(&self) -> usize {
        self.server
            .received_requests()
            .await
            .expect("the issuer records the requests it answers")
            .len()
    }

    /// The `kid` of the key that signs new tokens.
    #[must_use]
    pub fn active_kid(&self) -> String {
        lock(&self.ring).active().kid.clone()
    }

    /// The default claims, ready to be changed before [`mint`](Self::mint):
    /// `iss`, `aud`, `sub` ([`SUBJECT`]), `iat` and `nbf` (now), `exp` (now +
    /// [`LIFETIME_SECS`]) and a `jti` no other token from this issuer carries.
    ///
    /// # Panics
    ///
    /// If the system clock is before the Unix epoch.
    #[must_use]
    pub fn claims(&self) -> Claims {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the system clock is after the Unix epoch")
            .as_secs();
        let jti = format!(
            "jti-{}-{:016x}",
            self.next_jti.fetch_add(1, Ordering::Relaxed),
            rand::random::<u64>()
        );
        Claims(
            json!({
                "iss": self.issuer,
                "aud": AUDIENCE,
                "sub": SUBJECT,
                "iat": now,
                "nbf": now,
                "exp": now + LIFETIME_SECS,
                "jti": jti,
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        )
    }

    /// A token carrying exactly `claims`, signed by the active key.
    #[must_use]
    pub fn mint(&self, claims: &Claims) -> String {
        let ring = lock(&self.ring);
        let key = ring.active();
        key.sign(&key.kid, claims)
    }

    /// A token carrying the default [`claims`](Self::claims).
    #[must_use]
    pub fn token(&self) -> String {
        self.mint(&self.claims())
    }

    /// A token carrying `claims` and naming the active `kid`, signed by a key
    /// this issuer has never published: the negative control for a validator
    /// that must check the signature, not just find the key.
    #[must_use]
    pub fn foreign_token(&self, claims: &Claims) -> String {
        let kid = self.active_kid();
        SigningKey::generate(format!("{kid}-foreign")).sign(&kid, claims)
    }

    /// Add a key, publish it and make it the signing key. Earlier keys stay
    /// published until [`retire`](Self::retire)d. Returns the new `kid`.
    pub fn rotate(&self) -> String {
        let key = SigningKey::generate(kid(self.next_kid.fetch_add(1, Ordering::Relaxed)));
        let new_kid = key.kid.clone();
        let mut ring = lock(&self.ring);
        ring.keys.push(key);
        ring.active = ring.keys.len() - 1;
        new_kid
    }

    /// Stop publishing `kid`. Tokens it signed no longer verify against the key
    /// set.
    ///
    /// # Panics
    ///
    /// If `kid` is not one of this issuer's keys, or is the active one: an
    /// issuer that signs with a key it does not publish is not a shape a real
    /// issuer produces.
    pub fn retire(&self, kid: &str) {
        let mut ring = lock(&self.ring);
        let active = ring.active;
        let mut found = false;
        for (index, key) in ring.keys.iter_mut().enumerate() {
            if key.kid == kid {
                assert_ne!(index, active, "`{kid}` is the active key; rotate before retiring it");
                key.published = false;
                found = true;
            }
        }
        assert!(found, "`{kid}` is not one of this issuer's keys");
    }
}

fn kid(n: u64) -> String {
    format!("test-issuer-key-{n}")
}

/// The claims of a token about to be minted: a JSON object.
#[derive(Debug, Clone)]
pub struct Claims(Map<String, Value>);

impl Claims {
    /// Set `name` to `value`, replacing any default.
    #[must_use]
    pub fn set(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.0.insert(name.to_string(), value.into());
        self
    }

    /// Remove `name`, so the token does not carry it at all.
    #[must_use]
    pub fn without(mut self, name: &str) -> Self {
        self.0.remove(name);
        self
    }

    /// The claim `name`, if set.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.get(name)
    }
}
