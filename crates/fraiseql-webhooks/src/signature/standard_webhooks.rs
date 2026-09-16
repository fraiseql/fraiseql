//! The Standard Webhooks scheme — every Svix sender, and Clerk through it (#1323).
//!
//! ```text
//! signed content   {id}.{timestamp}.{body}          HMAC-SHA256, raw bytes
//! key              base64, behind a `whsec_` prefix
//! headers          {prefix}-id  {prefix}-timestamp  {prefix}-signature
//! signature header one or more space-separated `<version>,<base64>` entries
//! ```
//!
//! Two things make it a scheme of its own rather than a configuration of
//! `hmac-sha256`. Its signed content is not the body — it is the body joined to an
//! id and a timestamp that arrive in their own headers — and its credential is a
//! *list*, because a sender rotating its signing secret sends one entry per active
//! secret and the delivery is genuine when any of them matches.
//!
//! # The id is why this scheme exists
//!
//! `{prefix}-id` is inside the signed content, so it can key the replay defence.
//! That is the whole difference from #751, where the receiver keyed on the same
//! header *before anything signed it*: one captured delivery replayed under a fresh
//! id claimed a fresh key and re-fired every `after:ingest` function, indefinitely.
//! Verification here reports [`Verified::BodyWithId`], and the id the ledger claims
//! comes out of that value and nowhere else — so a replay under a fresh id does not
//! reach the ledger at all, it fails the signature.
//!
//! # `v1a` (Ed25519) is not implemented
//!
//! The spec defines an asymmetric `v1a` signature. No sender named in #1323 uses
//! it, so a `v1a,` entry is skipped like any other unrecognised version tag — but a
//! route configured with `whpk_` / `whsk_` key material is refused **at boot, by
//! name**, so the gap is loud instead of being a 401 on every delivery.

use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{
    request::InboundRequest,
    scheme::{SchemeConfig, SchemeError, SignatureEncoding},
    signature::{SignatureError, Verified, check_timestamp_freshness, constant_time_eq},
    traits::{Clock, SignatureVerifier, SystemClock},
};

/// The version tag of the symmetric HMAC-SHA256 signature — the only one this
/// scheme verifies.
const SIGNATURE_VERSION: &str = "v1";

/// The prefix a Standard Webhooks symmetric secret is published behind.
const SYMMETRIC_SECRET_PREFIX: &str = "whsec_";

/// The prefixes Svix publishes **asymmetric** (`v1a`, Ed25519) key material behind:
/// `whpk_` is the public verifying key, `whsk_` the private signing key.
///
/// Both are refused by name. An operator who configured one asked for a signature
/// scheme this verifier does not implement, and the alternative to refusing is a
/// route that answers 401 to every genuine delivery with no indication why.
const ASYMMETRIC_KEY_PREFIXES: [&str; 2] = ["whpk_", "whsk_"];

/// The default header prefix, from the spec: `webhook-id`, `webhook-timestamp`,
/// `webhook-signature`.
const DEFAULT_HEADER_PREFIX: &str = "webhook";

/// The header prefix Svix and Clerk send: `svix-id`, and so on.
pub const SVIX_HEADER_PREFIX: &str = "svix";

/// Decode a Standard Webhooks secret into its key bytes.
///
/// Strips a `whsec_` prefix if present — the reference libraries do, and a secret
/// pasted without it is the same key — then base64-decodes.
///
/// # The length is deliberately not validated
///
/// The spec states that secrets are 24–64 bytes. Svix's own manual-verification
/// documentation publishes `whsec_plJ3nmyCDGBKInavdOK15jsl`, which decodes to
/// **18**. A decoder enforcing the spec's range would refuse the provider's
/// documented key, so this validates that the secret decodes and nothing about how
/// long it is. `tests/standard_webhooks_test.rs` pins that case.
///
/// # Errors
///
/// [`SignatureError::KeyMaterial`] for every failure — this is the **server's** key
/// material, so it maps to a 5xx and never to a 401 (#1045). Nothing a sender can
/// put in a request reaches this function.
pub fn decode_key_material(secret: &str) -> Result<Vec<u8>, SignatureError> {
    if let Some(prefix) = ASYMMETRIC_KEY_PREFIXES.iter().find(|p| secret.starts_with(**p)) {
        return Err(SignatureError::KeyMaterial(format!(
            "asymmetric Standard Webhooks (v1a) is not supported: `{prefix}` names an \
             Ed25519 key pair, and this scheme verifies the symmetric `v1` signatures \
             only. Configure the endpoint's symmetric `{SYMMETRIC_SECRET_PREFIX}` secret \
             at the sender instead."
        )));
    }
    let encoded = secret.strip_prefix(SYMMETRIC_SECRET_PREFIX).unwrap_or(secret);
    let key = BASE64.decode(encoded).map_err(|error| {
        SignatureError::KeyMaterial(format!(
            "a Standard Webhooks secret is base64 behind a `{SYMMETRIC_SECRET_PREFIX}` \
             prefix, and this one does not decode: {error}"
        ))
    })?;
    if key.is_empty() {
        return Err(SignatureError::KeyMaterial(
            "a Standard Webhooks secret must not decode to nothing".to_string(),
        ));
    }
    Ok(key)
}

/// Whether `prefix` can form the three header names this scheme reads.
///
/// A prefix is joined to `-id`, `-timestamp` and `-signature`, so it has to be a
/// non-empty HTTP token that does not already end in `-`. Spelling is *not*
/// checked: restricting the value to `webhook` and `svix` would foreclose a third
/// Standard Webhooks sender, which is the compiled-in-provider-detail defect #1321
/// removed. A misspelled prefix fails the same way a misspelled `credential` header
/// does — every delivery 401s — and that is the accepted trade there too.
fn header_prefix_is_usable(prefix: &str) -> bool {
    !prefix.is_empty()
        && !prefix.ends_with('-')
        && prefix.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Verifies Standard Webhooks deliveries (Svix, Clerk, and any other sender of the
/// spec).
pub struct StandardWebhooksVerifier {
    /// `{prefix}-id`, pre-joined so a delivery costs no allocation to look up.
    id_header:        String,
    /// `{prefix}-timestamp`.
    timestamp_header: String,
    /// `{prefix}-signature`.
    signature_header: String,
    clock:            Arc<dyn Clock>,
    tolerance:        u64,
}

impl StandardWebhooksVerifier {
    /// The spec's header prefix (`webhook-*`), the system clock, and a 5-minute
    /// freshness window — the same window the Stripe and Slack schemes use.
    #[must_use]
    pub fn new() -> Self {
        Self::with_header_prefix(DEFAULT_HEADER_PREFIX)
    }

    /// The scheme under a given header prefix: `webhook` per the spec, `svix` for
    /// Svix and Clerk.
    #[must_use]
    pub fn with_header_prefix(prefix: &str) -> Self {
        Self {
            id_header:        format!("{prefix}-id"),
            timestamp_header: format!("{prefix}-timestamp"),
            signature_header: format!("{prefix}-signature"),
            clock:            Arc::new(SystemClock),
            tolerance:        300,
        }
    }

    /// Build from a route's scheme keys. Reads `header_prefix` and nothing else.
    ///
    /// # Errors
    ///
    /// [`SchemeError::InvalidHeaderPrefix`] when the configured prefix cannot form a
    /// header name.
    pub fn from_config(provider: &str, config: &SchemeConfig) -> Result<Self, SchemeError> {
        let prefix = config.header_prefix.as_deref().unwrap_or(DEFAULT_HEADER_PREFIX);
        if !header_prefix_is_usable(prefix) {
            return Err(SchemeError::InvalidHeaderPrefix {
                provider: provider.to_string(),
                value:    prefix.to_string(),
            });
        }
        Ok(Self::with_header_prefix(prefix))
    }

    /// Replace the clock, so a published vector's fixed timestamp can be verified
    /// at the moment it was signed.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Set the freshness window in seconds.
    #[must_use]
    pub const fn with_tolerance(mut self, seconds: u64) -> Self {
        self.tolerance = seconds;
        self
    }
}

impl Default for StandardWebhooksVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl SignatureVerifier for StandardWebhooksVerifier {
    fn name(&self) -> &'static str {
        "standard-webhooks"
    }

    fn check_key_material(&self, key_material: Option<&str>) -> Result<(), SignatureError> {
        // The absent case is the trait's, not this scheme's: a Standard Webhooks
        // sender signs with a shared secret like every other scheme here, so
        // "there is no secret" has one answer and it is written once.
        let Some(secret) = key_material else {
            return Err(SignatureError::KeyMaterial(format!(
                "the {} scheme verifies with a shared secret, so the route needs `secret_env` \
                 set to the endpoint's signing secret",
                self.name()
            )));
        };
        decode_key_material(secret).map(|_| ())
    }

    fn verify(
        &self,
        request: &InboundRequest<'_>,
        secret: &str,
    ) -> Result<Verified, SignatureError> {
        let presented = request
            .header(&self.signature_header)
            .ok_or_else(|| SignatureError::MissingCredential(self.signature_header.clone()))?;
        // The id is as load-bearing as the signature: without it there is no signed
        // content to compute, and it is what the ledger will claim. Absent, the
        // delivery is not a Standard Webhooks delivery.
        let id = request
            .header(&self.id_header)
            .ok_or_else(|| SignatureError::MissingCredential(self.id_header.clone()))?;
        // `MissingTimestamp` rather than `MissingCredential`, matching the three other
        // schemes that read a timestamp header.
        let timestamp =
            request.header(&self.timestamp_header).ok_or(SignatureError::MissingTimestamp)?;

        // Replay window first, through the shared overflow-safe check (#1049) — this
        // runs on an unauthenticated header, so it must not do unchecked arithmetic.
        check_timestamp_freshness(self.clock.now(), timestamp, self.tolerance)?;

        let key = decode_key_material(secret)?;

        // The signed content is built as BYTES. `StripeVerifier` builds its signing
        // string with `String::from_utf8_lossy(payload)`, which replaces every
        // ill-formed sequence with U+FFFD and so computes a MAC over bytes the sender
        // did not send. A body is arbitrary bytes and the sender signed those.
        let mut signed = format!("{id}.{timestamp}.").into_bytes();
        signed.extend_from_slice(request.body());

        let mut mac = Hmac::<Sha256>::new_from_slice(&key)
            .map_err(|error| SignatureError::KeyMaterial(error.to_string()))?;
        mac.update(&signed);
        let expected = mac.finalize().into_bytes();

        // The header is a space-separated list of `<version>,<value>` entries, and a
        // sender mid-rotation sends one per active secret in no guaranteed order — so
        // the delivery is genuine when ANY `v1` entry matches, exactly as Stripe's
        // several `v1=` entries are handled (#787).
        //
        // An unrecognised version tag (`v1a`, a future `v2`) is SKIPPED rather than
        // refused: a sender that adds a version during a migration would otherwise
        // break every delivery. Nothing in the spec text states this rule; it is the
        // only behaviour compatible with a sender growing a version, and it is pinned
        // by test rather than by citation.
        let mut saw_a_usable_version = false;
        let mut matched = false;
        for entry in presented.split_ascii_whitespace() {
            let Some((version, value)) = entry.split_once(',') else {
                continue;
            };
            if version != SIGNATURE_VERSION {
                continue;
            }
            saw_a_usable_version = true;
            // `|=`, not `||=`: every candidate is compared whether or not an earlier
            // one matched, so the work does not depend on which entry is the right
            // one. A candidate that is not base64 simply never matches — it is one
            // entry of a list, and refusing the whole delivery for it would break the
            // rotation case this loop exists for.
            matched |= SignatureEncoding::Base64
                .decode(value)
                .is_ok_and(|bytes| constant_time_eq(&bytes, expected.as_slice()));
        }

        if !saw_a_usable_version {
            // Sender-supplied bytes in a shape this scheme cannot read: `InvalidFormat`
            // maps to 401. Never `KeyMaterial`, which maps to 5xx and would let any
            // caller produce one on demand (#1045).
            return Err(SignatureError::InvalidFormat);
        }
        if !matched {
            return Err(SignatureError::Mismatch);
        }
        Ok(Verified::BodyWithId { id: id.to_string() })
    }
}

#[cfg(test)]
mod tests;
