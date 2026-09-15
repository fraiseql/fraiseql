//! The generic HMAC schemes — the two families no provider owns.
//!
//! `hmac-sha256` and `hmac-sha1` exist for senders that sign the raw body with a
//! shared secret and nothing more: a self-hosted service, a bespoke integration,
//! or a provider whose scheme is exactly this. Since #1321 that "and nothing more"
//! is configuration rather than a constant — where the credential is, how it is
//! encoded, and what literal precedes it — so one sender's `X-Lago-Signature`
//! base64 and another's `X-Hub-Signature-256` `sha256=`-prefixed hex are two
//! configurations of one scheme, not two Rust types.

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use sha2::Sha256;

use crate::{
    scheme::{SchemeConfig, SchemeError, SignatureEncoding, header_from},
    signature::{SignatureError, constant_time_eq},
    traits::SignatureVerifier,
};

/// Where a generic scheme's MAC is and how it is written, shared by both digests.
///
/// Its [`Default`] is the pre-#1321 behaviour — `X-Signature`, hex, no prefix —
/// which is **permissive**, and deliberately so: it is what every route configured
/// before this seam existed already relies on, and changing it would break them
/// silently rather than loudly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericCredential {
    header:   String,
    encoding: SignatureEncoding,
    prefix:   Option<String>,
}

impl Default for GenericCredential {
    fn default() -> Self {
        Self {
            header:   "X-Signature".to_string(),
            encoding: SignatureEncoding::Hex,
            prefix:   None,
        }
    }
}

impl GenericCredential {
    /// Build from a route's scheme keys.
    ///
    /// # Errors
    ///
    /// [`SchemeError::UnsupportedCredential`] if the credential is located
    /// somewhere these schemes cannot read.
    fn from_config(provider: &str, config: &SchemeConfig) -> Result<Self, SchemeError> {
        Ok(Self {
            header:   header_from(provider, config.credential.as_ref())?,
            encoding: config.encoding.unwrap_or(SignatureEncoding::Hex),
            prefix:   config.prefix.clone(),
        })
    }

    /// Decode the sender's credential into the MAC bytes it claims to be.
    ///
    /// Both failures are the sender's: a configured prefix that is absent, and a
    /// value that is not the configured encoding. Both are
    /// [`SignatureError::InvalidFormat`], which maps to 401 — never
    /// [`SignatureError::KeyMaterial`], which maps to 5xx and would hand an
    /// unauthenticated caller a way to produce one (#1045).
    fn decode(&self, signature: &str) -> Result<Vec<u8>, SignatureError> {
        let encoded = match self.prefix.as_deref() {
            Some(prefix) => signature.strip_prefix(prefix).ok_or(SignatureError::InvalidFormat)?,
            None => signature,
        };
        self.encoding.decode(encoded)
    }
}

/// Compare a sender-presented credential against the MAC of the payload.
///
/// The comparison is on **bytes**, after decoding, not on the two encodings' text.
/// Text comparison made the check case-sensitive for a case-insensitive encoding,
/// so a correct hex MAC written in upper case did not verify.
fn verify_hmac<M>(
    credential: &GenericCredential,
    payload: &[u8],
    signature: &str,
    secret: &str,
    empty_secret: &str,
) -> Result<bool, SignatureError>
where
    M: Mac + KeyInit,
{
    if secret.is_empty() {
        return Err(SignatureError::KeyMaterial(empty_secret.to_string()));
    }
    let presented = credential.decode(signature)?;

    let mut mac = M::new_from_slice(secret.as_bytes())
        .map_err(|e| SignatureError::KeyMaterial(e.to_string()))?;
    mac.update(payload);
    let expected = mac.finalize().into_bytes();

    Ok(constant_time_eq(&presented, expected.as_slice()))
}

/// `HMAC-SHA256` over the raw request body, with a configurable credential.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HmacSha256Verifier {
    credential: GenericCredential,
}

impl HmacSha256Verifier {
    /// Build from a route's scheme keys (`credential`, `encoding`, `prefix`).
    ///
    /// # Errors
    ///
    /// [`SchemeError`] when a key names something this scheme cannot honour.
    pub fn from_config(provider: &str, config: &SchemeConfig) -> Result<Self, SchemeError> {
        Ok(Self {
            credential: GenericCredential::from_config(provider, config)?,
        })
    }
}

impl SignatureVerifier for HmacSha256Verifier {
    fn name(&self) -> &'static str {
        "hmac-sha256"
    }

    fn signature_header(&self) -> &str {
        &self.credential.header
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
        _url: Option<&str>,
    ) -> Result<bool, SignatureError> {
        verify_hmac::<Hmac<Sha256>>(
            &self.credential,
            payload,
            signature,
            secret,
            "HMAC-SHA256 secret must not be empty",
        )
    }
}

/// `HMAC-SHA1` over the raw request body, with a configurable credential.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HmacSha1Verifier {
    credential: GenericCredential,
}

impl HmacSha1Verifier {
    /// Build from a route's scheme keys (`credential`, `encoding`, `prefix`).
    ///
    /// # Errors
    ///
    /// [`SchemeError`] when a key names something this scheme cannot honour.
    pub fn from_config(provider: &str, config: &SchemeConfig) -> Result<Self, SchemeError> {
        Ok(Self {
            credential: GenericCredential::from_config(provider, config)?,
        })
    }
}

impl SignatureVerifier for HmacSha1Verifier {
    fn name(&self) -> &'static str {
        "hmac-sha1"
    }

    fn signature_header(&self) -> &str {
        &self.credential.header
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
        _url: Option<&str>,
    ) -> Result<bool, SignatureError> {
        verify_hmac::<Hmac<Sha1>>(
            &self.credential,
            payload,
            signature,
            secret,
            "HMAC-SHA1 secret must not be empty",
        )
    }
}

#[cfg(test)]
mod tests;
