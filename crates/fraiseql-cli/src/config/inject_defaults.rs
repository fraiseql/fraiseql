//! `[inject_defaults]` in `fraiseql.toml` (#1384).
//!
//! The SDK config loaders (Python, Go, Java, PHP, C#, Elixir, F#) read this section from
//! `fraiseql.toml` and emit it into `schema.json`; the compiler parsed the same file with
//! `deny_unknown_fields` and refused the key, so a project following the SDK's documented
//! config could not compile at all. The section is now read here, in the SDKs' shape:
//!
//! ```toml
//! [inject_defaults]
//! tenant_id = "jwt:tenant_id"     # base: queries and mutations
//!
//! [inject_defaults.queries]
//! read_scope = "jwt:scope"
//!
//! [inject_defaults.mutations]
//! user_id = "jwt:sub"
//! ```
//!
//! When `schema.json` also carries `inject_defaults` — an SDK emitted it from this same
//! file — the two must agree; a difference is refused rather than one silently winning.

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::schema::intermediate::IntermediateInjectDefaults;

/// `[inject_defaults]` as written in `fraiseql.toml`: base keys at the top of the table,
/// `queries` / `mutations` sub-tables beside them.
///
/// A sub-table under any other name is refused by the base map, whose values must be
/// strings — so `[inject_defaults.querys]` is an error, not a silently ignored table (the
/// SDK loaders ignore it).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct InjectDefaultsToml {
    /// Applied to queries only, overriding a base entry of the same name.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub queries:   IndexMap<String, String>,
    /// Applied to mutations only, overriding a base entry of the same name.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub mutations: IndexMap<String, String>,
    /// Applied to queries and mutations.
    #[serde(flatten)]
    pub base:      IndexMap<String, String>,
}

impl InjectDefaultsToml {
    /// The compiler's form, validated exactly as a `schema.json` block is (`<source>:<claim>`).
    ///
    /// # Errors
    ///
    /// A source that is not `<source>:<claim>`.
    pub fn to_intermediate(&self) -> Result<IntermediateInjectDefaults> {
        serde_json::from_value(serde_json::json!({
            "base": self.base,
            "queries": self.queries,
            "mutations": self.mutations,
        }))
        .context("Invalid [inject_defaults] in fraiseql.toml")
    }

    /// The defaults a compile applies, from the config and from the schema document.
    ///
    /// Either alone is used as is. Both present must be equal: an SDK emits the document's
    /// block from this same config, so a difference means one of them is stale, and which
    /// one a reader meant cannot be guessed.
    ///
    /// # Errors
    ///
    /// The config's block is invalid, or it differs from the document's.
    pub fn reconcile(
        config: Option<&Self>,
        document: Option<IntermediateInjectDefaults>,
    ) -> Result<Option<IntermediateInjectDefaults>> {
        let Some(config) = config else {
            return Ok(document);
        };
        let from_config = config.to_intermediate()?;
        match document {
            Some(from_document) if from_document != from_config => bail!(
                "[inject_defaults] in fraiseql.toml differs from `inject_defaults` in the \
                 schema document.\n  fraiseql.toml: {}\n  schema:        {}\nRegenerate the \
                 schema from this fraiseql.toml, or remove the block from one of them.",
                serde_json::to_string(&from_config).unwrap_or_default(),
                serde_json::to_string(&from_document).unwrap_or_default(),
            ),
            _ => Ok(Some(from_config)),
        }
    }
}

#[cfg(test)]
mod tests;
