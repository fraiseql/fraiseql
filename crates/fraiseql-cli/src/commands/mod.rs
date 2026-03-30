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
pub mod generate_views;
pub mod init;
pub mod introspect_facts;
pub mod lint;
pub mod migrate;
#[cfg(feature = "run-server")]
pub mod run;
pub mod sbom;
pub mod serve;
#[cfg(feature = "run-server")]
pub mod gateway;
pub mod generate_proto;
pub mod openapi;
pub mod validate;
pub mod validate_documents;
pub mod validate_facts;
