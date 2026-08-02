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
    pub idps: HashMap<String, SamlIdpEntry>,
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
        if self.idps.is_empty() {
            return Err("[saml] is configured but declares no [saml.idps.<name>] — an empty \
                        section is a typo, not a deployment"
                .to_string());
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
