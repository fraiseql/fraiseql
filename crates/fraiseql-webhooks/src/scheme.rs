//! A route's verification scheme, built from its configuration (#1321).
//!
//! Before this module a scheme was a Rust type looked up by name: its identity and
//! its signature header were `&'static str` constants, so `hmac-sha256` could only
//! ever read `X-Signature` and could only ever compare hex. A sender that puts its
//! MAC somewhere else — Lago's `X-Lago-Signature`, base64 — could not be received
//! at all, and the `credential` / `encoding` / `prefix` keys an operator wrote to
//! say so were discarded at parse.
//!
//! So a scheme is now a **value built at boot** from the route's configuration:
//! [`build_scheme`] is the one construction, and what it returns is what serves the
//! route. Two kinds of scheme come out of it:
//!
//! * the **generic** HMAC families (`hmac-sha256`, `hmac-sha1`), whose credential location,
//!   encoding and prefix the operator describes, because no provider owns them;
//! * the **presets** (`stripe`, `github`, …), whose signing details belong to the provider. A
//!   preset reads none of those keys, and carrying one is refused at boot rather than ignored — a
//!   knob the operator believes is in force and that nothing consults is the same silent drop one
//!   level down.

use std::sync::Arc;

use fraiseql_jwks::JwksKeys;

use crate::{
    signature::{
        SignatureError,
        discord::DiscordVerifier,
        generic::{HmacSha1Verifier, HmacSha256Verifier},
        github::GitHubVerifier,
        gitlab::GitLabVerifier,
        jwt_jwks,
        lemonsqueezy::LemonSqueezyVerifier,
        paddle::PaddleVerifier,
        postmark::PostmarkVerifier,
        sendgrid::SendGridVerifier,
        shopify::ShopifyVerifier,
        slack::SlackVerifier,
        standard_webhooks::{SVIX_HEADER_PREFIX, StandardWebhooksVerifier},
        stripe::StripeVerifier,
        twilio::TwilioVerifier,
    },
    traits::SignatureVerifier,
};

/// Where in the request a scheme finds the credential it authenticates.
///
/// One grammar for every scheme family, so the next one does not add a second key
/// meaning the same thing: a JWT-carrying provider locates a **token** and an HMAC
/// provider locates a **MAC**, but both answer "where is it?" the same way.
///
/// ```text
/// header:<Name>     the named request header, matched case-insensitively
/// body              the whole request body
/// body:<field>      a top-level field of the request body
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialLocation {
    /// The named request header. HTTP header names are case-insensitive, and so is
    /// this: the configured spelling is the operator's, the wire spelling is the
    /// sender's, and they do not have to agree.
    Header(String),
    /// The whole request body (a bare token, `application/jwt` and the like).
    Body,
    /// A top-level field of the request body.
    BodyField(String),
}

impl std::fmt::Display for CredentialLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Header(name) => write!(f, "header:{name}"),
            Self::Body => f.write_str("body"),
            Self::BodyField(field) => write!(f, "body:{field}"),
        }
    }
}

impl std::str::FromStr for CredentialLocation {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.split_once(':') {
            Some(("header", name)) if !name.is_empty() => Ok(Self::Header(name.to_string())),
            Some(("body", field)) if !field.is_empty() => Ok(Self::BodyField(field.to_string())),
            None if value == "body" => Ok(Self::Body),
            _ => Err(format!(
                "`{value}` is not a credential location; expected `header:<Name>`, `body`, or \
                 `body:<field>`"
            )),
        }
    }
}

impl serde::Serialize for CredentialLocation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for CredentialLocation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// How a credential's bytes are written on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignatureEncoding {
    /// Base-16, either case. This is the pre-#1321 behaviour of the generic HMAC
    /// schemes and stays their default.
    Hex,
    /// Standard base64, with padding.
    Base64,
}

impl SignatureEncoding {
    /// Decode a sender-supplied credential into its bytes.
    ///
    /// # Errors
    ///
    /// [`SignatureError::InvalidFormat`] when the value is not this encoding. It is
    /// never [`SignatureError::KeyMaterial`]: these bytes come out of an
    /// unauthenticated request, and `KeyMaterial` maps to a 5xx, so raising it here
    /// would let any caller produce one on demand (#1045).
    pub fn decode(self, value: &str) -> Result<Vec<u8>, SignatureError> {
        match self {
            Self::Hex => hex::decode(value).map_err(|_| SignatureError::InvalidFormat),
            Self::Base64 => {
                use base64::{Engine as _, engine::general_purpose::STANDARD};
                STANDARD.decode(value).map_err(|_| SignatureError::InvalidFormat)
            },
        }
    }
}

/// The scheme keys a route may carry, as parsed from its configuration.
///
/// All three are optional, and absent means the generic schemes' pre-#1321
/// behaviour: the credential in `X-Signature`, hex, with no prefix. That default is
/// **permissive by design** — it is what every existing `hmac-sha256` route already
/// relies on — so it is named here rather than left to be inferred from a
/// `Default` impl somewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemeConfig {
    /// Where the credential is. `None` → `header:X-Signature`.
    pub credential: Option<CredentialLocation>,
    /// How it is encoded. `None` → [`SignatureEncoding::Hex`].
    pub encoding:   Option<SignatureEncoding>,
    /// A literal stripped from the front of the credential before decoding
    /// (GitHub-style `sha256=`). `None` → nothing is stripped.
    pub prefix:     Option<String>,

    /// The prefix the Standard Webhooks header triple is spelled with:
    /// `{prefix}-id`, `{prefix}-timestamp`, `{prefix}-signature`. `None` →
    /// `webhook`, the spec's own spelling. Svix and Clerk send `svix`.
    ///
    /// Read only by `standard-webhooks`; the `clerk` preset fixes it to `svix`, and
    /// every other scheme refuses the key.
    pub header_prefix: Option<String>,

    /// Where the sender's publisher serves the keys its tokens are signed by
    /// (#1322).
    ///
    /// Read by `jwt-jwks` and each of its presets, and by nothing else. It is the
    /// one key a preset still needs, because the JWKS of a *tenant* is not a detail
    /// the provider fixes — `https://{tenant}.hanko.io/.well-known/jwks.json` is
    /// per deployment.
    pub jwks_uri: Option<String>,

    /// The token algorithms this route accepts, as JWT `alg` names.
    ///
    /// `None` → `RS256`, which is what all three providers in #1322 sign with. The
    /// list is an allow-list checked **before** any key lookup, and `none` and the
    /// `HS*` family are refused whatever it says: a shared-secret algorithm
    /// verified against a public key set is the algorithm-confusion attack.
    pub algorithms: Option<Vec<String>>,

    /// The `aud` claim a token must carry.
    ///
    /// Read by `jwt-jwks` and its presets. Not optional in effect: a publisher's
    /// webhook JWKS is usually the **same** key set that signs its end-user
    /// sessions, so a route that accepts any token verifying against it accepts a
    /// logged-in user's own session token. See the presets for the other half of
    /// that defence.
    pub audience: Option<String>,

    /// The claim naming the event's type. `None` → the scheme's own default.
    pub event_type_claim: Option<String>,

    /// The claim carrying the event itself. `None` → the scheme's own default.
    pub payload_claim: Option<String>,

    /// The claim carrying the event's id, which the delivery ledger keys on.
    ///
    /// `None` → the scheme's own default, which for a publisher whose token
    /// carries no id at all is a digest of the verified token.
    pub id_claim: Option<String>,

    /// The claim carrying a digest of the raw request body, which is what binds a
    /// token in a *header* to the body it arrived with (FusionAuth).
    pub body_hash_claim: Option<String>,

    /// An additional freshness window in seconds, beyond the token's own `exp`.
    ///
    /// `None` → the token's `exp` is the only freshness rule, which is the right
    /// default and not a gap: a publisher that retries for 36 hours with a reused
    /// token would have every retry past the window refused by a FraiseQL-chosen
    /// age. Where a token carries no `exp` at all, the delivery ledger is what
    /// bounds replay, and the scheme's documentation says so.
    pub max_age_secs: Option<u64>,
}

impl SchemeConfig {
    /// Every key absent.
    ///
    /// This is exactly what a route carrying no scheme keys deserializes to — each
    /// field is `#[serde(default)]` on `WebhookRouteConfig` — so a fixture built
    /// from here has the same shape as one a configuration file produces, rather
    /// than a shape only a test can reach.
    ///
    /// `const`, because the two are the same value and a `Default` impl cannot be
    /// called from a `const` item.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            credential:       None,
            encoding:         None,
            prefix:           None,
            header_prefix:    None,
            jwks_uri:         None,
            algorithms:       None,
            audience:         None,
            event_type_claim: None,
            payload_claim:    None,
            id_claim:         None,
            body_hash_claim:  None,
            max_age_secs:     None,
        }
    }

    /// The scheme keys this config actually carries, by name, in the order a
    /// refusal should mention them.
    fn present_keys(&self) -> impl Iterator<Item = &'static str> + '_ {
        [
            ("credential", self.credential.is_some()),
            ("encoding", self.encoding.is_some()),
            ("prefix", self.prefix.is_some()),
            ("header_prefix", self.header_prefix.is_some()),
            ("jwks_uri", self.jwks_uri.is_some()),
            ("algorithms", self.algorithms.is_some()),
            ("audience", self.audience.is_some()),
            ("event_type_claim", self.event_type_claim.is_some()),
            ("payload_claim", self.payload_claim.is_some()),
            ("id_claim", self.id_claim.is_some()),
            ("body_hash_claim", self.body_hash_claim.is_some()),
            ("max_age_secs", self.max_age_secs.is_some()),
        ]
        .into_iter()
        .filter_map(|(key, present)| present.then_some(key))
    }

    /// Refuse every present key that is not in `reads`.
    ///
    /// Each scheme declares what it reads, so a key that reaches a scheme which
    /// does not consult it is a boot refusal rather than a knob the operator
    /// believes is in force. That has to be per scheme and not per *family*: before
    /// #1323 the rule was binary — a preset read nothing and the generic HMAC
    /// families read all three keys — and `header_prefix` is the key that makes the
    /// binary wrong, since `standard-webhooks` reads it and the generic families do
    /// not. A two-way split would have silently accepted `header_prefix` on an
    /// `hmac-sha256` route.
    ///
    /// # Errors
    ///
    /// [`SchemeError::IrrelevantKey`] naming the key, the scheme, and what the
    /// scheme does read.
    fn reads_only(&self, provider: &str, reads: &[&'static str]) -> Result<(), SchemeError> {
        let Some(key) = self.present_keys().find(|key| !reads.contains(key)) else {
            return Ok(());
        };
        Err(SchemeError::IrrelevantKey {
            provider: provider.to_string(),
            key,
            reads: if reads.is_empty() {
                "no scheme keys at all — it fixes its own signing details".to_string()
            } else {
                format!("only: {}", reads.join(", "))
            },
        })
    }
}

/// What [`build_scheme`] needs besides the route's own configuration.
///
/// One struct rather than a growing parameter list, because the two things in it
/// answer different questions and only one of them is per-route configuration:
/// `tolerance_secs` is a policy the receiver sets for every route, and `jwks` is a
/// *capability* the caller supplies because this crate does no network I/O and
/// owns no HTTP client.
pub struct SchemeContext {
    /// The replay window, in seconds, handed to the schemes that sign a timestamp
    /// (Stripe, Slack, SendGrid, Paddle, Discord, `standard-webhooks`). The others
    /// ignore it.
    tolerance_secs: u64,
    /// The published-key source for this route, when it has one.
    ///
    /// `None` is the honest state of a route whose configuration named no
    /// `jwks_uri`, and it is what makes `provider = "jwt-jwks"` without one a
    /// **boot refusal** rather than a route that 5xxes on its first delivery.
    jwks:           Option<Arc<dyn JwksKeys>>,
}

impl std::fmt::Debug for SchemeContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemeContext")
            .field("tolerance_secs", &self.tolerance_secs)
            .field("jwks", &self.jwks.is_some())
            .finish()
    }
}

impl SchemeContext {
    /// A context with a replay window and no published-key source.
    #[must_use]
    pub const fn with_tolerance(tolerance_secs: u64) -> Self {
        Self {
            tolerance_secs,
            jwks: None,
        }
    }

    /// Attach the source a token-verifying scheme looks its keys up in.
    ///
    /// The receiver builds one per route from that route's `jwks_uri`; a test
    /// hands in a local key, which is how a scheme's own suite runs with no
    /// network at all.
    #[must_use]
    pub fn with_jwks(mut self, keys: Arc<dyn JwksKeys>) -> Self {
        self.jwks = Some(keys);
        self
    }

    /// The published-key source, or a refusal naming the route's own scheme.
    fn require_jwks(&self, provider: &str) -> Result<Arc<dyn JwksKeys>, SchemeError> {
        self.jwks.clone().ok_or_else(|| SchemeError::MissingKeySource {
            provider: provider.to_string(),
        })
    }
}

impl Default for SchemeConfig {
    fn default() -> Self {
        Self::none()
    }
}

/// Why a route's scheme could not be built.
///
/// Every variant is an operator-facing configuration error raised at boot, never a
/// per-delivery outcome: a route that cannot be built must not be mounted, because
/// the alternative is a route that answers 500 — or worse, verifies against a
/// scheme the operator did not ask for — on its first genuine delivery.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemeError {
    /// `provider` names no scheme this crate can build.
    #[error("provider = {provider:?} is not a known webhook scheme; known schemes: {known}")]
    UnknownScheme {
        /// The value the route configured.
        provider: String,
        /// Every scheme [`build_scheme`] can build, comma-separated and sorted.
        known:    String,
    },

    /// The route carries a scheme key the selected scheme does not read.
    #[error(
        "`{key}` is not a key the {provider:?} scheme reads, so it is configuration nothing \
         consults. Remove it. The {provider:?} scheme reads {reads}."
    )]
    IrrelevantKey {
        /// The scheme that does not read the key.
        provider: String,
        /// The key that would have been ignored.
        key:      &'static str,
        /// What the scheme does read, rendered for the operator.
        reads:    String,
    },

    /// `header_prefix` cannot form the header names the scheme reads.
    #[error(
        "header_prefix = {value:?} cannot form a header name for the {provider:?} scheme, which \
         joins it to `-id`, `-timestamp` and `-signature`. Expected a token such as `webhook` \
         (the Standard Webhooks default) or `svix` (what Svix and Clerk send)."
    )]
    InvalidHeaderPrefix {
        /// The scheme that could not use it.
        provider: String,
        /// The value the route configured.
        value:    String,
    },

    /// The scheme verifies against keys its sender's publisher publishes, and the
    /// route did not say where those are.
    #[error(
        "the {provider:?} scheme verifies a token against the keys its sender's publisher \
         serves, so the route needs `jwks_uri` set to that key set's URL. It must be https \
         (or http on a loopback host, for local development)."
    )]
    MissingKeySource {
        /// The scheme that has nowhere to look a key up.
        provider: String,
    },

    /// The scheme cannot know where its credential is unless the route says.
    #[error(
        "the {provider:?} scheme needs `credential` to say where the token is: \
         `header:<Name>`, `body` (the whole body is the token), or `body:<field>`. It has no \
         default, because no provider puts a JWT in the generic schemes' `X-Signature` and \
         inheriting that spelling would only produce a 401 per delivery. The named presets \
         (`hanko`, `kinde`, `fusionauth`) each fix their own."
    )]
    MissingCredentialLocation {
        /// The scheme that was not told.
        provider: String,
    },

    /// `algorithms` names an algorithm this scheme will not verify with.
    #[error("algorithms = {value:?} cannot be used by the {provider:?} scheme: {reason}")]
    UnusableAlgorithm {
        /// The scheme that refuses it.
        provider: String,
        /// The value the route configured.
        value:    String,
        /// Why it is refused.
        reason:   String,
    },

    /// The scheme's credential cannot live where the route says it does.
    #[error(
        "credential = {location:?} is not something the {provider:?} scheme can read: its \
         credential is a MAC over the request body, so it cannot also be part of that body. \
         Use `header:<Name>`."
    )]
    UnsupportedCredential {
        /// The scheme that cannot honour the location.
        provider: String,
        /// The location the route configured.
        location: String,
    },
}

/// The scheme keys the generic HMAC families read: the operator owns their signing
/// details, so all three describe the credential and none of them is a header
/// *prefix* (these schemes read one header, not a triple).
const GENERIC_HMAC_KEYS: &[&str] = &["credential", "encoding", "prefix"];

/// Every scheme [`build_scheme`] can build.
///
/// Sorted, and the single list the refusal message and the fixture-coverage test
/// both read. A name added to [`build_scheme`]'s match belongs here too: the test
/// `every_known_scheme_builds` proves this list is not *wider* than that match, and
/// `signature::tests` requires a genuine + tampered fixture for every name in it.
pub const KNOWN_SCHEMES: &[&str] = &[
    "clerk",
    "discord",
    "fusionauth",
    "github",
    "gitlab",
    "hanko",
    "hmac-sha1",
    "hmac-sha256",
    "jwt-jwks",
    "kinde",
    "lemonsqueezy",
    "paddle",
    "postmark",
    "sendgrid",
    "shopify",
    "slack",
    "standard-webhooks",
    "stripe",
    "twilio",
];

/// Build the verification scheme a route configured.
///
/// This is the **one** construction. Boot validation and the mounted route are
/// served from the same call, so a configuration that boots can never meet a
/// different scheme — or no scheme at all — at request time.
///
/// `context` carries what is not the route's own configuration: the replay window
/// the receiver applies to every route, and the published-key source a
/// token-verifying scheme looks keys up in — supplied by the caller because this
/// crate does no network I/O.
///
/// # Errors
///
/// [`SchemeError`] — see its variants. Every one is a boot-time refusal.
pub fn build_scheme(
    provider: &str,
    config: &SchemeConfig,
    context: &SchemeContext,
) -> Result<Arc<dyn SignatureVerifier>, SchemeError> {
    let tolerance_secs = context.tolerance_secs;
    let built: Arc<dyn SignatureVerifier> = match provider {
        // The two families no provider owns: the operator describes the scheme.
        "hmac-sha256" => {
            config.reads_only(provider, GENERIC_HMAC_KEYS)?;
            Arc::new(HmacSha256Verifier::from_config(provider, config)?)
        },
        "hmac-sha1" => {
            config.reads_only(provider, GENERIC_HMAC_KEYS)?;
            Arc::new(HmacSha1Verifier::from_config(provider, config)?)
        },
        // The spec's own scheme (#1323): the sender's header spelling is the one
        // detail it leaves open, so `header_prefix` is the one key it reads.
        "standard-webhooks" => {
            config.reads_only(provider, &["header_prefix"])?;
            Arc::new(
                StandardWebhooksVerifier::from_config(provider, config)?
                    .with_tolerance(tolerance_secs),
            )
        },
        // Tokens verified against a key set the sender's publisher serves (#1322).
        // The one scheme whose key material is not the operator's, so it is also the
        // one that overrides `resolve_key` — and the route's key source is a
        // capability the caller supplies, not configuration, because this crate
        // makes no network requests.
        "jwt-jwks" => {
            config.reads_only(provider, jwt_jwks::JWT_JWKS_KEYS)?;
            jwt_jwks::build("jwt-jwks", config, context.require_jwks(provider)?)?
        },
        // Its presets. Each fixes its own token location, claims and event shape,
        // and reads only where its tenant's key set is and which audience this
        // deployment is — neither of which a provider can fix.
        "hanko" | "kinde" | "fusionauth" => {
            config.reads_only(provider, jwt_jwks::JWT_JWKS_PRESET_KEYS)?;
            let name = match provider {
                "hanko" => "hanko",
                "kinde" => "kinde",
                // Exhaustive over the arm's own pattern, so a fourth preset added to
                // the match above without a name here is a compile error rather than
                // a route silently reporting itself as `fusionauth`.
                _ => "fusionauth",
            };
            jwt_jwks::build(name, config, context.require_jwks(provider)?)?
        },
        // Presets. `preset` refuses any scheme key before constructing.
        //
        // `clerk` is Standard Webhooks under `svix-*` header names and nothing else,
        // so it is a preset over that scheme rather than a second implementation of
        // it — and `header_prefix` is refused here precisely because the preset IS
        // the prefix.
        "clerk" => preset(
            provider,
            config,
            StandardWebhooksVerifier::with_header_prefix(SVIX_HEADER_PREFIX)
                .with_tolerance(tolerance_secs),
        )?,
        "stripe" => preset(provider, config, StripeVerifier::new().with_tolerance(tolerance_secs))?,
        "github" => preset(provider, config, GitHubVerifier)?,
        "shopify" => preset(provider, config, ShopifyVerifier)?,
        "gitlab" => preset(provider, config, GitLabVerifier)?,
        "slack" => preset(provider, config, SlackVerifier::new().with_tolerance(tolerance_secs))?,
        "twilio" => preset(provider, config, TwilioVerifier)?,
        "sendgrid" => {
            preset(provider, config, SendGridVerifier::new().with_tolerance(tolerance_secs))?
        },
        "postmark" => preset(provider, config, PostmarkVerifier)?,
        "paddle" => preset(provider, config, PaddleVerifier::new().with_tolerance(tolerance_secs))?,
        "lemonsqueezy" => preset(provider, config, LemonSqueezyVerifier)?,
        "discord" => {
            preset(provider, config, DiscordVerifier::new().with_tolerance(tolerance_secs))?
        },
        other => {
            return Err(SchemeError::UnknownScheme {
                provider: other.to_string(),
                known:    KNOWN_SCHEMES.join(", "),
            });
        },
    };
    Ok(built)
}

/// Wrap a preset — a scheme whose signing details belong to the provider, so it
/// reads no scheme keys at all.
fn preset<V: SignatureVerifier + 'static>(
    provider: &str,
    config: &SchemeConfig,
    verifier: V,
) -> Result<Arc<dyn SignatureVerifier>, SchemeError> {
    config.reads_only(provider, &[])?;
    Ok(Arc::new(verifier))
}

/// The credential location a generic HMAC scheme accepts, from its configuration.
///
/// `None` is `header:X-Signature`: the pre-#1321 default, kept so that every route
/// configured before this seam existed keeps working unchanged.
///
/// Header-only, and not because the seam cannot read a body credential — it can,
/// through [`InboundRequest::credential`](crate::InboundRequest::credential). It is
/// because a *MAC in the body it covers* is not a scheme: the credential would be
/// part of its own signed material. Token schemes, whose credential is the body,
/// are #1322's.
pub(crate) fn header_only(
    provider: &str,
    credential: Option<&CredentialLocation>,
) -> Result<CredentialLocation, SchemeError> {
    match credential {
        None => Ok(CredentialLocation::Header("X-Signature".to_string())),
        Some(CredentialLocation::Header(name)) => Ok(CredentialLocation::Header(name.clone())),
        // `body` and `body:<field>` are part of the grammar because #1322's token
        // schemes need them. For an HMAC family they are not "not yet supported"
        // but meaningless, so this is a permanent refusal, at boot rather than on
        // every delivery.
        //
        // Named rather than caught by `_`, so the next location this grammar grows
        // is a compile error here instead of being absorbed into "unsupported".
        Some(location @ (CredentialLocation::Body | CredentialLocation::BodyField(_))) => {
            Err(SchemeError::UnsupportedCredential {
                provider: provider.to_string(),
                location: location.to_string(),
            })
        },
    }
}

#[cfg(test)]
mod tests;
