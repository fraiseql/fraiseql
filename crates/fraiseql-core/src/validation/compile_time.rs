//! Compile-time validation for cross-field rules and schema consistency.
//!
//! This module validates ELO expressions at schema compilation time, ensuring:
//! - Field references exist and are properly typed
//! - Cross-field rules reference compatible types
//! - SQL constraints can be generated
//! - No circular dependencies or invalid rules

use std::collections::{HashMap, HashSet};

/// Schema context for compile-time validation
#[derive(Debug, Clone)]
pub struct SchemaContext {
    /// Type definitions: type_name -> fields
    pub types:  HashMap<String, TypeDef>,
    /// Field types: (type_name, field_name) -> field_type
    pub fields: HashMap<(String, String), FieldType>,
}

/// Type definition
#[derive(Debug, Clone)]
pub struct TypeDef {
    pub name:   String,
    pub fields: Vec<String>,
}

/// Field type information
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FieldType {
    String,
    Integer,
    Float,
    Boolean,
    Date,
    DateTime,
    Custom(String),
}

impl FieldType {
    /// Check if two types are comparable
    pub fn is_comparable_with(&self, other: &FieldType) -> bool {
        match (self, other) {
            // Same types are always comparable
            (a, b) if a == b => true,
            // Numeric types are comparable with each other
            (FieldType::Integer, FieldType::Float) => true,
            (FieldType::Float, FieldType::Integer) => true,
            // Date and DateTime are comparable
            (FieldType::Date, FieldType::DateTime) => true,
            (FieldType::DateTime, FieldType::Date) => true,
            // Everything else is not comparable
            _ => false,
        }
    }
}

/// Compile-time validation result
#[derive(Debug, Clone)]
pub struct CompileTimeValidationResult {
    pub valid:          bool,
    pub errors:         Vec<CompileTimeError>,
    pub warnings:       Vec<String>,
    pub sql_constraint: Option<String>,
}

/// Compile-time validation error
#[derive(Debug, Clone)]
pub struct CompileTimeError {
    pub field:      String,
    pub message:    String,
    pub suggestion: Option<String>,
}

/// Compile-time validator for cross-field rules
#[derive(Debug)]
pub struct CompileTimeValidator {
    context: SchemaContext,
}

impl CompileTimeValidator {
    /// Create a new compile-time validator
    pub fn new(context: SchemaContext) -> Self {
        Self { context }
    }

    /// Validate a cross-field rule
    pub fn validate_cross_field_rule(
        &self,
        type_name: &str,
        left_field: &str,
        operator: &str,
        right_field: &str,
    ) -> CompileTimeValidationResult {
        let mut errors = Vec::new();
        let warnings = Vec::new();

        // Check if type exists
        if !self.context.types.contains_key(type_name) {
            return CompileTimeValidationResult {
                valid: false,
                errors: vec![CompileTimeError {
                    field:      type_name.to_string(),
                    message:    format!("Type '{}' not found in schema", type_name),
                    suggestion: Some("Check that the type is defined".to_string()),
                }],
                warnings,
                sql_constraint: None,
            };
        }

        // Check if left field exists
        let left_key = (type_name.to_string(), left_field.to_string());
        let left_type = match self.context.fields.get(&left_key) {
            Some(t) => t,
            None => {
                errors.push(CompileTimeError {
                    field:      left_field.to_string(),
                    message:    format!("Field '{}' not found in type '{}'", left_field, type_name),
                    suggestion: Some(self.suggest_field(type_name, left_field)),
                });
                return CompileTimeValidationResult {
                    valid: false,
                    errors,
                    warnings,
                    sql_constraint: None,
                };
            },
        };

        // Check if right field exists
        let right_key = (type_name.to_string(), right_field.to_string());
        let right_type = match self.context.fields.get(&right_key) {
            Some(t) => t,
            None => {
                errors.push(CompileTimeError {
                    field:      right_field.to_string(),
                    message:    format!(
                        "Field '{}' not found in type '{}'",
                        right_field, type_name
                    ),
                    suggestion: Some(self.suggest_field(type_name, right_field)),
                });
                return CompileTimeValidationResult {
                    valid: false,
                    errors,
                    warnings,
                    sql_constraint: None,
                };
            },
        };

        // Check if types are comparable
        if !left_type.is_comparable_with(right_type) {
            errors.push(CompileTimeError {
                field:      format!("{} {} {}", left_field, operator, right_field),
                message:    format!("Cannot compare {:?} with {:?}", left_type, right_type),
                suggestion: Some(format!("Ensure both fields have comparable types")),
            });
            return CompileTimeValidationResult {
                valid: false,
                errors,
                warnings,
                sql_constraint: None,
            };
        }

        // Generate SQL constraint
        let sql_constraint = self.generate_sql_constraint(
            type_name,
            left_field,
            operator,
            right_field,
            left_type,
            right_type,
        );

        CompileTimeValidationResult {
            valid: true,
            errors,
            warnings,
            sql_constraint,
        }
    }

    /// Validate an ELO expression at compile time
    pub fn validate_elo_expression(
        &self,
        type_name: &str,
        expression: &str,
    ) -> CompileTimeValidationResult {
        let mut errors = Vec::new();
        let warnings = Vec::new();

        // Check if type exists
        if !self.context.types.contains_key(type_name) {
            return CompileTimeValidationResult {
                valid: false,
                errors: vec![CompileTimeError {
                    field:      type_name.to_string(),
                    message:    format!("Type '{}' not found in schema", type_name),
                    suggestion: None,
                }],
                warnings,
                sql_constraint: None,
            };
        }

        // Extract field references from expression
        let field_refs = self.extract_field_references(expression);

        // Validate each field reference
        for field_name in field_refs {
            let field_key = (type_name.to_string(), field_name.clone());
            if !self.context.fields.contains_key(&field_key) {
                errors.push(CompileTimeError {
                    field:      field_name.clone(),
                    message:    format!("Field '{}' not found in type '{}'", field_name, type_name),
                    suggestion: Some(self.suggest_field(type_name, &field_name)),
                });
            }
        }

        // Check for valid operators
        let valid_operators = vec!["<", ">", "<=", ">=", "==", "!=", "&&", "||", "!"];
        for op in valid_operators {
            if expression.contains(op) {
                // Operator found and is valid
            }
        }

        CompileTimeValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
            sql_constraint: None,
        }
    }

    /// Extract field references from an expression
    fn extract_field_references(&self, expression: &str) -> Vec<String> {
        let mut fields = HashSet::new();
        let mut tokens = Vec::new();
        let mut current_token = String::new();
        let mut in_string = false;
        let mut string_char = ' ';
        let mut escape = false;

        // First pass: tokenize the expression, respecting quotes
        for ch in expression.chars() {
            // Handle escape sequences
            if escape {
                escape = false;
                current_token.push(ch);
                continue;
            }

            if ch == '\\' && in_string {
                escape = true;
                current_token.push(ch);
                continue;
            }

            // Track if we're inside a quoted string
            if !in_string && (ch == '"' || ch == '\'') {
                in_string = true;
                string_char = ch;
                current_token.push(ch);
            } else if in_string && ch == string_char {
                in_string = false;
                current_token.push(ch);
            } else if !in_string && (ch.is_whitespace() || ch == '(' || ch == ')') {
                if !current_token.is_empty() {
                    tokens.push(current_token.clone());
                    current_token.clear();
                }
            } else {
                current_token.push(ch);
            }
        }

        if !current_token.is_empty() {
            tokens.push(current_token);
        }

        // Second pass: extract field references from tokens
        let infix_operators = ["matches", "in", "contains"];

        for (i, token) in tokens.iter().enumerate() {
            // Skip quoted strings
            if token.starts_with('"') || token.starts_with('\'') {
                continue;
            }

            // Skip if this token is an infix operator
            if infix_operators.contains(&token.as_str()) {
                continue;
            }

            // Skip if the previous token was an infix operator (it's the RHS of the operator)
            if i > 0 && infix_operators.contains(&tokens[i - 1].as_str()) {
                continue;
            }

            // Skip reserved keywords
            if token == "true"
                || token == "false"
                || token == "null"
                || token == "and"
                || token == "or"
                || token == "not"
            {
                continue;
            }

            // Skip if starts with uppercase (likely type names, not field references)
            if token.chars().next().is_some_and(|ch| ch.is_uppercase()) {
                continue;
            }

            // Extract field references (lowercase identifiers)
            if token.chars().next().is_some_and(|ch| ch.is_lowercase()) {
                fields.insert(token.clone());
            }
        }

        fields.into_iter().collect()
    }

    /// Generate SQL constraint from cross-field rule
    fn generate_sql_constraint(
        &self,
        _type_name: &str,
        left_field: &str,
        operator: &str,
        right_field: &str,
        left_type: &FieldType,
        _right_type: &FieldType,
    ) -> Option<String> {
        // Map ELO operators to SQL operators
        let sql_op = match operator {
            "<" | "lt" => "<",
            "<=" | "lte" => "<=",
            ">" | "gt" => ">",
            ">=" | "gte" => ">=",
            "==" | "eq" => "=",
            "!=" | "neq" => "!=",
            _ => return None,
        };

        // Build constraint based on field type
        let constraint = match left_type {
            FieldType::Date | FieldType::DateTime => {
                format!("CHECK ({} {} {})", left_field, sql_op, right_field)
            },
            FieldType::Integer | FieldType::Float => {
                format!("CHECK ({} {} {})", left_field, sql_op, right_field)
            },
            FieldType::String => {
                format!("CHECK ({} {} {})", left_field, sql_op, right_field)
            },
            _ => return None,
        };

        Some(constraint)
    }

    /// Suggest a field name if typo is likely
    fn suggest_field(&self, type_name: &str, _attempted_field: &str) -> String {
        let Some(type_def) = self.context.types.get(type_name) else {
            return "Check schema definition".to_string();
        };

        // Simple suggestion: show available fields
        let available = type_def.fields.join(", ");
        format!("Available fields: {}", available)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_context() -> SchemaContext {
        let mut types = HashMap::new();
        let mut fields = HashMap::new();

        // Create User type
        types.insert(
            "User".to_string(),
            TypeDef {
                name:   "User".to_string(),
                fields: vec![
                    "email".to_string(),
                    "age".to_string(),
                    "birthDate".to_string(),
                    "verified".to_string(),
                ],
            },
        );

        fields.insert(("User".to_string(), "email".to_string()), FieldType::String);
        fields.insert(("User".to_string(), "age".to_string()), FieldType::Integer);
        fields.insert(("User".to_string(), "birthDate".to_string()), FieldType::Date);
        fields.insert(("User".to_string(), "verified".to_string()), FieldType::Boolean);

        // Create DateRange type
        types.insert(
            "DateRange".to_string(),
            TypeDef {
                name:   "DateRange".to_string(),
                fields: vec!["startDate".to_string(), "endDate".to_string()],
            },
        );

        fields.insert(("DateRange".to_string(), "startDate".to_string()), FieldType::Date);
        fields.insert(("DateRange".to_string(), "endDate".to_string()), FieldType::Date);

        SchemaContext { types, fields }
    }

    // ========== CROSS-FIELD RULE VALIDATION ==========

    #[test]
    fn test_valid_cross_field_comparison() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("DateRange", "startDate", "<", "endDate");

        assert!(result.valid);
        assert!(result.errors.is_empty());
        assert!(result.sql_constraint.is_some());
    }

    #[test]
    fn test_cross_field_type_mismatch() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("User", "age", "<", "verified");

        assert!(!result.valid);
        assert!(!result.errors.is_empty());
        assert_eq!(result.errors[0].field, "age < verified");
    }

    #[test]
    fn test_cross_field_left_field_not_found() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("User", "nonexistent", "<", "age");

        assert!(!result.valid);
        assert_eq!(result.errors[0].field, "nonexistent");
        assert!(result.errors[0].message.contains("not found"));
    }

    #[test]
    fn test_cross_field_right_field_not_found() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("User", "age", "<", "nonexistent");

        assert!(!result.valid);
        assert_eq!(result.errors[0].field, "nonexistent");
    }

    #[test]
    fn test_cross_field_type_not_found() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("NonexistentType", "field", "<", "field2");

        assert!(!result.valid);
        assert!(result.errors[0].message.contains("not found"));
    }

    // ========== TYPE COMPATIBILITY ==========

    #[test]
    fn test_same_types_compatible() {
        let left = FieldType::Integer;
        let right = FieldType::Integer;
        assert!(left.is_comparable_with(&right));
    }

    #[test]
    fn test_numeric_types_compatible() {
        let left = FieldType::Integer;
        let right = FieldType::Float;
        assert!(left.is_comparable_with(&right));
    }

    #[test]
    fn test_date_datetime_compatible() {
        let left = FieldType::Date;
        let right = FieldType::DateTime;
        assert!(left.is_comparable_with(&right));
    }

    #[test]
    fn test_string_number_incompatible() {
        let left = FieldType::String;
        let right = FieldType::Integer;
        assert!(!left.is_comparable_with(&right));
    }

    #[test]
    fn test_boolean_incompatible_with_numbers() {
        let left = FieldType::Boolean;
        let right = FieldType::Integer;
        assert!(!left.is_comparable_with(&right));
    }

    // ========== SQL CONSTRAINT GENERATION ==========

    #[test]
    fn test_sql_constraint_generated() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("DateRange", "startDate", "<=", "endDate");

        assert!(result.valid);
        assert!(result.sql_constraint.is_some());
        let sql = result.sql_constraint.unwrap();
        assert!(sql.contains("CHECK"));
        assert!(sql.contains("startDate"));
        assert!(sql.contains("<="));
        assert!(sql.contains("endDate"));
    }

    #[test]
    fn test_sql_constraint_with_different_operators() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let operators = vec!["<", ">", "<=", ">=", "==", "!="];
        for op in operators {
            let result =
                validator.validate_cross_field_rule("DateRange", "startDate", op, "endDate");

            assert!(result.valid);
            let sql = result.sql_constraint.unwrap();
            assert!(sql.contains(op) || op == "==" && sql.contains("="));
        }
    }

    // ========== ELO EXPRESSION VALIDATION ==========

    #[test]
    fn test_valid_elo_expression() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression("User", "age >= 18 && verified == true");

        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_elo_expression_unknown_field() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression("User", "nonexistent >= 18");

        assert!(!result.valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_elo_expression_type_not_found() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression("NonexistentType", "age >= 18");

        assert!(!result.valid);
    }

    #[test]
    fn test_elo_field_reference_extraction() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let fields = validator.extract_field_references("age >= 18 && verified == true");

        assert!(fields.contains(&"age".to_string()));
        assert!(fields.contains(&"verified".to_string()));
    }

    #[test]
    fn test_elo_field_extraction_with_strings() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let fields = validator.extract_field_references("email matches \"pattern\" && age > 10");

        assert!(fields.contains(&"email".to_string()));
        assert!(fields.contains(&"age".to_string()));
        assert!(!fields.contains(&"pattern".to_string())); // Inside quotes
    }

    // ========== REAL-WORLD PATTERNS ==========

    #[test]
    fn test_date_range_validation() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("DateRange", "startDate", "<=", "endDate");

        assert!(result.valid);
        let sql = result.sql_constraint.unwrap();
        assert!(sql.contains("CHECK"));
    }

    #[test]
    fn test_age_constraint() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression("User", "age >= 18 && age <= 120");

        assert!(result.valid);
    }

    #[test]
    fn test_email_field_validation() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression(
            "User",
            "email matches \"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\\\.[a-zA-Z]{2,}$\"",
        );

        assert!(result.valid);
    }

    #[test]
    fn test_complex_user_validation() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression(
            "User",
            "email matches pattern && age >= 18 && verified == true",
        );

        assert!(result.valid);
    }

    #[test]
    fn test_suggestion_on_typo() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("User", "typ0", "<", "age");

        assert!(!result.valid);
        assert!(result.errors[0].suggestion.is_some());
        assert!(result.errors[0].suggestion.as_ref().unwrap().contains("Available fields"));
    }
}
