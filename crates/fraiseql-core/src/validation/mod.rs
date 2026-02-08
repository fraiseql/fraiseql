//! Input validation module.
//!
//! Provides ID policy validation, GraphQL input processing, and comprehensive
//! field-level validation rules.

mod id_policy;
mod input_processor;
pub mod rules;
pub mod validators;

pub use id_policy::{
    IDPolicy, IDValidationError, IDValidationProfile, ValidationProfileType, validate_id,
};
pub use input_processor::{InputProcessingConfig, ProcessingError, process_variables};
pub use rules::ValidationRule;
pub use validators::{Validator, PatternValidator, LengthValidator, RangeValidator, EnumValidator};
