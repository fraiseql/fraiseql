//! Event listeners for the observer system.
//!
//! There is **one** listening strategy: [`change_log::ChangeLogListener`],
//! durable polling of `core.tb_entity_change_log` (requires the `postgres`
//! feature). It is what the server runs and the only path with a durable
//! dispatch record (#935).
//!
//! A second, LISTEN/NOTIFY-based `EventListener` used to live here. It was
//! removed in #931: nothing in the workspace wired it, and its declared
//! `overflow_policy` knob (`Block` / `DropOldest`) was never read — every
//! overflow silently drop-newest regardless of what an embedder configured.
//! Shipping a knob that lies is worse than shipping neither, and LISTEN/NOTIFY
//! is ephemeral by construction: a notification delivered while no listener is
//! connected is gone, which is the property the change-log ledger exists to
//! avoid.
//!
//! `lease.rs` holds the distributed checkpoint lease that keeps a second replica from
//! claiming the same change-log cursor.

#[cfg(feature = "postgres")]
pub mod change_log;
pub mod lease;
#[cfg(feature = "postgres")]
pub mod replay;
#[cfg(feature = "postgres")]
pub mod tail;

#[cfg(feature = "postgres")]
pub use change_log::{ChangeLogEntry, ChangeLogListener, ChangeLogListenerConfig};
pub use lease::CheckpointLease;
#[cfg(feature = "postgres")]
pub use replay::{ChangeLogReplayReader, ReplayScope, ReplayedEvent, ResumeAnchor, ResumePosition};
#[cfg(feature = "postgres")]
pub use tail::ChangeLogTail;

#[cfg(test)]
mod tests;
