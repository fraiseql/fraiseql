//! Supporting modules for executor runners.
//!
//! These modules provide stateless helpers and dispatch logic used by the
//! runner types. They do not define their own sub-executor types.

pub(super) mod authz;
pub(super) mod classify;
pub(super) mod explain;
#[cfg(feature = "federation")]
pub(super) mod federation;
pub mod pipeline;
pub(super) mod planning;
pub(super) mod relay;
pub(super) mod security;

#[cfg(test)]
mod tests;
