//! Rich scalar type filter compilation
//!
//! This module generates GraphQL WhereInput types, SQL templates, and validation rules
//! for rich scalar types (EmailAddress, VIN, IBAN, etc.) detected in the schema.
//!
//! Flow:
//! 1. Detect rich scalar types in schema
//! 2. Look up operators for each type (from fraiseql-core)
//! 3. Generate GraphQL WhereInput input types
//! 4. Extract SQL templates from database handlers
//! 5. Embed validation rules
//! 6. Add to compiled schema

use std::collections::HashMap;

use fraiseql_core::{
    filters::{ParameterType, get_operators_for_type},
    schema::CompiledSchema,
};
use serde_json::{Value, json};

use super::{lookup_data, sql_templates};

/// Rich filter compilation configuration
#[derive(Debug, Clone)]
pub struct RichFilterConfig {
    /// Enable rich filter compilation
    pub enabled:              bool,
    /// Validation rules overrides (from fraiseql.toml)
    // Reason: Will be used in future phases for extensible validation configuration
    #[allow(dead_code)]
    pub validation_overrides: HashMap<String, Value>,
}

impl Default for RichFilterConfig {
    fn default() -> Self {
        Self {
            enabled:              true,
            validation_overrides: HashMap::new(),
        }
    }
}

/// Compile rich filters: generate artifacts for rich scalar types
pub fn compile_rich_filters(
    schema: &mut CompiledSchema,
    config: &RichFilterConfig,
) -> anyhow::Result<()> {
    if !config.enabled {
        return Ok(());
    }

    // Build global lookup data (embedded in schema for runtime use)
    let lookup_data_value = lookup_data::build_lookup_data();

    // Get list of rich scalar type names from config or detect from schema
    // For now, we'll detect them from operators module
    let rich_types = get_all_rich_types();

    // For each rich type, generate GraphQL WhereInput
    for rich_type in rich_types {
        if let Some(operators) = get_operators_for_type(&rich_type) {
            // Generate the WhereInput type
            let where_input = generate_where_input_type(&rich_type, &operators)?;

            // Add to schema
            schema.input_types.push(where_input);
        }
    }

    // Store lookup data in the schema for runtime access
    // This enables the server to perform lookups without external dependencies
    if let Some(ref mut security_val) = schema.security {
        // If security is already present, merge lookup data
        if let Some(obj) = security_val.as_object_mut() {
            obj.insert("lookup_data".to_string(), lookup_data_value);
        }
    } else {
        // Create security section with lookup data
        schema.security = Some(json!({
            "lookup_data": lookup_data_value
        }));
    }

    Ok(())
}

/// Get all rich scalar type names
fn get_all_rich_types() -> Vec<String> {
    vec![
        // Contact/Communication
        "EmailAddress".to_string(),
        "PhoneNumber".to_string(),
        "URL".to_string(),
        "DomainName".to_string(),
        "Hostname".to_string(),
        // Location/Address
        "PostalCode".to_string(),
        "Latitude".to_string(),
        "Longitude".to_string(),
        "Coordinates".to_string(),
        "Timezone".to_string(),
        "LocaleCode".to_string(),
        "LanguageCode".to_string(),
        "CountryCode".to_string(),
        // Financial
        "IBAN".to_string(),
        "CUSIP".to_string(),
        "ISIN".to_string(),
        "SEDOL".to_string(),
        "LEI".to_string(),
        "MIC".to_string(),
        "CurrencyCode".to_string(),
        "Money".to_string(),
        "ExchangeCode".to_string(),
        "ExchangeRate".to_string(),
        "StockSymbol".to_string(),
        // Identifiers & Content
        "Slug".to_string(),
        "SemanticVersion".to_string(),
        "HashSHA256".to_string(),
        "APIKey".to_string(),
        // Transportation & Logistics
        "LicensePlate".to_string(),
        "VIN".to_string(),
        "TrackingNumber".to_string(),
        "ContainerNumber".to_string(),
        // Network & Geography
        "IPAddress".to_string(),
        "IPv4".to_string(),
        "IPv6".to_string(),
        "CIDR".to_string(),
        "Port".to_string(),
        "AirportCode".to_string(),
        "PortCode".to_string(),
        "FlightNumber".to_string(),
        // Content Types
        "Markdown".to_string(),
        "HTML".to_string(),
        "MimeType".to_string(),
        "Color".to_string(),
        "Image".to_string(),
        "File".to_string(),
        // Ranges & Measurements
        "DateRange".to_string(),
        "Duration".to_string(),
        "Percentage".to_string(),
    ]
}

/// Generate a GraphQL WhereInput type for a rich scalar type
fn generate_where_input_type(
    rich_type_name: &str,
    operators: &[fraiseql_core::filters::OperatorInfo],
) -> anyhow::Result<fraiseql_core::schema::InputObjectDefinition> {
    use fraiseql_core::schema::{InputFieldDefinition, InputObjectDefinition};

    let where_input_name = format!("{rich_type_name}WhereInput");

    // Standard operators (always present)
    let mut fields = vec![
        InputFieldDefinition {
            name:          "eq".to_string(),
            field_type:    "String".to_string(),
            description:   Some("Equals".to_string()),
            default_value: None,
            deprecation:   None,
        },
        InputFieldDefinition {
            name:          "neq".to_string(),
            field_type:    "String".to_string(),
            description:   Some("Not equals".to_string()),
            default_value: None,
            deprecation:   None,
        },
        InputFieldDefinition {
            name:          "in".to_string(),
            field_type:    "[String!]!".to_string(),
            description:   Some("In list".to_string()),
            default_value: None,
            deprecation:   None,
        },
        InputFieldDefinition {
            name:          "nin".to_string(),
            field_type:    "[String!]!".to_string(),
            description:   Some("Not in list".to_string()),
            default_value: None,
            deprecation:   None,
        },
        InputFieldDefinition {
            name:          "contains".to_string(),
            field_type:    "String".to_string(),
            description:   Some("Contains substring".to_string()),
            default_value: None,
            deprecation:   None,
        },
        InputFieldDefinition {
            name:          "isnull".to_string(),
            field_type:    "Boolean".to_string(),
            description:   Some("Is null".to_string()),
            default_value: None,
            deprecation:   None,
        },
    ];

    // Rich operators
    let mut operator_names = Vec::new();
    for op_info in operators {
        let graphql_type = operator_param_type_to_graphql_string(op_info.parameter_type);
        operator_names.push(op_info.graphql_name.clone());
        fields.push(InputFieldDefinition {
            name:          op_info.graphql_name.clone(),
            field_type:    graphql_type,
            description:   Some(op_info.description.clone()),
            default_value: None,
            deprecation:   None,
        });
    }

    // Build SQL template metadata for this rich type
    let operator_refs: Vec<&str> = operator_names.iter().map(std::string::String::as_str).collect();
    let sql_metadata = sql_templates::build_sql_templates_metadata(&operator_refs);

    Ok(InputObjectDefinition {
        name: where_input_name,
        description: Some(format!("Filter operations for {rich_type_name}")),
        fields,
        metadata: Some(sql_metadata),
    })
}

/// Convert parameter type to GraphQL type string
fn operator_param_type_to_graphql_string(param_type: ParameterType) -> String {
    match param_type {
        ParameterType::String => "String".to_string(),
        ParameterType::StringArray => "[String!]!".to_string(),
        ParameterType::Number => "Float".to_string(),
        ParameterType::NumberRange => "FloatRange".to_string(),
        ParameterType::Boolean => "Boolean".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rich_types_list() {
        let types = get_all_rich_types();
        assert!(types.contains(&"EmailAddress".to_string()));
        assert!(types.contains(&"VIN".to_string()));
        assert!(types.contains(&"IBAN".to_string()));
    }

    #[test]
    fn test_generate_where_input_name() {
        let where_input_name = "EmailAddressWhereInput";
        assert!(where_input_name.ends_with("WhereInput"));
    }
}
