//! Compiler sub-modules for FraiseQL v2.
//!
//! # Overview
//!
//! This module contains:
//!
//! - **Parser / IR / Validator** — parse authoring-time JSON into an intermediate representation
//!   (`AuthoringIR`) and validate it. Used by the CLI `validate-facts` command and by
//!   `SchemaConverter` (in `fraiseql-cli`).
//!
//! - **Runtime sub-modules** — aggregate types, aggregation planning, fact tables, and window
//!   functions, consumed by the runtime executor.

pub mod aggregate_types;
pub mod aggregation;
pub mod enum_validator;
pub mod fact_table;
pub mod ir;
pub mod parser;
pub mod validator;
pub mod window_allowlist;
pub mod window_functions;

pub use aggregate_types::{AggregateType, AggregateTypeGenerator, GroupByInput, HavingInput};
pub use aggregation::{AggregationPlan, AggregationPlanner, AggregationRequest};
pub use enum_validator::EnumValidator;
pub use ir::{
    AuthoringIR, AutoParams, IRArgument, IRField, IRMutation, IRQuery, IRSubscription, IRType,
    MutationOperation,
};
pub use parser::SchemaParser;
pub use validator::{SchemaValidationError, SchemaValidator};
pub use window_functions::{WindowExecutionPlan, WindowFunction, WindowFunctionPlanner};

#[cfg(test)]
mod tests;
