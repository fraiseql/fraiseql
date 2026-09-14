//! What one compile writes to `schema.compiled.json`.
//!
//! The file is not the serialization of a single type. It is the core
//! [`CompiledSchema`] plus the **platform-extension sections that sit beside it**:
//! keys the GraphQL engine knows nothing about, read by subsystems that are compiled
//! in optionally. `functions` is the first such section, and it cannot be a
//! `CompiledSchema` field for a structural reason — the crate that defines
//! [`FunctionDefinition`](fraiseql_functions::FunctionDefinition) already depends on
//! `fraiseql-core`, so the dependency cannot run back the other way.
//!
//! Before #1325 that distinction did not matter, because nothing could author a
//! `functions` section: `IntermediateSchema` had no field for it, and the only writer
//! of one in the whole tree was a test that hand-built the JSON. The section existed
//! on the reading side only — `fraiseql-server`'s `ExtendedCompiledSchema` parsed a
//! key the compiler could not emit.
//!
//! This type is the writing side of that pair, and [`CompiledArtifact::to_json_value`]
//! is the one routine that assembles the file — so the `_content_hash` covers every
//! section rather than only the ones that happen to be `CompiledSchema` fields.

use anyhow::{Context, Result};
use fraiseql_core::schema::CompiledSchema;
use fraiseql_functions::FunctionsConfig;

/// The compiled schema together with the platform sections written alongside it.
#[derive(Debug, Clone)]
pub struct CompiledArtifact {
    /// GraphQL types, queries, mutations, subscriptions and their generated SQL.
    pub schema: CompiledSchema,

    /// The compiled `functions` section (#1325), when the project declares one.
    ///
    /// `None` — not an empty section — when it declares none, so a project that has
    /// never heard of functions produces byte-for-byte the artifact it did before.
    pub functions: Option<FunctionsConfig>,
}

impl CompiledArtifact {
    /// Assemble the artifact's JSON: the compiled schema, plus each platform section
    /// spliced in as a sibling key.
    ///
    /// Hashing happens **after** this, on the whole object, so schema integrity
    /// (`_content_hash`, #899) covers a tampered `functions` section exactly as it
    /// covers a tampered query.
    ///
    /// # Errors
    ///
    /// Returns an error if the compiled schema or one of its sibling sections fails
    /// to serialize, or if the schema does not serialize to a JSON object.
    pub fn to_json_value(&self) -> Result<serde_json::Value> {
        // Serialize through a string exactly as the writer always has, so the value
        // being hashed is the value that reaches disk.
        let body = serde_json::to_string_pretty(&self.schema)
            .context("Failed to serialize compiled schema")?;
        let mut value: serde_json::Value = serde_json::from_str(&body)?;

        if let Some(functions) = &self.functions {
            let obj = value.as_object_mut().context("schema must serialise as JSON object")?;
            obj.insert(
                "functions".to_string(),
                serde_json::to_value(functions).context("Failed to serialize functions section")?,
            );
        }

        Ok(value)
    }
}
