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

    /// The authority this source's background mutations run under (#573) — its
    /// `run_as` *ceiling*. Absent ⇒ **fail-closed**: the source runs with no
    /// roles/scopes/tenant and can write nothing. See [`identity`](Self::identity).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_as: Option<RunAs>,
}

/// The least-privilege authority ceiling a scheduled source's background mutations
/// run under (#573) — the source's `run_as`.
///
/// This is a *ceiling*: the source can never exceed these `roles`/`scopes`. It is
/// authored on `@source`/`@fraiseql.source` and compiled into
/// [`SourceDefinition::run_as`]. A [`SourceDefinition`] with no `run_as` runs
/// fail-closed (no authority → RLS/authz deny), so granting a ceiling is a
/// deliberate operator act.
///
/// `tenant` scopes writes for a single-tenant or global source; a multi-tenant
/// source leaves it unset and re-scopes each write per message at runtime (only the
/// handler knows which tenant a given payload belongs to).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunAs {
    /// Roles granted to the source's background identity (the RBAC ceiling).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,

    /// Scopes granted to the source's background identity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,

    /// The single tenant this source's writes are scoped to, if any. Unset ⇒
    /// global/system (NULL tenant) or a multi-tenant source that re-scopes per
    /// message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
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
            run_as:   None,
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

    /// Set the [`run_as`](Self::run_as) authority ceiling (#573).
    #[must_use]
    pub fn with_run_as(mut self, run_as: RunAs) -> Self {
        self.run_as = Some(run_as);
        self
    }

    /// The background [`SecurityContext`](crate::security::SecurityContext) this
    /// source's mutations run under (#573).
    ///
    /// Built from [`run_as`](Self::run_as) via
    /// [`SecurityContext::system_job`](crate::security::SecurityContext::system_job);
    /// **absent `run_as` yields a fail-closed identity** (no roles, no scopes, no
    /// tenant → every authz/RLS decision denies). `request_id` correlates one firing
    /// of the source (typically its per-fire idempotency token).
    #[must_use]
    pub fn identity(&self, request_id: impl Into<String>) -> crate::security::SecurityContext {
        let (roles, scopes, tenant) = match &self.run_as {
            Some(run_as) => (
                run_as.roles.clone(),
                run_as.scopes.clone(),
                run_as.tenant.clone().map(crate::types::TenantId::from),
            ),
            None => (Vec::new(), Vec::new(), None),
        };
        crate::security::SecurityContext::system_job(
            self.name.as_str(),
            request_id,
            roles,
            scopes,
            tenant,
        )
    }

    /// Mark the source disabled (compiled, but not scheduled).
    #[must_use]
    pub const fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// The `cron:<schedule>` trigger type this source desugars to — the sugar-over-
    /// cron binding. The runtime schedules [`function`](Self::function)
    /// on it.
    #[must_use]
    pub fn cron_trigger(&self) -> String {
        format!("cron:{}", self.schedule)
    }
}
