//! Input validation module.
//!
//! Provides ID policy validation and GraphQL input processing.

mod id_policy;
mod input_processor;

pub use id_policy::{
    IDPolicy, IDValidationError, IDValidationProfile, ValidationProfileType, validate_id,
};
pub use input_processor::{InputProcessingConfig, ProcessingError, process_variables};
