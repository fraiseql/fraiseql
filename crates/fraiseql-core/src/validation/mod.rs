//! Input validation module.
//!
//! Provides ID policy validation, GraphQL input processing, and comprehensive
//! field-level validation rules including checksum, rich scalar, and async validation.

pub mod async_validators;
pub mod checksum;
pub mod compile_time;
pub mod composite;
pub mod cross_field;
pub mod custom_scalar;
pub mod custom_scalar_registry;
pub mod custom_type_registry;
pub mod date_validators;
pub mod elo_expressions;
pub mod elo_rust_integration;
pub mod js_codegen;
pub mod error_responses;
mod id_policy;
pub mod inheritance;
pub mod input_object;
mod input_processor;
pub mod mutual_exclusivity;
pub(crate) mod patterns;
pub mod rate_limiting;
pub mod rich_scalars;
pub mod rules;
pub mod scalar_validator;
pub mod validators;

pub use async_validators::{
    ArcAsyncValidator, AsyncValidator, AsyncValidatorConfig, AsyncValidatorProvider,
    ChecksumAsyncValidator, EmailFormatValidator, PhoneE164Validator,
};
pub use checksum::{LuhnValidator, Mod97Validator};
pub use compile_time::{
    CompileTimeError, CompileTimeValidationResult, CompileTimeValidator, FieldType, SchemaContext,
    TypeDef,
};
pub use composite::{
    CompositeError, CompositeOperator, validate_all, validate_any, validate_not, validate_optional,
};
pub use cross_field::{ComparisonOperator, validate_cross_field_comparison};
pub use custom_scalar::CustomScalar;
pub use custom_scalar_registry::CustomScalarRegistry;
pub use custom_type_registry::{CustomTypeDef, CustomTypeRegistry, CustomTypeRegistryConfig};
pub use date_validators::{
    validate_date_range, validate_max_age, validate_max_date, validate_max_days_in_future,
    validate_max_days_in_past, validate_min_age, validate_min_date,
};
pub use elo_expressions::{EloExpressionEvaluator, EloValidationResult};
pub use elo_rust_integration::{
    EloRustValidator, RustValidatorRegistry, RustValidatorRegistryConfig,
};
pub use id_policy::{
    IDPolicy, IDValidationError, IDValidationProfile, ValidationProfileType, validate_id,
};
pub use inheritance::{
    InheritanceMode, RuleMetadata, ValidationRuleRegistry, inherit_validation_rules,
    validate_inheritance,
};
pub use input_object::{InputObjectRule, InputObjectValidationResult, validate_input_object};
pub use input_processor::{InputProcessingConfig, ProcessingError, process_variables};
pub use mutual_exclusivity::{
    AnyOfValidator, ConditionalRequiredValidator, OneOfValidator, RequiredIfAbsentValidator,
};
pub use rate_limiting::{ValidationRateLimiter, ValidationRateLimitingConfig};
pub use rich_scalars::{CountryCodeValidator, EmailValidator, PhoneNumberValidator, VinValidator};
pub use rules::ValidationRule;
pub use scalar_validator::{
    ScalarValidationError, ValidationContext, validate_custom_scalar,
    validate_custom_scalar_parse_value,
};
pub use validators::{EnumValidator, LengthValidator, PatternValidator, RangeValidator, Validator};
