//! CLI commands module

pub mod analyze;
pub mod compile;
pub mod cost;
pub mod dependency_graph;
pub mod doctor;
pub mod explain;
pub mod extract;
pub mod federation;
pub mod generate;
pub mod generate_capture_triggers;
pub mod generate_client;
pub mod generate_views;
pub mod init;
pub mod introspect_facts;
pub mod lint;
pub mod migrate;
pub mod perf;
#[cfg(feature = "run-server")]
pub mod run;
pub mod sbom;
pub mod schema;
pub mod setup;
pub mod validate;
pub mod validate_documents;
pub mod validate_facts;
pub mod watch;

#[cfg(test)]
mod tests;
