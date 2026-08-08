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
//! Multi-listener coordination for high availability (feature-independent):
//! - `state.rs`: Listener lifecycle state machine
//! - `lease.rs`: Distributed checkpoint leasing
//! - `coordinator.rs`: Multi-listener coordination
//! - `failover.rs`: Automatic failover management

#[cfg(feature = "postgres")]
pub mod change_log;
pub mod coordinator;
pub mod failover;
pub mod lease;
pub mod state;

#[cfg(feature = "postgres")]
pub use change_log::{ChangeLogEntry, ChangeLogListener, ChangeLogListenerConfig};
pub use coordinator::{ListenerHandle, ListenerHealth, MultiListenerCoordinator};
pub use failover::{FailoverEvent, FailoverManager};
pub use lease::CheckpointLease;
pub use state::{ListenerState, ListenerStateMachine};

#[cfg(test)]
mod tests;
