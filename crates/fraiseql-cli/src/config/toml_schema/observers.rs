//! Observer/event system configuration for TOML schema.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Observers/event system configuration — the **declarative compile-time**
/// shape of the `[observers]` table.
///
/// This is NOT the server's runtime schema. The same `fraiseql.toml` is fed to
/// both `fraiseql compile` (this struct) and `fraiseql-server`; the server's
/// runtime tuning lives under `[observers.runtime]` (see the server's
/// `ObserverRuntimeSettings` / issue #342). The [`runtime`](Self::runtime)
/// field is declared-and-ignored here so a shared config carrying the server's
/// `[observers.runtime]` sub-table still compiles — the compiler does not
/// validate its contents.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObserversConfig {
    /// Enable observers system
    #[serde(default)]
    pub enabled:   bool,
    /// Backend service (redis, nats, postgresql, mysql, in-memory)
    pub backend:   String,
    /// Redis connection URL (required when backend = "redis")
    pub redis_url: Option<String>,
    /// NATS connection URL (required when backend = "nats")
    ///
    /// Example: `nats://localhost:4222`
    /// Can be overridden at runtime via the `FRAISEQL_NATS_URL` environment variable.
    pub nats_url:  Option<String>,
    /// Event handlers
    pub handlers:  Vec<EventHandler>,
    /// Server-runtime tuning sub-table (`[observers.runtime]`).
    ///
    /// Declared-and-ignored: owned and validated by `fraiseql-server`, not the
    /// compiler. Captured as an opaque value so a shared `fraiseql.toml` that
    /// carries server runtime tuning still passes `fraiseql compile` (#342).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime:   Option<toml::Value>,
}

impl Default for ObserversConfig {
    fn default() -> Self {
        Self {
            enabled:   false,
            backend:   "redis".to_string(),
            redis_url: None,
            nats_url:  None,
            handlers:  vec![],
            runtime:   None,
        }
    }
}

impl ObserversConfig {
    /// Whether the TOML workflow embeds the table: observers are enabled or a backend URL
    /// is set.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        self.enabled || self.redis_url.is_some() || self.nats_url.is_some()
    }

    /// The compiled shape of the table. `handlers` and `runtime` are not part of it.
    #[must_use]
    pub fn to_compiled(&self) -> fraiseql_core::schema::ObserversConfig {
        fraiseql_core::schema::ObserversConfig {
            enabled:   self.enabled,
            backend:   self.backend.clone(),
            redis_url: self.redis_url.clone(),
            nats_url:  self.nats_url.clone(),
        }
    }

    /// Refuse `[[observers.handlers]]`.
    ///
    /// # Errors
    ///
    /// Returns an error when the table declares a handler.
    pub fn refuse_handlers(&self) -> Result<()> {
        // #8 / #631 (decided): compiled handler declarations are not a runtime
        // concept. Runtime observers have exactly one source of truth — the
        // `tb_observer` table and the admin observer API — and the compiled
        // schema cannot even represent a `handlers` array any more. This bail
        // is permanent policy, not a stopgap awaiting a boot-time loader.
        if !self.handlers.is_empty() {
            bail!(
                "[[observers.handlers]] is not supported: compiled TOML handlers are not a \
                 runtime concept (#631) — runtime observers come only from the `tb_observer` \
                 table and the admin observer API. Define observers in `tb_observer` / via \
                 `POST /api/observers` and remove the [[observers.handlers]] block."
            );
        }
        Ok(())
    }

    /// Lower the project config's `[observers]` beside a schema document's own copy.
    ///
    /// A missing table is absence, so the document's copy stands. A declared table, even
    /// one saying only `enabled = false`, owns these deployment settings: a document copy
    /// must state the same `enabled`, `backend`, `redis_url` and `nats_url`, and the
    /// refusal names both sources and the first key they disagree on. An agreeing copy is
    /// kept as written, so the converter still refuses any `handlers` it declares.
    ///
    /// # Errors
    ///
    /// Returns an error when the table declares a handler, or when the document's copy
    /// differs from the table.
    pub fn reconcile(table: Option<&Self>, document: Option<Value>) -> Result<Option<Value>> {
        let Some(table) = table else {
            return Ok(document);
        };
        table.refuse_handlers()?;
        let from_config = table.to_compiled();
        let Some(document) = document else {
            return Ok(Some(serde_json::to_value(&from_config)?));
        };
        let mut settings = document.clone();
        if let Some(map) = settings.as_object_mut() {
            map.remove("handlers");
        }
        // A copy that is not an observers section is the converter's to refuse, naming it.
        let Ok(from_document) =
            serde_json::from_value::<fraiseql_core::schema::ObserversConfig>(settings)
        else {
            return Ok(Some(document));
        };
        let differing = [
            ("enabled", from_config.enabled != from_document.enabled),
            ("backend", from_config.backend != from_document.backend),
            ("redis_url", from_config.redis_url != from_document.redis_url),
            ("nats_url", from_config.nats_url != from_document.nats_url),
        ]
        .into_iter()
        .find_map(|(key, differs)| differs.then_some(key));
        if let Some(key) = differing {
            bail!(
                "fraiseql.toml [observers] and schema.json observers_config disagree on \
                 `{key}`.\n  fraiseql.toml [observers]:      {}\n  schema.json observers_config: \
                 {}\nDeclare the observers settings once, in fraiseql.toml.",
                serde_json::to_string(&from_config).unwrap_or_default(),
                serde_json::to_string(&from_document).unwrap_or_default(),
            );
        }
        Ok(Some(document))
    }
}

/// Event handler configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventHandler {
    /// Handler name
    pub name:           String,
    /// Event type to handle
    pub event:          String,
    /// Action to perform: an observer action type, such as `webhook`, `slack` or `email`
    pub action:         String,
    /// Webhook URL for webhook actions
    pub webhook_url:    Option<String>,
    /// Retry strategy
    pub retry_strategy: Option<String>,
    /// Maximum retry attempts
    pub max_retries:    Option<u32>,
    /// Handler description
    pub description:    Option<String>,
}
