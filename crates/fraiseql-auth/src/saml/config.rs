//! SAML IdP/SP configuration types.
//!
//! A [`SamlIdpConfig`] wraps a `samael` [`ServiceProvider`] (SP identity + IdP metadata +
//! signature-algorithm allow-list) together with the FraiseQL-side policy knobs:
//! the logical IdP name (used as the `"saml:<idp>"` account-store provider key), an
//! optional tenant binding, the [`SamlAttributeMapping`], and the opt-in
//! `trust_asserted_email` flag (see [`super::effective_saml_email_verified`]).

use base64::Engine as _;
use chrono::{DateTime, Utc};
use samael::{
    crypto::AllowedSignatureAlgorithm,
    metadata::EntityDescriptor,
    service_provider::{ServiceProvider, ServiceProviderBuilder},
};

use super::SamlError;

/// SAML attribute names probed (in order) for the user's email address. Covers the LDAP
/// OID form (`urn:oid:0.9.2342.19200300.100.1.3` = `mail`), the WS-* claim URI emitted by
/// Azure AD / ADFS, and common friendly names.
const DEFAULT_EMAIL_ATTRS: &[&str] = &[
    "urn:oid:0.9.2342.19200300.100.1.3",
    "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress",
    "email",
    "mail",
    "emailAddress",
];

/// SAML attribute names probed (in order) for the user's display name.
const DEFAULT_NAME_ATTRS: &[&str] = &[
    "urn:oid:2.16.840.1.113730.3.1.241",
    "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/name",
    "displayName",
    "name",
    "cn",
];

/// The signature algorithms FraiseQL accepts on a SAML assertion. Restricting the set
/// blocks signature-algorithm substitution/downgrade attacks. SHA-1 based algorithms are
/// deliberately excluded.
fn default_allowed_algorithms() -> Vec<AllowedSignatureAlgorithm> {
    vec![
        AllowedSignatureAlgorithm::RsaSha256,
        AllowedSignatureAlgorithm::RsaSha384,
        AllowedSignatureAlgorithm::RsaSha512,
        AllowedSignatureAlgorithm::EcdsaSha256,
        AllowedSignatureAlgorithm::EcdsaSha384,
        AllowedSignatureAlgorithm::EcdsaSha512,
    ]
}

/// Mapping from SAML assertion attribute names to FraiseQL identity fields. Each field is a
/// priority-ordered list of attribute names; the first present, non-empty value wins.
#[derive(Debug, Clone)]
pub struct SamlAttributeMapping {
    /// Attribute names to probe for the email address.
    pub email:        Vec<String>,
    /// Attribute names to probe for the display name.
    pub display_name: Vec<String>,
}

impl Default for SamlAttributeMapping {
    fn default() -> Self {
        Self {
            email:        DEFAULT_EMAIL_ATTRS.iter().map(|s| (*s).to_string()).collect(),
            display_name: DEFAULT_NAME_ATTRS.iter().map(|s| (*s).to_string()).collect(),
        }
    }
}

/// XML-Encryption key-transport algorithms FraiseQL accepts on an `EncryptedAssertion`
/// (#948).
///
/// RSA-OAEP only. `rsa-1_5` (PKCS#1 v1.5) is deliberately excluded: it is the algorithm
/// behind the Bleichenbacher-style adaptive chosen-ciphertext break of XML Encryption, and
/// XML Encryption 1.1 removed it. Our outer-signature requirement already denies an
/// attacker the chosen-ciphertext oracle those attacks need, so this is defence in depth —
/// but an IdP configured for `rsa-1_5` is a misconfiguration worth refusing loudly rather
/// than accepting quietly.
const ALLOWED_KEY_TRANSPORT_ALGORITHMS: &[&str] = &[
    "http://www.w3.org/2001/04/xmlenc#rsa-oaep-mgf1p",
    "http://www.w3.org/2009/xmlenc11#rsa-oaep",
];

/// XML-Encryption content-encryption algorithms FraiseQL accepts (#948).
///
/// AES-GCM only — an authenticated cipher. The CBC modes are excluded for the same reason:
/// unauthenticated CBC is what makes the XML-Encryption padding-oracle attack possible.
const ALLOWED_CONTENT_ENCRYPTION_ALGORITHMS: &[&str] = &[
    "http://www.w3.org/2009/xmlenc11#aes128-gcm",
    "http://www.w3.org/2009/xmlenc11#aes192-gcm",
    "http://www.w3.org/2009/xmlenc11#aes256-gcm",
];

/// Whether `algorithm` is an accepted key-transport algorithm.
#[must_use]
pub fn key_transport_algorithm_allowed(algorithm: &str) -> bool {
    ALLOWED_KEY_TRANSPORT_ALGORITHMS.contains(&algorithm)
}

/// Whether `algorithm` is an accepted content-encryption algorithm.
#[must_use]
pub fn content_encryption_algorithm_allowed(algorithm: &str) -> bool {
    ALLOWED_CONTENT_ENCRYPTION_ALGORITHMS.contains(&algorithm)
}

/// An SP key pair: the private key plus the certificate published in SP metadata.
struct SpKeyPair {
    key:      openssl::pkey::PKey<openssl::pkey::Private>,
    cert_der: Vec<u8>,
}

impl std::fmt::Debug for SpKeyPair {
    /// Hand-written: a derived `Debug` on a struct holding a private key would print key
    /// material into any log line that formats the value.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpKeyPair").finish_non_exhaustive()
    }
}

/// Parse a private key in either PEM or DER form.
fn parse_private_key(
    bytes: &[u8],
) -> Result<openssl::pkey::PKey<openssl::pkey::Private>, SamlError> {
    openssl::pkey::PKey::private_key_from_pem(bytes)
        .or_else(|_| openssl::pkey::PKey::private_key_from_der(bytes))
        .map_err(|e| SamlError::Config(format!("SP private key is neither valid PEM nor DER: {e}")))
}

/// Parse a certificate in either PEM or DER form and return its DER encoding.
fn parse_certificate_der(bytes: &[u8]) -> Result<Vec<u8>, SamlError> {
    let cert = openssl::x509::X509::from_pem(bytes)
        .or_else(|_| openssl::x509::X509::from_der(bytes))
        .map_err(|e| {
            SamlError::Config(format!("SP certificate is neither valid PEM nor DER: {e}"))
        })?;
    cert.to_der()
        .map_err(|e| SamlError::Config(format!("SP certificate could not be re-encoded: {e}")))
}

/// Per-IdP SAML configuration: the `samael` service provider plus FraiseQL policy.
pub struct SamlIdpConfig {
    /// Logical IdP name. Used as the account-store provider key `"saml:<idp_name>"` and in
    /// audit logs. Must be stable for an IdP across restarts.
    pub idp_name:             String,
    /// Tenant this IdP provisions for. `None` = single-tenant deployment.
    ///
    /// Load-bearing for [`super::effective_saml_email_verified`]: when set, the v1
    /// global-email account store cannot bound an email merge to this tenant, so email
    /// auto-linking fails closed even if `trust_asserted_email` is on.
    pub tenant_id:            Option<String>,
    /// Whether a verified assertion's email may be used as a cross-provider auto-linking
    /// key. Default `false` (fail-closed). See [`super::effective_saml_email_verified`].
    pub trust_asserted_email: bool,
    /// Attribute → identity-field mapping.
    pub attribute_mapping:    SamlAttributeMapping,
    /// Whether to sign outbound `AuthnRequest`s. Requires an SP key pair; unsigned is the
    /// default, so existing deployments are unaffected (#948).
    pub sign_authn_requests:  bool,
    /// The underlying `samael` service provider (SP identity, IdP metadata, allowed algos).
    /// Carries the primary SP key pair in `sp.key` / `sp.certificate`.
    pub(crate) sp:            ServiceProvider,
    /// The SP certificate in DER, mirrored here for metadata publishing (#948).
    sp_certificate_der:       Option<Vec<u8>>,
    /// The previous SP key pair, accepted for **decryption only** during a rotation window
    /// (#948). Never used to sign: an SP signs with exactly one current key.
    sp_previous:              Option<SpKeyPair>,
}

impl std::fmt::Debug for SamlIdpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SamlIdpConfig")
            .field("idp_name", &self.idp_name)
            .field("tenant_id", &self.tenant_id)
            .field("trust_asserted_email", &self.trust_asserted_email)
            .field("attribute_mapping", &self.attribute_mapping)
            .field("sign_authn_requests", &self.sign_authn_requests)
            .field("has_sp_key", &self.sp.key.is_some())
            .field("has_previous_sp_key", &self.sp_previous.is_some())
            .finish_non_exhaustive()
    }
}

impl SamlIdpConfig {
    /// Start building a config for an IdP named `idp_name`, with this SP's `entity_id` and
    /// `acs_url` (the absolute URL the IdP POSTs the `SAMLResponse` back to).
    #[must_use]
    pub fn builder(
        idp_name: impl Into<String>,
        sp_entity_id: impl Into<String>,
        acs_url: impl Into<String>,
    ) -> SamlIdpConfigBuilder {
        SamlIdpConfigBuilder {
            idp_name:             idp_name.into(),
            sp_entity_id:         sp_entity_id.into(),
            acs_url:              acs_url.into(),
            idp_metadata:         None,
            tenant_id:            None,
            trust_asserted_email: false,
            attribute_mapping:    SamlAttributeMapping::default(),
            sp_key_pair:          None,
            sp_previous_key_pair: None,
            sign_authn_requests:  false,
        }
    }

    /// The account-store provider key for this IdP: `"saml:<idp_name>"`.
    #[must_use]
    pub fn provider_key(&self) -> String {
        super::saml_provider_key(&self.idp_name)
    }

    /// The IdP's HTTP-Redirect Single-Sign-On URL, taken from its metadata. `None` if the
    /// IdP metadata advertises no redirect-binding SSO endpoint.
    #[must_use]
    pub fn sso_redirect_url(&self) -> Option<String> {
        self.sp.sso_binding_location(samael::metadata::HTTP_REDIRECT_BINDING)
    }

    /// The IdP's entity ID, as declared by its metadata. Empty only for metadata that
    /// [`SamlIdpConfigBuilder::build`] would already have refused.
    #[must_use]
    pub fn idp_entity_id(&self) -> &str {
        self.sp.idp_metadata.entity_id.as_deref().unwrap_or_default()
    }

    /// Earliest `NotAfter` among the IdP's signing certificates.
    ///
    /// A silently expired IdP certificate is an outage whose cause is invisible from the
    /// login failure alone (#947), so the expiry is read once here and surfaced — by the
    /// registry's periodic warning and by the admin API — rather than discovered by an
    /// operator when SSO stops. `None` when the metadata carries no parseable certificate;
    /// the *earliest* is taken because the first to expire is the one that breaks SSO.
    #[must_use]
    pub fn signing_certificate_expiry(&self) -> Option<DateTime<Utc>> {
        let certs = self.sp.idp_signing_certs().ok().flatten()?;
        certs.iter().filter_map(|cert| certificate_not_after(cert.der_data())).min()
    }

    /// Whether an SP key pair is configured (enabling request signing and assertion
    /// decryption).
    #[must_use]
    pub const fn has_sp_key(&self) -> bool {
        self.sp.key.is_some()
    }

    /// Whether outbound `AuthnRequest`s are signed. Requires [`has_sp_key`](Self::has_sp_key).
    #[must_use]
    pub const fn signs_authn_requests(&self) -> bool {
        self.sign_authn_requests && self.sp.key.is_some()
    }

    /// The SP's own metadata document, for an IdP to consume (#948).
    ///
    /// Hand-built rather than taken from `samael`'s `ServiceProvider::metadata`, which
    /// labels its encryption `KeyDescriptor` `use="signing"`, hard-codes
    /// `AuthnRequestsSigned="false"`, requires an SLO endpoint this SP does not offer, and
    /// has no way to publish a second certificate — all four matter here, and the document
    /// is small enough to own.
    ///
    /// During a rotation window the previous certificate is published as an additional
    /// `use="encryption"` descriptor, so an IdP that has not yet picked up the new one keeps
    /// encrypting to a key we can still read. Only the *current* certificate is advertised
    /// for signing: an SP signs with exactly one key.
    #[must_use]
    pub fn sp_metadata_xml(&self) -> String {
        use std::fmt::Write as _;

        let entity_id = xml_escape(self.sp.entity_id.as_deref().unwrap_or_default());
        let acs_url = xml_escape(self.sp.acs_url.as_deref().unwrap_or_default());
        let signed = if self.signs_authn_requests() {
            "true"
        } else {
            "false"
        };

        let mut descriptors = String::new();
        if let Some(der) = self.sp_certificate_der.as_deref() {
            let _ = write!(descriptors, "{}", key_descriptor("signing", der));
            let _ = write!(descriptors, "{}", key_descriptor("encryption", der));
        }
        if let Some(previous) = &self.sp_previous {
            let _ = write!(descriptors, "{}", key_descriptor("encryption", &previous.cert_der));
        }

        format!(
            r#"<EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata" entityID="{entity_id}">
  <SPSSODescriptor protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol" AuthnRequestsSigned="{signed}" WantAssertionsSigned="true">
{descriptors}    <AssertionConsumerService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST" Location="{acs_url}" index="0" isDefault="true"/>
  </SPSSODescriptor>
</EntityDescriptor>"#
        )
    }

    /// The SP private key used to sign an `AuthnRequest`, if signing is enabled.
    pub(crate) const fn signing_key(&self) -> Option<&openssl::pkey::PKey<openssl::pkey::Private>> {
        if self.sign_authn_requests {
            self.sp.key.as_ref()
        } else {
            None
        }
    }

    /// A service provider carrying the *previous* SP key, for one retry of a failed
    /// decryption during a rotation window. `None` when no previous key is configured.
    pub(crate) fn service_provider_with_previous_key(&self) -> Option<ServiceProvider> {
        let previous = self.sp_previous.as_ref()?;
        let mut sp = self.sp.clone();
        sp.key = Some(previous.key.clone());
        Some(sp)
    }

    /// Borrow the underlying `samael` service provider (used by the verifier and handlers).
    pub(crate) const fn service_provider(&self) -> &ServiceProvider {
        &self.sp
    }
}

/// One `<KeyDescriptor use="…">` carrying a DER certificate.
fn key_descriptor(key_use: &str, cert_der: &[u8]) -> String {
    let cert_b64 = base64::engine::general_purpose::STANDARD.encode(cert_der);
    format!(
        "    <KeyDescriptor use=\"{key_use}\">\n      \
         <KeyInfo xmlns=\"http://www.w3.org/2000/09/xmldsig#\">\n        \
         <X509Data><X509Certificate>{cert_b64}</X509Certificate></X509Data>\n      \
         </KeyInfo>\n    </KeyDescriptor>\n"
    )
}

/// Escape the five XML predefined entities. Applied to the operator-supplied entity ID and
/// ACS URL so a stray `&` cannot produce a malformed metadata document.
fn xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Read a DER-encoded X.509 certificate's `NotAfter` as a UTC instant.
///
/// openssl exposes `notAfter` only as an ASN.1 time, whose textual forms (UTCTime vs
/// GeneralizedTime, `Z` vs offset) are a parsing trap; `Asn1Time::diff` against "now" is the
/// library's own comparison path, so the conversion inherits its handling of every form.
fn certificate_not_after(der: &[u8]) -> Option<DateTime<Utc>> {
    let cert = openssl::x509::X509::from_der(der).ok()?;
    let now = openssl::asn1::Asn1Time::days_from_now(0).ok()?;
    let diff = now.diff(cert.not_after()).ok()?;
    Some(
        Utc::now()
            + chrono::Duration::days(i64::from(diff.days))
            + chrono::Duration::seconds(i64::from(diff.secs)),
    )
}

/// Builder for [`SamlIdpConfig`]. Obtain via [`SamlIdpConfig::builder`].
#[derive(Debug)]
pub struct SamlIdpConfigBuilder {
    idp_name:             String,
    sp_entity_id:         String,
    acs_url:              String,
    idp_metadata:         Option<EntityDescriptor>,
    tenant_id:            Option<String>,
    trust_asserted_email: bool,
    attribute_mapping:    SamlAttributeMapping,
    sp_key_pair:          Option<SpKeyPair>,
    sp_previous_key_pair: Option<SpKeyPair>,
    sign_authn_requests:  bool,
}

impl SamlIdpConfigBuilder {
    /// Supply the IdP's SAML metadata as XML (the realistic config path — every IdP
    /// publishes an `EntityDescriptor`). The signing certificate and SSO endpoints are read
    /// from it.
    ///
    /// # Errors
    ///
    /// [`SamlError::Config`] if the metadata XML cannot be parsed.
    pub fn idp_metadata_xml(mut self, xml: &str) -> Result<Self, SamlError> {
        let descriptor: EntityDescriptor = xml
            .parse()
            .map_err(|e| SamlError::Config(format!("invalid IdP metadata XML: {e}")))?;
        self.idp_metadata = Some(descriptor);
        Ok(self)
    }

    /// Supply the IdP from explicit parts: its `entity_id`, HTTP-Redirect SSO URL, and the
    /// DER-encoded signing certificate. A minimal `EntityDescriptor` is synthesized.
    ///
    /// # Errors
    ///
    /// [`SamlError::Config`] if the synthesized metadata cannot be parsed.
    pub fn idp_parts(
        self,
        idp_entity_id: &str,
        sso_redirect_url: &str,
        signing_cert_der: &[u8],
    ) -> Result<Self, SamlError> {
        let xml = idp_metadata_xml_from_parts(idp_entity_id, sso_redirect_url, signing_cert_der);
        self.idp_metadata_xml(&xml)
    }

    /// Bind this IdP to a tenant. See [`SamlIdpConfig::tenant_id`].
    #[must_use]
    pub fn tenant_id(mut self, tenant_id: Option<String>) -> Self {
        self.tenant_id = tenant_id;
        self
    }

    /// Opt in to using a verified assertion's email as a cross-provider auto-linking key
    /// (default `false`). Honored only subject to the tenant-bounding rule in
    /// [`super::effective_saml_email_verified`].
    #[must_use]
    pub const fn trust_asserted_email(mut self, trust: bool) -> Self {
        self.trust_asserted_email = trust;
        self
    }

    /// Override the attribute → identity-field mapping.
    #[must_use]
    pub fn attribute_mapping(mut self, mapping: SamlAttributeMapping) -> Self {
        self.attribute_mapping = mapping;
        self
    }

    /// Configure the SP key pair used to sign `AuthnRequest`s and decrypt
    /// `EncryptedAssertion`s (#948). Both inputs accept PEM or DER.
    ///
    /// Configuring a key does **not** by itself start signing — call
    /// [`sign_authn_requests`](Self::sign_authn_requests) for that. It does enable
    /// decryption, because an `EncryptedAssertion` is otherwise simply unreadable.
    ///
    /// # Errors
    ///
    /// [`SamlError::Config`] if either input parses as neither PEM nor DER, or if the key
    /// and certificate do not belong together — a mismatched pair publishes a certificate
    /// no IdP can encrypt to and produces signatures no IdP can verify, and it fails at the
    /// first login rather than at configuration time.
    pub fn sp_key_pair(
        mut self,
        private_key: &[u8],
        certificate: &[u8],
    ) -> Result<Self, SamlError> {
        self.sp_key_pair = Some(build_key_pair(private_key, certificate, "sp_key_pair")?);
        Ok(self)
    }

    /// Configure the *previous* SP key pair, accepted for decryption only, during a
    /// rotation window (#948).
    ///
    /// An IdP picks up new SP metadata on its own schedule, so between publishing a new
    /// certificate and the IdP adopting it, assertions keep arriving encrypted to the old
    /// key. Retiring the old key immediately turns that window into an outage.
    ///
    /// # Errors
    ///
    /// As [`sp_key_pair`](Self::sp_key_pair).
    pub fn sp_previous_key_pair(
        mut self,
        private_key: &[u8],
        certificate: &[u8],
    ) -> Result<Self, SamlError> {
        self.sp_previous_key_pair =
            Some(build_key_pair(private_key, certificate, "sp_previous_key_pair")?);
        Ok(self)
    }

    /// Sign outbound `AuthnRequest`s (default `false`).
    ///
    /// Some IdPs reject an unsigned `AuthnRequest` outright. Unsigned stays the default so
    /// existing deployments are unaffected; [`build`](Self::build) refuses the combination
    /// of signing with no key rather than silently sending unsigned requests.
    #[must_use]
    pub const fn sign_authn_requests(mut self, sign: bool) -> Self {
        self.sign_authn_requests = sign;
        self
    }

    /// Finalize the configuration.
    ///
    /// # Errors
    ///
    /// [`SamlError::Config`] if no IdP metadata was supplied or the service provider could
    /// not be constructed.
    pub fn build(self) -> Result<SamlIdpConfig, SamlError> {
        let idp_metadata = self
            .idp_metadata
            .ok_or_else(|| SamlError::Config("IdP metadata not supplied".to_string()))?;

        if self.sign_authn_requests && self.sp_key_pair.is_none() {
            return Err(SamlError::Config(
                "sign_authn_requests is on but no SP key pair is configured — an IdP that \
                 requires signed AuthnRequests would reject every login, and sending them \
                 unsigned instead would be a silent downgrade"
                    .to_string(),
            ));
        }

        let sp_certificate_der = self.sp_key_pair.as_ref().map(|kp| kp.cert_der.clone());
        let mut builder = ServiceProviderBuilder::default();
        builder
            .entity_id(Some(self.sp_entity_id))
            .acs_url(Some(self.acs_url))
            .idp_metadata(idp_metadata)
            .allowed_signature_algorithms(Some(default_allowed_algorithms()))
            .allow_idp_initiated(false);
        if let Some(key_pair) = self.sp_key_pair {
            builder.key(Some(key_pair.key));
        }
        let sp = builder
            .build()
            .map_err(|e| SamlError::Config(format!("service provider build failed: {e}")))?;

        let config = SamlIdpConfig {
            idp_name: self.idp_name,
            tenant_id: self.tenant_id,
            trust_asserted_email: self.trust_asserted_email,
            attribute_mapping: self.attribute_mapping,
            sign_authn_requests: self.sign_authn_requests,
            sp,
            sp_certificate_der,
            sp_previous: self.sp_previous_key_pair,
        };
        // Refuse metadata that can never start a login: samael's metadata parse
        // is lenient (an arbitrary XML document deserializes to an empty
        // EntityDescriptor), so without this check a typo'd metadata file
        // builds an IdP whose /auth/saml/login can only ever 500 — a
        // configured-but-broken shape that must fail at build time instead.
        if config.sso_redirect_url().is_none() {
            return Err(SamlError::Config(
                "IdP metadata declares no HTTP-Redirect SingleSignOnService binding — \
                 SP-initiated login would be impossible. Check the metadata XML."
                    .to_string(),
            ));
        }
        Ok(config)
    }
}

/// Parse and pair an SP private key with its certificate, refusing a mismatched pair.
fn build_key_pair(
    private_key: &[u8],
    certificate: &[u8],
    field: &str,
) -> Result<SpKeyPair, SamlError> {
    let key = parse_private_key(private_key)?;
    let cert_der = parse_certificate_der(certificate)?;
    let cert = openssl::x509::X509::from_der(&cert_der)
        .map_err(|e| SamlError::Config(format!("SP certificate could not be re-read: {e}")))?;
    // A pair that does not match is a configuration error that would otherwise surface as
    // "the IdP rejects our signature" or "we cannot decrypt", long after deployment.
    if !cert.public_key().is_ok_and(|public| public.public_eq(&key)) {
        return Err(SamlError::Config(format!(
            "{field}: the SP certificate does not match the SP private key"
        )));
    }
    Ok(SpKeyPair { key, cert_der })
}

/// Synthesize a minimal IdP `EntityDescriptor` XML from explicit parts, used by
/// [`SamlIdpConfigBuilder::idp_parts`].
fn idp_metadata_xml_from_parts(
    idp_entity_id: &str,
    sso_redirect_url: &str,
    signing_cert_der: &[u8],
) -> String {
    let cert_b64 = base64::engine::general_purpose::STANDARD.encode(signing_cert_der);
    format!(
        r#"<EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata" entityID="{idp_entity_id}">
  <IDPSSODescriptor protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol">
    <KeyDescriptor use="signing">
      <KeyInfo xmlns="http://www.w3.org/2000/09/xmldsig#">
        <X509Data><X509Certificate>{cert_b64}</X509Certificate></X509Data>
      </KeyInfo>
    </KeyDescriptor>
    <SingleSignOnService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect" Location="{sso_redirect_url}"/>
  </IDPSSODescriptor>
</EntityDescriptor>"#
    )
}
