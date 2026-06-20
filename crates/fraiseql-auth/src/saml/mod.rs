//! SAML 2.0 service-provider login + Assertion Consumer Service (#381).
//!
//! This module is gated behind the **non-default** `auth-saml` Cargo feature, which pulls
//! in [`samael`](https://crates.io/crates/samael) and its `xmlsec` backend (libxml2 +
//! openssl + the xmlsec1 C library). The default build stays lean and free of the C
//! XML/crypto stack.
//!
//! # What this slice ships
//!
//! - SP-initiated SSO: [`saml_login`] builds a signed-or-unsigned `AuthnRequest` and redirects the
//!   browser to the IdP (HTTP-Redirect binding).
//! - Assertion Consumer Service: [`saml_acs`] receives the IdP's `SAMLResponse` (HTTP-POST
//!   binding), verifies it, resolves a local user, and creates a session.
//!
//! The broader #381 umbrella (multi-IdP discovery, per-tenant SAML config storage, SCIM)
//! stays open; this is the smallest shippable, security-complete SP login + ACS slice.
//!
//! # Security model (all fail-closed)
//!
//! [`verify_saml_response`] is the heart. It owns, in order:
//!
//! 1. **XXE defense** — the raw response is rejected outright if it carries a `DOCTYPE` or entity
//!    declaration (SAML never legitimately needs a DTD), so no entity expansion or external-entity
//!    fetch can occur — rejected before the XML ever reaches a parser.
//! 2. **Signature + assertion validation** — delegated to `samael`, which *reduces* the document to
//!    only the bytes covered by a verified signature and parses **that** (XML-Signature-Wrapping
//!    defense by construction — the asserted element is taken by reference to what was signed,
//!    never re-queried from the original DOM), then enforces audience, `Recipient`/`Destination`,
//!    `NotBefore`/`NotOnOrAfter`, issuer, and `InResponseTo`. A configured **signature-algorithm
//!    allow-list** blocks algorithm substitution.
//! 3. **Replay protection** — the assertion `ID` is recorded single-use in a [`SamlReplayCache`]; a
//!    second presentation of the same assertion is rejected.
//!
//! # Account linking (tenant-bounded trust, #368/#381)
//!
//! A successfully verified assertion maps to a local user via the existing
//! [`crate::account_linking::AccountStore::link_or_create_user`] keyed on
//! `("saml:<idp>", NameID)`. Whether the asserted email is allowed to *merge* across
//! providers is governed by [`effective_saml_email_verified`] — opt-in per IdP, default
//! off, and **never** by registering the IdP into the global
//! [`crate::account_linking::TrustedEmailProviders`] set. See that function's docs for the
//! tenant-bounding rule that prevents a cross-tenant nOAuth merge.

mod config;
mod handler;
mod linking;
mod replay;
mod verify;

pub use config::{SamlAttributeMapping, SamlIdpConfig, SamlIdpConfigBuilder};
pub use handler::{SamlAuthState, saml_acs, saml_login, saml_routes};
pub use linking::{effective_saml_email_verified, saml_provider_key};
pub use replay::SamlReplayCache;
pub use verify::{VerifiedAssertion, verify_saml_response};

#[cfg(test)]
mod tests;

/// Errors raised by the SAML SP login + ACS flow.
#[derive(Debug, thiserror::Error)]
pub enum SamlError {
    /// IdP/SP configuration is invalid (e.g. unparseable IdP metadata or certificate).
    #[error("SAML configuration error: {0}")]
    Config(String),

    /// The `SAMLResponse` could not be base64-decoded or is not well-formed XML.
    #[error("malformed SAMLResponse: {0}")]
    Malformed(String),

    /// The response carried a `DOCTYPE`/entity declaration and was rejected before parsing
    /// (XXE / entity-expansion defense).
    #[error("SAMLResponse contained a DOCTYPE or entity declaration; rejected (XXE defense)")]
    DocTypeForbidden,

    /// Signature, audience, recipient, destination, conditions, issuer or `InResponseTo`
    /// validation failed. The detail is for logs only — never surfaced to the client.
    #[error("SAML assertion verification failed: {0}")]
    Verification(String),

    /// The assertion `ID` was already consumed — a replayed assertion.
    #[error("SAML assertion replay detected (assertion ID already consumed)")]
    Replay,

    /// A required field was absent from an otherwise-valid assertion.
    #[error("SAML assertion missing required field: {0}")]
    MissingField(&'static str),
}
