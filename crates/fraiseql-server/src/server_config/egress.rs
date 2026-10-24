//! `[egress]`: the hosts an application's guest code may call.
//!
//! Functions and source connectors are code the application ships into the server. Any
//! outbound HTTP call one makes needs the deployment's permission by host name, on top
//! of the address checks every outbound call gets. One list grants it, to all of them:
//! a host allowed for a function is allowed for a connector, and one refused is refused
//! everywhere.
//!
//! Observer webhooks, CDC sinks, JWKS and the like are not governed here: their
//! destinations are written in the application's or the operator's configuration, not
//! chosen by guest code at run time.

use serde::{Deserialize, Serialize};

/// The environment variable that replaces [`EgressConfig::allowed_domains`], as a
/// comma-separated list.
pub const EGRESS_ALLOWED_DOMAINS_ENV: &str = "FRAISEQL_EGRESS_ALLOWED_DOMAINS";

/// The variables that each set one guest kind's list before `[egress]`, refused by name.
const REPLACED_ENV: [&str; 2] = [
    "FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS",
    "FRAISEQL_SOURCES_ALLOWED_DOMAINS",
];

/// The `[egress]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EgressConfig {
    /// The hosts guest code may call: `api.example.com`, `*.example.com` (its
    /// subdomains, not the domain itself), or `*` (any host). Empty, the default,
    /// permits none.
    pub allowed_domains: Vec<String>,
}

impl EgressConfig {
    /// The list in force: [`EGRESS_ALLOWED_DOMAINS_ENV`] when it is set, replacing the
    /// configured one, so production can grant a host without editing the file.
    ///
    /// # Errors
    ///
    /// Returns an error naming the variable when one of the per-guest variables `[egress]`
    /// replaced is set: honouring it for one guest kind would bring back two lists.
    pub fn resolve(&self, get: impl Fn(&str) -> Option<String>) -> Result<Vec<String>, String> {
        if let Some(replaced) = REPLACED_ENV.iter().find(|name| get(name).is_some()) {
            return Err(format!(
                "{replaced} was replaced by {EGRESS_ALLOWED_DOMAINS_ENV}, the one list every \
                 function and source connector calls out under (`[egress] allowed_domains` \
                 in the server config). Rename the variable"
            ));
        }
        Ok(get(EGRESS_ALLOWED_DOMAINS_ENV).map_or_else(
            || self.allowed_domains.clone(),
            |list| {
                list.split(',')
                    .map(str::trim)
                    .filter(|domain| !domain.is_empty())
                    .map(String::from)
                    .collect()
            },
        ))
    }
}

#[cfg(test)]
mod tests;
