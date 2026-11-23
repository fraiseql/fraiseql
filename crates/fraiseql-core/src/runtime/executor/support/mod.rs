//! Supporting modules for executor runners.
//!
//! These modules provide stateless helpers and dispatch logic used by the
//! runner types. They do not define their own sub-executor types.

pub(super) mod authz;
pub(super) mod classify;
pub(super) mod explain;
#[cfg(feature = "federation")]
pub(super) mod federation;
pub(super) mod hook_reader;
pub(super) mod introspection_projection;
pub(super) mod node_selection;
pub mod pipeline;
pub(super) mod planning;
pub(super) mod relay;
pub(super) mod security;

#[cfg(test)]
mod multi_root_tests;
#[cfg(test)]
mod node_completion_tests;
#[cfg(test)]
mod node_publication_tests;
#[cfg(test)]
mod node_roots_tests;
#[cfg(test)]
mod scope_coverage_tests;
#[cfg(test)]
mod tests;
