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

use crate::{
    signature::{
        SignatureError,
        discord::DiscordVerifier,
        generic::{HmacSha1Verifier, HmacSha256Verifier},
        github::GitHubVerifier,
        gitlab::GitLabVerifier,
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
}

impl SchemeConfig {
    /// The scheme keys this config actually carries, by name, in the order a
    /// refusal should mention them.
    fn present_keys(&self) -> impl Iterator<Item = &'static str> + '_ {
        [
            ("credential", self.credential.is_some()),
            ("encoding", self.encoding.is_some()),
            ("prefix", self.prefix.is_some()),
            ("header_prefix", self.header_prefix.is_some()),
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
    "github",
    "gitlab",
    "hmac-sha1",
    "hmac-sha256",
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
/// `tolerance_secs` is the replay window handed to the schemes that sign a
/// timestamp (Stripe, Slack, SendGrid, Paddle, Discord); the others ignore it.
///
/// # Errors
///
/// [`SchemeError`] — see its variants. Every one is a boot-time refusal.
pub fn build_scheme(
    provider: &str,
    config: &SchemeConfig,
    tolerance_secs: u64,
) -> Result<Arc<dyn SignatureVerifier>, SchemeError> {
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
