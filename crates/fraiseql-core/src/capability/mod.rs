//! Capability metadata: what an optional capability is, where it is configured, and
//! which build serves it.
//!
//! A capability is a subsystem behind a `fraiseql-server` Cargo feature: REST, MCP,
//! observers, inbound webhooks and so on. Each one is described once, by a
//! [`CapabilityMeta`], and every list that used to enumerate them by hand (the boot
//! refusal for an unservable compiled-schema section, the build-feature hint on a
//! config parse error, `fraiseql capabilities`) is derived from that description.
//!
//! The types live here so the compiler and the server describe a capability with the
//! same vocabulary. The table itself lives in `fraiseql-server`, next to the
//! `cfg!(feature = …)` that says whether a given build has it.

use serde::Serialize;

/// How far a capability can be relied on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Maturity {
    /// Compiled into a published artifact and covered by the release gates.
    Stable,
    /// Built from source only. The code exists and is tested, but no published
    /// artifact compiles it, so nothing an adopter downloads can run it.
    Preview,
}

/// The smallest published artifact that compiles a capability in.
///
/// The artifacts nest: the default image ⊂ `-full` ⊂ `-platform`. A capability marked
/// [`Release::Full`] is in `-full` and `-platform`; one marked [`Release::Source`] is in
/// none of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Release {
    /// The default image and every artifact above it.
    Default,
    /// The `-full` image and release tarball, and `-platform`.
    Full,
    /// The `-platform` image only.
    Platform,
    /// No published artifact: build with the feature.
    Source,
}

/// What makes a present compiled-schema section a request for something to run.
///
/// A section that is present but switched off, or that declares nothing, asks for
/// nothing — refusing it would fail a boot over a key a serializer emitted with its
/// default value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SectionActivity {
    /// An object carrying `enabled: bool`. Active when it is `true`.
    EnabledFlag,
    /// An object carrying a `definitions` array. Active when it is non-empty.
    NonEmptyDefinitions,
    /// A top-level array of declarations, each with its own `enabled`. Active when at
    /// least one entry is enabled.
    AnyEntryEnabled,
}

impl SectionActivity {
    /// Whether `value`, the section as it appears in the compiled schema, asks for
    /// something to run.
    #[must_use]
    pub fn is_active(self, value: &serde_json::Value) -> bool {
        let enabled = |v: &serde_json::Value| {
            v.get("enabled").and_then(serde_json::Value::as_bool).unwrap_or(false)
        };
        match self {
            Self::EnabledFlag => enabled(value),
            Self::NonEmptyDefinitions => value
                .get("definitions")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|d| !d.is_empty()),
            Self::AnyEntryEnabled => value.as_array().is_some_and(|e| e.iter().any(enabled)),
        }
    }
}

/// A top-level `schema.compiled.json` key that only this capability can serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CompiledSection {
    /// The key in `schema.compiled.json`.
    pub key:      &'static str,
    /// When a present section is asking for something to run.
    pub activity: SectionActivity,
}

/// One optional capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CapabilityMeta {
    /// Stable identifier, as `fraiseql capabilities` prints it.
    pub name:              &'static str,
    /// The `fraiseql-server` Cargo feature that compiles it in.
    pub feature:           &'static str,
    /// The compiled-schema sections only this capability serves.
    pub compiled_sections: &'static [CompiledSection],
    /// The server config (`ServerConfig`) keys that exist only when it is compiled in.
    pub runtime_keys:      &'static [&'static str],
    /// The documentation page, relative to the repository root, if one exists.
    pub docs:              Option<&'static str>,
    /// How far it can be relied on.
    pub maturity:          Maturity,
    /// The smallest published artifact that has it.
    pub release:           Release,
}

impl CapabilityMeta {
    /// What an operator whose build lacks this capability should do to get it.
    #[must_use]
    pub fn remedy(&self) -> String {
        let rebuild = format!("rebuild with `--features {}`", self.feature);
        match self.release {
            Release::Default | Release::Source => rebuild,
            Release::Full => format!("pull the `-full` or `-platform` image tag, or {rebuild}"),
            Release::Platform => format!("pull the `-platform` image tag, or {rebuild}"),
        }
    }
}

#[cfg(test)]
mod tests;
