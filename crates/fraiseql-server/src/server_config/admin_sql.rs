//! `[admin_sql]` server configuration — the operator SQL console (#962).
//!
//! Studio's SQL tab runs statements the operator typed. Every other admin
//! endpoint executes SQL FraiseQL generated; this one does not, which is why it
//! is a decision with its own section, its own cargo feature and its own boot
//! refusals rather than a route that appears because a UI wanted it.

use serde::{Deserialize, Serialize};

/// Default statement timeout: long enough for a real inspection query on a
/// production-sized table, short enough that a mistake does not hold a pooled
/// connection and an `ACCESS SHARE` lock for a deploy window.
const fn default_statement_timeout_ms() -> u32 {
    30_000
}

/// Default row cap. A console is for looking at rows, not for extracting them;
/// an operator who needs the whole relation has the export surface.
const fn default_max_rows() -> usize {
    1_000
}

/// The `[admin_sql]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminSqlConfig {
    /// Mount `POST /api/v1/admin/sql`.
    ///
    /// Off by default, and both of the things that could make it reachable are
    /// explicit: the `admin-sql` cargo feature must be compiled in, and this must
    /// be set. With the feature off, setting this is a **boot error** naming the
    /// feature rather than a silently ignored key — a server that accepted the
    /// configuration and mounted nothing would report a console the operator
    /// believes they enabled.
    #[serde(default)]
    pub enabled: bool,

    /// `SET LOCAL statement_timeout` for each execution, in milliseconds.
    ///
    /// A ceiling, not a suggestion: a request may ask for less, never for more.
    #[serde(default = "default_statement_timeout_ms")]
    pub statement_timeout_ms: u32,

    /// Rows returned before the result is reported as truncated.
    ///
    /// Also a ceiling. The response says when it applied, so a partial answer is
    /// never mistaken for a complete one.
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,

    /// Allow `commit: true` at all.
    ///
    /// The console's default is a preview: the transaction rolls back, so a
    /// statement can be run and read without changing anything. This key decides
    /// whether an operator may opt out of that. Left `true` — the acceptance for
    /// #962 is an endpoint that *can* commit on request — but a deployment that
    /// wants a strictly read-through console sets it `false` and gets one, with
    /// the refusal naming the setting.
    #[serde(default = "crate::server_config::defaults::default_true")]
    pub allow_commit: bool,
}

impl Default for AdminSqlConfig {
    fn default() -> Self {
        Self {
            enabled:              false,
            statement_timeout_ms: default_statement_timeout_ms(),
            max_rows:             default_max_rows(),
            allow_commit:         true,
        }
    }
}

impl AdminSqlConfig {
    /// Validate the section shape.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending field.
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        if self.statement_timeout_ms == 0 {
            return Err("[admin_sql] statement_timeout_ms must be greater than 0: \
                        PostgreSQL reads 0 as 'no timeout', so the one bound that stops a \
                        runaway console statement would be disabled by a value that looks \
                        like a limit."
                .to_string());
        }
        if self.max_rows == 0 {
            return Err("[admin_sql] max_rows must be greater than 0: a console that can \
                        return no rows answers every SELECT as truncated-to-nothing."
                .to_string());
        }
        Ok(())
    }
}
