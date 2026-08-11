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
