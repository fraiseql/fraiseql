//! The capability registry: every optional capability this server can be built with,
//! and whether this build has it.
//!
//! One entry per capability. The boot refusal for a compiled-schema section this build
//! cannot serve, the build-feature hint on a config parse error and
//! `fraiseql capabilities` all read this table; none of them keeps a list of its own.
//! The manifest tests hold it to the structs it describes, so a section added behind a
//! `#[cfg(feature = …)]` without an entry here fails the build.

use fraiseql_core::capability::{
    CapabilityMeta, CompiledSection, Maturity, Release, SectionActivity,
};
use serde::Serialize;

/// A capability and whether this build compiled it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Registration {
    /// What the capability is and where it is configured.
    #[serde(flatten)]
    pub meta:        CapabilityMeta,
    /// Whether this build has the capability's feature.
    pub compiled_in: bool,
}

/// Build a [`Registration`] whose `compiled_in` is the `cfg!` of the same feature
/// literal as `meta.feature`, so the two cannot name different features.
macro_rules! register {
    (
        name: $name:literal,feature: $feature:literal,compiled_sections: $compiled:expr,runtime_keys:
        $runtime:expr,docs: $docs:expr,maturity: $maturity:expr,release: $release:expr $(,)?
    ) => {
        Registration {
            meta:        CapabilityMeta {
                name:              $name,
                feature:           $feature,
                compiled_sections: $compiled,
                runtime_keys:      $runtime,
                docs:              $docs,
                maturity:          $maturity,
                release:           $release,
            },
            compiled_in: cfg!(feature = $feature),
        }
    };
}

const fn section(key: &'static str, activity: SectionActivity) -> CompiledSection {
    CompiledSection { key, activity }
}

/// Every optional capability, in the order `fraiseql capabilities` lists them.
pub static REGISTRY: &[Registration] = &[
    register! {
        name: "functions",
        feature: "functions-runtime",
        compiled_sections: &[section("functions", SectionActivity::NonEmptyDefinitions)],
        runtime_keys: &[],
        docs: Some("docs/architecture/functions.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
    },
    register! {
        name: "sources",
        feature: "sources",
        compiled_sections: &[section("sources", SectionActivity::AnyEntryEnabled)],
        runtime_keys: &["sources"],
        docs: Some("docs/architecture/sources.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
    },
    register! {
        name: "mcp",
        feature: "mcp",
        compiled_sections: &[section("mcp_config", SectionActivity::EnabledFlag)],
        runtime_keys: &[],
        docs: Some("docs/mcp.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
    },
    register! {
        name: "rest",
        feature: "rest",
        compiled_sections: &[section("rest_config", SectionActivity::EnabledFlag)],
        runtime_keys: &["export"],
        docs: None,
        maturity: Maturity::Stable,
        release: Release::Full,
    },
    register! {
        name: "grpc",
        feature: "grpc",
        compiled_sections: &[section("grpc_config", SectionActivity::EnabledFlag)],
        runtime_keys: &[],
        docs: None,
        maturity: Maturity::Preview,
        release: Release::Source,
    },
    register! {
        name: "federation",
        feature: "federation",
        compiled_sections: &[section("federation", SectionActivity::EnabledFlag)],
        runtime_keys: &[],
        docs: Some("docs/architecture/federation.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
    },
    register! {
        name: "observers",
        feature: "observers",
        compiled_sections: &[section("observers_config", SectionActivity::EnabledFlag)],
        runtime_keys: &["observers"],
        docs: Some("docs/operations/observers.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
    },
    register! {
        name: "arrow_flight",
        feature: "arrow",
        compiled_sections: &[],
        runtime_keys: &["flight_bind_addr", "flight_upload_tables", "flight_views"],
        docs: None,
        maturity: Maturity::Stable,
        release: Release::Full,
    },
    register! {
        name: "inbound_webhooks",
        feature: "inbound",
        compiled_sections: &[],
        runtime_keys: &["webhooks", "inbound_dispatch"],
        docs: Some("docs/architecture/webhooks.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
    },
    register! {
        name: "inbound_email",
        feature: "inbound-email",
        compiled_sections: &[],
        runtime_keys: &["mailbox", "send"],
        docs: Some("docs/architecture/inbound-email.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
    },
    register! {
        name: "identity",
        feature: "auth",
        compiled_sections: &[],
        runtime_keys: &["identity"],
        docs: Some("docs/auth/identity-store.md"),
        maturity: Maturity::Stable,
        release: Release::Default,
    },
    register! {
        name: "session_state",
        feature: "auth",
        compiled_sections: &[],
        runtime_keys: &["session_state"],
        docs: Some("docs/features/session-state.md"),
        maturity: Maturity::Stable,
        release: Release::Default,
    },
    register! {
        name: "saml",
        feature: "auth-saml",
        compiled_sections: &[],
        runtime_keys: &["saml"],
        docs: Some("docs/auth/saml-sso.md"),
        maturity: Maturity::Preview,
        release: Release::Source,
    },
    register! {
        name: "cdc_outbound",
        feature: "cdc-outbound",
        compiled_sections: &[],
        runtime_keys: &["cdc_outbound"],
        docs: Some("docs/features/cdc-outbound.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
    },
    register! {
        name: "subscription_kafka",
        feature: "subscription-kafka",
        compiled_sections: &[],
        runtime_keys: &["subscription_kafka"],
        docs: Some("docs/features/cdc-outbound.md"),
        maturity: Maturity::Preview,
        release: Release::Source,
    },
];

/// Every `ServerConfig` key that exists only under a Cargo feature, as
/// `(key, feature, compiled_in)`.
///
/// With `deny_unknown_fields` (#839), a binary built without a feature refuses a config
/// that declares the feature's section. serde's "unknown field" message cannot say *why*
/// the field is unknown, so `ServerConfig::from_file` appends the build-feature hint for
/// any of these keys found in the raw TOML.
#[must_use]
pub fn feature_gated_keys() -> Vec<(&'static str, &'static str, bool)> {
    REGISTRY
        .iter()
        .flat_map(|r| r.meta.runtime_keys.iter().map(|key| (*key, r.meta.feature, r.compiled_in)))
        .collect()
}

#[cfg(test)]
mod manifest_tests;
