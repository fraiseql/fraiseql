//! Input validation module.
//!
//! Provides ID policy validation, GraphQL input processing, and comprehensive
//! field-level validation rules including checksum and rich scalar validation.

pub mod checksum;
mod id_policy;
mod input_processor;
pub mod rich_scalars;
pub mod rules;
pub mod validators;

pub use checksum::{LuhnValidator, Mod97Validator};
pub use id_policy::{
    IDPolicy, IDValidationError, IDValidationProfile, ValidationProfileType, validate_id,
};
pub use input_processor::{InputProcessingConfig, ProcessingError, process_variables};
pub use rich_scalars::{EmailValidator, PhoneNumberValidator, VinValidator, CountryCodeValidator};
pub use rules::ValidationRule;
pub use validators::{Validator, PatternValidator, LengthValidator, RangeValidator, EnumValidator};
