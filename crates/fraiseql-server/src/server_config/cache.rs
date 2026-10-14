//! Result-cache settings beyond `cache_enabled`.

use serde::{Deserialize, Serialize};

/// How a server learns of the writes other servers make (#1577).
///
/// The result cache lives in one process, and a write evicts entries on the server that ran
/// it. With several servers, the others keep serving what it made stale until the entry's
/// `cache_ttl_seconds` runs out, forever at `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum CacheCrossInstance {
    /// Each server's cache is evicted by its own writes only: correct for one server, or
    /// within each entry's TTL for several.
    #[default]
    Off,
    /// Each server announces its writes' evictions over PostgreSQL `NOTIFY` after commit, and
    /// applies every other server's, on one connection of its own to `database_url`. While
    /// that connection is down the server caches nothing.
    ///
    /// The connection must reach PostgreSQL itself, or a pooler in session mode: through
    /// `PgBouncer` in transaction mode a `LISTEN` receives nothing, and the server, which
    /// checks that it hears its own messages, then never caches.
    Notify,
}
