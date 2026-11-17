//! How a principal was authenticated (#1628).
//!
//! A principal stored to act again later (a function dispatch run after a restart) is
//! re-verified by the rule of the authenticator that produced it, so that authenticator has to
//! be known. It is recorded once, where the principal is built: as a field every
//! [`AuthenticatedUser`](super::AuthenticatedUser) must state, and by the machine-principal
//! constructors. It travels in the reserved
//! [`PROVENANCE_ATTRIBUTE`](super::SecurityContext::PROVENANCE_ATTRIBUTE), which no claim can
//! set: token claims never reach the `fraiseql.` namespace.

/// The authenticator a principal came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Provenance {
    /// A bearer token the OIDC validator verified.
    BearerToken,
    /// A static or stored API key.
    ApiKey,
    /// A `[service_accounts]` secret.
    ServiceAccount,
    /// The server acting as itself under a `run_as` ceiling.
    SystemJob,
    /// An operator's impersonation preview (admin SQL), never a live request.
    Impersonation,
    /// A session token the Arrow Flight handshake minted.
    FlightSession,
}

impl Provenance {
    /// Every provenance, for exhaustive tests.
    pub const ALL: [Self; 6] = [
        Self::BearerToken,
        Self::ApiKey,
        Self::ServiceAccount,
        Self::SystemJob,
        Self::Impersonation,
        Self::FlightSession,
    ];

    /// The token stored in the attribute.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BearerToken => "bearer_token",
            Self::ApiKey => "api_key",
            Self::ServiceAccount => "service_account",
            Self::SystemJob => "system_job",
            Self::Impersonation => "impersonation",
            Self::FlightSession => "flight_session",
        }
    }

    /// The authenticator in words, for a reason an operator reads: "a bearer token",
    /// "an API key", ….
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::BearerToken => "a bearer token",
            Self::ApiKey => "an API key",
            Self::ServiceAccount => "a service account",
            Self::SystemJob => "a system job",
            Self::Impersonation => "an impersonation preview",
            Self::FlightSession => "a Flight session",
        }
    }

    /// The provenance a stored token names, or `None` for any other value.
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|provenance| provenance.as_str() == token)
    }
}
