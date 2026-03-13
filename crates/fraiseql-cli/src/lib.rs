//! FraiseQL CLI library - exposes internal modules for testing and reuse

#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::format_push_string)]      // Reason: push_str + format! is clearer for incremental SQL/query string building
#![allow(clippy::option_if_let_else)]      // Reason: style preference — if let chains are more readable in command handlers
#![allow(clippy::needless_pass_by_value)]  // Reason: API consistency; command handler functions receive owned values from clap
#![allow(clippy::must_use_candidate)]      // Reason: output/builder methods don't require #[must_use] in CLI context
#![allow(clippy::module_name_repetitions)] // Reason: standard Rust API style (e.g. CliError, CliConfig, CliOutput)
#![allow(clippy::missing_errors_doc)]      // Reason: error types are self-documenting via thiserror display messages
#![allow(clippy::doc_markdown)]            // Reason: CLI help text uses backtick-free prose intentionally for readability
#![allow(clippy::too_many_lines)]          // Reason: some command handlers are inherently long (e.g. generate.rs)
#![allow(clippy::unnecessary_wraps)]       // Reason: API consistency — some fns return Result for future extensibility
#![allow(clippy::match_same_arms)]         // Reason: explicit duplicate arms improve readability for exhaustive schema matches
#![allow(clippy::similar_names)]           // Reason: domain names (type_name/type_kind, field_name/field_type) are intentionally similar
#![allow(clippy::struct_excessive_bools)]  // Reason: schema config structs use bool flags by design (per CLAUDE.md)
#![allow(clippy::derive_partial_eq_without_eq)] // Reason: Eq not derivable for all schema structs (contain f64 fields)
#![allow(clippy::missing_const_for_fn)]    // Reason: const fn not stable for all patterns used here

pub mod cli;
pub mod commands;
pub mod config;
pub mod introspection;
pub mod output;
pub mod output_schemas;
pub mod runner;
pub mod schema;

pub use runner::run;
