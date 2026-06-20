//! SAML IdP/SP configuration types.
//!
//! A [`SamlIdpConfig`] wraps a `samael` [`ServiceProvider`] (SP identity + IdP metadata +
//! signature-algorithm allow-list) together with the FraiseQL-side policy knobs:
//! the logical IdP name (used as the `"saml:<idp>"` account-store provider key), an
//! optional tenant binding, the [`SamlAttributeMapping`], and the opt-in
//! `trust_asserted_email` flag (see [`super::effective_saml_email_verified`]).

use base64::Engine as _;
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
    /// The underlying `samael` service provider (SP identity, IdP metadata, allowed algos).
    pub(crate) sp:            ServiceProvider,
}

impl std::fmt::Debug for SamlIdpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SamlIdpConfig")
            .field("idp_name", &self.idp_name)
            .field("tenant_id", &self.tenant_id)
            .field("trust_asserted_email", &self.trust_asserted_email)
            .field("attribute_mapping", &self.attribute_mapping)
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

    /// Borrow the underlying `samael` service provider (used by the verifier and handlers).
    pub(crate) const fn service_provider(&self) -> &ServiceProvider {
        &self.sp
    }
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

        let sp = ServiceProviderBuilder::default()
            .entity_id(Some(self.sp_entity_id))
            .acs_url(Some(self.acs_url))
            .idp_metadata(idp_metadata)
            .allowed_signature_algorithms(Some(default_allowed_algorithms()))
            .allow_idp_initiated(false)
            .build()
            .map_err(|e| SamlError::Config(format!("service provider build failed: {e}")))?;

        Ok(SamlIdpConfig {
            idp_name: self.idp_name,
            tenant_id: self.tenant_id,
            trust_asserted_email: self.trust_asserted_email,
            attribute_mapping: self.attribute_mapping,
            sp,
        })
    }
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
