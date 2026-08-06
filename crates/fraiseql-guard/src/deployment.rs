//! The single answer to "is this process running in production?".
//!
//! # Fail closed
//!
//! An unset `FRAISEQL_ENV` means **production**. An operator never has to set
//! anything to be treated as production; they must positively declare a
//! development environment to get development behaviour. Every server-side safety
//! gate — PKCE state encryption, failed-login lockout, proxy trust, observer
//! transport, the playground and CORS refusals, the error sanitiser — is keyed
//! off this, and each of them is a gate whose false-negative is a security
//! incident and whose false-positive is a config error the operator can see.
//!
//! # Why this is not per-crate
//!
//! It used to be. `ServerConfig::is_production_mode()` defaulted unset to
//! production; `observers::insecure_guard::is_production_environment()` defaulted
//! unset to *not* production. On any deployment that is not Kubernetes — Docker
//! Compose, systemd, a VM, ECS — that meant the server believed it was in
//! production while the observer subsystem honoured an SSRF bypass that is
//! documented as development-only (#836). Two answers to one question is the bug;
//! reconciling the defaults without merging the functions would only have reset
//! the clock on it.

/// The environment variable that declares the deployment environment.
///
/// Absent, or any value other than those in [`DEVELOPMENT_VALUES`], means production.
pub const ENV_VAR: &str = "FRAISEQL_ENV";

/// A secondary environment variable carrying the same declaration.
///
/// Retained because deployments in the wild set it; it can only ever make the
/// answer *more* production, never less.
pub const PROFILE_VAR: &str = "FRAISEQL_PROFILE";

/// Values of [`ENV_VAR`] that declare a development environment.
pub const DEVELOPMENT_VALUES: &[&str] = &["development", "dev"];

/// Returns `true` unless the operator has positively declared a development
/// environment.
///
/// An unset or unrecognised `FRAISEQL_ENV` is production. `FRAISEQL_PROFILE` and
/// the Kubernetes service host can only force production on, never off.
#[must_use]
pub fn is_production() -> bool {
    // Kubernetes injects this into every pod; nothing else does.
    if std::env::var_os("KUBERNETES_SERVICE_HOST").is_some() {
        return true;
    }
    if declares_production(&std::env::var(PROFILE_VAR).unwrap_or_default()) {
        return true;
    }
    !declares_development(&std::env::var(ENV_VAR).unwrap_or_default())
}

/// Returns `true` if `value` explicitly declares a development environment.
///
/// The empty string — an unset variable — is not a declaration.
#[must_use]
pub fn declares_development(value: &str) -> bool {
    DEVELOPMENT_VALUES.iter().any(|dev| value.eq_ignore_ascii_case(dev))
}

/// Returns `true` if `value` explicitly declares production.
#[must_use]
pub const fn declares_production(value: &str) -> bool {
    value.eq_ignore_ascii_case("production") || value.eq_ignore_ascii_case("prod")
}

/// Whether an insecure-mode escape hatch may be honoured.
///
/// This is the shape every bypass in the product should use: the operator must
/// both request the bypass *and* be in a declared development environment.
/// Returns `false` in production no matter how the variable is set.
///
/// Prefer [`insecure_bypass`], which reads the variable and logs the outcome.
/// This entry point is for callers that already have the request as a `bool` and
/// do their own logging — a refused bypass must be visible, and an honoured one
/// must be too.
#[must_use]
pub fn insecure_bypass_allowed(requested: bool) -> bool {
    requested && !is_production()
}

/// What happened when an insecure-mode escape hatch was consulted.
///
/// Distinguishing "not requested" from "refused" is what lets the refusal be
/// logged without putting a line in every log stream for the overwhelmingly
/// common case of a hatch nobody set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BypassDecision {
    /// The operator did not request the bypass. Guards engaged, nothing to report.
    NotRequested,
    /// The bypass was requested and refused: this is a production deployment.
    RefusedInProduction,
    /// The bypass was requested in a declared development environment, and stands.
    Honoured,
}

impl BypassDecision {
    /// Whether the guards this hatch controls should be skipped.
    #[must_use]
    pub const fn is_honoured(self) -> bool {
        matches!(self, Self::Honoured)
    }
}

/// Consult an insecure-mode escape hatch, and log what was decided.
///
/// `var` is the environment variable that requests the bypass (`1`/`true`,
/// case-insensitively — see [`env_opt_in`]). The bypass is honoured only in a
/// declared development environment.
///
/// # Why the logging lives here
///
/// #882: two of the product's four hatches disabled their SSRF guard on the
/// environment variable alone, with no production check at all. Routing them
/// through [`insecure_bypass_allowed`] fixed the policy but left the second half
/// of the defect — a refused bypass reaching the operator as an unexplained
/// connection failure, with nothing in the log stream saying the guard is the
/// reason. A caller can forget to log; it cannot forget to call this.
#[must_use]
pub fn insecure_bypass(var: &str) -> BypassDecision {
    if !env_opt_in(var) {
        return BypassDecision::NotRequested;
    }
    if is_production() {
        tracing::error!(
            target: "fraiseql_guard::deployment",
            "{var} was set in a production environment (FRAISEQL_ENV/FRAISEQL_PROFILE \
             declares production, or KUBERNETES_SERVICE_HOST is set). The bypass is \
             REFUSED and the guards remain engaged — it is for local development and \
             integration testing only. Connections this variable was meant to permit \
             will be rejected."
        );
        return BypassDecision::RefusedInProduction;
    }
    tracing::warn!(
        target: "fraiseql_guard::deployment",
        "{var} is set — the guards it controls are BYPASSED. This must never be set \
         in production."
    );
    BypassDecision::Honoured
}

/// Parses an environment variable's value as a boolean opt-in.
///
/// Accepts `1` and `true` (case-insensitive); everything else, including an
/// unparseable or absent value, is `false`. Shared so that one bypass cannot
/// accept a spelling another rejects.
#[must_use]
pub fn env_opt_in(var: &str) -> bool {
    std::env::var(var).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

#[cfg(test)]
mod tests;
