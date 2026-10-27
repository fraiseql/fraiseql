//! The capability registry: every optional capability this server can be built with,
//! and whether this build has it.
//!
//! One entry per capability. The boot refusal for a compiled-schema section this build
//! cannot serve, the build-feature hint on a config parse error and
//! `fraiseql capabilities` all read this table; none of them keeps a list of its own.
//! The manifest tests hold it to the structs it describes, so a section added behind a
//! `#[cfg(feature = …)]` without an entry here fails the build.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::RwLock,
};

use fraiseql_core::capability::{
    CapabilityMeta, CompiledSection, EntryPoints, Maturity, Release, SectionActivity,
};
use serde::Serialize;

use crate::{
    routes::graphql::AppState,
    server::{
        capabilities::{self as impls, RunCx, ServeCx},
        provision::{BootCx, ProvisionStep},
    },
};

mod schema;

/// A capability and whether this build compiled it in.
///
/// Serialized with its metadata flattened, `compiled_in`, and `schema`: the JSON Schema of
/// each compiled-schema section (`schema.compiled`) and server-config key
/// (`schema.runtime`) it lists, `null` for a key this build cannot describe.
#[derive(Debug, Clone, Copy)]
pub struct Registration {
    /// What the capability is and where it is configured.
    pub meta:              CapabilityMeta,
    /// Whether this build has the capability's feature.
    pub compiled_in:       bool,
    /// The code that runs it.
    pub(crate) capability: &'static dyn Capability,
}

impl Registration {
    /// The schema of every section and key the metadata lists, keyed as it lists them.
    fn schema(&self) -> serde_json::Value {
        let compiled: serde_json::Map<_, _> = self
            .meta
            .compiled_sections
            .iter()
            .map(|s| (s.key.to_owned(), schema::of_compiled(s.key).unwrap_or_default()))
            .collect();
        let runtime: serde_json::Map<_, _> = self
            .meta
            .runtime_keys
            .iter()
            .map(|key| ((*key).to_owned(), schema::of_runtime_key(key).unwrap_or_default()))
            .collect();
        serde_json::json!({ "compiled": compiled, "runtime": runtime })
    }
}

impl Serialize for Registration {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct View<'a> {
            #[serde(flatten)]
            meta:        &'a CapabilityMeta,
            compiled_in: bool,
            schema:      serde_json::Value,
        }
        View {
            meta:        &self.meta,
            compiled_in: self.compiled_in,
            schema:      self.schema(),
        }
        .serialize(serializer)
    }
}

/// What a capability's start left running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Started {
    /// Everything it was configured to run.
    Running,
    /// The server serves on without it: `/health` reports it not started.
    #[cfg_attr(
        not(any(feature = "observers", feature = "sources")),
        allow(dead_code) // Reason: only those two capabilities can serve on without starting
    )]
    Degraded,
}

/// The code behind one [`REGISTRY`] entry.
///
/// The serve entry points never name a capability: every compiled-in one adds its steps to
/// the provisioning table and merges its routes, and each one that is configured starts
/// where the entry point is one its metadata declares. A method whose capability is not compiled in
/// keeps its default body.
#[async_trait::async_trait]
pub(crate) trait Capability: Send + Sync + std::fmt::Debug {
    /// Whether this server's configuration asks for the capability.
    fn configured(&self, _cx: &ServeCx<'_>) -> bool {
        false
    }

    /// The tables it needs before anything is mounted or started, as steps of the one
    /// provisioning table, which runs each under the boot lock.
    fn provision(&self, _cx: &BootCx<'_>) -> Vec<ProvisionStep> {
        Vec::new()
    }

    /// Start its background work on the server's task set, once the router is built.
    ///
    /// Called only when [`Self::configured`] and the entry point is one the metadata
    /// declares. A capability that only serves routes has nothing to start.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ServerError`] when a configured capability cannot start and the
    /// server must not serve without it.
    async fn start(&self, _cx: &mut RunCx<'_>) -> crate::Result<Started> {
        Ok(Started::Running)
    }

    /// The routes it serves, built for this server, or `None` when it serves none here.
    ///
    /// Merged before the admission layer, so every capability route is under it. Each
    /// route carries its own authentication: `route_layer` does not cross a merge.
    fn routes(&self, _cx: &ServeCx<'_>) -> Option<axum::Router> {
        None
    }

    /// Whether it is working right now, for a capability that can tell; `None` otherwise.
    async fn health(&self, _app: &AppState) -> Option<bool> {
        None
    }
}

/// Build a [`Registration`] whose `compiled_in` is the `cfg!` of the same feature
/// literal as `meta.feature`, so the two cannot name different features.
macro_rules! register {
    (
        capability: $capability:expr,name: $name:literal,feature: $feature:literal,compiled_sections: $compiled:expr,runtime_keys:
        $runtime:expr,docs: $docs:expr,maturity: $maturity:expr,release: $release:expr,entry_points:
        $entry_points:expr $(,)?
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
                entry_points:      $entry_points,
            },
            compiled_in: cfg!(feature = $feature),
            capability:  &$capability,
        }
    };
}

const fn section(key: &'static str, activity: SectionActivity) -> CompiledSection {
    CompiledSection { key, activity }
}

/// Every optional capability, in the order `fraiseql capabilities` lists them.
pub static REGISTRY: &[Registration] = &[
    register! {
        capability: impls::Functions,
        name: "functions",
        feature: "functions-runtime",
        compiled_sections: &[section("functions", SectionActivity::NonEmptyDefinitions)],
        runtime_keys: &[],
        docs: Some("docs/architecture/functions.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
        entry_points: EntryPoints::ALL,
    },
    register! {
        capability: impls::Sources,
        name: "sources",
        feature: "sources",
        compiled_sections: &[section("sources", SectionActivity::AnyEntryEnabled)],
        runtime_keys: &["sources"],
        docs: Some("docs/architecture/sources.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
        entry_points: EntryPoints::HTTP,
    },
    register! {
        capability: impls::Mcp,
        name: "mcp",
        feature: "mcp",
        compiled_sections: &[section("mcp_config", SectionActivity::EnabledFlag)],
        runtime_keys: &[],
        docs: Some("docs/mcp.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
        entry_points: EntryPoints::ALL,
    },
    register! {
        capability: impls::Rest,
        name: "rest",
        feature: "rest",
        compiled_sections: &[section("rest_config", SectionActivity::EnabledFlag)],
        runtime_keys: &["export"],
        docs: Some("docs/architecture/rest.md"),
        maturity: Maturity::Stable,
        release: Release::Full,
        entry_points: EntryPoints::HTTP,
    },
    register! {
        capability: impls::Grpc,
        name: "grpc",
        feature: "grpc",
        compiled_sections: &[section("grpc_config", SectionActivity::EnabledFlag)],
        runtime_keys: &[],
        docs: Some("docs/architecture/grpc.md"),
        maturity: Maturity::Preview,
        release: Release::Source,
        entry_points: EntryPoints::NONE,
    },
    register! {
        capability: impls::Federation,
        name: "federation",
        feature: "federation",
        compiled_sections: &[section("federation", SectionActivity::EnabledFlag)],
        runtime_keys: &[],
        docs: Some("docs/architecture/federation.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
        entry_points: EntryPoints::HTTP,
    },
    register! {
        capability: impls::Observers,
        name: "observers",
        feature: "observers",
        compiled_sections: &[section("observers_config", SectionActivity::EnabledFlag)],
        runtime_keys: &["observers"],
        docs: Some("docs/operations/observers.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
        entry_points: EntryPoints::HTTP,
    },
    register! {
        capability: impls::ArrowFlight,
        name: "arrow_flight",
        feature: "arrow",
        compiled_sections: &[],
        runtime_keys: &["flight_bind_addr", "flight_upload_tables", "flight_views"],
        docs: Some("docs/architecture/arrow-flight.md"),
        maturity: Maturity::Stable,
        release: Release::Full,
        entry_points: EntryPoints {
            listener: false,
            ..EntryPoints::HTTP
        },
    },
    register! {
        capability: impls::InboundWebhooks,
        name: "inbound_webhooks",
        feature: "inbound",
        compiled_sections: &[],
        runtime_keys: &["webhooks", "inbound_dispatch"],
        docs: Some("docs/architecture/webhooks.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
        entry_points: EntryPoints::HTTP,
    },
    register! {
        capability: impls::InboundEmail,
        name: "inbound_email",
        feature: "inbound-email",
        compiled_sections: &[],
        runtime_keys: &["mailbox", "send"],
        docs: Some("docs/architecture/inbound-email.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
        entry_points: EntryPoints::HTTP,
    },
    register! {
        capability: impls::Identity,
        name: "identity",
        feature: "auth",
        compiled_sections: &[],
        runtime_keys: &["identity"],
        docs: Some("docs/auth/identity-store.md"),
        maturity: Maturity::Stable,
        release: Release::Default,
        entry_points: EntryPoints::HTTP,
    },
    register! {
        capability: impls::SessionState,
        name: "session_state",
        feature: "auth",
        compiled_sections: &[],
        runtime_keys: &["session_state"],
        docs: Some("docs/features/session-state.md"),
        maturity: Maturity::Stable,
        release: Release::Default,
        // Not `mcp_stdio`: a session is keyed by `mcp-session-id`, which stdio has no header
        // to carry, so no value is ever held there.
        entry_points: EntryPoints::HTTP,
    },
    register! {
        capability: impls::Saml,
        name: "saml",
        feature: "auth-saml",
        compiled_sections: &[],
        runtime_keys: &["saml"],
        docs: Some("docs/auth/saml-sso.md"),
        maturity: Maturity::Preview,
        release: Release::Source,
        entry_points: EntryPoints::HTTP,
    },
    register! {
        capability: impls::CdcOutbound,
        name: "cdc_outbound",
        feature: "cdc-outbound",
        compiled_sections: &[],
        runtime_keys: &["cdc_outbound"],
        docs: Some("docs/features/cdc-outbound.md"),
        maturity: Maturity::Stable,
        release: Release::Platform,
        entry_points: EntryPoints::HTTP,
    },
    register! {
        capability: impls::SubscriptionKafka,
        name: "subscription_kafka",
        feature: "subscription-kafka",
        compiled_sections: &[],
        runtime_keys: &["subscription_kafka"],
        docs: Some("docs/features/cdc-outbound.md"),
        maturity: Maturity::Preview,
        release: Release::Source,
        entry_points: EntryPoints::HTTP,
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
#[cfg(test)]
mod schema_tests;

/// What the serve entry point started and mounted, by capability name.
///
/// Recorded by the start loop and the route mount, and read by `/health`.
#[derive(Debug, Default)]
pub struct CapabilityStarts(RwLock<Recorded>);

#[derive(Debug, Default)]
struct Recorded {
    started: BTreeMap<&'static str, bool>,
    serving: BTreeSet<&'static str>,
}

impl CapabilityStarts {
    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Recorded> {
        self.0.write().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Record whether `name`, a configured capability, started. `false` is one that did
    /// not, or that this entry point does not start.
    pub(crate) fn record(&self, name: &'static str, started: bool) {
        self.write().started.insert(name, started);
    }

    /// What was recorded for `name`: whether it started, if it is configured.
    #[cfg(all(test, feature = "observers"))]
    pub(crate) fn started(&self, name: &str) -> Option<bool> {
        self.0
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .started
            .get(name)
            .copied()
    }

    /// Record that `name` mounted routes.
    pub(crate) fn record_serving(&self, name: &'static str) {
        self.write().serving.insert(name);
    }

    /// Every registered capability, as `/health` reports it.
    pub async fn health(&self, app: &AppState) -> BTreeMap<&'static str, CapabilityHealth> {
        let (started, serving) = {
            let recorded = self.0.read().unwrap_or_else(std::sync::PoisonError::into_inner);
            (recorded.started.clone(), recorded.serving.clone())
        };
        let mut report = BTreeMap::new();
        for entry in REGISTRY {
            let healthy = if entry.compiled_in {
                entry.capability.health(app).await
            } else {
                None
            };
            report.insert(
                entry.meta.name,
                CapabilityHealth {
                    compiled_in: entry.compiled_in,
                    started: started.get(entry.meta.name).copied(),
                    serving: serving.contains(entry.meta.name),
                    healthy,
                },
            );
        }
        report
    }
}

/// One capability, as `/health` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CapabilityHealth {
    /// Whether this build has the capability's feature.
    pub compiled_in: bool,
    /// For a capability this server is configured to run: whether this entry point
    /// started it. Absent when it is not configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started:     Option<bool>,
    /// Whether it mounted routes on this server.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub serving:     bool,
    /// Whether it is working right now, for a capability that can tell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthy:     Option<bool>,
}
