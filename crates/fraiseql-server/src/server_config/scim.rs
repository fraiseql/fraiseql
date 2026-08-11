//! `[scim]` server configuration (#946).
//!
//! SCIM 2.0 provisioning: `/scim/v2/Users`, `/scim/v2/Groups` and the discovery documents,
//! plus the admin endpoints that mint the provisioning credentials.

use serde::{Deserialize, Serialize};

/// The `[scim]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScimServerConfig {
    /// Mount the SCIM surface. Off by default: it is a privileged provisioning API, and a
    /// deployment that does not federate provisioning should not expose one.
    ///
    /// Requires a database pool (SCIM provisions into `core.tb_user`) and an `admin_token`
    /// (without one, provisioning credentials could never be minted, so the surface would
    /// exist with no way to authenticate to it).
    #[serde(default)]
    pub enabled: bool,

    /// Externally reachable base URL of the SCIM surface, used verbatim for `meta.location`
    /// and `$ref`.
    ///
    /// Provisioning clients follow these URLs, so a wrong value produces a directory whose
    /// links point somewhere the client cannot reach. Defaults to the relative `/scim/v2`,
    /// which is correct for a client talking to this host directly.
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

fn default_base_url() -> String {
    "/scim/v2".to_string()
}

impl Default for ScimServerConfig {
    fn default() -> Self {
        Self {
            enabled:  false,
            base_url: default_base_url(),
        }
    }
}

impl ScimServerConfig {
    /// Validate the section shape.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending field.
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled && self.base_url.trim().is_empty() {
            return Err("[scim] base_url must not be empty — provisioning clients follow \
                        meta.location, and an empty base makes every resource unreachable"
                .to_string());
        }
        Ok(())
    }
}
