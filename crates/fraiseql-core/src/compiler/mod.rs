//! Compiler sub-modules for FraiseQL v2: aggregate functions, aggregation planning, fact
//! tables and window functions, consumed by the runtime executor. Schema compilation
//! itself is `fraiseql-cli`'s.

pub mod aggregate_types;
pub mod aggregation;
pub mod fact_table;
pub mod time_reduction;
pub mod window_allowlist;
pub mod window_functions;

pub use aggregation::{AggregationPlan, AggregationPlanner, AggregationRequest};
pub use window_functions::{WindowExecutionPlan, WindowFunction, WindowPlanner};

#[cfg(test)]
mod tests;
