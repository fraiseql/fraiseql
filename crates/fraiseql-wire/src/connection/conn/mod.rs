//! Core connection type

mod config;
mod core;
mod helpers;
mod tests;

pub use config::{ConnectionConfig, ConnectionConfigBuilder};
pub use core::Connection;
