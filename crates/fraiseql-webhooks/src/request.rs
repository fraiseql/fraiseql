//! The request a scheme verifies, free of any web framework (#1321).
//!
//! Before this, the receiving route read one header — the name a `&'static str`
//! on the trait — and handed the value in. That shape decided three things the
//! route had no business deciding: that the credential is in a header, that there
//! is exactly one of them, and that a request without it is refused *before* the
//! scheme ever runs.
//!
//! All three are wrong for schemes this seam exists to carry. A Standard Webhooks
//! sender signs `{id}.{timestamp}.{body}` and needs three headers; a JWT-signing
//! IdP puts its credential in the body; and a route that refuses before verifying
//! lets an unauthenticated caller learn things about the endpoint by sending
//! rubbish. So the scheme is handed the request and locates what it needs.

use std::{borrow::Cow, collections::BTreeMap};

use crate::{scheme::CredentialLocation, signature::SignatureError};

/// One inbound HTTP request, as a scheme sees it.
///
/// Headers are matched **case-insensitively**: the name a scheme asks for is its
/// own spelling and the name on the wire is the sender's, and HTTP does not
/// require them to agree.
#[derive(Debug, Clone, Copy)]
pub struct InboundRequest<'a> {
    headers: &'a BTreeMap<String, String>,
    body:    &'a [u8],
    url:     Option<&'a str>,
}

impl<'a> InboundRequest<'a> {
    /// Build a view over a request's headers and raw body.
    ///
    /// `url` is the **configured** signing URL for schemes that cover it (Twilio),
    /// never one reconstructed from request headers — that would put the signed
    /// material under the sender's control (#781).
    #[must_use]
    pub const fn new(
        headers: &'a BTreeMap<String, String>,
        body: &'a [u8],
        url: Option<&'a str>,
    ) -> Self {
        Self { headers, body, url }
    }

    /// The value of a header, matched case-insensitively.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&'a str> {
        // The fast path assumes the caller already lower-cased its keys (the
        // server does — `HeaderName` is lower-case). The scan is the fallback, so
        // that a caller which did not is merely slower rather than silently
        // failing to find a header that is present.
        self.headers.get(&name.to_ascii_lowercase()).map(String::as_str).or_else(|| {
            self.headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        })
    }

    /// The value of a header, or [`SignatureError::MissingCredential`] naming it.
    ///
    /// # Errors
    ///
    /// [`SignatureError::MissingCredential`] when the header is absent.
    pub fn require_header(&self, name: &'static str) -> Result<&'a str, SignatureError> {
        self.header(name).ok_or(SignatureError::MissingCredential(name.to_string()))
    }

    /// The raw request body, exactly as received.
    #[must_use]
    pub const fn body(&self) -> &'a [u8] {
        self.body
    }

    /// The configured signing URL, for schemes whose signature covers it.
    #[must_use]
    pub const fn url(&self) -> Option<&'a str> {
        self.url
    }

    /// Every header, by lower-cased name — for a caller that needs the whole map
    /// rather than one value.
    #[must_use]
    pub const fn headers(&self) -> &'a BTreeMap<String, String> {
        self.headers
    }

    /// Read the credential a scheme declared the location of.
    ///
    /// The shared locator for every scheme family, so the grammar has one
    /// implementation: a MAC in a header, a bare token that *is* the body, and a
    /// token in a body field are three configurations of one question.
    ///
    /// The body arms parse **only** on the arm that needs it — a scheme whose
    /// credential is a header never pays for, nor is refused by, a body that is
    /// not JSON.
    ///
    /// # Errors
    ///
    /// [`SignatureError::MissingCredential`] when the named header or body field
    /// is absent, and [`SignatureError::InvalidFormat`] when the body cannot be
    /// read in the shape the location implies. Both are the sender's fault and
    /// map to 401 — never [`SignatureError::KeyMaterial`], which is the
    /// operator's and maps to 5xx (#1045).
    pub fn credential(
        &self,
        location: &CredentialLocation,
    ) -> Result<Cow<'a, str>, SignatureError> {
        match location {
            CredentialLocation::Header(name) => self
                .header(name)
                .map(Cow::Borrowed)
                .ok_or_else(|| SignatureError::MissingCredential(name.clone())),
            // The whole body is the credential (a bare `application/jwt` token, and
            // whatever else #1322 brings). Trimmed, because a sender that ends its
            // body with a newline has not sent a different token.
            CredentialLocation::Body => std::str::from_utf8(self.body)
                .map(|body| Cow::Borrowed(body.trim()))
                .map_err(|_| SignatureError::InvalidFormat),
            CredentialLocation::BodyField(field) => {
                let body: serde_json::Value =
                    serde_json::from_slice(self.body).map_err(|_| SignatureError::InvalidFormat)?;
                body.get(field.as_str())
                    .and_then(serde_json::Value::as_str)
                    .map(|value| Cow::Owned(value.to_string()))
                    .ok_or_else(|| SignatureError::MissingCredential(format!("body:{field}")))
            },
        }
    }
}
