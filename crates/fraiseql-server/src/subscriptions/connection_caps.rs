//! How many `WebSocket` subscription connections may be open at once.
//!
//! Admission holds a request's permit only while the request runs, and an upgrade returns
//! once it answers `101`, so nothing else counts a connection for as long as it stays open.
//! [`OpenConnections`] does. A connection takes a [`ConnectionSlot`] at the upgrade, before
//! the `101`, and gives it back when the socket closes, which bounds:
//!
//! - one principal, within its tenant (`max_connections_per_principal`, 32);
//! - the authenticated principals of one tenant (`max_connections_per_tenant`, 1000, or the
//!   tenant's own `max_subscription_connections` quota);
//! - connections with no principal, across the server (`max_anonymous_connections`, 1000), and from
//!   one client address (`max_anonymous_connections_per_ip`, 32).
//!
//! A connection with no principal names its tenant with a request hint, which any client
//! can write. It is therefore never counted against a tenant: counting it there would let
//! a client with no credentials use up the connections another tenant's principals hold.

use std::{collections::HashMap, num::NonZeroU32, sync::Arc};

use fraiseql_core::schema::{ConnectionCap, SubscriptionsConfig};
use parking_lot::Mutex;

const fn cap(n: u32) -> ConnectionCap {
    match NonZeroU32::new(n) {
        Some(n) => ConnectionCap::Limit(n),
        None => ConnectionCap::Unlimited,
    }
}

/// The bounds [`OpenConnections`] enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionCaps {
    /// One principal's open connections, within its tenant.
    pub per_principal:    ConnectionCap,
    /// One tenant's authenticated principals' open connections, unless the tenant's own
    /// quota says otherwise.
    pub per_tenant:       ConnectionCap,
    /// Open connections with no principal, across the server.
    pub anonymous:        ConnectionCap,
    /// Open connections with no principal from one client address.
    pub anonymous_per_ip: ConnectionCap,
}

impl ConnectionCaps {
    /// The bounds a server applies when `[subscriptions]` names none.
    pub const DEFAULT: Self = Self {
        per_principal:    cap(32),
        per_tenant:       cap(1000),
        anonymous:        cap(1000),
        anonymous_per_ip: cap(32),
    };

    /// These bounds, with each one `config` names in its place.
    #[must_use]
    pub fn configured_by(self, config: &SubscriptionsConfig) -> Self {
        Self {
            per_principal:    config.max_connections_per_principal.unwrap_or(self.per_principal),
            per_tenant:       config.max_connections_per_tenant.unwrap_or(self.per_tenant),
            anonymous:        config.max_anonymous_connections.unwrap_or(self.anonymous),
            anonymous_per_ip: config
                .max_anonymous_connections_per_ip
                .unwrap_or(self.anonymous_per_ip),
        }
    }
}

impl Default for ConnectionCaps {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Who a connection is held by, as the upgrade resolved it.
#[derive(Debug, Clone, Copy)]
pub enum Holder<'a> {
    /// An authenticated principal, in its tenant (`None` for the default executor).
    Principal {
        /// The tenant the connection runs as.
        tenant:     Option<&'a str>,
        /// The principal's user id.
        user:       &'a str,
        /// The tenant's bound, when its quota sets one; otherwise
        /// [`ConnectionCaps::per_tenant`].
        tenant_cap: Option<ConnectionCap>,
    },
    /// No principal, from a client address read as the rate limiter reads it.
    Anonymous {
        /// The client address key.
        address: &'a str,
    },
}

/// A bound an upgrade would have gone over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exhausted {
    /// The principal's own connections.
    Principal(u32),
    /// The tenant's connections.
    Tenant(u32),
    /// Connections with no principal, across the server.
    Anonymous(u32),
    /// Connections with no principal from this address.
    AnonymousAddress(u32),
}

impl std::fmt::Display for Exhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (whose, limit) = match self {
            Self::Principal(n) => ("this principal", n),
            Self::Tenant(n) => ("this tenant", n),
            Self::Anonymous(n) => ("unauthenticated clients", n),
            Self::AnonymousAddress(n) => ("unauthenticated clients at this address", n),
        };
        write!(
            f,
            "{whose} already holds {limit} open subscription connections, the most allowed; \
             close one to open another"
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Principal {
        tenant: Option<String>,
        user:   String,
    },
    Tenant(String),
    Anonymous,
    AnonymousAddress(String),
}

/// The open connections, by every key that bounds them.
#[derive(Debug)]
pub struct OpenConnections {
    caps:   ConnectionCaps,
    counts: Mutex<HashMap<Key, u32>>,
}

impl OpenConnections {
    /// A count of open connections enforcing `caps`.
    #[must_use]
    pub fn new(caps: ConnectionCaps) -> Arc<Self> {
        Arc::new(Self {
            caps,
            counts: Mutex::new(HashMap::new()),
        })
    }

    /// The bounds this count enforces.
    #[must_use]
    pub const fn caps(&self) -> ConnectionCaps {
        self.caps
    }

    /// Take a slot for a connection held by `holder`: every bound it counts against is
    /// checked, and charged, together.
    ///
    /// # Errors
    ///
    /// The first bound the connection would go over. Nothing is charged then.
    pub fn admit(self: &Arc<Self>, holder: Holder<'_>) -> Result<ConnectionSlot, Exhausted> {
        let bounds: Vec<(Key, ConnectionCap, fn(u32) -> Exhausted)> = match holder {
            Holder::Principal {
                tenant,
                user,
                tenant_cap,
            } => {
                let mut bounds = vec![(
                    Key::Principal {
                        tenant: tenant.map(str::to_owned),
                        user:   user.to_owned(),
                    },
                    self.caps.per_principal,
                    Exhausted::Principal as fn(u32) -> Exhausted,
                )];
                if let Some(tenant) = tenant {
                    bounds.push((
                        Key::Tenant(tenant.to_owned()),
                        tenant_cap.unwrap_or(self.caps.per_tenant),
                        Exhausted::Tenant,
                    ));
                }
                bounds
            },
            Holder::Anonymous { address } => vec![
                (
                    Key::Anonymous,
                    self.caps.anonymous,
                    Exhausted::Anonymous as fn(u32) -> Exhausted,
                ),
                (
                    Key::AnonymousAddress(address.to_owned()),
                    self.caps.anonymous_per_ip,
                    Exhausted::AnonymousAddress,
                ),
            ],
        };

        let mut counts = self.counts.lock();
        for (key, cap, exhausted) in &bounds {
            if let Some(limit) = cap.limit() {
                if counts.get(key).copied().unwrap_or(0) >= limit {
                    return Err(exhausted(limit));
                }
            }
        }
        let keys: Vec<Key> = bounds.into_iter().map(|(key, ..)| key).collect();
        for key in &keys {
            *counts.entry(key.clone()).or_insert(0) += 1;
        }
        drop(counts);
        Ok(ConnectionSlot {
            owner: Arc::clone(self),
            keys,
        })
    }

    /// How many open connections `holder`'s own key counts: the principal's, or the
    /// address's for a connection with no principal.
    #[must_use]
    pub fn held_by(&self, holder: Holder<'_>) -> u32 {
        let key = match holder {
            Holder::Principal { tenant, user, .. } => Key::Principal {
                tenant: tenant.map(str::to_owned),
                user:   user.to_owned(),
            },
            Holder::Anonymous { address } => Key::AnonymousAddress(address.to_owned()),
        };
        self.counts.lock().get(&key).copied().unwrap_or(0)
    }

    /// How many open connections tenant `key`'s principals hold.
    #[must_use]
    pub fn held_in_tenant(&self, key: &str) -> u32 {
        self.counts.lock().get(&Key::Tenant(key.to_owned())).copied().unwrap_or(0)
    }

    /// How many open connections have no principal.
    #[must_use]
    pub fn held_anonymously(&self) -> u32 {
        self.counts.lock().get(&Key::Anonymous).copied().unwrap_or(0)
    }
}

/// One open connection's place in [`OpenConnections`], given back when it is dropped.
#[derive(Debug)]
pub struct ConnectionSlot {
    owner: Arc<OpenConnections>,
    keys:  Vec<Key>,
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        let mut counts = self.owner.counts.lock();
        for key in &self.keys {
            if let Some(count) = counts.get_mut(key) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    counts.remove(key);
                }
            }
        }
    }
}
