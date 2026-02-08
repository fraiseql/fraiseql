//! Input validation module.
//!
//! Provides ID policy validation, GraphQL input processing, and comprehensive
//! field-level validation rules including checksum, rich scalar, and async validation.

pub mod async_validators;
pub mod checksum;
pub mod cross_field;
pub mod error_responses;
mod id_policy;
mod input_processor;
pub mod mutual_exclusivity;
pub mod rich_scalars;
pub mod rules;
pub mod validators;

pub use async_validators::{
    AsyncValidator, AsyncValidatorConfig, AsyncValidatorProvider, MockEmailDomainValidator,
    MockPhoneNumberValidator,
};
pub use checksum::{LuhnValidator, Mod97Validator};
pub use cross_field::{ComparisonOperator, validate_cross_field_comparison};
pub use id_policy::{
    IDPolicy, IDValidationError, IDValidationProfile, ValidationProfileType, validate_id,
};
pub use input_processor::{InputProcessingConfig, ProcessingError, process_variables};
pub use mutual_exclusivity::{
    AnyOfValidator, ConditionalRequiredValidator, OneOfValidator, RequiredIfAbsentValidator,
};
pub use rich_scalars::{EmailValidator, PhoneNumberValidator, VinValidator, CountryCodeValidator};
pub use rules::ValidationRule;
pub use validators::{Validator, PatternValidator, LengthValidator, RangeValidator, EnumValidator};
