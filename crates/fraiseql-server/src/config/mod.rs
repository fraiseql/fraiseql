//! Auxiliary configuration types consumed by [`crate::ServerConfig`] sections.
//!
//! The server's configuration file is deserialized directly into
//! [`crate::ServerConfig`] (see `server_config/`); this module holds the
//! supporting types some of its sections reference, plus the error-sanitization
//! and pool-tuning subsystem configs.
//!
//! The former `RuntimeConfig` layer that used to live here — a parallel
//! `[server]`/`[database]`-shaped config tree with its own loader and
//! `ConfigValidator` — was constructed by nothing but its own tests, while the
//! architecture docs described it as the binary's config path (#839). It was
//! deleted rather than wired in.

#[cfg(feature = "webhooks")]
use fraiseql_webhooks::{CredentialLocation, SchemeConfig, SignatureEncoding};

pub mod error_sanitization;
pub mod pool_tuning;
#[cfg(test)]
mod tests;

// Re-export config types
pub use error_sanitization::{ErrorSanitizationConfig, ErrorSanitizer};
#[allow(deprecated)] // Reason: re-export deprecated alias for backwards compatibility
pub use pool_tuning::{PoolPressureMonitorConfig, PoolTuningConfig};

/// Configuration for durable usage counter persistence.
///
/// Add a `[usage]` section to the server config TOML to enable:
///
/// ```toml
/// [usage]
/// flush_interval_secs = 60
/// ```
///
/// When absent (default), the [`NoopBackend`] is used and counters are
/// in-memory only (reset on process restart).
///
/// [`NoopBackend`]: crate::usage::aggregator::NoopBackend
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct UsagePersistenceConfig {
    /// How often (in seconds) to flush in-memory counters to PostgreSQL.
    ///
    /// Defaults to `60` seconds.
    #[serde(default = "default_flush_interval_secs")]
    pub flush_interval_secs: u64,
}

const fn default_flush_interval_secs() -> u64 {
    60
}

/// Configuration for a single incoming webhook route.
///
/// Gated on `webhooks` — the feature that brings `fraiseql-webhooks` in — because
/// the scheme keys below are that crate's types. `ServerConfig::webhooks` is gated
/// on `inbound`, which implies `webhooks`, so the two cannot come apart.
///
/// `deny_unknown_fields` (#1321): the parent `ServerConfig`'s attribute does **not**
/// propagate into a nested struct, so before this a mistyped `encodng = "base64"`
/// parsed exactly like the correct spelling and the route silently served the
/// default scheme. A key this struct does not know is now a boot refusal that names
/// it. (The other 17 sections with the same hole are #1337.)
///
/// Knowing a key is not the same as *reading* it: `credential`, `encoding` and
/// `prefix` describe a signing scheme, and a preset like `stripe` fixes its own.
/// Carrying one there is refused too, by `scheme::build_scheme` — see
/// [`SchemeError::IrrelevantKey`](fraiseql_webhooks::SchemeError::IrrelevantKey).
#[cfg(feature = "webhooks")]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookRouteConfig {
    /// Name of the environment variable that holds the webhook signing secret.
    ///
    /// Optional since #1322, and **not** merely relaxed: a `jwt-jwks` route
    /// verifies tokens against keys its sender's publisher serves, so it has no
    /// shared secret to configure, and one set there is key material nothing
    /// consults. Which way round a given route needs it is the scheme's answer —
    /// `SignatureVerifier::check_key_material` refuses both mismatches at boot.
    #[serde(default)]
    pub secret_env: Option<String>,
    /// Webhook provider identifier (e.g. `"github"`, `"stripe"`).
    pub provider:   String,
    /// URL path override; if absent, the route name is used as the path segment.
    #[serde(default)]
    pub path:       Option<String>,
    /// The exact public URL this route is registered under at the provider.
    ///
    /// Required for providers whose signing scheme covers the request URL
    /// (Twilio signs scheme + host + path + query). It must be the URL as the
    /// provider knows it — reconstructing it from `Host`/`X-Forwarded-*` headers
    /// would put the signed material under the sender's control, so the server
    /// refuses to boot instead when a URL-signing provider lacks this (#781).
    #[serde(default)]
    pub public_url: Option<String>,

    /// Where this sender puts the credential the scheme authenticates:
    /// `header:<Name>`, `body`, or `body:<field>`.
    ///
    /// Read by the `hmac-sha256` / `hmac-sha1` schemes, whose signing details belong
    /// to the operator rather than to a provider. Absent means `header:X-Signature`,
    /// the pre-#1321 default. A preset (`stripe`, `github`, …) refuses the key.
    #[serde(default)]
    pub credential: Option<CredentialLocation>,

    /// How the credential is written on the wire: `hex` or `base64`. Absent means
    /// `hex`. Same readership as `credential`.
    #[serde(default)]
    pub encoding: Option<SignatureEncoding>,

    /// A literal stripped from the front of the credential before it is decoded
    /// (GitHub-style `sha256=`). Absent means nothing is stripped. Same readership
    /// as `credential`.
    #[serde(default)]
    pub prefix: Option<String>,

    /// The prefix this sender spells the Standard Webhooks header triple with:
    /// `{prefix}-id`, `{prefix}-timestamp`, `{prefix}-signature`. Absent means
    /// `webhook`, the spec's own spelling; Svix and Clerk send `svix` (#1323).
    ///
    /// Read by `standard-webhooks` alone. The `clerk` preset *is* the `svix`
    /// spelling and refuses the key, as does every other scheme — including the
    /// generic HMAC families, which read one header rather than a triple.
    #[serde(default)]
    pub header_prefix: Option<String>,

    /// Where the sender's publisher serves the keys its tokens are signed by
    /// (#1322). Read by `jwt-jwks` and its `hanko` / `kinde` / `fusionauth`
    /// presets, and refused on every other scheme.
    ///
    /// Must be https, or http on a loopback host for local development. A route
    /// whose scheme verifies a token and that names no key set is refused at boot.
    #[serde(default)]
    pub jwks_uri: Option<String>,

    /// The token algorithms this route accepts. Absent means `RS256`.
    ///
    /// `none` and the `HS*` family are refused whatever this says: a shared-secret
    /// algorithm verified against a *public* key set is the algorithm-confusion
    /// attack, because anyone who can read the key set can forge a token.
    #[serde(default)]
    pub algorithms: Option<Vec<String>>,

    /// The `aud` claim a token must carry.
    ///
    /// Strongly recommended and not required: a publisher's webhook JWKS is
    /// usually the same key set that signs its end-user sessions, so this is what
    /// stops a logged-in user's own token being accepted as a delivery. It is not
    /// mandatory because a publisher's webhook token may carry no `aud` at all,
    /// and each preset also fixes the claims that distinguish the two.
    #[serde(default)]
    pub audience: Option<String>,

    /// The claim naming the event's type. A preset fixes its own and refuses this.
    #[serde(default)]
    pub event_type_claim: Option<String>,

    /// The claim carrying the event itself. A preset fixes its own.
    #[serde(default)]
    pub payload_claim: Option<String>,

    /// The claim carrying the event's id, which the delivery ledger keys on.
    /// Absent means a digest of the verified token.
    #[serde(default)]
    pub id_claim: Option<String>,

    /// The claim carrying `base64(SHA-256(raw body))`, which binds a token in a
    /// header to the body it arrived with. Setting it makes the **body** the
    /// event, so it cannot be combined with the claim keys above.
    #[serde(default)]
    pub body_hash_claim: Option<String>,

    /// An additional freshness window in seconds, beyond the token's own `exp`.
    ///
    /// Absent is the right default: a publisher that retries for 36 hours with a
    /// reused token would have every retry past a FraiseQL-chosen age refused.
    #[serde(default)]
    pub max_age_secs: Option<u64>,
}

#[cfg(feature = "webhooks")]
impl WebhookRouteConfig {
    /// This route's scheme keys, as `fraiseql-webhooks` takes them.
    #[must_use]
    pub fn scheme_config(&self) -> SchemeConfig {
        SchemeConfig {
            credential:       self.credential.clone(),
            encoding:         self.encoding,
            prefix:           self.prefix.clone(),
            header_prefix:    self.header_prefix.clone(),
            jwks_uri:         self.jwks_uri.clone(),
            algorithms:       self.algorithms.clone(),
            audience:         self.audience.clone(),
            event_type_claim: self.event_type_claim.clone(),
            payload_claim:    self.payload_claim.clone(),
            id_claim:         self.id_claim.clone(),
            body_hash_claim:  self.body_hash_claim.clone(),
            max_age_secs:     self.max_age_secs,
        }
    }
}
