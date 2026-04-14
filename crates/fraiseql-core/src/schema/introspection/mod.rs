//! GraphQL introspection types per GraphQL spec §4.1-4.2.
//!
//! This module provides standard GraphQL introspection support, enabling
//! tools like Apollo Sandbox, `GraphiQL`, and Altair to query the schema.
//!
//! # Architecture
//!
//! FraiseQL generates introspection responses at **compile time** for performance.
//! The `IntrospectionSchema` is built from `CompiledSchema` and cached.
//!
//! # Supported Queries
//!
//! - `__schema` - Returns the full schema introspection
//! - `__type(name: String!)` - Returns a specific type's introspection
//! - `__typename` - Handled at projection level, not here
//!
//! # Module Layout
//!
//! | Sub-module | Responsibility |
//! |---|---|
//! | `types` | All `__*` introspection structs and enums |
//! | `field_resolver` | `FieldType` → `IntrospectionType` conversion, validation rules |
//! | `type_resolver` | Per-type builders (object, enum, input, interface, union, scalars) |
//! | `directive_builder` | Built-in and custom directive definitions |
//! | `schema_builder` | Root type builders, `IntrospectionBuilder`, `IntrospectionResponses` |

mod directive_builder;
mod field_resolver;
mod schema_builder;
mod type_resolver;
mod types;

// Re-export the complete public API (unchanged from the old flat module).
pub use schema_builder::{IntrospectionBuilder, IntrospectionResponses};
pub use types::{
    DirectiveLocation, IntrospectionDirective, IntrospectionEnumValue, IntrospectionField,
    IntrospectionInputValue, IntrospectionSchema, IntrospectionType, IntrospectionTypeRef,
    IntrospectionValidationRule, TypeKind,
};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::collections::HashMap;

    use indexmap::IndexMap;

    use super::*;
    use crate::schema::{AutoParams, CursorType, FieldDenyPolicy, FieldType};

    fn test_schema() -> crate::schema::CompiledSchema {
        use crate::schema::{CompiledSchema, FieldDefinition, QueryDefinition, TypeDefinition};

        let mut schema = CompiledSchema::new();

        // Add a User type
        schema.types.push(
            TypeDefinition::new("User", "v_user")
                .with_field(FieldDefinition::new("id", FieldType::Id))
                .with_field(FieldDefinition::new("name", FieldType::String))
                .with_field(FieldDefinition::nullable("email", FieldType::String))
                .with_description("A user in the system"),
        );

        // Add a users query
        schema.queries.push(QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           vec![],
            sql_source:          Some("v_user".to_string()),
            description:         Some("Get all users".to_string()),
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        });

        // Add a user query with argument
        schema.queries.push(QueryDefinition {
            name:                "user".to_string(),
            return_type:         "User".to_string(),
            returns_list:        false,
            nullable:            true,
            arguments:           vec![crate::schema::ArgumentDefinition {
                name:          "id".to_string(),
                arg_type:      FieldType::Id,
                nullable:      false, // required
                default_value: None,
                description:   Some("User ID".to_string()),
                deprecation:   None,
            }],
            sql_source:          Some("v_user".to_string()),
            description:         Some("Get user by ID".to_string()),
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        });

        schema
    }

    #[test]
    fn test_build_introspection_schema() {
        let schema = test_schema();
        let introspection = IntrospectionBuilder::build(&schema);

        // Should have Query type
        assert_eq!(introspection.query_type.name, "Query");

        // Should not have Mutation type (no mutations)
        assert!(introspection.mutation_type.is_none());

        // Should have built-in scalars
        let scalar_names: Vec<_> = introspection
            .types
            .iter()
            .filter(|t| t.kind == TypeKind::Scalar)
            .filter_map(|t| t.name.as_ref())
            .collect();
        assert!(scalar_names.contains(&&"Int".to_string()));
        assert!(scalar_names.contains(&&"String".to_string()));
        assert!(scalar_names.contains(&&"Boolean".to_string()));

        // Should have User type
        let user_type = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"User".to_string()));
        assert!(user_type.is_some());
        let user_type = user_type.unwrap();
        assert_eq!(user_type.kind, TypeKind::Object);
        assert!(user_type.fields.is_some());
        assert_eq!(user_type.fields.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_build_introspection_responses() {
        let schema = test_schema();
        let responses = IntrospectionResponses::build(&schema);

        // Should have schema response
        assert!(responses.schema_response.get("data").is_some());
        assert!(responses.schema_response["data"].get("__schema").is_some());

        // Should have type responses
        assert!(responses.type_responses.contains_key("User"));
        assert!(responses.type_responses.contains_key("Query"));
        assert!(responses.type_responses.contains_key("Int"));

        // Unknown type should return null
        let unknown = responses.get_type_response("Unknown");
        assert!(unknown["data"]["__type"].is_null());
    }

    #[test]
    fn test_query_field_introspection() {
        let schema = test_schema();
        let introspection = IntrospectionBuilder::build(&schema);

        let query_type = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Query".to_string()))
            .unwrap();

        let fields = query_type.fields.as_ref().unwrap();

        // Should have 'users' query
        let users_field = fields.iter().find(|f| f.name == "users").unwrap();
        assert_eq!(users_field.field_type.kind, TypeKind::NonNull);
        assert!(users_field.args.is_empty());

        // Should have 'user' query with argument
        let user_field = fields.iter().find(|f| f.name == "user").unwrap();
        assert!(!user_field.args.is_empty());
        assert_eq!(user_field.args[0].name, "id");
    }

    #[test]
    fn test_field_type_non_null() {
        let schema = test_schema();
        let introspection = IntrospectionBuilder::build(&schema);

        let user_type = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"User".to_string()))
            .unwrap();

        let fields = user_type.fields.as_ref().unwrap();

        // 'id' should be NON_NULL
        let id_field = fields.iter().find(|f| f.name == "id").unwrap();
        assert_eq!(id_field.field_type.kind, TypeKind::NonNull);

        // 'email' should be nullable (not wrapped in NON_NULL)
        let email_field = fields.iter().find(|f| f.name == "email").unwrap();
        assert_ne!(email_field.field_type.kind, TypeKind::NonNull);
    }

    #[test]
    fn test_deprecated_field_introspection() {
        use crate::schema::{CompiledSchema, DeprecationInfo, FieldDefinition, TypeDefinition};

        // Create a schema with a deprecated field
        let mut schema = CompiledSchema::new();
        schema.types.push(TypeDefinition {
            name:                "Product".into(),
            sql_source:          "products".into(),
            jsonb_column:        "data".to_string(),
            description:         None,
            sql_projection_hint: None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            relationships:       vec![],
            fields:              vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition {
                    name:           "oldSku".into(),
                    field_type:     FieldType::String,
                    nullable:       false,
                    description:    Some("Legacy SKU field".to_string()),
                    default_value:  None,
                    vector_config:  None,
                    alias:          None,
                    deprecation:    Some(DeprecationInfo {
                        reason: Some("Use 'sku' instead".to_string()),
                    }),
                    requires_scope: None,
                    on_deny:        FieldDenyPolicy::default(),
                    encryption:     None,
                },
                FieldDefinition::new("sku", FieldType::String),
            ],
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find Product type
        let product_type = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Product".to_string()))
            .unwrap();

        let fields = product_type.fields.as_ref().unwrap();

        // 'oldSku' should be deprecated
        let old_sku_field = fields.iter().find(|f| f.name == "oldSku").unwrap();
        assert!(old_sku_field.is_deprecated);
        assert_eq!(old_sku_field.deprecation_reason, Some("Use 'sku' instead".to_string()));

        // 'sku' should NOT be deprecated
        let sku_field = fields.iter().find(|f| f.name == "sku").unwrap();
        assert!(!sku_field.is_deprecated);
        assert!(sku_field.deprecation_reason.is_none());

        // 'id' should NOT be deprecated
        let id_field = fields.iter().find(|f| f.name == "id").unwrap();
        assert!(!id_field.is_deprecated);
        assert!(id_field.deprecation_reason.is_none());
    }

    #[test]
    fn test_enum_type_introspection() {
        use crate::schema::{CompiledSchema, DeprecationInfo, EnumDefinition, EnumValueDefinition};

        let mut schema = CompiledSchema::new();

        // Add an enum type with some values, one deprecated
        schema.enums.push(EnumDefinition {
            name:        "OrderStatus".to_string(),
            description: Some("Status of an order".to_string()),
            values:      vec![
                EnumValueDefinition {
                    name:        "PENDING".to_string(),
                    description: Some("Order is pending".to_string()),
                    deprecation: None,
                },
                EnumValueDefinition {
                    name:        "PROCESSING".to_string(),
                    description: None,
                    deprecation: None,
                },
                EnumValueDefinition {
                    name:        "SHIPPED".to_string(),
                    description: None,
                    deprecation: None,
                },
                EnumValueDefinition {
                    name:        "CANCELLED".to_string(),
                    description: Some("Order was cancelled".to_string()),
                    deprecation: Some(DeprecationInfo {
                        reason: Some("Use REFUNDED instead".to_string()),
                    }),
                },
            ],
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find OrderStatus enum
        let order_status = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"OrderStatus".to_string()))
            .unwrap();

        assert_eq!(order_status.kind, TypeKind::Enum);
        assert_eq!(order_status.description, Some("Status of an order".to_string()));

        // Should have enum_values
        let enum_values = order_status.enum_values.as_ref().unwrap();
        assert_eq!(enum_values.len(), 4);

        // Check PENDING value
        let pending = enum_values.iter().find(|v| v.name == "PENDING").unwrap();
        assert_eq!(pending.description, Some("Order is pending".to_string()));
        assert!(!pending.is_deprecated);
        assert!(pending.deprecation_reason.is_none());

        // Check CANCELLED value (deprecated)
        let cancelled = enum_values.iter().find(|v| v.name == "CANCELLED").unwrap();
        assert!(cancelled.is_deprecated);
        assert_eq!(cancelled.deprecation_reason, Some("Use REFUNDED instead".to_string()));

        // Enum should not have fields
        assert!(order_status.fields.is_none());
    }

    #[test]
    fn test_input_object_introspection() {
        use crate::schema::{CompiledSchema, InputFieldDefinition, InputObjectDefinition};

        let mut schema = CompiledSchema::new();

        // Add an input object type
        schema.input_types.push(InputObjectDefinition {
            name:        "UserFilter".to_string(),
            description: Some("Filter for user queries".to_string()),
            fields:      vec![
                InputFieldDefinition {
                    name:             "name".to_string(),
                    field_type:       "String".to_string(),
                    description:      Some("Filter by name".to_string()),
                    default_value:    None,
                    deprecation:      None,
                    validation_rules: Vec::new(),
                },
                InputFieldDefinition {
                    name:             "email".to_string(),
                    field_type:       "String".to_string(),
                    description:      None,
                    default_value:    None,
                    deprecation:      None,
                    validation_rules: Vec::new(),
                },
                InputFieldDefinition {
                    name:             "limit".to_string(),
                    field_type:       "Int".to_string(),
                    description:      Some("Max results".to_string()),
                    default_value:    Some("10".to_string()),
                    deprecation:      None,
                    validation_rules: Vec::new(),
                },
            ],
            metadata:    None,
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find UserFilter input type
        let user_filter = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"UserFilter".to_string()))
            .unwrap();

        assert_eq!(user_filter.kind, TypeKind::InputObject);
        assert_eq!(user_filter.description, Some("Filter for user queries".to_string()));

        // Should have input_fields
        let input_fields = user_filter.input_fields.as_ref().unwrap();
        assert_eq!(input_fields.len(), 3);

        // Check name field
        let name_field = input_fields.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(name_field.description, Some("Filter by name".to_string()));
        assert!(name_field.default_value.is_none());

        // Check limit field with default value
        let limit_field = input_fields.iter().find(|f| f.name == "limit").unwrap();
        assert_eq!(limit_field.description, Some("Max results".to_string()));
        assert_eq!(limit_field.default_value, Some("10".to_string()));

        // Input object should not have fields
        assert!(user_filter.fields.is_none());
    }

    #[test]
    fn test_enum_in_type_map() {
        use crate::schema::{CompiledSchema, EnumDefinition};

        let mut schema = CompiledSchema::new();
        schema.enums.push(EnumDefinition {
            name:        "Status".to_string(),
            description: None,
            values:      vec![],
        });

        let introspection = IntrospectionBuilder::build(&schema);
        let type_map = IntrospectionBuilder::build_type_map(&introspection);

        // Enum should be in the type map
        assert!(type_map.contains_key("Status"));
        let status = type_map.get("Status").unwrap();
        assert_eq!(status.kind, TypeKind::Enum);
    }

    #[test]
    fn test_input_object_in_type_map() {
        use crate::schema::{CompiledSchema, InputObjectDefinition};

        let mut schema = CompiledSchema::new();
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateUserInput".to_string(),
            description: None,
            fields:      vec![],
            metadata:    None,
        });

        let introspection = IntrospectionBuilder::build(&schema);
        let type_map = IntrospectionBuilder::build_type_map(&introspection);

        // Input object should be in the type map
        assert!(type_map.contains_key("CreateUserInput"));
        let input = type_map.get("CreateUserInput").unwrap();
        assert_eq!(input.kind, TypeKind::InputObject);
    }

    #[test]
    fn test_interface_introspection() {
        use crate::schema::{CompiledSchema, FieldDefinition, InterfaceDefinition, TypeDefinition};

        let mut schema = CompiledSchema::new();

        // Add a Node interface
        schema.interfaces.push(InterfaceDefinition {
            name:        "Node".to_string(),
            description: Some("An object with a globally unique ID".to_string()),
            fields:      vec![FieldDefinition::new("id", FieldType::Id)],
        });

        // Add types that implement the interface
        schema.types.push(TypeDefinition {
            name:                "User".into(),
            sql_source:          "users".into(),
            jsonb_column:        "data".to_string(),
            description:         Some("A user".to_string()),
            sql_projection_hint: None,
            implements:          vec!["Node".to_string()],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            relationships:       vec![],
            fields:              vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("name", FieldType::String),
            ],
        });

        schema.types.push(TypeDefinition {
            name:                "Post".into(),
            sql_source:          "posts".into(),
            jsonb_column:        "data".to_string(),
            description:         Some("A blog post".to_string()),
            sql_projection_hint: None,
            implements:          vec!["Node".to_string()],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            relationships:       vec![],
            fields:              vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("title", FieldType::String),
            ],
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find Node interface
        let node = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Node".to_string()))
            .unwrap();

        assert_eq!(node.kind, TypeKind::Interface);
        assert_eq!(node.description, Some("An object with a globally unique ID".to_string()));

        // Interface should have fields
        let fields = node.fields.as_ref().unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "id");

        // Interface should have possible_types (implementors)
        let possible_types = node.possible_types.as_ref().unwrap();
        assert_eq!(possible_types.len(), 2);
        assert!(possible_types.iter().any(|t| t.name == "User"));
        assert!(possible_types.iter().any(|t| t.name == "Post"));

        // Interface should not have enum_values or input_fields
        assert!(node.enum_values.is_none());
        assert!(node.input_fields.is_none());
    }

    #[test]
    fn test_type_implements_interface() {
        use crate::schema::{CompiledSchema, FieldDefinition, InterfaceDefinition, TypeDefinition};

        let mut schema = CompiledSchema::new();

        // Add interfaces
        schema.interfaces.push(InterfaceDefinition {
            name:        "Node".to_string(),
            description: None,
            fields:      vec![FieldDefinition::new("id", FieldType::Id)],
        });

        schema.interfaces.push(InterfaceDefinition {
            name:        "Timestamped".to_string(),
            description: None,
            fields:      vec![FieldDefinition::new("createdAt", FieldType::DateTime)],
        });

        // Add a type that implements both interfaces
        schema.types.push(TypeDefinition {
            name:                "Comment".into(),
            sql_source:          "comments".into(),
            jsonb_column:        "data".to_string(),
            description:         None,
            sql_projection_hint: None,
            implements:          vec!["Node".to_string(), "Timestamped".to_string()],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            relationships:       vec![],
            fields:              vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("createdAt", FieldType::DateTime),
                FieldDefinition::new("text", FieldType::String),
            ],
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find Comment type
        let comment = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Comment".to_string()))
            .unwrap();

        assert_eq!(comment.kind, TypeKind::Object);

        // Type should list interfaces it implements
        let interfaces = comment.interfaces.as_ref().unwrap();
        assert_eq!(interfaces.len(), 2);
        assert!(interfaces.iter().any(|i| i.name == "Node"));
        assert!(interfaces.iter().any(|i| i.name == "Timestamped"));
    }

    #[test]
    fn test_interface_in_type_map() {
        use crate::schema::{CompiledSchema, InterfaceDefinition};

        let mut schema = CompiledSchema::new();
        schema.interfaces.push(InterfaceDefinition {
            name:        "Searchable".to_string(),
            description: None,
            fields:      vec![],
        });

        let introspection = IntrospectionBuilder::build(&schema);
        let type_map = IntrospectionBuilder::build_type_map(&introspection);

        // Interface should be in the type map
        assert!(type_map.contains_key("Searchable"));
        let interface = type_map.get("Searchable").unwrap();
        assert_eq!(interface.kind, TypeKind::Interface);
    }

    #[test]
    fn test_filter_deprecated_fields() {
        // Create a type with some deprecated fields
        let introspection_type = IntrospectionType {
            kind:               TypeKind::Object,
            name:               Some("TestType".to_string()),
            description:        None,
            fields:             Some(vec![
                IntrospectionField {
                    name:               "id".to_string(),
                    description:        None,
                    args:               vec![],
                    field_type:         IntrospectionBuilder::type_ref("ID"),
                    is_deprecated:      false,
                    deprecation_reason: None,
                },
                IntrospectionField {
                    name:               "oldField".to_string(),
                    description:        None,
                    args:               vec![],
                    field_type:         IntrospectionBuilder::type_ref("String"),
                    is_deprecated:      true,
                    deprecation_reason: Some("Use newField instead".to_string()),
                },
                IntrospectionField {
                    name:               "newField".to_string(),
                    description:        None,
                    args:               vec![],
                    field_type:         IntrospectionBuilder::type_ref("String"),
                    is_deprecated:      false,
                    deprecation_reason: None,
                },
            ]),
            interfaces:         None,
            possible_types:     None,
            enum_values:        None,
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        };

        // With includeDeprecated = false, should only have 2 fields
        let filtered = introspection_type.filter_deprecated_fields(false);
        let fields = filtered.fields.as_ref().unwrap();
        assert_eq!(fields.len(), 2);
        assert!(fields.iter().any(|f| f.name == "id"));
        assert!(fields.iter().any(|f| f.name == "newField"));
        assert!(!fields.iter().any(|f| f.name == "oldField"));

        // With includeDeprecated = true, should have all 3 fields
        let unfiltered = introspection_type.filter_deprecated_fields(true);
        let fields = unfiltered.fields.as_ref().unwrap();
        assert_eq!(fields.len(), 3);
    }

    #[test]
    fn test_filter_deprecated_enum_values() {
        // Create an enum type with some deprecated values
        let introspection_type = IntrospectionType {
            kind:               TypeKind::Enum,
            name:               Some("Status".to_string()),
            description:        None,
            fields:             None,
            interfaces:         None,
            possible_types:     None,
            enum_values:        Some(vec![
                IntrospectionEnumValue {
                    name:               "ACTIVE".to_string(),
                    description:        None,
                    is_deprecated:      false,
                    deprecation_reason: None,
                },
                IntrospectionEnumValue {
                    name:               "INACTIVE".to_string(),
                    description:        None,
                    is_deprecated:      true,
                    deprecation_reason: Some("Use DISABLED instead".to_string()),
                },
                IntrospectionEnumValue {
                    name:               "DISABLED".to_string(),
                    description:        None,
                    is_deprecated:      false,
                    deprecation_reason: None,
                },
            ]),
            input_fields:       None,
            of_type:            None,
            specified_by_u_r_l: None,
        };

        // With includeDeprecated = false, should only have 2 values
        let filtered = introspection_type.filter_deprecated_enum_values(false);
        let values = filtered.enum_values.as_ref().unwrap();
        assert_eq!(values.len(), 2);
        assert!(values.iter().any(|v| v.name == "ACTIVE"));
        assert!(values.iter().any(|v| v.name == "DISABLED"));
        assert!(!values.iter().any(|v| v.name == "INACTIVE"));

        // With includeDeprecated = true, should have all 3 values
        let unfiltered = introspection_type.filter_deprecated_enum_values(true);
        let values = unfiltered.enum_values.as_ref().unwrap();
        assert_eq!(values.len(), 3);
    }

    #[test]
    fn test_specified_by_url_for_custom_scalars() {
        use crate::schema::CompiledSchema;

        let schema = CompiledSchema::new();
        let introspection = IntrospectionBuilder::build(&schema);

        // Find DateTime scalar
        let datetime = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"DateTime".to_string()))
            .unwrap();

        assert_eq!(datetime.kind, TypeKind::Scalar);
        assert!(datetime.specified_by_u_r_l.is_some());
        assert!(datetime.specified_by_u_r_l.as_ref().unwrap().contains("date-time"));

        // Find UUID scalar
        let uuid = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"UUID".to_string()))
            .unwrap();

        assert_eq!(uuid.kind, TypeKind::Scalar);
        assert!(uuid.specified_by_u_r_l.is_some());
        assert!(uuid.specified_by_u_r_l.as_ref().unwrap().contains("rfc4122"));

        // Built-in scalars (Int, String, etc.) should NOT have specifiedByURL
        let int = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Int".to_string()))
            .unwrap();

        assert_eq!(int.kind, TypeKind::Scalar);
        assert!(int.specified_by_u_r_l.is_none());
    }

    #[test]
    fn test_deprecated_query_introspection() {
        use crate::schema::{
            ArgumentDefinition, AutoParams, CompiledSchema, DeprecationInfo, QueryDefinition,
        };

        let mut schema = CompiledSchema::new();

        // Add a deprecated query
        schema.queries.push(QueryDefinition {
            name:                "oldUsers".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           vec![],
            sql_source:          Some("v_user".to_string()),
            description:         Some("Old way to get users".to_string()),
            auto_params:         AutoParams::default(),
            deprecation:         Some(DeprecationInfo {
                reason: Some("Use 'users' instead".to_string()),
            }),
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        });

        // Add a non-deprecated query with a deprecated argument
        schema.queries.push(QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           vec![
                ArgumentDefinition {
                    name:          "first".to_string(),
                    arg_type:      FieldType::Int,
                    nullable:      true,
                    default_value: None,
                    description:   Some("Number of results to return".to_string()),
                    deprecation:   None,
                },
                ArgumentDefinition {
                    name:          "limit".to_string(),
                    arg_type:      FieldType::Int,
                    nullable:      true,
                    default_value: None,
                    description:   Some("Old parameter for limiting results".to_string()),
                    deprecation:   Some(DeprecationInfo {
                        reason: Some("Use 'first' instead".to_string()),
                    }),
                },
            ],
            sql_source:          Some("v_user".to_string()),
            description:         Some("Get users with pagination".to_string()),
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find Query type
        let query_type = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"Query".to_string()))
            .unwrap();

        let fields = query_type.fields.as_ref().unwrap();

        // 'oldUsers' should be deprecated
        let old_users = fields.iter().find(|f| f.name == "oldUsers").unwrap();
        assert!(old_users.is_deprecated);
        assert_eq!(old_users.deprecation_reason, Some("Use 'users' instead".to_string()));

        // 'users' should NOT be deprecated
        let users = fields.iter().find(|f| f.name == "users").unwrap();
        assert!(!users.is_deprecated);
        assert!(users.deprecation_reason.is_none());

        // 'users' should have 2 arguments
        assert_eq!(users.args.len(), 2);

        // 'first' argument should NOT be deprecated
        let first_arg = users.args.iter().find(|a| a.name == "first").unwrap();
        assert!(!first_arg.is_deprecated);
        assert!(first_arg.deprecation_reason.is_none());

        // 'limit' argument should be deprecated
        let limit_arg = users.args.iter().find(|a| a.name == "limit").unwrap();
        assert!(limit_arg.is_deprecated);
        assert_eq!(limit_arg.deprecation_reason, Some("Use 'first' instead".to_string()));
    }

    #[test]
    fn test_deprecated_input_field_introspection() {
        use crate::schema::{
            CompiledSchema, DeprecationInfo, InputFieldDefinition, InputObjectDefinition,
        };

        let mut schema = CompiledSchema::new();

        // Add an input type with a deprecated field
        schema.input_types.push(InputObjectDefinition {
            name:        "CreateUserInput".to_string(),
            description: Some("Input for creating a user".to_string()),
            fields:      vec![
                InputFieldDefinition {
                    name:             "name".to_string(),
                    field_type:       "String!".to_string(),
                    default_value:    None,
                    description:      Some("User name".to_string()),
                    deprecation:      None,
                    validation_rules: Vec::new(),
                },
                InputFieldDefinition {
                    name:             "oldEmail".to_string(),
                    field_type:       "String".to_string(),
                    default_value:    None,
                    description:      Some("Legacy email field".to_string()),
                    deprecation:      Some(DeprecationInfo {
                        reason: Some("Use 'email' instead".to_string()),
                    }),
                    validation_rules: Vec::new(),
                },
            ],
            metadata:    None,
        });

        let introspection = IntrospectionBuilder::build(&schema);

        // Find CreateUserInput type
        let create_user_input = introspection
            .types
            .iter()
            .find(|t| t.name.as_ref() == Some(&"CreateUserInput".to_string()))
            .unwrap();

        let input_fields = create_user_input.input_fields.as_ref().unwrap();

        // 'name' should NOT be deprecated
        let name_field = input_fields.iter().find(|f| f.name == "name").unwrap();
        assert!(!name_field.is_deprecated);
        assert!(name_field.deprecation_reason.is_none());

        // 'oldEmail' should be deprecated
        let old_email = input_fields.iter().find(|f| f.name == "oldEmail").unwrap();
        assert!(old_email.is_deprecated);
        assert_eq!(old_email.deprecation_reason, Some("Use 'email' instead".to_string()));
    }
}
