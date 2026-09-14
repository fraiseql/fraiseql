//! Serverless-function settings for TOML schema (`[functions]`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The `[functions]` table — the operational half of the functions surface.
///
/// The split is the one `docs/architecture/config-vs-settings.md` prescribes, and
/// it has exactly one owner per key. The **definitions** (what fires, on what
/// trigger, under what authority) are schema: an SDK authors them and they travel
/// in `schema.json`. The **settings** here say where the modules live and which
/// dead-letter store backs dispatch — deployment facts that differ between a
/// laptop and production, and that no decorator should be able to set.
///
/// Neither half can reach into the other: a `FunctionDefinition` is
/// `deny_unknown_fields` and has no `module_dir` or `dlq_store` key, so a schema
/// cannot set a setting; and this table has no way to declare a function.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FunctionsSettings {
    /// Directory holding the function modules (`.wasm`, `.js`, `.ts`), resolved
    /// relative to the server's working directory.
    ///
    /// Unset ⇒ the compiler emits the convention, `functions/`. The compiled
    /// artifact always carries an explicit value, so the server can keep requiring
    /// one and a hand-written compiled schema that omits it still fails loudly.
    pub module_dir: Option<PathBuf>,

    /// Which dead-letter store backs function dispatch (#598): `"memory"` (the
    /// default — dead-letters vanish on restart) or `"postgres"` (durable).
    /// `FRAISEQL_FUNCTIONS_DLQ_STORE` overrides it in production.
    pub dlq_store: Option<String>,
}

impl FunctionsSettings {
    /// Whether the project configured anything here.
    ///
    /// A table left entirely at its defaults is indistinguishable from no table at
    /// all, and must not produce a compiled section — an empty one would read as a
    /// deliberate choice.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        self.module_dir.is_some() || self.dlq_store.is_some()
    }
}
