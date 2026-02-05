//! FraiseQL CLI library - exposes internal modules for testing and reuse

#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::format_push_string)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::similar_names)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::derive_partial_eq_without_eq)]

pub mod commands;
pub mod config;
pub mod output;
pub mod schema;
