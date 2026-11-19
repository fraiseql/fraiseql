//! The PostgreSQL pools a server runs on.
//!
//! Two pools, two types:
//!
//! - [`ApplicationPool`]: what every pool-backed capability provisions its tables in and runs
//!   against — the RBAC backend, SCIM, tenant admin tokens, API keys, async operations, the
//!   `postgres` session-state backend, sources, inbound webhooks and mail, function dispatch
//!   tracking, the before-mutation hooks and the observer management API.
//! - [`ObserverPool`]: the observer runtime's own. Its listener holds a connection for as long as
//!   it runs, which must not take a slot a request needs.
//!
//! Each type is constructed by name and read by name, so handing the observer runtime the
//! application pool, or the reverse, is a visible edit rather than a swapped argument.
//!
//! `[auth.database]` is a third pool, on its own role, owned by first-party auth
//! (`crate::auth_database`).

use sqlx::PgPool;

/// The pool every pool-backed capability except the observer runtime runs against.
#[derive(Clone, Debug)]
pub struct ApplicationPool(PgPool);

impl ApplicationPool {
    /// Wrap `pool` as the application pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self(pool)
    }

    /// The underlying pool.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.0
    }
}

/// The observer runtime's own pool.
#[derive(Clone, Debug)]
pub struct ObserverPool(PgPool);

impl ObserverPool {
    /// Wrap `pool` as the observer runtime's pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self(pool)
    }

    /// The underlying pool.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.0
    }
}

/// The pools a [`Server`](super::Server) is constructed with.
///
/// The binary opens the two separately. A library embedder that hands a constructor one
/// `Option<PgPool>` gets that pool in both roles, as before the split.
#[derive(Clone, Debug, Default)]
pub struct ServerPools {
    pub(super) application: Option<ApplicationPool>,
    pub(super) observer:    Option<ObserverPool>,
}

impl ServerPools {
    /// Two pools, each in its own role.
    ///
    /// On a build without the `observers` feature there is no observer runtime, and
    /// `observer` is unused.
    #[must_use]
    pub const fn new(application: Option<ApplicationPool>, observer: Option<ObserverPool>) -> Self {
        Self {
            application,
            observer,
        }
    }

    /// The application pool, if any.
    #[must_use]
    pub const fn application(&self) -> Option<&ApplicationPool> {
        self.application.as_ref()
    }

    /// The observer runtime's pool, if any.
    #[must_use]
    pub const fn observer(&self) -> Option<&ObserverPool> {
        self.observer.as_ref()
    }
}

/// One pool in both roles: what every constructor took before the split.
impl From<Option<PgPool>> for ServerPools {
    fn from(pool: Option<PgPool>) -> Self {
        Self {
            application: pool.clone().map(ApplicationPool::new),
            observer:    pool.map(ObserverPool::new),
        }
    }
}

impl super::Server {
    /// The pools this server was constructed with.
    #[must_use]
    pub const fn pools(&self) -> &ServerPools {
        &self.pools
    }

    /// The application pool: what every pool-backed capability except the observer runtime
    /// provisions its tables in and runs against.
    pub(crate) const fn application_pool(&self) -> Option<&PgPool> {
        match self.pools.application() {
            Some(pool) => Some(pool.pool()),
            None => None,
        }
    }

    /// The observer runtime's pool: its tables, its start-up checks and its listener.
    #[cfg(feature = "observers")]
    pub(crate) const fn observer_pool(&self) -> Option<&PgPool> {
        match self.pools.observer() {
            Some(pool) => Some(pool.pool()),
            None => None,
        }
    }
}
