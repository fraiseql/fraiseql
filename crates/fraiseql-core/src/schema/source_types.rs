//! Source definition types for scheduled ingress connectors (#573).

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// A scheduled ingress `Source` — the dual of an
/// [`ObserverDefinition`](super::ObserverDefinition).
///
/// An observer is egress ("a database change → tell the world"); a source is
/// ingress ("on a schedule → fetch the world → write the database via mutations",
/// resuming from a durable cursor). The `function` runs on the cron `schedule`;
/// authoring is metadata-only — the body is a Deno function (Model B) or a built-in
/// native `PullSource` (Model A).
///
/// This desugars to a `cron:<schedule>` scheduling of `function`, bound to a durable
/// cursor named [`cursor_name`](Self::cursor_name) — sugar over the existing cron
/// trigger, not a new trigger kind.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::SourceDefinition;
///
/// let source = SourceDefinition::new("orders", "*/5 * * * *", "pollOrders");
/// assert_eq!(source.cursor_name(), "orders"); // defaults to the source name
/// assert!(source.enabled);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDefinition {
    /// Source name (unique) — the durable-cursor row and the advisory-lease key.
    pub name: String,

    /// POSIX cron expression the source polls on (e.g. `"*/5 * * * *"`).
    pub schedule: String,

    /// The durable cursor this source advances. Defaults to [`name`](Self::name)
    /// when absent, so most sources need not set it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,

    /// The bound handler: a Deno function name (Model B) or a built-in native
    /// `PullSource` name (Model A).
    pub function: String,

    /// Whether the source is enabled. A disabled source is compiled but not
    /// scheduled. Defaults to `true`.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Connector-specific options handed to the source, opaque to the framework.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub options: serde_json::Value,
}

/// A source is enabled unless explicitly disabled.
const fn default_enabled() -> bool {
    true
}

impl SourceDefinition {
    /// Create a source with the required fields; `cursor` defaults to the name,
    /// `enabled` to `true`, and `options` to null.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        schedule: impl Into<String>,
        function: impl Into<String>,
    ) -> Self {
        Self {
            name:     name.into(),
            schedule: schedule.into(),
            cursor:   None,
            function: function.into(),
            enabled:  true,
            options:  serde_json::Value::Null,
        }
    }

    /// The durable cursor this source advances — its explicit
    /// [`cursor`](Self::cursor), or its [`name`](Self::name) when unset.
    #[must_use]
    pub fn cursor_name(&self) -> &str {
        self.cursor.as_deref().unwrap_or(&self.name)
    }

    /// Set an explicit cursor name (distinct from the source name).
    #[must_use]
    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }

    /// Set connector options.
    #[must_use]
    pub fn with_options(mut self, options: serde_json::Value) -> Self {
        self.options = options;
        self
    }

    /// Mark the source disabled (compiled, but not scheduled).
    #[must_use]
    pub const fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// The `cron:<schedule>` trigger type this source desugars to — the sugar-over-
    /// cron binding (decision D1). The runtime schedules [`function`](Self::function)
    /// on it.
    #[must_use]
    pub fn cron_trigger(&self) -> String {
        format!("cron:{}", self.schedule)
    }
}
