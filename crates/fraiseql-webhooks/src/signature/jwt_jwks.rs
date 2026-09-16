//! Tokens verified against a key set the sender's publisher serves (#1322).
//!
//! Three identity providers authenticate a delivery this way, and none of them
//! could be received before this:
//!
//! ```text
//!             token is                        signature covers   result
//! hanko       body field `token`              `evt` + `data`     Verified::Event
//! kinde       the whole body (application/jwt) the claims        Verified::Event
//! fusionauth  header X-FusionAuth-Signature-JWT the BODY, via a  Verified::Body
//!                                             `request_body_sha256` claim
//! ```
//!
//! # The two shapes, and why the last column is not a detail
//!
//! A scheme here either **binds the body** or it does not, and that single fact
//! decides what verification can report.
//!
//! Hanko's outer JSON is an envelope: its `event` field is not signed, so nothing
//! in the body is trusted and the event has to come out of the claims —
//! [`Verified::Event`]. Kinde's body *is* the token, so there is no body to be the
//! event at all. FusionAuth is the opposite: its `request_body_sha256` claim is a
//! digest of the raw bytes, so once the token verifies **the body is verified**,
//! and it is an ordinary body-signing scheme — [`Verified::Body`], with the
//! receiver's own id and type rules applying unchanged.
//!
//! Reading [`Verified::Body`]'s contract against the case is the check #1323 found
//! had been skipped; it says "the signature covers the request body, and the body
//! **is** the event", which is exactly FusionAuth and exactly not Hanko.
//!
//! # Token confusion is the requirement the issue does not state
//!
//! Hanko verifies its webhook tokens against the **same tenant JWKS** that signs
//! its end-user session tokens — #708's issuer-less mode exists to validate those.
//! Kinde's webhook JWKS is likewise its access-token JWKS. So a scheme that
//! accepts "any token that verifies against this key set" accepts **any logged-in
//! end user posting their own session token as a webhook**, and the signature is
//! genuine.
//!
//! Each preset therefore fixes the claims that tell a webhook token from a user
//! token and refuses a token without them. `audience` is defence in depth on top,
//! validated when configured — not instead of the claims, because a publisher's
//! webhook token may not carry an `aud` at all and a route that required one would
//! refuse every genuine delivery.
//!
//! # Freshness is the token's own `exp`
//!
//! `jsonwebtoken`'s default `required_spec_claims` is `{exp}`, which would refuse
//! every Kinde delivery outright — Kinde documents no `iat` or `exp` and retries
//! for up to 36 hours. So `exp` is checked when present and not required, and
//! `max_age_secs` is an *additional* window that applies only when an operator
//! configures one. For a publisher whose token carries no `exp`, the delivery
//! ledger is what bounds replay.

use std::{borrow::Cow, sync::Arc};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use fraiseql_jwks::{BoxFuture, Jwk, JwksKeys};
use jsonwebtoken::{Algorithm, Validation, decode, decode_header};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

use crate::{
    request::InboundRequest,
    scheme::{CredentialLocation, SchemeConfig, SchemeError},
    signature::{SignatureError, Verified},
    traits::{Clock, SignatureVerifier, SystemClock},
};

/// The default allow-list: what Hanko and Kinde sign with, and what the generic
/// scheme assumes unless the route says otherwise.
const DEFAULT_ALGORITHMS: &[&str] = &["RS256"];

/// FusionAuth's default allow-list.
///
/// Its signing key may be **RSA or EC** — an operator picks when they configure
/// the key at FusionAuth, and neither is more standard than the other. An
/// `RS256`-only default would therefore refuse every genuine delivery from a
/// perfectly ordinary FusionAuth deployment, with nothing but a 401 to say why.
///
/// `EdDSA` and HMAC keys are the two FusionAuth also allows and this crate does
/// not verify. Both are refused **by name**: an `EdDSA` token fails the allow-list
/// here, and an `oct` key fails `fraiseql_jwks::Jwk::decoding_key` with its own
/// diagnosis.
const FUSIONAUTH_ALGORITHMS: &[&str] = &["RS256", "RS384", "RS512", "ES256", "ES384"];

/// The largest clock skew `max_age_secs` is evaluated with, matching the cap the
/// `[auth]` path applies to `clock_skew_secs`.
const MAX_CLOCK_SKEW_SECS: u64 = 300;

/// What a verified token says the delivery's id is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum IdSource {
    /// A claim carries it.
    Claim(String),
    /// Nothing does, so the id is a digest of the verified token.
    ///
    /// Hanko's tokens carry no `jti` and no event id — the issue's "dedup on
    /// `jti`, or the provider's event id" has nothing to name. The token itself is
    /// the only signed, delivery-stable material there is.
    ///
    /// ⚠ If such a publisher **re-signs on retry** (a fresh `iat` gives a fresh
    /// digest), each attempt is a distinct delivery and the ledger cannot coalesce
    /// retries. That makes delivery at-least-once and `after:ingest` handlers must
    /// be idempotent. Confirming it needs a captured retry.
    TokenDigest,
}

/// A claim the token must carry for this route to accept it — the token-confusion
/// guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaimRequirement {
    /// The claim must be present, with any value.
    Present(&'static str),
    /// The claim must be present and equal this string.
    Equals(&'static str, &'static str),
}

impl ClaimRequirement {
    /// The claim this requirement is about.
    const fn claim(self) -> &'static str {
        match self {
            Self::Present(name) | Self::Equals(name, _) => name,
        }
    }

    /// Whether `claims` satisfies it.
    fn satisfied_by(self, claims: &Map<String, Value>) -> bool {
        match self {
            Self::Present(name) => claims.contains_key(name),
            Self::Equals(name, expected) => {
                claims.get(name).and_then(Value::as_str) == Some(expected)
            },
        }
    }
}

/// What the token authenticates: the claims, or the body it arrived in.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Authenticates {
    /// The event is in the claims and the body is an untrusted envelope.
    TheClaims {
        id:               IdSource,
        event_type_claim: String,
        payload_claim:    String,
    },
    /// A claim carries a digest of the raw body, so the body is the event and the
    /// receiver's own id and type rules apply to it.
    TheBody {
        /// The claim carrying `base64(SHA-256(raw body))`.
        hash_claim: String,
    },
}

/// Verifies a delivery authenticated by a JWT checked against a published key set.
pub struct JwtJwksVerifier {
    /// The scheme's name, so a preset reports itself rather than the family.
    name:       &'static str,
    /// Where keys are looked up. Supplied by the caller: this crate makes no
    /// network requests and owns no HTTP client.
    keys:       Arc<dyn JwksKeys>,
    /// Where the token is.
    token_from: CredentialLocation,
    /// The `alg` values this route accepts, checked before any key lookup.
    algorithms: Vec<Algorithm>,
    /// The `aud` a token must carry, when configured.
    audience:   Option<String>,
    /// What the token authenticates.
    subject:    Authenticates,
    /// The claims that distinguish a webhook token from a user token.
    required:   Vec<ClaimRequirement>,
    /// An additional freshness window on `iat`, beyond the token's own `exp`.
    max_age:    Option<u64>,
    clock:      Arc<dyn Clock>,
}

impl std::fmt::Debug for JwtJwksVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtJwksVerifier")
            .field("name", &self.name)
            .field("token_from", &self.token_from)
            .field("algorithms", &self.algorithms)
            .field("audience", &self.audience)
            .field("subject", &self.subject)
            .field("required", &self.required)
            .field("max_age", &self.max_age)
            .finish_non_exhaustive()
    }
}

/// Parse and vet one configured `alg` name.
///
/// `none` and the `HS*` family are refused whatever the route says, and refused
/// **by name**: an HMAC algorithm verified against a key set is the
/// algorithm-confusion attack, and `none` is not a signature. Saying which one it
/// was is the difference between an operator fixing their config and an operator
/// reading a generic refusal.
fn parse_algorithm(provider: &str, name: &str) -> Result<Algorithm, SchemeError> {
    let unusable = |reason: &str| SchemeError::UnusableAlgorithm {
        provider: provider.to_string(),
        value:    name.to_string(),
        reason:   reason.to_string(),
    };
    if name.eq_ignore_ascii_case("none") {
        return Err(unusable(
            "`alg: none` is the absence of a signature, so a route accepting it \
             authenticates nothing at all",
        ));
    }
    if name.len() >= 2 && name[..2].eq_ignore_ascii_case("HS") {
        return Err(unusable(
            "the `HS*` family verifies with a shared secret, and a shared secret taken from a \
             *public* key set is the algorithm-confusion attack: anyone who can read the key \
             set can forge a token. Use an asymmetric algorithm such as `RS256`",
        ));
    }
    name.parse::<Algorithm>().map_err(|_| {
        unusable("it is not a JWT algorithm name; expected one of RS256/384/512, PS256/384/512, ES256/384, EdDSA")
    })
}

/// The preset for one named provider: everything it fixes about its own tokens.
struct Preset {
    token_from: CredentialLocation,
    subject:    Authenticates,
    required:   &'static [ClaimRequirement],
    /// The allow-list this provider's tokens need when the route names none.
    algorithms: &'static [&'static str],
}

/// Hanko: the token is in a body field, and the outer `event` is unsigned.
///
/// `sub == "hanko webhooks"` is what tells a webhook token from an end-user
/// session token signed by the same tenant key.
fn hanko() -> Preset {
    Preset {
        token_from: CredentialLocation::BodyField("token".to_string()),
        subject:    Authenticates::TheClaims {
            id:               IdSource::TokenDigest,
            event_type_claim: "evt".to_string(),
            payload_claim:    "data".to_string(),
        },
        required:   &[
            ClaimRequirement::Equals("sub", "hanko webhooks"),
            ClaimRequirement::Present("evt"),
            ClaimRequirement::Present("data"),
        ],
        algorithms: DEFAULT_ALGORITHMS,
    }
}

/// Kinde: the body is the token, `Content-Type: application/jwt`.
///
/// Its type claim is `type` and its id claim is `event_id` — **not** the `evt` the
/// issue proposed as a shared default, which is Hanko's. The `webhook-id` header
/// is stable across retries but unsigned, so it cannot be the ledger's key.
fn kinde() -> Preset {
    Preset {
        token_from: CredentialLocation::Body,
        subject:    Authenticates::TheClaims {
            id:               IdSource::Claim("event_id".to_string()),
            event_type_claim: "type".to_string(),
            payload_claim:    "data".to_string(),
        },
        required:   &[
            ClaimRequirement::Present("event_id"),
            ClaimRequirement::Present("type"),
            ClaimRequirement::Present("source"),
        ],
        // Kinde documents RS256.
        algorithms: DEFAULT_ALGORITHMS,
    }
}

/// FusionAuth: the token is in a header, and its `request_body_sha256` claim is a
/// digest of the raw body — which is what binds the two.
///
/// So the body is verified material, and this is a body-signing scheme.
fn fusionauth() -> Preset {
    Preset {
        token_from: CredentialLocation::Header("X-FusionAuth-Signature-JWT".to_string()),
        subject:    Authenticates::TheBody {
            hash_claim: "request_body_sha256".to_string(),
        },
        // No claim requirement, deliberately. The digest claim IS the guard here:
        // `check_body_hash` refuses a token that does not carry it, so requiring
        // its presence as well would be a second copy of one rule — and a second
        // copy is why neither of the two could then be shown to be load-bearing.
        // A FusionAuth end-user token has no `request_body_sha256`, so it is
        // refused by the binding check, which is the mechanism that also makes the
        // body verified material.
        required:   &[],
        algorithms: FUSIONAUTH_ALGORITHMS,
    }
}

/// The scheme keys `jwt-jwks` reads. Its presets read only the first two.
pub(crate) const JWT_JWKS_KEYS: &[&str] = &[
    "jwks_uri",
    "audience",
    "credential",
    "algorithms",
    "event_type_claim",
    "payload_claim",
    "id_claim",
    "body_hash_claim",
    "max_age_secs",
];

/// The scheme keys a `jwt-jwks` **preset** reads.
///
/// The split is not "a preset reads nothing". It is **format versus policy**: what
/// the provider decided about its own tokens is fixed here, and what *this
/// deployment* decided is the operator's.
///
/// Fixed, because the provider owns it: where the token is, which claims carry the
/// event, and which claims distinguish a webhook token from a user token.
///
/// Read, because the deployment owns it:
///
/// * `jwks_uri` — a *tenant's* key set is per deployment, so not even a preset can fix it;
/// * `audience` — it names this service;
/// * `algorithms` — which of the algorithms the provider supports this route will accept.
///   FusionAuth can be configured with an RSA or an EC key, so narrowing the list is a real choice
///   an operator makes about their own deployment;
/// * `max_age_secs` — a freshness policy on top of the token's own `exp`, which is this receiver's
///   risk appetite and nothing the provider states.
pub(crate) const JWT_JWKS_PRESET_KEYS: &[&str] =
    &["jwks_uri", "audience", "algorithms", "max_age_secs"];

impl JwtJwksVerifier {
    /// Build the generic scheme from a route's configuration.
    ///
    /// # Errors
    ///
    /// [`SchemeError::UnusableAlgorithm`] for an `alg` this scheme will not verify
    /// with, and [`SchemeError::IrrelevantKey`] when `body_hash_claim` is combined
    /// with the claim keys it makes unreadable.
    pub fn from_config(
        provider: &'static str,
        config: &SchemeConfig,
        keys: Arc<dyn JwksKeys>,
    ) -> Result<Self, SchemeError> {
        // The allow-list first, and deliberately before the credential location: a
        // route naming `HS256` has asked for the algorithm-confusion attack, and it
        // should be told *that* even if it also forgot to say where the token is.
        // Refusals arrive one at a time, so which one arrives first is a choice.
        let algorithms = Self::algorithms_from_config(provider, config, DEFAULT_ALGORITHMS)?;
        let subject = Self::subject_from_config(provider, config)?;
        Ok(Self {
            name: provider,
            keys,
            // `credential` is phase 31's existing key and it is the one that says
            // where a credential is — a second `token_from` spelling of the same
            // question is how a grammar comes to have two.
            //
            // Required, with no default. Inheriting the HMAC families'
            // `header:X-Signature` would be sharing their default by a coincidence
            // of code rather than by any decision about tokens: no provider puts a
            // JWT there, so it would only ever produce a 401 per delivery naming a
            // header the sender never set.
            token_from: config.credential.clone().ok_or_else(|| {
                SchemeError::MissingCredentialLocation {
                    provider: provider.to_string(),
                }
            })?,
            algorithms,
            audience: config.audience.clone(),
            subject,
            required: Vec::new(),
            max_age: config.max_age_secs,
            clock: Arc::new(SystemClock),
        })
    }

    /// Build a named preset: it fixes everything but its key set and audience.
    ///
    /// # Errors
    ///
    /// [`SchemeError::UnusableAlgorithm`] if the route configured one — a preset
    /// refuses the key, so this is unreachable through `build_scheme`, which vets
    /// the keys first.
    fn from_preset(
        provider: &'static str,
        config: &SchemeConfig,
        keys: Arc<dyn JwksKeys>,
        preset: Preset,
    ) -> Result<Self, SchemeError> {
        Ok(Self {
            name: provider,
            keys,
            algorithms: Self::algorithms_from_config(provider, config, preset.algorithms)?,
            token_from: preset.token_from,
            audience: config.audience.clone(),
            subject: preset.subject,
            required: preset.required.to_vec(),
            max_age: config.max_age_secs,
            clock: Arc::new(SystemClock),
        })
    }

    /// Replace the clock, so a captured delivery can be verified at the instant it
    /// was signed.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// The allow-list the route configured, or `fallback` when it named none.
    fn algorithms_from_config(
        provider: &str,
        config: &SchemeConfig,
        fallback: &[&str],
    ) -> Result<Vec<Algorithm>, SchemeError> {
        let names: Vec<String> = config
            .algorithms
            .clone()
            .unwrap_or_else(|| fallback.iter().map(|name| (*name).to_string()).collect());
        if names.is_empty() {
            return Err(SchemeError::UnusableAlgorithm {
                provider: provider.to_string(),
                value:    "[]".to_string(),
                reason:   "an empty allow-list accepts no token at all".to_string(),
            });
        }
        names.iter().map(|name| parse_algorithm(provider, name)).collect()
    }

    /// Whether this route's token binds the body or carries the event.
    fn subject_from_config(
        provider: &str,
        config: &SchemeConfig,
    ) -> Result<Authenticates, SchemeError> {
        if let Some(hash_claim) = config.body_hash_claim.clone() {
            // The two are contradictory rather than merely redundant: a claim that
            // digests the body makes the body the event, so the claim keys naming
            // where the event is inside the token are configuration nothing
            // consults — the same silent drop `reads_only` exists to prevent, one
            // combination down.
            let conflicting = [
                ("event_type_claim", config.event_type_claim.is_some()),
                ("payload_claim", config.payload_claim.is_some()),
                ("id_claim", config.id_claim.is_some()),
            ]
            .into_iter()
            .find_map(|(key, present)| present.then_some(key));
            if let Some(key) = conflicting {
                return Err(SchemeError::IrrelevantKey {
                    provider: provider.to_string(),
                    key,
                    reads: "`body_hash_claim`, which digests the raw body — so the body IS \
                               the event and the receiver's own id and type rules apply to \
                               it. Remove one of the two."
                        .to_string(),
                });
            }
            return Ok(Authenticates::TheBody { hash_claim });
        }
        Ok(Authenticates::TheClaims {
            id:               config
                .id_claim
                .clone()
                .map_or(IdSource::TokenDigest, IdSource::Claim),
            event_type_claim: config
                .event_type_claim
                .clone()
                .unwrap_or_else(|| "event_type".to_string()),
            payload_claim:    config.payload_claim.clone().unwrap_or_else(|| "data".to_string()),
        })
    }

    /// `alg` if this route accepts it, refused **without touching the network**.
    ///
    /// This is #1335's rule at its most exposed caller: a webhook route is
    /// unauthenticated by construction, so a token whose algorithm the route
    /// refuses on its header alone must not cost an outbound request to the
    /// publisher.
    fn permitted_algorithm(&self, alg: Algorithm) -> Result<Algorithm, SignatureError> {
        if self.algorithms.contains(&alg) {
            Ok(alg)
        } else {
            // The sender's fault: a 401, never the `KeyMaterial` 5xx (#1045).
            Err(SignatureError::InvalidFormat)
        }
    }

    /// Refuse a token that lacks the claims distinguishing a webhook from a user
    /// token.
    fn check_required_claims(&self, claims: &Map<String, Value>) -> Result<(), SignatureError> {
        for requirement in &self.required {
            if !requirement.satisfied_by(claims) {
                tracing::debug!(
                    scheme = self.name,
                    claim = requirement.claim(),
                    "refused a token that verified but is not a webhook token from this \
                     provider: the claim distinguishing one is missing or wrong"
                );
                // A genuine signature over the wrong kind of token. The sender is at
                // fault — this is precisely the end user posting their own session
                // token — so a 401.
                return Err(SignatureError::Mismatch);
            }
        }
        Ok(())
    }

    /// Refuse a token older than an operator-configured window.
    ///
    /// Separate from the token's own `exp`, which `jsonwebtoken` checks. Applies
    /// only when `max_age_secs` is configured, because a publisher that retries
    /// for 36 hours with a reused token would otherwise have every retry past the
    /// window refused.
    fn check_max_age(&self, claims: &Map<String, Value>) -> Result<(), SignatureError> {
        let Some(max_age) = self.max_age else {
            return Ok(());
        };
        let issued = claims.get("iat").and_then(Value::as_i64).ok_or_else(|| {
            tracing::debug!(
                scheme = self.name,
                "max_age_secs is configured but the token carries no `iat`, so its age cannot \
                 be established"
            );
            SignatureError::MissingTimestamp
        })?;
        // Overflow-safe on attacker-chosen input, the same way
        // `check_timestamp_freshness` is (#1049): saturating, widened, compared in
        // `u64`.
        let age = self.clock.now().saturating_sub(issued).unsigned_abs();
        if age > max_age.saturating_add(MAX_CLOCK_SKEW_SECS) {
            return Err(SignatureError::TimestampExpired);
        }
        Ok(())
    }

    /// The event the verified claims describe.
    fn event_from_claims(
        &self,
        token: &str,
        claims: &Map<String, Value>,
        id: &IdSource,
        event_type_claim: &str,
        payload_claim: &str,
    ) -> Result<Verified, SignatureError> {
        let event_type = claims
            .get(event_type_claim)
            .and_then(Value::as_str)
            .ok_or(SignatureError::InvalidFormat)?
            .to_string();
        let payload = claims.get(payload_claim).cloned().ok_or(SignatureError::InvalidFormat)?;
        let id = match id {
            IdSource::Claim(name) => {
                claims.get(name).and_then(claim_as_id).ok_or(SignatureError::InvalidFormat)?
            },
            // The verified token, digested. Not the envelope it arrived in: that is
            // unsigned, and an id that moved with it would let one captured
            // delivery be replayed indefinitely under a fresh outer field (#751).
            IdSource::TokenDigest => hex::encode(Sha256::digest(token.as_bytes())),
        };
        Ok(Verified::Event {
            id,
            event_type,
            payload,
        })
    }

    /// Check that the body is the one the token's digest claim covers.
    fn check_body_hash(
        &self,
        request: &InboundRequest<'_>,
        claims: &Map<String, Value>,
        hash_claim: &str,
    ) -> Result<(), SignatureError> {
        let claimed = claims.get(hash_claim).and_then(Value::as_str).ok_or_else(|| {
            tracing::debug!(
                scheme = self.name,
                claim = hash_claim,
                "a token whose signature covers the body only through a digest claim, without \
                 that claim, covers nothing about the body"
            );
            SignatureError::Mismatch
        })?;
        // Over the RAW bytes, base64. Digesting re-serialized JSON would compute a
        // digest of bytes the sender did not send — the same defect
        // `StandardWebhooksVerifier` documents about signing over
        // `from_utf8_lossy`.
        let expected = BASE64.encode(Sha256::digest(request.body()));
        // Constant-time is not required — both sides are derived from material the
        // sender supplied, and the digest is not a secret — but the comparison is
        // done on bytes rather than on a `String` so a padding difference cannot
        // read as equal.
        if claimed.as_bytes() == expected.as_bytes() {
            Ok(())
        } else {
            Err(SignatureError::Mismatch)
        }
    }
}

/// An id claim, as a string. JSON numbers are accepted because a publisher
/// numbering its events is entitled to send `1001` rather than `"1001"`.
fn claim_as_id(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// Map a `jsonwebtoken` failure onto this crate's error, keeping fault where it
/// belongs (#1045).
fn map_decode_error(error: &jsonwebtoken::errors::Error) -> SignatureError {
    use jsonwebtoken::errors::ErrorKind;
    match error.kind() {
        ErrorKind::ExpiredSignature => SignatureError::TimestampExpired,
        // Everything else is a token this route will not accept: a bad signature, a
        // wrong audience, a malformed body. All the sender's, all a 401.
        _ => SignatureError::Mismatch,
    }
}

impl SignatureVerifier for JwtJwksVerifier {
    fn name(&self) -> &'static str {
        self.name
    }

    fn check_key_material(&self, key_material: Option<&str>) -> Result<(), SignatureError> {
        // Phase 31's rule: key material nothing consults is refused, not ignored.
        // This scheme verifies against keys the *publisher* serves, so a
        // `secret_env` on such a route is a secret the operator believes is in
        // force and that nothing reads.
        if key_material.is_some() {
            return Err(SignatureError::KeyMaterial(format!(
                "the {} scheme verifies a token against the keys its sender's publisher \
                 serves, so it has no shared secret to configure. Remove `secret_env`; a \
                 secret set here is key material nothing consults.",
                self.name
            )));
        }
        Ok(())
    }

    fn resolve_key<'a>(
        &'a self,
        request: &'a InboundRequest<'a>,
        secret: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Cow<'a, str>, SignatureError>> {
        Box::pin(async move {
            // The same rule as `check_key_material`, at the only moment a caller
            // that skipped the boot check still has. An embedder driving this crate
            // directly gets the operator's 5xx rather than silent acceptance.
            self.check_key_material(secret)?;

            let token = request.credential(&self.token_from)?;
            let header =
                decode_header(token.as_ref()).map_err(|_| SignatureError::InvalidFormat)?;

            // Structural parse, then the allow-list — and only then the network.
            let _ = self.permitted_algorithm(header.alg)?;

            let kid = header.kid.as_deref().ok_or_else(|| {
                tracing::debug!(
                    scheme = self.name,
                    "refused a token with no `kid`: a key set cannot be searched without one, \
                     and trying every key in it would make the publisher's key rotation a \
                     signature oracle"
                );
                SignatureError::InvalidFormat
            })?;

            let jwk = self
                .keys
                .key(kid)
                .await
                // The publisher's key set is unreachable or unusable: the
                // operator's or the publisher's problem, so a 5xx. Reporting it to
                // the sender as a 401 is how a provider comes to disable an
                // endpoint (#1045).
                .map_err(|error| SignatureError::KeyMaterial(error.to_string()))?
                .ok_or_else(|| {
                    tracing::debug!(
                        scheme = self.name,
                        kid,
                        "the publisher does not publish this key id, so the signature cannot \
                         be attributed to them"
                    );
                    // The sender's: a 401. Distinct from the arm above, which is why
                    // `JwksKeys` keeps `Ok(None)` and `Err` apart.
                    SignatureError::Mismatch
                })?;

            serde_json::to_string(&jwk)
                .map(Cow::Owned)
                .map_err(|error| SignatureError::KeyMaterial(error.to_string()))
        })
    }

    fn verify(&self, request: &InboundRequest<'_>, key: &str) -> Result<Verified, SignatureError> {
        let token = request.credential(&self.token_from)?;
        // `key` is what this scheme's own `resolve_key` produced, so a failure here
        // is this crate's bug or a caller pairing two schemes — the operator's 5xx.
        let jwk: Jwk = serde_json::from_str(key)
            .map_err(|error| SignatureError::KeyMaterial(error.to_string()))?;
        let decoding = jwk
            .decoding_key()
            .map_err(|error| SignatureError::KeyMaterial(error.to_string()))?;

        let header = decode_header(token.as_ref()).map_err(|_| SignatureError::InvalidFormat)?;
        // Re-checked rather than trusted from `resolve_key`: this method is a pure
        // function of the request and the key, so it does not depend on another
        // having run first.
        let algorithm = self.permitted_algorithm(header.alg)?;

        let mut validation = Validation::new(algorithm);
        // `exp` is checked when present (`validate_exp` defaults true) and NOT
        // required. The default `required_spec_claims` of `{exp}` would refuse
        // every Kinde delivery: Kinde documents no `exp`, and for such a publisher
        // the delivery ledger is what bounds replay.
        validation.set_required_spec_claims::<&str>(&[]);
        validation.leeway = MAX_CLOCK_SKEW_SECS;
        // `iss` is not validated: #708's issuer-less mode exists because these
        // publishers' tokens do not reliably carry one, and the pinned key set plus
        // the required claims are what establish who signed.
        match self.audience.as_deref() {
            Some(audience) => validation.set_audience(&[audience]),
            // Defence in depth, not the only defence: a publisher's webhook token
            // may carry no `aud` at all, and a route that required one would refuse
            // every genuine delivery. `required` is what always applies.
            None => validation.validate_aud = false,
        }

        let data = decode::<Map<String, Value>>(token.as_ref(), &decoding, &validation)
            .map_err(|error| map_decode_error(&error))?;
        let claims = data.claims;

        self.check_required_claims(&claims)?;
        self.check_max_age(&claims)?;

        match &self.subject {
            Authenticates::TheClaims {
                id,
                event_type_claim,
                payload_claim,
            } => {
                self.event_from_claims(token.as_ref(), &claims, id, event_type_claim, payload_claim)
            },
            Authenticates::TheBody { hash_claim } => {
                self.check_body_hash(request, &claims, hash_claim)?;
                // The digest claim is what makes the body verified material. From
                // here it is an ordinary body-signing delivery, and the receiver's
                // own id and type rules apply to it unchanged.
                Ok(Verified::Body)
            },
        }
    }
}

/// Build the scheme a `jwt-jwks` route or one of its presets configured.
///
/// # Errors
///
/// [`SchemeError`] — every variant a boot-time refusal.
pub(crate) fn build(
    provider: &'static str,
    config: &SchemeConfig,
    keys: Arc<dyn JwksKeys>,
) -> Result<Arc<dyn SignatureVerifier>, SchemeError> {
    let preset = match provider {
        "hanko" => Some(hanko()),
        "kinde" => Some(kinde()),
        "fusionauth" => Some(fusionauth()),
        _ => None,
    };
    Ok(match preset {
        Some(preset) => Arc::new(JwtJwksVerifier::from_preset(provider, config, keys, preset)?),
        None => Arc::new(JwtJwksVerifier::from_config(provider, config, keys)?),
    })
}

#[cfg(test)]
pub mod tests;
