//! Code generator - produces final CompiledSchema JSON.
//!
//! # Overview
//!
//! Takes validated IR and SQL templates, produces runtime-optimized
//! CompiledSchema ready for execution.

use std::collections::HashSet;

use super::{
    ir::{
        AuthoringIR, IRArgument, IREnum, IREnumValue, IRField, IRInputField, IRInputType,
        IRInterface, IRMutation, IRUnion,
    },
    lowering::SqlTemplate,
};
use crate::{
    error::Result,
    schema::{
        ArgumentDefinition, AutoParams as SchemaAutoParams, CompiledSchema, DeprecationInfo,
        EnumDefinition, EnumValueDefinition, FieldDefinition, FieldType, InputFieldDefinition,
        InputObjectDefinition, InterfaceDefinition, MutationDefinition, QueryDefinition,
        SubscriptionDefinition, TypeDefinition, UnionDefinition,
    },
};

/// Code generator.
pub struct CodeGenerator {
    optimize: bool,
}

impl CodeGenerator {
    /// Create new code generator.
    #[must_use]
    pub fn new(optimize: bool) -> Self {
        Self { optimize }
    }

    /// Generate CompiledSchema from IR and SQL templates.
    ///
    /// # Arguments
    ///
    /// * `ir` - Validated IR
    /// * `templates` - SQL templates
    ///
    /// # Returns
    ///
    /// CompiledSchema ready for runtime execution
    ///
    /// # Errors
    ///
    /// Returns error if code generation fails.
    pub fn generate(&self, ir: &AuthoringIR, _templates: &[SqlTemplate]) -> Result<CompiledSchema> {
        // Build set of known type names for field type parsing
        let known_types: HashSet<String> = ir.types.iter().map(|t| t.name.clone()).collect();

        let types = ir
            .types
            .iter()
            .map(|t| {
                TypeDefinition {
                    name:                t.name.clone(),
                    sql_source:          t.sql_source.clone().unwrap_or_else(|| t.name.clone()),
                    jsonb_column:        "data".to_string(),
                    fields:              Self::map_fields(&t.fields, &known_types),
                    description:         t.description.clone(),
                    sql_projection_hint: None, // Populated during optimization pass
                    implements:          Vec::new(), /* Note: IR doesn't have interface
                                                * implementation yet */
                }
            })
            .collect();

        let queries = ir
            .queries
            .iter()
            .map(|q| {
                QueryDefinition {
                    name:         q.name.clone(),
                    return_type:  q.return_type.clone(),
                    returns_list: q.returns_list,
                    nullable:     q.nullable,
                    arguments:    Self::map_arguments(&q.arguments, &known_types),
                    sql_source:   q.sql_source.clone(),
                    description:  q.description.clone(),
                    auto_params:  SchemaAutoParams {
                        has_where:    q.auto_params.has_where,
                        has_order_by: q.auto_params.has_order_by,
                        has_limit:    q.auto_params.has_limit,
                        has_offset:   q.auto_params.has_offset,
                    },
                    deprecation:  None, // Note: IR doesn't have deprecation info yet
                }
            })
            .collect();

        let mutations = ir.mutations.iter().map(|m| Self::map_mutation(m, &known_types)).collect();

        let subscriptions = ir
            .subscriptions
            .iter()
            .map(|s| {
                SubscriptionDefinition {
                    name:        s.name.clone(),
                    return_type: s.return_type.clone(),
                    arguments:   Self::map_arguments(&s.arguments, &known_types),
                    description: s.description.clone(),
                    topic:       None, // Populated from decorator topic binding
                    filter:      None, // Populated from decorator filters
                    fields:      Vec::new(), // Populated from decorator field selection
                    deprecation: None, // Note: IR subscriptions don't have deprecation yet
                }
            })
            .collect();

        // Map enums
        let enums = ir.enums.iter().map(|e| Self::map_enum(e)).collect();

        // Map interfaces
        let interfaces =
            ir.interfaces.iter().map(|i| Self::map_interface(i, &known_types)).collect();

        // Map unions
        let unions = ir.unions.iter().map(|u| Self::map_union(u)).collect();

        // Map input types
        let input_types =
            ir.input_types.iter().map(|i| Self::map_input_type(i, &known_types)).collect();

        Ok(CompiledSchema {
            types,
            enums,
            input_types,
            interfaces,
            unions,
            queries,
            mutations,
            subscriptions,
            directives: Vec::new(), // Note: IR doesn't have custom directive definitions yet
            fact_tables: std::collections::HashMap::new(), /* Populated by compiler from
                                     * ir.fact_tables */
        })
    }

    /// Map IR fields to compiled schema fields.
    fn map_fields(ir_fields: &[IRField], known_types: &HashSet<String>) -> Vec<FieldDefinition> {
        ir_fields
            .iter()
            .map(|f| {
                let field_type = FieldType::parse(&f.field_type, known_types);
                FieldDefinition {
                    name: f.name.clone(),
                    field_type,
                    nullable: f.nullable,
                    description: f.description.clone(),
                    default_value: None,  // Fields don't have defaults in GraphQL
                    vector_config: None,  // Would be set if field_type is Vector
                    alias: None,          // Aliases come from query, not schema
                    deprecation: None,    // Note: IR fields don't have deprecation yet
                    requires_scope: None, // Note: IR fields don't have scope requirements yet
                }
            })
            .collect()
    }

    /// Map IR arguments to compiled schema arguments.
    fn map_arguments(
        ir_args: &[IRArgument],
        known_types: &HashSet<String>,
    ) -> Vec<ArgumentDefinition> {
        ir_args
            .iter()
            .map(|a| {
                let arg_type = FieldType::parse(&a.arg_type, known_types);
                ArgumentDefinition {
                    name: a.name.clone(),
                    arg_type,
                    nullable: a.nullable,
                    default_value: a.default_value.clone(),
                    description: a.description.clone(),
                    deprecation: None, // Note: IR args don't have deprecation yet
                }
            })
            .collect()
    }

    /// Map IR mutation to compiled schema mutation.
    fn map_mutation(m: &IRMutation, known_types: &HashSet<String>) -> MutationDefinition {
        use super::ir::MutationOperation as IRMutationOp;
        use crate::schema::MutationOperation;

        // The compiled schema MutationOperation needs a table name for Insert/Update/Delete
        // Since IR doesn't have this, we use Custom as default or derive from return type
        let operation = match m.operation {
            IRMutationOp::Create => MutationOperation::Insert {
                table: m.return_type.to_lowercase(), // Infer table from return type
            },
            IRMutationOp::Update => MutationOperation::Update {
                table: m.return_type.to_lowercase(),
            },
            IRMutationOp::Delete => MutationOperation::Delete {
                table: m.return_type.to_lowercase(),
            },
            IRMutationOp::Custom => MutationOperation::Custom,
        };

        MutationDefinition {
            name: m.name.clone(),
            return_type: m.return_type.clone(),
            arguments: Self::map_arguments(&m.arguments, known_types),
            description: m.description.clone(),
            operation,
            deprecation: None, // Note: IR mutations don't have deprecation yet
        }
    }

    /// Map IR enum to compiled schema enum.
    fn map_enum(e: &IREnum) -> EnumDefinition {
        EnumDefinition {
            name:        e.name.clone(),
            values:      e.values.iter().map(|v| Self::map_enum_value(v)).collect(),
            description: e.description.clone(),
        }
    }

    /// Map IR enum value to compiled schema enum value.
    fn map_enum_value(v: &IREnumValue) -> EnumValueDefinition {
        EnumValueDefinition {
            name:        v.name.clone(),
            description: v.description.clone(),
            deprecation: v.deprecation_reason.as_ref().map(|reason| DeprecationInfo {
                reason: Some(reason.clone()),
            }),
        }
    }

    /// Map IR interface to compiled schema interface.
    fn map_interface(i: &IRInterface, known_types: &HashSet<String>) -> InterfaceDefinition {
        InterfaceDefinition {
            name:        i.name.clone(),
            fields:      Self::map_fields(&i.fields, known_types),
            description: i.description.clone(),
        }
    }

    /// Map IR union to compiled schema union.
    fn map_union(u: &IRUnion) -> UnionDefinition {
        UnionDefinition {
            name:         u.name.clone(),
            member_types: u.types.clone(),
            description:  u.description.clone(),
        }
    }

    /// Map IR input type to compiled schema input object.
    fn map_input_type(i: &IRInputType, known_types: &HashSet<String>) -> InputObjectDefinition {
        InputObjectDefinition {
            name:        i.name.clone(),
            fields:      Self::map_input_fields(&i.fields, known_types),
            description: i.description.clone(),
        }
    }

    /// Map IR input fields to compiled schema input fields.
    fn map_input_fields(
        ir_fields: &[IRInputField],
        _known_types: &HashSet<String>,
    ) -> Vec<InputFieldDefinition> {
        ir_fields
            .iter()
            .map(|f| {
                InputFieldDefinition {
                    name:          f.name.clone(),
                    field_type:    f.field_type.clone(), /* InputFieldDefinition uses String,
                                                          * not FieldType */
                    description:   f.description.clone(),
                    default_value: f.default_value.as_ref().map(|v| v.to_string()),
                    deprecation:   None, // Note: IR input fields don't have deprecation yet
                }
            })
            .collect()
    }

    /// Check if optimization is enabled.
    #[must_use]
    pub const fn optimize(&self) -> bool {
        self.optimize
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::ir::{AutoParams, IRArgument, IRField, IRQuery, IRSubscription, IRType},
        *,
    };

    #[test]
    fn test_code_generator_new() {
        let generator = CodeGenerator::new(true);
        assert!(generator.optimize());

        let generator = CodeGenerator::new(false);
        assert!(!generator.optimize());
    }

    #[test]
    fn test_generate_empty_schema() {
        let generator = CodeGenerator::new(true);
        let ir = AuthoringIR::new();
        let templates = Vec::new();

        let result = generator.generate(&ir, &templates);
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert!(schema.types.is_empty());
        assert!(schema.queries.is_empty());
    }

    #[test]
    fn test_generate_types_with_fields() {
        let generator = CodeGenerator::new(true);
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "User".to_string(),
            fields:      vec![
                IRField {
                    name:        "id".to_string(),
                    field_type:  "ID!".to_string(),
                    nullable:    false,
                    description: Some("User ID".to_string()),
                    sql_column:  Some("id".to_string()),
                },
                IRField {
                    name:        "name".to_string(),
                    field_type:  "String".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "age".to_string(),
                    field_type:  "Int".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  Some("v_user".to_string()),
            description: Some("User type".to_string()),
        });

        let result = generator.generate(&ir, &[]);
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert_eq!(schema.types.len(), 1);

        let user_type = &schema.types[0];
        assert_eq!(user_type.name, "User");
        assert_eq!(user_type.sql_source, "v_user");
        assert_eq!(user_type.fields.len(), 3);

        // Check field types were parsed correctly
        assert_eq!(user_type.fields[0].name, "id");
        assert_eq!(user_type.fields[0].field_type, FieldType::Id);
        assert!(!user_type.fields[0].nullable);

        assert_eq!(user_type.fields[1].name, "name");
        assert_eq!(user_type.fields[1].field_type, FieldType::String);

        assert_eq!(user_type.fields[2].name, "age");
        assert_eq!(user_type.fields[2].field_type, FieldType::Int);
    }

    #[test]
    fn test_generate_queries_with_arguments() {
        let generator = CodeGenerator::new(true);
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "User".to_string(),
            fields:      vec![],
            sql_source:  None,
            description: None,
        });

        ir.queries.push(IRQuery {
            name:         "user".to_string(),
            return_type:  "User".to_string(),
            returns_list: false,
            nullable:     true,
            arguments:    vec![IRArgument {
                name:          "id".to_string(),
                arg_type:      "ID!".to_string(),
                nullable:      false,
                default_value: None,
                description:   Some("User ID to fetch".to_string()),
            }],
            sql_source:   Some("v_user".to_string()),
            description:  Some("Fetch a single user".to_string()),
            auto_params:  AutoParams::default(),
        });

        ir.queries.push(IRQuery {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![IRArgument {
                name:          "limit".to_string(),
                arg_type:      "Int".to_string(),
                nullable:      true,
                default_value: Some(serde_json::json!(10)),
                description:   None,
            }],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams {
                has_where:    true,
                has_order_by: true,
                has_limit:    true,
                has_offset:   true,
            },
        });

        let result = generator.generate(&ir, &[]);
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert_eq!(schema.queries.len(), 2);

        // Check single user query
        let user_query = &schema.queries[0];
        assert_eq!(user_query.name, "user");
        assert!(!user_query.returns_list);
        assert!(user_query.nullable);
        assert_eq!(user_query.arguments.len(), 1);
        assert_eq!(user_query.arguments[0].name, "id");
        assert_eq!(user_query.arguments[0].arg_type, FieldType::Id);

        // Check users query with auto_params
        let users_query = &schema.queries[1];
        assert_eq!(users_query.name, "users");
        assert!(users_query.returns_list);
        assert!(users_query.auto_params.has_where);
        assert!(users_query.auto_params.has_order_by);
        assert_eq!(users_query.arguments[0].default_value, Some(serde_json::json!(10)));
    }

    #[test]
    fn test_generate_mutations() {
        use super::super::ir::MutationOperation as IRMutationOp;

        let generator = CodeGenerator::new(true);
        let mut ir = AuthoringIR::new();

        ir.mutations.push(IRMutation {
            name:        "createUser".to_string(),
            return_type: "User".to_string(),
            nullable:    false,
            arguments:   vec![IRArgument {
                name:          "name".to_string(),
                arg_type:      "String!".to_string(),
                nullable:      false,
                default_value: None,
                description:   None,
            }],
            description: Some("Create a new user".to_string()),
            operation:   IRMutationOp::Create,
        });

        let result = generator.generate(&ir, &[]);
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert_eq!(schema.mutations.len(), 1);

        let mutation = &schema.mutations[0];
        assert_eq!(mutation.name, "createUser");
        // Insert operation should be inferred from Create
        assert!(matches!(
            &mutation.operation,
            crate::schema::MutationOperation::Insert { table } if table == "user"
        ));
        assert_eq!(mutation.arguments.len(), 1);
    }

    #[test]
    fn test_generate_subscriptions() {
        let generator = CodeGenerator::new(true);
        let mut ir = AuthoringIR::new();

        ir.subscriptions.push(IRSubscription {
            name:        "userCreated".to_string(),
            return_type: "User".to_string(),
            arguments:   vec![IRArgument {
                name:          "tenantId".to_string(),
                arg_type:      "ID!".to_string(),
                nullable:      false,
                default_value: None,
                description:   None,
            }],
            description: Some("Subscribe to user creation events".to_string()),
        });

        let result = generator.generate(&ir, &[]);
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert_eq!(schema.subscriptions.len(), 1);

        let subscription = &schema.subscriptions[0];
        assert_eq!(subscription.name, "userCreated");
        assert_eq!(subscription.return_type, "User");
        assert_eq!(subscription.arguments.len(), 1);
        assert_eq!(subscription.arguments[0].name, "tenantId");
    }

    #[test]
    fn test_field_type_parsing_list_types() {
        let generator = CodeGenerator::new(true);
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "Post".to_string(),
            fields:      vec![
                IRField {
                    name:        "tags".to_string(),
                    field_type:  "[String]".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "comments".to_string(),
                    field_type:  "[Comment!]!".to_string(),
                    nullable:    false,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  None,
            description: None,
        });

        let result = generator.generate(&ir, &[]);
        assert!(result.is_ok());

        let schema = result.unwrap();
        let post_type = &schema.types[0];

        // [String] -> List(String)
        assert!(matches!(
            &post_type.fields[0].field_type,
            FieldType::List(inner) if **inner == FieldType::String
        ));

        // [Comment!]! -> List(Object("Comment"))
        assert!(matches!(
            &post_type.fields[1].field_type,
            FieldType::List(inner) if matches!(**inner, FieldType::Object(ref name) if name == "Comment")
        ));
    }

    #[test]
    fn test_generate_enums() {
        use super::super::ir::{IREnum, IREnumValue};

        let generator = CodeGenerator::new(true);
        let mut ir = AuthoringIR::new();

        ir.enums.push(IREnum {
            name:        "OrderStatus".to_string(),
            values:      vec![
                IREnumValue {
                    name:               "PENDING".to_string(),
                    description:        Some("Order is pending".to_string()),
                    deprecation_reason: None,
                },
                IREnumValue {
                    name:               "COMPLETED".to_string(),
                    description:        None,
                    deprecation_reason: None,
                },
                IREnumValue {
                    name:               "CANCELLED".to_string(),
                    description:        None,
                    deprecation_reason: Some("Use REJECTED instead".to_string()),
                },
            ],
            description: Some("Possible order statuses".to_string()),
        });

        let result = generator.generate(&ir, &[]);
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert_eq!(schema.enums.len(), 1);

        let order_status = &schema.enums[0];
        assert_eq!(order_status.name, "OrderStatus");
        assert_eq!(order_status.values.len(), 3);
        assert_eq!(order_status.values[0].name, "PENDING");
        assert_eq!(order_status.values[0].description, Some("Order is pending".to_string()));
        assert!(order_status.values[2].deprecation.is_some());
    }

    #[test]
    fn test_generate_interfaces() {
        use super::super::ir::IRInterface;

        let generator = CodeGenerator::new(true);
        let mut ir = AuthoringIR::new();

        ir.interfaces.push(IRInterface {
            name:        "Node".to_string(),
            fields:      vec![IRField {
                name:        "id".to_string(),
                field_type:  "ID!".to_string(),
                nullable:    false,
                description: Some("Unique identifier".to_string()),
                sql_column:  None,
            }],
            description: Some("An object with an ID".to_string()),
        });

        let result = generator.generate(&ir, &[]);
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert_eq!(schema.interfaces.len(), 1);

        let node = &schema.interfaces[0];
        assert_eq!(node.name, "Node");
        assert_eq!(node.fields.len(), 1);
        assert_eq!(node.fields[0].name, "id");
        assert_eq!(node.fields[0].field_type, FieldType::Id);
    }

    #[test]
    fn test_generate_unions() {
        use super::super::ir::IRUnion;

        let generator = CodeGenerator::new(true);
        let mut ir = AuthoringIR::new();

        ir.unions.push(IRUnion {
            name:        "SearchResult".to_string(),
            types:       vec![
                "User".to_string(),
                "Post".to_string(),
                "Comment".to_string(),
            ],
            description: Some("Possible search result types".to_string()),
        });

        let result = generator.generate(&ir, &[]);
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert_eq!(schema.unions.len(), 1);

        let search_result = &schema.unions[0];
        assert_eq!(search_result.name, "SearchResult");
        assert_eq!(search_result.member_types.len(), 3);
        assert_eq!(search_result.member_types[0], "User");
    }

    #[test]
    fn test_generate_input_types() {
        use super::super::ir::{IRInputField, IRInputType};

        let generator = CodeGenerator::new(true);
        let mut ir = AuthoringIR::new();

        ir.input_types.push(IRInputType {
            name:        "CreateUserInput".to_string(),
            fields:      vec![
                IRInputField {
                    name:          "name".to_string(),
                    field_type:    "String!".to_string(),
                    nullable:      false,
                    default_value: None,
                    description:   Some("User's name".to_string()),
                },
                IRInputField {
                    name:          "age".to_string(),
                    field_type:    "Int".to_string(),
                    nullable:      true,
                    default_value: Some(serde_json::json!(18)),
                    description:   None,
                },
            ],
            description: Some("Input for creating a user".to_string()),
        });

        let result = generator.generate(&ir, &[]);
        assert!(result.is_ok());

        let schema = result.unwrap();
        assert_eq!(schema.input_types.len(), 1);

        let create_user = &schema.input_types[0];
        assert_eq!(create_user.name, "CreateUserInput");
        assert_eq!(create_user.fields.len(), 2);
        assert_eq!(create_user.fields[0].name, "name");
        assert_eq!(create_user.fields[1].default_value, Some("18".to_string()));
    }
}
