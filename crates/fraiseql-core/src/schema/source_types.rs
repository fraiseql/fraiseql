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
/// authoring is metadata-only — the body is a Deno connector (Model B) loaded by name
/// from `<module_dir>/<function>.<ext>`. (A native `PullSource`, Model A, is configured
/// by its own section — poll-IMAP by `[mailbox.*.imap]` — never through `sources`.)
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SourceDefinition {
    /// Source name (unique) — the durable-cursor row and the advisory-lease key.
    pub name: String,

    /// POSIX cron expression the source polls on (e.g. `"*/5 * * * *"`).
    pub schedule: String,

    /// The IANA timezone [`schedule`](Self::schedule) is read in, such as
    /// `"Europe/Paris"`. Absent: UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,

    /// The durable cursor this source advances. Defaults to [`name`](Self::name)
    /// when absent, so most sources need not set it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,

    /// The bound Deno connector: the server loads `<module_dir>/<function>.<ext>` at
    /// boot, and an enabled source whose connector cannot load refuses the boot
    /// (#1399). A declared function of the same name is used as-is.
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
///
/// `user_id` is what the identity answers to `jwt:sub` and `jwt:user_id` (#1700), so
/// a mutation that injects the caller's id can be called from a source. Its
/// provenance stays `system_job:<name>`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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

    /// The id this identity presents to `jwt:sub` / `jwt:user_id` injects (#1700) —
    /// a service principal's id. Unset ⇒ they receive `system_job:<name>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

impl RunAs {
    /// The background [`SecurityContext`](crate::security::SecurityContext) this
    /// ceiling grants, identified as `system_job:<job_id>` and correlated by
    /// `request_id`; it answers `jwt:sub` with [`user_id`](Self::user_id) when set.
    #[must_use]
    pub fn identity(
        &self,
        job_id: impl Into<String>,
        request_id: impl Into<String>,
    ) -> crate::security::SecurityContext {
        let identity = crate::security::SecurityContext::system_job(
            job_id,
            request_id,
            self.roles.clone(),
            self.scopes.clone(),
            self.tenant.clone().map(crate::types::TenantId::from),
        );
        match &self.user_id {
            Some(user_id) => identity.with_subject(user_id.as_str()),
            None => identity,
        }
    }
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
            timezone: None,
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
        self.run_as.clone().unwrap_or_default().identity(self.name.as_str(), request_id)
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
