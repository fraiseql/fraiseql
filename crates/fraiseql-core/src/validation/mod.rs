//! Input validation module.
//!
//! Provides ID policy validation, GraphQL input processing, and comprehensive
//! field-level validation rules including checksum, rich scalar, and async validation.

pub mod async_validators;
pub mod checksum;
pub mod composite;
pub mod cross_field;
pub mod error_responses;
mod id_policy;
mod input_processor;
pub mod inheritance;
pub mod input_object;
pub mod mutual_exclusivity;
pub mod rich_scalars;
pub mod rules;
pub mod validators;

pub use async_validators::{
    AsyncValidator, AsyncValidatorConfig, AsyncValidatorProvider, MockEmailDomainValidator,
    MockPhoneNumberValidator,
};
pub use checksum::{LuhnValidator, Mod97Validator};
pub use composite::{
    validate_all, validate_any, validate_not, validate_optional, CompositeError, CompositeOperator,
};
pub use cross_field::{ComparisonOperator, validate_cross_field_comparison};
pub use id_policy::{
    IDPolicy, IDValidationError, IDValidationProfile, ValidationProfileType, validate_id,
};
pub use inheritance::{
    inherit_validation_rules, validate_inheritance, InheritanceMode, RuleMetadata,
    ValidationRuleRegistry,
};
pub use input_object::{
    validate_input_object, InputObjectRule, InputObjectValidationResult,
};
pub use input_processor::{InputProcessingConfig, ProcessingError, process_variables};
pub use mutual_exclusivity::{
    AnyOfValidator, ConditionalRequiredValidator, OneOfValidator, RequiredIfAbsentValidator,
};
pub use rich_scalars::{EmailValidator, PhoneNumberValidator, VinValidator, CountryCodeValidator};
pub use rules::ValidationRule;
pub use validators::{Validator, PatternValidator, LengthValidator, RangeValidator, EnumValidator};
