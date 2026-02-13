//! Connection management
//!
//! This module handles:
//! * Transport abstraction (TCP vs Unix socket)
//! * Connection lifecycle (startup, auth, query execution)
//! * State machine enforcement
//! * TLS configuration and support

mod conn;
mod state;
mod tls;
mod transport;

pub use conn::{Connection, ConnectionConfig, ConnectionConfigBuilder};
pub use state::ConnectionState;
pub use tls::{parse_server_name, TlsConfig};
pub use transport::Transport;
