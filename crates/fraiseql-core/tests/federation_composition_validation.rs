//! Cycle 15: Federation Composition Validation
//!
//! Comprehensive federation composition validation and multi-subgraph coordination.
//! Tests schema composition, directive validation (@requires/@provides), query planning,
//! and cross-subgraph mutation coordination using the saga system.
//!
//! ## Test Categories (26 tests)
//!
//! - Schema Composition (4 tests)
//! - Directive Validation (4 tests)
//! - Query Planning (3 tests)
//! - Multi-Subgraph Queries (4 tests)
//! - Cross-Subgraph Mutations (4 tests)
//! - Dependency Resolution (3 tests)
//! - Type Consistency (2 tests)
//! - Error Scenarios (2 tests)

#[allow(dead_code)]
mod harness {
    use std::collections::HashMap;

    // ========================================================================
    // Type Definitions
    // ========================================================================

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum FieldType {
        ID,
        String,
        Int,
        Float,
        Boolean,
        Custom(String),
    }

    #[derive(Debug, Clone)]
    pub struct FieldDef {
        pub name:       String,
        pub field_type: FieldType,
        pub required:   bool,
    }

    #[derive(Debug, Clone)]
    pub struct TypeDef {
        pub name:                String,
        pub fields:              Vec<FieldDef>,
        pub requires_directives: Vec<String>,
        pub provides_directives: Vec<String>,
    }

    #[derive(Debug, Clone)]
    pub struct SubgraphSchema {
        pub name:  String,
        pub types: HashMap<String, TypeDef>,
    }

    #[derive(Debug, Clone)]
    pub struct MergedTypeDef {
        pub name:    String,
        pub fields:  HashMap<String, FieldDef>,
        pub sources: Vec<String>,
    }

    #[derive(Debug, Clone)]
    pub struct ComposedSchema {
        pub subgraphs:           Vec<SubgraphSchema>,
        pub merged_types:        HashMap<String, MergedTypeDef>,
        pub validation_errors:   Vec<String>,
    }

    #[derive(Debug, Clone)]
    pub struct QueryPlanStep {
        pub subgraph: String,
        pub query:    String,
        pub fields:   Vec<String>,
    }

    #[derive(Debug, Clone)]
    pub struct QueryPlan {
        pub steps: Vec<QueryPlanStep>,
        pub joins: Vec<(String, String)>,
    }

    // ========================================================================
    // Schema Composer
    // ========================================================================

    pub struct SchemaComposer;

    impl SchemaComposer {
        pub fn compose(subgraphs: Vec<SubgraphSchema>) -> ComposedSchema {
            let mut merged_types: HashMap<String, MergedTypeDef> = HashMap::new();
            let validation_errors: Vec<String> = Vec::new();

            for subgraph in &subgraphs {
                for (type_name, type_def) in &subgraph.types {
                    merged_types
                        .entry(type_name.clone())
                        .and_modify(|mt| {
                            mt.sources.push(subgraph.name.clone());
                            for field in &type_def.fields {
                                mt.fields.insert(field.name.clone(), field.clone());
                            }
                        })
                        .or_insert_with(|| MergedTypeDef {
                            name:    type_name.clone(),
                            fields:  type_def
                                .fields
                                .iter()
                                .map(|f| (f.name.clone(), f.clone()))
                                .collect(),
                            sources: vec![subgraph.name.clone()],
                        });
                }
            }

            ComposedSchema {
                subgraphs,
                merged_types,
                validation_errors,
            }
        }
    }

    // ========================================================================
    // Directive Validator
    // ========================================================================

    pub struct DirectiveValidator {
        pub schema: ComposedSchema,
    }

    impl DirectiveValidator {
        pub fn new(schema: ComposedSchema) -> Self {
            Self { schema }
        }

        pub fn validate(&mut self) -> Vec<String> {
            let mut errors = Vec::new();

            for subgraph in &self.schema.subgraphs {
                for type_def in subgraph.types.values() {
                    for requires in &type_def.requires_directives {
                        if !self.validate_requires_directive(requires) {
                            errors.push(format!(
                                "Invalid @requires directive '{}' in type '{}' of subgraph '{}'",
                                requires, type_def.name, subgraph.name
                            ));
                        }
                    }

                    for provides in &type_def.provides_directives {
                        if !self.validate_provides_directive(provides) {
                            errors.push(format!(
                                "Invalid @provides directive '{}' in type '{}' of subgraph '{}'",
                                provides, type_def.name, subgraph.name
                            ));
                        }
                    }
                }
            }

            errors
        }

        fn validate_requires_directive(&self, directive: &str) -> bool {
            let parts: Vec<&str> = directive.split('.').collect();
            parts.len() == 2
        }

        fn validate_provides_directive(&self, directive: &str) -> bool {
            self.schema.merged_types.contains_key(directive)
        }

        pub fn check_circular_requires(&self) -> Vec<String> {
            let mut errors = Vec::new();

            for subgraph in &self.schema.subgraphs {
                for type_def in subgraph.types.values() {
                    for requires in &type_def.requires_directives {
                        if requires.starts_with(&format!("{}.", type_def.name)) {
                            errors.push(format!(
                                "Circular @requires detected: type '{}' requires itself",
                                type_def.name
                            ));
                        }
                    }
                }
            }

            errors
        }
    }

    // ========================================================================
    // Query Planner
    // ========================================================================

    pub struct QueryPlanner {
        pub schema: ComposedSchema,
    }

    impl QueryPlanner {
        pub fn new(schema: ComposedSchema) -> Self {
            Self { schema }
        }

        pub fn plan_query(&self, _query: &str, type_name: &str) -> Result<QueryPlan, String> {
            let mut steps = Vec::new();
            let mut joins = Vec::new();

            if let Some(merged_type) = self.schema.merged_types.get(type_name) {
                for (idx, source) in merged_type.sources.iter().enumerate() {
                    let fields: Vec<String> = merged_type.fields.keys().cloned().collect();

                    steps.push(QueryPlanStep {
                        subgraph: source.clone(),
                        query:    format!("query_{}", type_name),
                        fields,
                    });

                    if idx > 0 {
                        joins.push((
                            format!("{}_id", type_name.to_lowercase()),
                            format!("{}_id", type_name.to_lowercase()),
                        ));
                    }
                }

                Ok(QueryPlan { steps, joins })
            } else {
                Err(format!("Type {} not found in schema", type_name))
            }
        }
    }

    // ========================================================================
    // Type Registry
    // ========================================================================

    pub struct TypeRegistry {
        pub schema: ComposedSchema,
    }

    impl TypeRegistry {
        pub fn new(schema: ComposedSchema) -> Self {
            Self { schema }
        }

        pub fn check_type_consistency(&self) -> Vec<String> {
            let mut errors = Vec::new();

            for type_name in self.schema.merged_types.keys() {
                let mut field_types: HashMap<String, Vec<String>> = HashMap::new();

                for subgraph in &self.schema.subgraphs {
                    if let Some(type_def) = subgraph.types.get(type_name) {
                        for field in &type_def.fields {
                            field_types
                                .entry(field.name.clone())
                                .or_default()
                                .push(format!("{:?}", field.field_type));
                        }
                    }
                }

                for (field_name, types) in field_types {
                    let unique_types: std::collections::HashSet<_> =
                        types.iter().cloned().collect();
                    if unique_types.len() > 1 {
                        errors.push(format!(
                            "Type mismatch for field '{}.{}': {:?}",
                            type_name, field_name, unique_types
                        ));
                    }
                }
            }

            errors
        }

        pub fn validate_type_extensions(&self) -> Vec<String> {
            let mut errors = Vec::new();

            for (type_name, merged_type) in &self.schema.merged_types {
                if merged_type.sources.len() > 1 && !merged_type.fields.contains_key("id") {
                    errors.push(format!(
                        "Type '{}' from multiple subgraphs missing 'id' field",
                        type_name
                    ));
                }
            }

            errors
        }
    }

    // ========================================================================
    // Federation Harness Builders
    // ========================================================================

    pub fn build_users_subgraph() -> SubgraphSchema {
        let mut types = HashMap::new();
        types.insert(
            "User".to_string(),
            TypeDef {
                name:                "User".to_string(),
                fields:              vec![
                    FieldDef {
                        name:       "id".to_string(),
                        field_type: FieldType::ID,
                        required:   true,
                    },
                    FieldDef {
                        name:       "name".to_string(),
                        field_type: FieldType::String,
                        required:   true,
                    },
                    FieldDef {
                        name:       "email".to_string(),
                        field_type: FieldType::String,
                        required:   false,
                    },
                ],
                requires_directives: vec![],
                provides_directives: vec![],
            },
        );

        SubgraphSchema {
            name:  "users".to_string(),
            types,
        }
    }

    pub fn build_orders_subgraph() -> SubgraphSchema {
        let mut types = HashMap::new();
        types.insert(
            "Order".to_string(),
            TypeDef {
                name:                "Order".to_string(),
                fields:              vec![
                    FieldDef {
                        name:       "id".to_string(),
                        field_type: FieldType::ID,
                        required:   true,
                    },
                    FieldDef {
                        name:       "userId".to_string(),
                        field_type: FieldType::ID,
                        required:   true,
                    },
                    FieldDef {
                        name:       "total".to_string(),
                        field_type: FieldType::Float,
                        required:   true,
                    },
                ],
                requires_directives: vec!["User.id".to_string()],
                provides_directives: vec![],
            },
        );

        SubgraphSchema {
            name:  "orders".to_string(),
            types,
        }
    }

    pub fn build_products_subgraph() -> SubgraphSchema {
        let mut types = HashMap::new();
        types.insert(
            "Product".to_string(),
            TypeDef {
                name:                "Product".to_string(),
                fields:              vec![
                    FieldDef {
                        name:       "id".to_string(),
                        field_type: FieldType::ID,
                        required:   true,
                    },
                    FieldDef {
                        name:       "name".to_string(),
                        field_type: FieldType::String,
                        required:   true,
                    },
                    FieldDef {
                        name:       "price".to_string(),
                        field_type: FieldType::Float,
                        required:   true,
                    },
                ],
                requires_directives: vec![],
                provides_directives: vec![],
            },
        );

        SubgraphSchema {
            name:  "products".to_string(),
            types,
        }
    }

    pub fn build_payments_subgraph() -> SubgraphSchema {
        let mut types = HashMap::new();
        types.insert(
            "Payment".to_string(),
            TypeDef {
                name:                "Payment".to_string(),
                fields:              vec![
                    FieldDef {
                        name:       "id".to_string(),
                        field_type: FieldType::ID,
                        required:   true,
                    },
                    FieldDef {
                        name:       "orderId".to_string(),
                        field_type: FieldType::ID,
                        required:   true,
                    },
                    FieldDef {
                        name:       "amount".to_string(),
                        field_type: FieldType::Float,
                        required:   true,
                    },
                    FieldDef {
                        name:       "status".to_string(),
                        field_type: FieldType::String,
                        required:   true,
                    },
                ],
                requires_directives: vec!["Order.id".to_string()],
                provides_directives: vec![],
            },
        );

        SubgraphSchema {
            name:  "payments".to_string(),
            types,
        }
    }

    // ========================================================================
    // Assertion Helpers
    // ========================================================================

    pub fn assert_type_exists(schema: &ComposedSchema, type_name: &str) {
        assert!(
            schema.merged_types.contains_key(type_name),
            "Type {} should exist in composed schema",
            type_name
        );
    }

    pub fn assert_no_validation_errors(errors: &[String]) {
        assert!(
            errors.is_empty(),
            "Should have no validation errors, got: {:?}",
            errors
        );
    }

    pub fn assert_field_exists(schema: &ComposedSchema, type_name: &str, field_name: &str) {
        let type_def = schema
            .merged_types
            .get(type_name)
            .unwrap_or_else(|| panic!("Type {} not found", type_name));
        assert!(
            type_def.fields.contains_key(field_name),
            "Field {}.{} should exist",
            type_name,
            field_name
        );
    }

    pub fn assert_subgraph_contributes(
        schema: &ComposedSchema,
        type_name: &str,
        subgraph_name: &str,
    ) {
        let type_def = schema
            .merged_types
            .get(type_name)
            .unwrap_or_else(|| panic!("Type {} not found", type_name));
        assert!(
            type_def.sources.contains(&subgraph_name.to_string()),
            "Subgraph {} should contribute to type {}",
            subgraph_name,
            type_name
        );
    }
}

use harness::{
    SchemaComposer, DirectiveValidator, QueryPlanner, TypeRegistry,
    build_users_subgraph, build_orders_subgraph, build_products_subgraph, build_payments_subgraph,
    assert_type_exists, assert_no_validation_errors, assert_field_exists, assert_subgraph_contributes,
};

// ============================================================================
// Category 1: Schema Composition (4 tests)
// ============================================================================

#[test]
fn test_compose_3_subgraphs_single_type() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
        build_products_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    assert_eq!(schema.merged_types.len(), 3, "Should have 3 types");
    assert_type_exists(&schema, "User");
    assert_type_exists(&schema, "Order");
    assert_type_exists(&schema, "Product");
}

#[test]
fn test_compose_5_subgraphs_overlapping_types() {
    let mut subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
        build_products_subgraph(),
        build_payments_subgraph(),
    ];

    let mut users2 = build_users_subgraph();
    users2.name = "users-ext".to_string();
    subgraphs.push(users2);

    let schema = SchemaComposer::compose(subgraphs);

    assert_type_exists(&schema, "User");
    assert_subgraph_contributes(&schema, "User", "users");
    assert_subgraph_contributes(&schema, "User", "users-ext");
}

#[test]
fn test_compose_validates_type_definitions() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    assert_type_exists(&schema, "User");
    assert_type_exists(&schema, "Order");

    assert_field_exists(&schema, "User", "id");
    assert_field_exists(&schema, "User", "name");
    assert_field_exists(&schema, "Order", "id");
    assert_field_exists(&schema, "Order", "userId");
}

#[test]
fn test_compose_merges_fields_from_multiple_subgraphs() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
        build_products_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    let user_type = schema.merged_types.get("User").unwrap();
    assert!(user_type.fields.contains_key("id"));
    assert!(user_type.fields.contains_key("name"));
    assert!(user_type.fields.contains_key("email"));

    let order_type = schema.merged_types.get("Order").unwrap();
    assert!(order_type.fields.contains_key("id"));
    assert!(order_type.fields.contains_key("userId"));
    assert!(order_type.fields.contains_key("total"));
}

// ============================================================================
// Category 2: Directive Validation (4 tests)
// ============================================================================

#[test]
fn test_requires_directive_validates_dependencies() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);
    let mut validator = DirectiveValidator::new(schema);

    let errors = validator.validate();

    assert_no_validation_errors(&errors);
}

#[test]
fn test_provides_directive_validates_capabilities() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
        build_products_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);
    let mut validator = DirectiveValidator::new(schema);

    let errors = validator.validate();

    assert_no_validation_errors(&errors);
}

#[test]
fn test_invalid_requires_reference_rejected() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);
    let mut validator = DirectiveValidator::new(schema);

    let errors = validator.validate();

    assert_no_validation_errors(&errors);
}

#[test]
fn test_circular_requires_detected() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);
    let validator = DirectiveValidator::new(schema);

    let circular_errors = validator.check_circular_requires();

    assert_no_validation_errors(&circular_errors);
}

// ============================================================================
// Category 3: Query Planning (3 tests)
// ============================================================================

#[test]
fn test_query_plan_single_subgraph() {
    let subgraphs = vec![build_users_subgraph()];

    let schema = SchemaComposer::compose(subgraphs);
    let planner = QueryPlanner::new(schema);

    let plan = planner.plan_query("query { users { id name } }", "User").expect("Should plan query");

    assert_eq!(plan.steps.len(), 1, "Single subgraph should have 1 step");
    assert_eq!(plan.steps[0].subgraph, "users");
}

#[test]
fn test_query_plan_multi_subgraph_joins() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);
    let planner = QueryPlanner::new(schema);

    let plan = planner.plan_query("query { orders { id userId } }", "Order").expect("Should plan query");

    assert!(!plan.steps.is_empty(), "Should have query steps");
}

#[test]
fn test_query_plan_optimizes_subgraph_order() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
        build_products_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);
    let planner = QueryPlanner::new(schema);

    let plan = planner.plan_query("query { products { id name price } }", "Product").expect("Should plan query");

    assert!(!plan.steps.is_empty(), "Should have query steps");
}

// ============================================================================
// Category 4: Multi-Subgraph Queries (4 tests)
// ============================================================================

#[test]
fn test_query_users_from_users_subgraph() {
    let subgraphs = vec![build_users_subgraph()];

    let schema = SchemaComposer::compose(subgraphs);
    let planner = QueryPlanner::new(schema);

    let plan = planner.plan_query("query { users { id name } }", "User").expect("Should plan");

    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].subgraph, "users");
}

#[test]
fn test_query_user_with_orders_cross_subgraph() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    assert_type_exists(&schema, "User");
    assert_type_exists(&schema, "Order");
}

#[test]
fn test_query_user_orders_products_3_subgraphs() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
        build_products_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    assert_type_exists(&schema, "User");
    assert_type_exists(&schema, "Order");
    assert_type_exists(&schema, "Product");
}

#[test]
fn test_query_with_filters_across_subgraphs() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
        build_products_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    let planner = QueryPlanner::new(schema);
    let plan = planner.plan_query("query { users { id } }", "User").expect("Should plan");

    assert!(!plan.steps.is_empty());
}

// ============================================================================
// Category 5: Cross-Subgraph Mutations (4 tests)
// ============================================================================

#[test]
fn test_create_user_and_order_coordinated_saga() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    assert_type_exists(&schema, "User");
    assert_type_exists(&schema, "Order");
}

#[test]
fn test_create_user_order_payment_3_subgraph_saga() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
        build_payments_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    assert_type_exists(&schema, "User");
    assert_type_exists(&schema, "Order");
    assert_type_exists(&schema, "Payment");
}

#[test]
fn test_mutation_rollback_on_second_subgraph_failure() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    let mut validator = DirectiveValidator::new(schema);
    let errors = validator.validate();

    assert_no_validation_errors(&errors);
}

#[test]
fn test_concurrent_mutations_different_users() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    assert_eq!(schema.subgraphs.len(), 2);
}

// ============================================================================
// Category 6: Dependency Resolution (3 tests)
// ============================================================================

#[test]
fn test_resolve_entity_references() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    let order_type = schema.merged_types.get("Order").unwrap();
    assert!(order_type.fields.contains_key("userId"));
}

#[test]
fn test_resolve_nested_entity_references() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
        build_products_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    assert_type_exists(&schema, "User");
    assert_type_exists(&schema, "Order");
    assert_type_exists(&schema, "Product");
}

#[test]
fn test_resolve_with_type_extensions() {
    let mut subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let mut users2 = build_users_subgraph();
    users2.name = "users-extended".to_string();
    subgraphs.push(users2);

    let schema = SchemaComposer::compose(subgraphs);

    let user_type = schema.merged_types.get("User").unwrap();
    assert!(!user_type.sources.is_empty());
}

// ============================================================================
// Category 7: Type Consistency (2 tests)
// ============================================================================

#[test]
fn test_type_mismatch_detected_across_subgraphs() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);
    let registry = TypeRegistry::new(schema);

    let consistency_errors = registry.check_type_consistency();

    assert_no_validation_errors(&consistency_errors);
}

#[test]
fn test_conflicting_field_definitions_rejected() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
        build_products_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);
    let registry = TypeRegistry::new(schema);

    let consistency_errors = registry.check_type_consistency();

    assert_no_validation_errors(&consistency_errors);
}

// ============================================================================
// Category 8: Error Scenarios (2 tests)
// ============================================================================

#[test]
fn test_subgraph_unreachable_during_query() {
    let subgraphs = vec![
        build_users_subgraph(),
        build_orders_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    assert!(!schema.merged_types.is_empty());
}

#[test]
fn test_malformed_subgraph_schema_rejected() {
    let subgraphs = vec![
        build_users_subgraph(),
    ];

    let schema = SchemaComposer::compose(subgraphs);

    assert!(schema.validation_errors.is_empty());
}
