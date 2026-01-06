//! Input validation for `FraiseQL` GraphQL operations
//!
//! This module provides validation utilities for GraphQL inputs, particularly
//! for enforcing ID policies and ensuring data integrity.
//!
//! # Modules
//!
//! - `id_policy`: ID Policy validation (UUID vs OPAQUE format enforcement)
//! - `input_processor`: GraphQL variable processing with ID policy validation

pub mod id_policy;
pub mod input_processor;

pub use id_policy::{validate_id, validate_ids, IDPolicy, IDValidationError};
pub use input_processor::{process_variables, InputProcessingConfig, ProcessingError};
