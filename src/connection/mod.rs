//! Connection management
//!
//! This module handles:
//! * Transport abstraction (TCP vs Unix socket)
//! * Connection lifecycle (startup, auth, query execution)
//! * State machine enforcement

mod conn;
mod state;
mod transport;

pub use conn::{Connection, ConnectionConfig};
pub use state::ConnectionState;
pub use transport::Transport;
