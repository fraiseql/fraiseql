//! Observer/event system configuration for TOML schema.

use anyhow::Result;
use fraiseql_core::{schema::ObserversBackend, utils::config_diagnostic::DiagnosticCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The refusal of `redis_url`, wherever it is declared: the project config's
/// `[observers]` or a schema document's `observers_config`.
pub const REDIS_URL_REFUSED: &str = "observers `redis_url` configures nothing: the observer \
     runtime has no Redis transport. Remove it, and select the transport in fraiseql.toml \
     [observers] with backend = \"postgres\" (the default) or backend = \"nats\" and nats_url";

/// Observers/event system configuration — the **declarative compile-time**
/// shape of the `[observers]` table.
///
/// This is NOT the server's runtime schema. The same `fraiseql.toml` is fed to
/// both `fraiseql compile` (this struct) and `fraiseql-server`; the server's
/// runtime tuning lives under `[observers.runtime]` (see the server's
/// `ObserverRuntimeSettings` / issue #342). The [`runtime`](Self::runtime)
/// field is declared-and-ignored here so a shared config carrying the server's
/// `[observers.runtime]` sub-table still compiles — the compiler does not
/// validate its contents; `fraiseql run` hands them to the server, which does.
#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ObserversConfig {
    /// Enable observers system
    #[serde(default)]
    pub enabled:      bool,
    /// The runtime's event transport: `postgres` (the default) or `nats`.
    pub backend:      ObserversBackend,
    /// Refused: the runtime has no Redis transport. Captured only so the refusal can
    /// say so, instead of an unknown-field error.
    #[serde(default, skip_serializing)]
    #[schemars(with = "Option<serde_json::Value>")]
    pub redis_url:    Option<toml::Value>,
    /// NATS connection URL (required when backend = "nats", unless `nats_url_env` names
    /// the variable holding it)
    ///
    /// Example: `tls://nats.internal:4443`
    /// Compiled as written, so it may not hold a credential (user info before the host):
    /// a URL that does belongs in the variable `nats_url_env` names.
    /// Can be overridden at runtime via the `FRAISEQL_NATS_URL` environment variable.
    pub nats_url:     Option<String>,
    /// The environment variable holding the NATS URL, read by the server at startup.
    ///
    /// The compiled schema carries the variable's name, never its value. At most one of
    /// `nats_url` and `nats_url_env` is set.
    pub nats_url_env: Option<String>,
    /// Event handlers
    pub handlers:     Vec<EventHandler>,
    /// Server-runtime tuning sub-table (`[observers.runtime]`).
    ///
    /// Not read by the compiler: owned and validated by `fraiseql-server`, and
    /// handed to it by `fraiseql run`. Captured as an opaque value so a shared
    /// `fraiseql.toml` that carries server runtime tuning still passes
    /// `fraiseql compile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<serde_json::Value>")]
    pub runtime:      Option<toml::Value>,
}

impl ObserversConfig {
    /// Whether the TOML workflow embeds the table: observers are enabled or a backend URL
    /// is set.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        self.enabled || self.nats_url.is_some() || self.nats_url_env.is_some()
    }

    /// The compiled shape of the table. `handlers` and `runtime` are not part of it.
    #[must_use]
    pub fn to_compiled(&self) -> fraiseql_core::schema::ObserversConfig {
        fraiseql_core::schema::ObserversConfig {
            enabled:      self.enabled,
            backend:      self.backend,
            nats_url:     self.nats_url.clone(),
            nats_url_env: self.nats_url_env.clone(),
        }
    }

    /// Refuse `[[observers.handlers]]`.
    ///
    /// # Errors
    ///
    /// Returns an error when the table declares a handler.
    pub fn refuse_handlers(&self) -> Result<()> {
        // A TOML handler is not an observer the runtime runs. An observer is a
        // `tb_observer` row: one the authoring schema declares, stored at boot, or one
        // the admin API adds.
        if !self.handlers.is_empty() {
            return Err(crate::config::refusal(
                DiagnosticCode::Unread,
                "observers.handlers",
                "",
                "[[observers.handlers]] is not supported: compiled TOML handlers are not a \
                 runtime concept (#631) — runtime observers are the `tb_observer` rows the \
                 authoring schema declares and the admin observer API adds. Declare observers \
                 in the schema, or add them with `POST /api/observers`, and remove the \
                 [[observers.handlers]] block."
                    .to_owned(),
            ));
        }
        Ok(())
    }

    /// Refuse `redis_url`: no transport reads it.
    ///
    /// # Errors
    ///
    /// Returns an error when the table declares `redis_url`.
    pub fn refuse_redis_url(&self) -> Result<()> {
        if self.redis_url.is_some() {
            return Err(crate::config::refusal(
                DiagnosticCode::Unread,
                "observers",
                "redis_url",
                REDIS_URL_REFUSED.to_owned(),
            ));
        }
        Ok(())
    }

    /// Lower the project config's `[observers]` beside a schema document's own copy.
    ///
    /// A missing table is absence, so the document's copy stands. A declared table, even
    /// one saying only `enabled = false`, owns these deployment settings: a document copy
    /// must state the same `enabled`, `backend`, `nats_url` and `nats_url_env`, and the
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
        table.refuse_redis_url()?;
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
            ("nats_url", from_config.nats_url != from_document.nats_url),
            ("nats_url_env", from_config.nats_url_env != from_document.nats_url_env),
        ]
        .into_iter()
        .find_map(|(key, differs)| differs.then_some(key));
        if let Some(key) = differing {
            return Err(crate::config::refusal(
                DiagnosticCode::Disagrees,
                "observers",
                key,
                format!(
                    "fraiseql.toml [observers] and schema.json observers_config disagree on \
                     `{key}`.\n  fraiseql.toml [observers]:      {}\n  schema.json \
                     observers_config: {}\nDeclare the observers settings once, in \
                     fraiseql.toml.",
                    serde_json::to_string(&from_config).unwrap_or_default(),
                    serde_json::to_string(&from_document).unwrap_or_default(),
                ),
            ));
        }
        Ok(Some(document))
    }
}

/// Event handler configuration
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
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
