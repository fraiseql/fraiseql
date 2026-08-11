//! `[saml]` server configuration (#381, `auth-saml` feature).
//!
//! Deployment-level SAML `SP` configuration: per-`IdP` metadata, `SP` identity, and
//! the linking-trust knobs. Validated by [`ServerConfig::validate`] and lowered
//! into `fraiseql_auth::saml::SamlIdpConfig` at construction time.
//!
//! [`ServerConfig::validate`]: super::ServerConfig::validate

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The `[saml]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamlServerConfig {
    /// Configured `IdPs` keyed by logical name (`[saml.idps.<name>]`). The name
    /// is the account-store provider key (`saml:<name>`) and must stay
    /// stable across restarts.
    ///
    /// May be empty when [`store_enabled`](Self::store_enabled) is on — a
    /// multi-tenant deployment manages its `IdPs` over the admin API instead.
    #[serde(default)]
    pub idps: HashMap<String, SamlIdpEntry>,

    /// Enable the durable per-tenant `IdP` store (`core.tb_saml_idp`) and the
    /// admin CRUD API that manages it (#947).
    ///
    /// Off by default: a single-tenant deployment needs only `[saml.idps.*]`,
    /// and turning the store on mounts a new privileged surface.
    #[serde(default)]
    pub store_enabled: bool,

    /// How often to reload stored `IdPs`, in seconds.
    ///
    /// Writes made through *this* server's admin API take effect immediately;
    /// this bounds only how long a change made by another replica takes to
    /// propagate. Ignored when [`store_enabled`](Self::store_enabled) is off.
    #[serde(default = "default_refresh_interval_secs")]
    pub refresh_interval_secs: u64,

    /// How many days before a signing certificate expires to start warning.
    ///
    /// A silently expired `IdP` certificate is an outage whose cause is invisible
    /// from the login failure alone.
    #[serde(default = "default_certificate_expiry_warning_days")]
    pub certificate_expiry_warning_days: i64,

    /// This `SP`'s own key pair (#948), applied to **every** `IdP` — config-file and
    /// stored alike.
    ///
    /// Deployment-level rather than per-`IdP` on purpose: the key pair is this
    /// deployment's identity, so one key to rotate and one certificate to publish
    /// beats a private key per `IdP` entry. Configuring it enables decryption of
    /// `EncryptedAssertion`s; signing outbound `AuthnRequest`s additionally needs
    /// [`SamlSpKeyConfig::sign_authn_requests`].
    #[serde(default)]
    pub sp: Option<SamlSpKeyConfig>,
}

/// `SP` key material under `[saml.sp]` (#948).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamlSpKeyConfig {
    /// Environment variable holding the `SP` private key (PEM or DER). Exactly one
    /// of this and [`private_key_path`](Self::private_key_path) must be set.
    ///
    /// A private key does not belong in a config file, so the env form is listed
    /// first; the path form exists for mounted-secret deployments.
    #[serde(default)]
    pub private_key_env: Option<String>,

    /// Path to the `SP` private key file (PEM or DER).
    #[serde(default)]
    pub private_key_path: Option<std::path::PathBuf>,

    /// Path to the `SP` certificate (PEM or DER). Exactly one of this and
    /// [`certificate_pem`](Self::certificate_pem) must be set. The certificate is
    /// public, so either form is fine.
    #[serde(default)]
    pub certificate_path: Option<std::path::PathBuf>,

    /// Inline `SP` certificate PEM.
    #[serde(default)]
    pub certificate_pem: Option<String>,

    /// Sign outbound `AuthnRequest`s. Default `false` — some `IdPs` require signed
    /// requests, but unsigned stays the default so existing deployments are
    /// unaffected. Turning it on without a key pair is refused rather than silently
    /// sending unsigned requests.
    #[serde(default)]
    pub sign_authn_requests: bool,

    /// Environment variable holding the **previous** `SP` private key, accepted for
    /// decryption only during a rotation window.
    ///
    /// An `IdP` picks up new `SP` metadata on its own schedule, so between publishing
    /// a new certificate and the `IdP` adopting it, assertions keep arriving encrypted
    /// to the old key. Retiring it immediately turns that window into an outage.
    #[serde(default)]
    pub previous_private_key_env: Option<String>,

    /// Path to the previous `SP` private key file.
    #[serde(default)]
    pub previous_private_key_path: Option<std::path::PathBuf>,

    /// Path to the previous `SP` certificate.
    #[serde(default)]
    pub previous_certificate_path: Option<std::path::PathBuf>,

    /// Inline previous `SP` certificate PEM.
    #[serde(default)]
    pub previous_certificate_pem: Option<String>,
}

impl SamlSpKeyConfig {
    /// Validate the shape: exactly one source per artifact, and no half-configured
    /// rotation window.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending field.
    pub fn validate(&self) -> Result<(), String> {
        exactly_one(
            self.private_key_env.is_some(),
            self.private_key_path.is_some(),
            "[saml.sp]",
            "private_key_env",
            "private_key_path",
        )?;
        exactly_one(
            self.certificate_path.is_some(),
            self.certificate_pem.is_some(),
            "[saml.sp]",
            "certificate_path",
            "certificate_pem",
        )?;

        let has_previous_key =
            self.previous_private_key_env.is_some() || self.previous_private_key_path.is_some();
        let has_previous_cert =
            self.previous_certificate_path.is_some() || self.previous_certificate_pem.is_some();
        if has_previous_key || has_previous_cert {
            exactly_one(
                self.previous_private_key_env.is_some(),
                self.previous_private_key_path.is_some(),
                "[saml.sp]",
                "previous_private_key_env",
                "previous_private_key_path",
            )?;
            exactly_one(
                self.previous_certificate_path.is_some(),
                self.previous_certificate_pem.is_some(),
                "[saml.sp]",
                "previous_certificate_path",
                "previous_certificate_pem",
            )?;
        }
        Ok(())
    }
}

/// Refuse both-or-neither for a pair of mutually exclusive config sources.
fn exactly_one(a: bool, b: bool, section: &str, name_a: &str, name_b: &str) -> Result<(), String> {
    match (a, b) {
        (true, false) | (false, true) => Ok(()),
        (false, false) => {
            Err(format!("{section} needs exactly one of {name_a} / {name_b} — neither is set"))
        },
        (true, true) => {
            Err(format!("{section} needs exactly one of {name_a} / {name_b} — both are set"))
        },
    }
}

const fn default_refresh_interval_secs() -> u64 {
    30
}

const fn default_certificate_expiry_warning_days() -> i64 {
    30
}

/// One `IdP` under `[saml.idps.<name>]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamlIdpEntry {
    /// `SP` entity ID (the audience the `IdP` asserts to).
    pub sp_entity_id: String,

    /// Assertion Consumer Service URL — must be the externally reachable URL
    /// of `POST /auth/saml/acs` on this server.
    pub acs_url: String,

    /// Path to the `IdP` metadata XML file. Exactly one of this and
    /// [`metadata_xml`](Self::metadata_xml) must be set.
    #[serde(default)]
    pub metadata_xml_path: Option<std::path::PathBuf>,

    /// Inline `IdP` metadata XML. Exactly one of this and
    /// [`metadata_xml_path`](Self::metadata_xml_path) must be set.
    #[serde(default)]
    pub metadata_xml: Option<String>,

    /// Tenant this `IdP` provisions for (`None` = single-tenant deployment).
    /// Load-bearing for the email-linking fail-closed policy: a tenant-bound
    /// `IdP` never email-merges through the v1 global account store.
    #[serde(default)]
    pub tenant_id: Option<String>,

    /// Whether a verified assertion's email may auto-link to an existing
    /// account with the same email. Default `false` (fail-closed).
    #[serde(default)]
    pub trust_asserted_email: bool,
}

impl SamlServerConfig {
    /// Validate the section shape. Called by `ServerConfig::validate`.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending `IdP` and field.
    pub fn validate(&self) -> Result<(), String> {
        if self.idps.is_empty() && !self.store_enabled {
            return Err("[saml] is configured but declares no [saml.idps.<name>] and has \
                        store_enabled = false — an empty section is a typo, not a deployment"
                .to_string());
        }
        if self.store_enabled && self.refresh_interval_secs == 0 {
            return Err("[saml] refresh_interval_secs must be greater than zero — a zero \
                        interval would spin the refresher on the database"
                .to_string());
        }
        if self.certificate_expiry_warning_days < 0 {
            return Err(
                "[saml] certificate_expiry_warning_days must not be negative — a negative \
                 horizon warns about nothing until SSO is already down"
                    .to_string(),
            );
        }
        if let Some(sp) = &self.sp {
            sp.validate()?;
        }
        for (name, idp) in &self.idps {
            match (&idp.metadata_xml, &idp.metadata_xml_path) {
                (None, None) => {
                    return Err(format!(
                        "[saml.idps.{name}] needs exactly one of metadata_xml / \
                         metadata_xml_path — neither is set"
                    ));
                },
                (Some(_), Some(_)) => {
                    return Err(format!(
                        "[saml.idps.{name}] needs exactly one of metadata_xml / \
                         metadata_xml_path — both are set"
                    ));
                },
                (None, Some(path)) if !path.exists() => {
                    return Err(format!(
                        "[saml.idps.{name}] metadata_xml_path not found: {}",
                        path.display()
                    ));
                },
                _ => {},
            }
            if idp.sp_entity_id.trim().is_empty() {
                return Err(format!("[saml.idps.{name}] sp_entity_id must not be empty"));
            }
            if idp.acs_url.trim().is_empty() {
                return Err(format!("[saml.idps.{name}] acs_url must not be empty"));
            }
        }
        Ok(())
    }
}
