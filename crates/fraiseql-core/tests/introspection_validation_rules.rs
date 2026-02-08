//! Integration tests for validation rules in introspection schema.

#[cfg(test)]
mod tests {
    use fraiseql_core::{
        schema::{
            CompiledSchema, InputFieldDefinition, InputObjectDefinition, IntrospectionBuilder,
            TypeKind,
        },
        validation::rules::ValidationRule,
    };

    fn create_test_schema_with_validation() -> CompiledSchema {
        let mut schema = CompiledSchema::default();

        // Create an input object with various validation rules
        let mut fields = vec![];

        // Required field
        let mut required_field = InputFieldDefinition::new("id", "String!".to_string());
        required_field = required_field.with_validation_rule(ValidationRule::Required);
        fields.push(required_field);

        // Pattern validation
        let mut email_field = InputFieldDefinition::new("email", "String".to_string());
        email_field = email_field.with_validation_rule(ValidationRule::Pattern {
            pattern: r"^[^\s@]+@[^\s@]+\.[^\s@]+$".to_string(),
            message: Some("Invalid email format".to_string()),
        });
        fields.push(email_field);

        // Range validation
        let mut age_field = InputFieldDefinition::new("age", "Int".to_string());
        age_field = age_field.with_validation_rule(ValidationRule::Range {
            min: Some(0),
            max: Some(150),
        });
        fields.push(age_field);

        // Length validation
        let mut password_field = InputFieldDefinition::new("password", "String".to_string());
        password_field = password_field.with_validation_rule(ValidationRule::Length {
            min: Some(8),
            max: Some(128),
        });
        fields.push(password_field);

        // Enum validation
        let mut status_field = InputFieldDefinition::new("status", "String".to_string());
        status_field = status_field.with_validation_rule(ValidationRule::Enum {
            values: vec![
                "ACTIVE".to_string(),
                "INACTIVE".to_string(),
                "PENDING".to_string(),
            ],
        });
        fields.push(status_field);

        // Cross-field validation
        let mut start_date_field = InputFieldDefinition::new("startDate", "String".to_string());
        start_date_field = start_date_field.with_validation_rule(ValidationRule::CrossField {
            field:    "endDate".to_string(),
            operator: "lt".to_string(),
        });
        fields.push(start_date_field);

        // Checksum validation
        let mut card_field = InputFieldDefinition::new("cardNumber", "String".to_string());
        card_field = card_field.with_validation_rule(ValidationRule::Checksum {
            algorithm: "luhn".to_string(),
        });
        fields.push(card_field);

        // OneOf validation
        let mut reference_field = InputFieldDefinition::new("reference", "String".to_string());
        reference_field = reference_field.with_validation_rule(ValidationRule::OneOf {
            fields: vec![
                "entityId".to_string(),
                "name".to_string(),
                "description".to_string(),
            ],
        });
        fields.push(reference_field);

        // AnyOf validation
        let mut contact_field = InputFieldDefinition::new("contact", "String".to_string());
        contact_field = contact_field.with_validation_rule(ValidationRule::AnyOf {
            fields: vec![
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
        });
        fields.push(contact_field);

        // ConditionalRequired validation
        let mut created_at_field = InputFieldDefinition::new("createdAt", "String".to_string());
        created_at_field =
            created_at_field.with_validation_rule(ValidationRule::ConditionalRequired {
                if_field_present: "entityId".to_string(),
                then_required:    vec!["updatedAt".to_string()],
            });
        fields.push(created_at_field);

        // RequiredIfAbsent validation
        let mut address_id_field = InputFieldDefinition::new("addressId", "String".to_string());
        address_id_field =
            address_id_field.with_validation_rule(ValidationRule::RequiredIfAbsent {
                absent_field:  "street".to_string(),
                then_required: vec!["city".to_string(), "zip".to_string()],
            });
        fields.push(address_id_field);

        let input_type = InputObjectDefinition::new("TestInput").with_fields(fields);
        schema.input_types.push(input_type);

        schema
    }

    #[test]
    fn test_introspection_includes_validation_rules() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        // Find the TestInput type
        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false));

        assert!(test_input.is_some(), "TestInput should be in introspection");

        let test_input = test_input.unwrap();
        assert_eq!(test_input.kind, TypeKind::InputObject);

        // Check that input_fields contains validation_rules
        assert!(test_input.input_fields.is_some());
        let input_fields = test_input.input_fields.as_ref().unwrap();
        assert!(!input_fields.is_empty());

        // Each field should have validation_rules populated
        for field in input_fields {
            assert!(
                !field.validation_rules.is_empty(),
                "Field {} should have validation rules",
                field.name
            );
        }
    }

    #[test]
    fn test_required_rule_in_introspection() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false))
            .unwrap();

        let id_field = test_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "id")
            .unwrap();

        assert_eq!(id_field.validation_rules.len(), 1);
        assert_eq!(id_field.validation_rules[0].rule_type, "required");
    }

    #[test]
    fn test_pattern_rule_in_introspection() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false))
            .unwrap();

        let email_field = test_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "email")
            .unwrap();

        assert_eq!(email_field.validation_rules.len(), 1);
        assert_eq!(email_field.validation_rules[0].rule_type, "pattern");
        assert!(email_field.validation_rules[0].pattern.is_some());
    }

    #[test]
    fn test_range_rule_in_introspection() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false))
            .unwrap();

        let age_field = test_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "age")
            .unwrap();

        assert_eq!(age_field.validation_rules.len(), 1);
        assert_eq!(age_field.validation_rules[0].rule_type, "range");
        assert_eq!(age_field.validation_rules[0].min, Some(0));
        assert_eq!(age_field.validation_rules[0].max, Some(150));
    }

    #[test]
    fn test_length_rule_in_introspection() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false))
            .unwrap();

        let password_field = test_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "password")
            .unwrap();

        assert_eq!(password_field.validation_rules.len(), 1);
        assert_eq!(password_field.validation_rules[0].rule_type, "length");
        assert_eq!(password_field.validation_rules[0].min, Some(8));
        assert_eq!(password_field.validation_rules[0].max, Some(128));
    }

    #[test]
    fn test_enum_rule_in_introspection() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false))
            .unwrap();

        let status_field = test_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "status")
            .unwrap();

        assert_eq!(status_field.validation_rules.len(), 1);
        assert_eq!(status_field.validation_rules[0].rule_type, "enum");
        assert!(status_field.validation_rules[0].allowed_values.is_some());
        let values = status_field.validation_rules[0].allowed_values.as_ref().unwrap();
        assert_eq!(values.len(), 3);
    }

    #[test]
    fn test_cross_field_rule_in_introspection() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false))
            .unwrap();

        let field = test_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "startDate")
            .unwrap();

        assert_eq!(field.validation_rules.len(), 1);
        assert_eq!(field.validation_rules[0].rule_type, "cross_field");
        assert_eq!(field.validation_rules[0].field_reference, Some("endDate".to_string()));
        assert_eq!(field.validation_rules[0].operator, Some("lt".to_string()));
    }

    #[test]
    fn test_checksum_rule_in_introspection() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false))
            .unwrap();

        let field = test_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "cardNumber")
            .unwrap();

        assert_eq!(field.validation_rules.len(), 1);
        assert_eq!(field.validation_rules[0].rule_type, "checksum");
        assert_eq!(field.validation_rules[0].algorithm, Some("luhn".to_string()));
    }

    #[test]
    fn test_one_of_rule_in_introspection() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false))
            .unwrap();

        let field = test_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "reference")
            .unwrap();

        assert_eq!(field.validation_rules.len(), 1);
        assert_eq!(field.validation_rules[0].rule_type, "one_of");
        assert!(field.validation_rules[0].field_list.is_some());
    }

    #[test]
    fn test_any_of_rule_in_introspection() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false))
            .unwrap();

        let field = test_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "contact")
            .unwrap();

        assert_eq!(field.validation_rules.len(), 1);
        assert_eq!(field.validation_rules[0].rule_type, "any_of");
        assert!(field.validation_rules[0].field_list.is_some());
    }

    #[test]
    fn test_conditional_required_rule_in_introspection() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false))
            .unwrap();

        let field = test_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "createdAt")
            .unwrap();

        assert_eq!(field.validation_rules.len(), 1);
        assert_eq!(field.validation_rules[0].rule_type, "conditional_required");
    }

    #[test]
    fn test_required_if_absent_rule_in_introspection() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);

        let test_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "TestInput").unwrap_or(false))
            .unwrap();

        let field = test_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "addressId")
            .unwrap();

        assert_eq!(field.validation_rules.len(), 1);
        assert_eq!(field.validation_rules[0].rule_type, "required_if_absent");
    }

    #[test]
    fn test_validation_rules_json_serialization() {
        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);
        let json = serde_json::to_string(&introspection).unwrap();

        // Should contain validation rule types
        assert!(json.contains("\"required\""));
        assert!(json.contains("\"pattern\""));
        assert!(json.contains("\"range\""));
    }

    #[test]
    fn test_field_without_validation_rules() {
        let mut schema = CompiledSchema::default();
        let field = InputFieldDefinition::new("unvalidated", "String".to_string());
        let input_type = InputObjectDefinition::new("SimpleInput").with_fields(vec![field]);
        schema.input_types.push(input_type);

        let introspection = IntrospectionBuilder::build(&schema);
        let simple_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref().map(|n| n == "SimpleInput").unwrap_or(false))
            .unwrap();

        let unvalidated_field = simple_input
            .input_fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.name == "unvalidated")
            .unwrap();

        assert!(unvalidated_field.validation_rules.is_empty());
    }

    #[test]
    fn test_apollo_sandbox_compatibility() {
        use fraiseql_core::schema::IntrospectionSchema;

        let schema = create_test_schema_with_validation();
        let introspection = IntrospectionBuilder::build(&schema);
        let json = serde_json::to_string_pretty(&introspection).unwrap();

        // Apollo Sandbox should be able to parse the response
        let deserialized: IntrospectionSchema = serde_json::from_str(&json).unwrap();

        assert!(!deserialized.types.is_empty());
    }
}
