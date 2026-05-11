//! Core connection type

mod config;
mod core;
mod helpers;
#[cfg(test)]
mod tests;

pub use config::{ConnectionConfig, ConnectionConfigBuilder};
pub use core::Connection;
