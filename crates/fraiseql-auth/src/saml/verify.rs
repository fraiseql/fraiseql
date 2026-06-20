//! SAML assertion verification — the security core (#381).
//!
//! [`verify_saml_response`] decodes, defends against XXE, delegates signature/condition
//! validation to `samael` (which reduces the document to the signed bytes — XSW defense),
//! enforces single-use replay protection, and extracts a [`VerifiedAssertion`].

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::{SamlError, SamlIdpConfig, replay::SamlReplayCache};

/// SAML 1.1 email-address `NameID` format URN. When the subject `NameID` uses this format
/// and no email attribute is present, the `NameID` value itself is the email.
const NAMEID_FORMAT_EMAIL: &str = "urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress";

/// Fallback replay-window length used when an assertion carries no `Conditions/NotOnOrAfter`
/// (`samael` would already have rejected a missing/expired window, so this is belt-and-braces).
const FALLBACK_REPLAY_WINDOW_SECS: i64 = 300;

/// A SAML assertion that has passed full verification.
#[derive(Debug, Clone)]
pub struct VerifiedAssertion {
    /// Subject `NameID` value — the stable per-IdP user identifier (the account-store
    /// `provider_id`).
    pub name_id:         String,
    /// Subject `NameID` `Format`, if present.
    pub name_id_format:  Option<String>,
    /// Resolved email address (from the attribute mapping, or the `NameID` when it is in
    /// email format). `None` if the assertion carried no usable email.
    pub email:           Option<String>,
    /// Resolved display name, if present.
    pub display_name:    Option<String>,
    /// All assertion attributes, keyed by SAML attribute `Name`.
    pub attributes:      HashMap<String, Vec<String>>,
    /// The assertion's `Conditions/NotOnOrAfter`, if present.
    pub not_on_or_after: Option<DateTime<Utc>>,
}

/// Reject a `SAMLResponse` carrying a `DOCTYPE` or entity declaration before any XML
/// parsing.
///
/// A legitimate SAML message never needs a DTD. Refusing one closes off XML eXternal Entity
/// (XXE) and entity-expansion ("billion laughs") attacks regardless of the underlying
/// parser's entity-handling defaults — a defense we own rather than assume.
///
/// # Errors
///
/// [`SamlError::DocTypeForbidden`] if a `<!DOCTYPE` or `<!ENTITY` token is present.
pub fn reject_doctype(xml: &str) -> Result<(), SamlError> {
    let lowered = xml.to_ascii_lowercase();
    if lowered.contains("<!doctype") || lowered.contains("<!entity") {
        return Err(SamlError::DocTypeForbidden);
    }
    Ok(())
}

/// Verify a base64-encoded `SAMLResponse` and extract its verified assertion.
///
/// Steps, all fail-closed:
/// 1. base64-decode and reject any `DOCTYPE`/entity declaration (XXE defense);
/// 2. delegate to `samael`, which verifies the XML signature against the IdP cert using the
///    configured algorithm allow-list, *reduces* the document to the signed bytes (XML Signature
///    Wrapping defense), and validates audience, `Recipient`/`Destination`,
///    `NotBefore`/`NotOnOrAfter`, issuer and `InResponseTo` against `possible_request_ids`;
/// 3. record the assertion `ID` single-use in `replay` and reject a replay.
///
/// `possible_request_ids` are the `AuthnRequest` IDs this SP issued and is still awaiting;
/// an empty slice means no in-flight request matches, which (fail-closed) rejects any
/// `InResponseTo`. `now` is injected for deterministic testing.
///
/// # Errors
///
/// - [`SamlError::Malformed`] — not valid base64 or UTF-8.
/// - [`SamlError::DocTypeForbidden`] — a DTD/entity declaration was present.
/// - [`SamlError::Verification`] — signature/condition/audience/recipient/issuer/ `InResponseTo`
///   validation failed.
/// - [`SamlError::Replay`] — the assertion `ID` was already consumed.
/// - [`SamlError::MissingField`] — a required field (`NameID`) was absent.
pub fn verify_saml_response(
    idp: &SamlIdpConfig,
    response_b64: &str,
    possible_request_ids: &[&str],
    replay: &SamlReplayCache,
    now: DateTime<Utc>,
) -> Result<VerifiedAssertion, SamlError> {
    use base64::Engine as _;

    let raw = base64::engine::general_purpose::STANDARD
        .decode(response_b64.trim())
        .map_err(|e| SamlError::Malformed(format!("base64 decode failed: {e}")))?;
    let xml = std::str::from_utf8(&raw)
        .map_err(|e| SamlError::Malformed(format!("response is not valid UTF-8: {e}")))?;

    // XXE / entity-expansion defense — before the XML ever reaches a parser.
    reject_doctype(xml)?;

    // Signature + condition + audience + recipient + destination + issuer + InResponseTo,
    // via samael's reduce-to-signed path (XSW-safe). Detail is logged, never returned.
    let assertion = idp
        .service_provider()
        .parse_xml_response(xml, Some(possible_request_ids))
        .map_err(|e| SamlError::Verification(e.to_string()))?;

    // Single-use replay protection, keyed on the assertion ID.
    if assertion.id.trim().is_empty() {
        return Err(SamlError::MissingField("assertion ID"));
    }
    let not_on_or_after = assertion.conditions.as_ref().and_then(|c| c.not_on_or_after);
    let replay_expiry = not_on_or_after
        .unwrap_or_else(|| now + chrono::Duration::seconds(FALLBACK_REPLAY_WINDOW_SECS));
    if !replay.check_and_record(&assertion.id, replay_expiry, now) {
        return Err(SamlError::Replay);
    }

    // Subject NameID — the stable per-IdP user key.
    let name_id_subject = assertion
        .subject
        .as_ref()
        .and_then(|s| s.name_id.as_ref())
        .ok_or(SamlError::MissingField("subject NameID"))?;
    let name_id = name_id_subject.value.trim().to_string();
    if name_id.is_empty() {
        return Err(SamlError::MissingField("subject NameID"));
    }
    let name_id_format = name_id_subject.format.clone();

    // Flatten attributes into a name → values map.
    let mut attributes: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(statements) = &assertion.attribute_statements {
        for statement in statements {
            for attribute in &statement.attributes {
                let Some(name) = attribute.name.clone() else {
                    continue;
                };
                let values = attribute.values.iter().filter_map(|v| v.value.clone());
                attributes.entry(name).or_default().extend(values);
            }
        }
    }

    // Resolve email: first present mapped attribute, else the NameID when in email format.
    let email = first_nonempty(&attributes, &idp.attribute_mapping.email)
        .map(str::to_string)
        .or_else(|| {
            (name_id_format.as_deref() == Some(NAMEID_FORMAT_EMAIL)).then(|| name_id.clone())
        });
    let display_name =
        first_nonempty(&attributes, &idp.attribute_mapping.display_name).map(str::to_string);

    Ok(VerifiedAssertion {
        name_id,
        name_id_format,
        email,
        display_name,
        attributes,
        not_on_or_after,
    })
}

/// First non-empty value among `names`, probed in order, in the attribute map.
fn first_nonempty<'a>(
    attributes: &'a HashMap<String, Vec<String>>,
    names: &[String],
) -> Option<&'a str> {
    names.iter().find_map(|name| {
        attributes
            .get(name)
            .and_then(|values| values.iter().map(String::as_str).find(|v| !v.trim().is_empty()))
    })
}
