//! Connection management
//!
//! This module handles:
//! * Transport abstraction (TCP vs Unix socket)
//! * Connection lifecycle (startup, auth, query execution)
//! * State machine enforcement
//! * TLS configuration and support

mod conn;
mod state;
mod transport;
mod tls;

pub use conn::{Connection, ConnectionConfig, ConnectionConfigBuilder};
pub use state::ConnectionState;
pub use transport::Transport;
pub use tls::{TlsConfig, parse_server_name};
