//! `[egress]`, its environment override, and the variables it replaced.

#![allow(clippy::unwrap_used)] // Reason: test code

use super::{EGRESS_ALLOWED_DOMAINS_ENV, EgressConfig};
use crate::server_config::ServerConfig;

fn config(domains: &[&str]) -> EgressConfig {
    EgressConfig {
        allowed_domains: domains.iter().map(|d| (*d).to_owned()).collect(),
    }
}

#[test]
fn the_section_is_read_from_the_server_config() {
    let parsed: ServerConfig =
        toml::from_str("[egress]\nallowed_domains = [\"api.example.com\"]\n").unwrap();
    assert_eq!(parsed.egress, config(&["api.example.com"]));
}

#[test]
fn an_unknown_key_in_the_section_is_refused() {
    let error = toml::from_str::<ServerConfig>("[egress]\nallowed_hosts = []\n").unwrap_err();
    assert!(error.to_string().contains("allowed_hosts"), "{error}");
}

#[test]
fn the_configured_list_stands_when_the_environment_is_silent() {
    let resolved = config(&["api.example.com"]).resolve(|_| None).unwrap();
    assert_eq!(resolved, ["api.example.com"]);
}

#[test]
fn the_environment_replaces_the_configured_list() {
    let resolved = config(&["api.example.com"])
        .resolve(|key| {
            (key == EGRESS_ALLOWED_DOMAINS_ENV).then(|| " a.example.com, ,b.example.com ".into())
        })
        .unwrap();
    assert_eq!(resolved, ["a.example.com", "b.example.com"]);
}

#[test]
fn a_replaced_variable_is_refused_by_name() {
    for replaced in [
        "FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS",
        "FRAISEQL_SOURCES_ALLOWED_DOMAINS",
    ] {
        let error = config(&[])
            .resolve(|key| (key == replaced).then(|| "api.example.com".into()))
            .unwrap_err();
        assert!(
            error.contains(replaced) && error.contains(EGRESS_ALLOWED_DOMAINS_ENV),
            "{error}"
        );
    }
}

/// No alias: a server config that still lists hosts under `[sources]` is refused,
/// naming where they go, rather than booting with an allowlist that no longer applies.
#[cfg(feature = "sources")]
#[test]
fn a_sources_allowed_domains_key_is_refused_with_a_pointer_to_egress() {
    let parsed: ServerConfig =
        toml::from_str("[sources]\nallowed_domains = [\"api.example.com\"]\n").unwrap();
    let error = parsed.validate().unwrap_err();
    assert!(error.contains("[egress] allowed_domains"), "{error}");
}
