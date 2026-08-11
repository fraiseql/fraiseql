//! SAML assertion verification — the security core (#381).
//!
//! [`verify_saml_response`] decodes, defends against XXE, delegates signature/condition
//! validation to `samael` (which reduces the document to the signed bytes — XSW defense),
//! enforces single-use replay protection, and extracts a [`VerifiedAssertion`].

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::{SamlError, SamlIdpConfig, replay::SamlReplayStore};

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
pub async fn verify_saml_response(
    idp: &SamlIdpConfig,
    response_b64: &str,
    possible_request_ids: &[&str],
    replay: &dyn SamlReplayStore,
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

    // An EncryptedAssertion is only ever as trustworthy as the signature over its
    // ciphertext — see `check_encryption_algorithms`. Run the algorithm gate first, on
    // verified bytes, so a weak cipher is refused before any decryption happens.
    check_encryption_algorithms(idp, xml)?;

    // Signature + condition + audience + recipient + destination + issuer + InResponseTo,
    // via samael's reduce-to-signed path (XSW-safe). Detail is logged, never returned.
    let assertion = parse_with_key_rotation(idp, xml, possible_request_ids)?;

    // Single-use replay protection, keyed on the assertion ID.
    if assertion.id.trim().is_empty() {
        return Err(SamlError::MissingField("assertion ID"));
    }
    let not_on_or_after = assertion.conditions.as_ref().and_then(|c| c.not_on_or_after);
    let replay_expiry = not_on_or_after
        .unwrap_or_else(|| now + chrono::Duration::seconds(FALLBACK_REPLAY_WINDOW_SECS));
    if !replay.check_and_record(&assertion.id, replay_expiry, now).await? {
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

/// Refuse an `EncryptedAssertion` encrypted with an algorithm outside the allow-list (#948).
///
/// # Why the check runs on *reduced* bytes
///
/// Reading algorithms off the raw document would mean trusting attacker-controlled XML — the
/// very thing the reduce-to-signed discipline exists to avoid. So the document is reduced to
/// the signature-verified bytes first, using the IdP's own signature-algorithm allow-list,
/// and the encryption methods are read from *that*. The reduce is repeated work
/// (`parse_xml_response` will do it again), but it happens only on the encrypted path, and
/// this function can only ever *refuse* — never admit something the main path would reject.
///
/// A response with no `EncryptedAssertion`, or one whose signature does not verify, is
/// passed through untouched for the main path to reject with its own error.
///
/// # Errors
///
/// [`SamlError::Verification`] when a key-transport or content-encryption algorithm is not
/// on the allow-list.
fn check_encryption_algorithms(idp: &SamlIdpConfig, xml: &str) -> Result<(), SamlError> {
    use samael::crypto::{Crypto, CryptoProvider as _, ReduceMode};

    // Cheap pre-filter: no ciphertext, nothing to gate.
    if !xml.contains("EncryptedAssertion") {
        return Ok(());
    }
    let sp = idp.service_provider();
    let Ok(Some(certs)) = sp.idp_signing_certs() else {
        return Ok(());
    };
    let Ok(reduced) = Crypto::reduce_xml_to_signed_with_allowed_algorithms(
        xml,
        &certs,
        ReduceMode::default(),
        sp.allowed_signature_algorithms.as_deref(),
    ) else {
        // Signature did not verify; the main path rejects it. Refusing here too would only
        // change which error is reported.
        return Ok(());
    };
    let Ok(response) = reduced.parse::<samael::schema::Response>() else {
        return Ok(());
    };
    let Some(encrypted) = response.encrypted_assertion.as_ref() else {
        return Ok(());
    };

    if let Some((_, method)) = encrypted.encrypted_key_info() {
        if !super::config::key_transport_algorithm_allowed(method) {
            return Err(SamlError::Verification(format!(
                "encrypted assertion uses a refused key-transport algorithm: {method}"
            )));
        }
    }
    if let Some((_, method)) = encrypted.encrypted_value_info() {
        if !super::config::content_encryption_algorithm_allowed(method) {
            return Err(SamlError::Verification(format!(
                "encrypted assertion uses a refused content-encryption algorithm: {method}"
            )));
        }
    }
    Ok(())
}

/// Parse and validate the response, retrying once with the previous SP key if decryption
/// failed during a key-rotation window (#948).
///
/// The retry is deliberately narrow: only *decryption-class* failures are retried. A
/// signature, audience, condition or `InResponseTo` failure is final and must never get a
/// second evaluation against a different key — that would turn key rotation into a second,
/// weaker path through the same gate.
fn parse_with_key_rotation(
    idp: &SamlIdpConfig,
    xml: &str,
    possible_request_ids: &[&str],
) -> Result<samael::schema::Assertion, SamlError> {
    let first = match idp.service_provider().parse_xml_response(xml, Some(possible_request_ids)) {
        Ok(assertion) => return Ok(assertion),
        Err(e) => e,
    };
    if !is_decryption_failure(&first) {
        return Err(SamlError::Verification(first.to_string()));
    }
    let Some(previous_sp) = idp.service_provider_with_previous_key() else {
        return Err(SamlError::Verification(first.to_string()));
    };
    previous_sp
        .parse_xml_response(xml, Some(possible_request_ids))
        .map_err(|e| SamlError::Verification(format!("{first}; previous SP key also failed: {e}")))
}

/// Whether a `samael` error means "this ciphertext did not open with the key we tried",
/// as opposed to any failure of the security checks.
const fn is_decryption_failure(e: &samael::service_provider::Error) -> bool {
    use samael::service_provider::Error;
    matches!(
        e,
        Error::FailedToDecryptAssertion
            | Error::EncryptedAssertionInvalid
            | Error::CryptoProviderError(_)
    )
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
