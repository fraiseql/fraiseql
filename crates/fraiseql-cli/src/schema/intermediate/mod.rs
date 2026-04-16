//! Intermediate Schema Format
//!
//! Language-agnostic schema representation that all language libraries output.
//! See `docs/architecture/intermediate-schema.md` for full specification.

pub mod advanced_types;
pub mod analytics;
pub mod fragments;
pub mod operations;
pub mod subscriptions;
pub mod types;

pub use advanced_types::{
    IntermediateInputField, IntermediateInputObject, IntermediateInterface, IntermediateUnion,
};
pub use analytics::{
    IntermediateAggregateQuery, IntermediateDimensionPath, IntermediateDimensions,
    IntermediateFactTable, IntermediateFilter, IntermediateMeasure,
};
pub use fragments::{
    IntermediateAppliedDirective, IntermediateDirective, IntermediateFragment,
    IntermediateFragmentField, IntermediateFragmentFieldDef,
};
use fraiseql_core::schema::{
    DebugConfig, McpConfig, SessionVariablesConfig, SubscriptionsConfig, ValidationConfig,
};
pub use operations::{
    IntermediateArgument, IntermediateAutoParams, IntermediateMutation, IntermediateQuery,
    IntermediateQueryDefaults,
};
use fraiseql_core::schema::NamingConvention;
use serde::{Deserialize, Serialize};
pub use subscriptions::{
    IntermediateFilterCondition, IntermediateObserver, IntermediateObserverAction,
    IntermediateRetryConfig, IntermediateSubscription, IntermediateSubscriptionFilter,
};
pub use types::{
    IntermediateDeprecation, IntermediateEnum, IntermediateEnumValue, IntermediateField,
    IntermediateScalar, IntermediateType,
};

/// Intermediate schema - universal format from all language libraries
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct IntermediateSchema {
    /// Schema format version
    #[serde(default = "default_version")]
    pub version: String,

    /// GraphQL object types
    #[serde(default)]
    pub types: Vec<IntermediateType>,

    /// GraphQL enum types
    #[serde(default)]
    pub enums: Vec<IntermediateEnum>,

    /// GraphQL input object types
    #[serde(default)]
    pub input_types: Vec<IntermediateInputObject>,

    /// GraphQL interface types (per GraphQL spec §3.7)
    #[serde(default)]
    pub interfaces: Vec<IntermediateInterface>,

    /// GraphQL union types (per GraphQL spec §3.10)
    #[serde(default)]
    pub unions: Vec<IntermediateUnion>,

    /// GraphQL queries
    #[serde(default)]
    pub queries: Vec<IntermediateQuery>,

    /// GraphQL mutations
    #[serde(default)]
    pub mutations: Vec<IntermediateMutation>,

    /// GraphQL subscriptions
    #[serde(default)]
    pub subscriptions: Vec<IntermediateSubscription>,

    /// GraphQL fragments (reusable field selections)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragments: Option<Vec<IntermediateFragment>>,

    /// GraphQL directive definitions (custom directives)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directives: Option<Vec<IntermediateDirective>>,

    /// Analytics fact tables (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_tables: Option<Vec<IntermediateFactTable>>,

    /// Analytics aggregate queries (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregate_queries: Option<Vec<IntermediateAggregateQuery>>,

    /// Observer definitions (database change event listeners)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observers: Option<Vec<IntermediateObserver>>,

    /// Custom scalar type definitions
    ///
    /// Defines custom GraphQL scalar types with validation rules.
    /// Custom scalars can be defined in Python, TypeScript, Java, Go, and Rust SDKs,
    /// and are compiled into the CompiledSchema's CustomTypeRegistry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_scalars: Option<Vec<IntermediateScalar>>,

    /// Security configuration (from fraiseql.toml)
    /// Compiled from the security section of fraiseql.toml at compile time.
    /// Optional - if not provided, defaults are used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<serde_json::Value>,

    /// Observers/event system configuration (from fraiseql.toml).
    ///
    /// Contains backend connection settings (redis_url, nats_url, etc.) compiled
    /// from the `[observers]` TOML section. Embedded verbatim into the compiled schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observers_config: Option<serde_json::Value>,

    /// Federation configuration (from fraiseql.toml).
    ///
    /// Contains Apollo Federation settings and circuit breaker configuration compiled
    /// from the `[federation]` TOML section. Embedded verbatim into the compiled schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub federation_config: Option<serde_json::Value>,

    /// WebSocket subscription configuration (hooks, limits).
    ///
    /// Compiled from the `[subscriptions]` TOML section. Embedded verbatim into
    /// the compiled schema for server-side consumption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscriptions_config: Option<SubscriptionsConfig>,

    /// Query validation config (depth/complexity limits).
    ///
    /// Compiled from `[validation]` in `fraiseql.toml`. Embedded into the compiled
    /// schema for server-side consumption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_config: Option<ValidationConfig>,

    /// Debug/development configuration.
    ///
    /// Compiled from `[debug]` in `fraiseql.toml`. Embedded into the compiled
    /// schema for server-side consumption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_config: Option<DebugConfig>,

    /// MCP (Model Context Protocol) server configuration.
    ///
    /// Compiled from `[mcp]` in `fraiseql.toml`. Embedded into the compiled
    /// schema for server-side consumption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_config: Option<McpConfig>,

    /// Global auto-param defaults for list queries (injected from TOML by the merger).
    ///
    /// Never present in `schema.json` — populated at compile time from `[query_defaults]`
    /// in `fraiseql.toml`. Used by the converter to resolve per-query `auto_params`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_defaults: Option<IntermediateQueryDefaults>,

    /// Naming convention for GraphQL operation names.
    ///
    /// Compiled from `fraiseql.toml` top-level `naming_convention` setting.
    #[serde(default)]
    pub naming_convention: NamingConvention,

    /// Session variable injection configuration.
    ///
    /// When populated, the executor calls `set_config()` before each query and
    /// mutation to inject per-request values (JWT claims, HTTP headers, or literals)
    /// as PostgreSQL transaction-scoped settings.
    ///
    /// Embedded verbatim from the `session_variables` key in `schema.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_variables: Option<SessionVariablesConfig>,
}

fn default_version() -> String {
    "2.0.0".to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_schema() {
        let json = r#"{
            "types": [],
            "queries": [],
            "mutations": []
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.version, "2.0.0");
        assert_eq!(schema.types.len(), 0);
        assert_eq!(schema.queries.len(), 0);
        assert_eq!(schema.mutations.len(), 0);
    }

    #[test]
    fn test_parse_type_with_type_field() {
        let json = r#"{
            "types": [{
                "name": "User",
                "fields": [
                    {
                        "name": "id",
                        "type": "Int",
                        "nullable": false
                    },
                    {
                        "name": "name",
                        "type": "String",
                        "nullable": false
                    }
                ]
            }],
            "queries": [],
            "mutations": []
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.types.len(), 1);
        assert_eq!(schema.types[0].name, "User");
        assert_eq!(schema.types[0].fields.len(), 2);
        assert_eq!(schema.types[0].fields[0].name, "id");
        assert_eq!(schema.types[0].fields[0].field_type, "Int");
        assert!(!schema.types[0].fields[0].nullable);
    }

    #[test]
    fn test_parse_query_with_arguments() {
        let json = r#"{
            "types": [],
            "queries": [{
                "name": "users",
                "return_type": "User",
                "returns_list": true,
                "nullable": false,
                "arguments": [
                    {
                        "name": "limit",
                        "type": "Int",
                        "nullable": false,
                        "default": 10
                    }
                ],
                "sql_source": "v_user"
            }],
            "mutations": []
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.queries.len(), 1);
        assert_eq!(schema.queries[0].arguments.len(), 1);
        assert_eq!(schema.queries[0].arguments[0].arg_type, "Int");
        assert_eq!(schema.queries[0].arguments[0].default, Some(serde_json::json!(10)));
    }

    #[test]
    fn test_parse_fragment_simple() {
        let json = r#"{
            "types": [],
            "queries": [],
            "mutations": [],
            "fragments": [{
                "name": "UserFields",
                "on": "User",
                "fields": ["id", "name", "email"]
            }]
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert!(schema.fragments.is_some());
        let fragments = schema.fragments.unwrap();
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].name, "UserFields");
        assert_eq!(fragments[0].type_condition, "User");
        assert_eq!(fragments[0].fields.len(), 3);

        // Check simple fields
        match &fragments[0].fields[0] {
            IntermediateFragmentField::Simple(name) => assert_eq!(name, "id"),
            IntermediateFragmentField::Complex(_) => panic!("Expected simple field"),
        }
    }

    #[test]
    fn test_parse_fragment_with_nested_fields() {
        let json = r#"{
            "types": [],
            "queries": [],
            "mutations": [],
            "fragments": [{
                "name": "PostFields",
                "on": "Post",
                "fields": [
                    "id",
                    "title",
                    {
                        "name": "author",
                        "alias": "writer",
                        "fields": ["id", "name"]
                    }
                ]
            }]
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        let fragments = schema.fragments.unwrap();
        assert_eq!(fragments[0].fields.len(), 3);

        // Check nested field
        match &fragments[0].fields[2] {
            IntermediateFragmentField::Complex(def) => {
                assert_eq!(def.name, "author");
                assert_eq!(def.alias, Some("writer".to_string()));
                assert!(def.fields.is_some());
                assert_eq!(def.fields.as_ref().unwrap().len(), 2);
            },
            IntermediateFragmentField::Simple(_) => panic!("Expected complex field"),
        }
    }

    #[test]
    fn test_parse_directive_definition() {
        let json = r#"{
            "types": [],
            "queries": [],
            "mutations": [],
            "directives": [{
                "name": "auth",
                "locations": ["FIELD_DEFINITION", "OBJECT"],
                "arguments": [
                    {"name": "role", "type": "String", "nullable": false}
                ],
                "description": "Requires authentication"
            }]
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert!(schema.directives.is_some());
        let directives = schema.directives.unwrap();
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].name, "auth");
        assert_eq!(directives[0].locations, vec!["FIELD_DEFINITION", "OBJECT"]);
        assert_eq!(directives[0].arguments.len(), 1);
        assert_eq!(directives[0].description, Some("Requires authentication".to_string()));
    }

    #[test]
    fn test_parse_field_with_directive() {
        let json = r#"{
            "types": [{
                "name": "User",
                "fields": [
                    {
                        "name": "oldId",
                        "type": "Int",
                        "nullable": false,
                        "directives": [
                            {"name": "deprecated", "arguments": {"reason": "Use 'id' instead"}}
                        ]
                    }
                ]
            }],
            "queries": [],
            "mutations": []
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        let field = &schema.types[0].fields[0];
        assert_eq!(field.name, "oldId");
        assert!(field.directives.is_some());
        let directives = field.directives.as_ref().unwrap();
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].name, "deprecated");
        assert_eq!(
            directives[0].arguments,
            Some(serde_json::json!({"reason": "Use 'id' instead"}))
        );
    }

    #[test]
    fn test_parse_fragment_with_spread() {
        let json = r#"{
            "types": [],
            "queries": [],
            "mutations": [],
            "fragments": [
                {
                    "name": "UserFields",
                    "on": "User",
                    "fields": ["id", "name"]
                },
                {
                    "name": "PostWithAuthor",
                    "on": "Post",
                    "fields": [
                        "id",
                        "title",
                        {
                            "name": "author",
                            "spread": "UserFields"
                        }
                    ]
                }
            ]
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        let fragments = schema.fragments.unwrap();
        assert_eq!(fragments.len(), 2);

        // Check the spread reference
        match &fragments[1].fields[2] {
            IntermediateFragmentField::Complex(def) => {
                assert_eq!(def.name, "author");
                assert_eq!(def.spread, Some("UserFields".to_string()));
            },
            IntermediateFragmentField::Simple(_) => panic!("Expected complex field"),
        }
    }

    #[test]
    fn test_parse_enum() {
        let json = r#"{
            "types": [],
            "queries": [],
            "mutations": [],
            "enums": [{
                "name": "OrderStatus",
                "values": [
                    {"name": "PENDING"},
                    {"name": "PROCESSING", "description": "Currently being processed"},
                    {"name": "SHIPPED"},
                    {"name": "DELIVERED"}
                ],
                "description": "Possible states of an order"
            }]
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.enums.len(), 1);
        let enum_def = &schema.enums[0];
        assert_eq!(enum_def.name, "OrderStatus");
        assert_eq!(enum_def.description, Some("Possible states of an order".to_string()));
        assert_eq!(enum_def.values.len(), 4);
        assert_eq!(enum_def.values[0].name, "PENDING");
        assert_eq!(enum_def.values[1].description, Some("Currently being processed".to_string()));
    }

    #[test]
    fn test_parse_enum_with_deprecated_value() {
        let json = r#"{
            "types": [],
            "queries": [],
            "mutations": [],
            "enums": [{
                "name": "UserRole",
                "values": [
                    {"name": "ADMIN"},
                    {"name": "USER"},
                    {"name": "GUEST", "deprecated": {"reason": "Use USER with limited permissions instead"}}
                ]
            }]
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        let enum_def = &schema.enums[0];
        assert_eq!(enum_def.values.len(), 3);

        // Check deprecated value
        let guest = &enum_def.values[2];
        assert_eq!(guest.name, "GUEST");
        assert!(guest.deprecated.is_some());
        assert_eq!(
            guest.deprecated.as_ref().unwrap().reason,
            Some("Use USER with limited permissions instead".to_string())
        );
    }

    #[test]
    fn test_parse_input_object() {
        let json = r#"{
            "types": [],
            "queries": [],
            "mutations": [],
            "input_types": [{
                "name": "UserFilter",
                "fields": [
                    {"name": "name", "type": "String", "nullable": true},
                    {"name": "email", "type": "String", "nullable": true},
                    {"name": "active", "type": "Boolean", "nullable": true, "default": true}
                ],
                "description": "Filter criteria for users"
            }]
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.input_types.len(), 1);
        let input = &schema.input_types[0];
        assert_eq!(input.name, "UserFilter");
        assert_eq!(input.description, Some("Filter criteria for users".to_string()));
        assert_eq!(input.fields.len(), 3);

        // Check fields
        assert_eq!(input.fields[0].name, "name");
        assert_eq!(input.fields[0].field_type, "String");
        assert!(input.fields[0].nullable);

        // Check default value
        assert_eq!(input.fields[2].name, "active");
        assert_eq!(input.fields[2].default, Some(serde_json::json!(true)));
    }

    #[test]
    fn test_parse_interface() {
        let json = r#"{
            "types": [],
            "queries": [],
            "mutations": [],
            "interfaces": [{
                "name": "Node",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false}
                ],
                "description": "An object with a globally unique ID"
            }]
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.interfaces.len(), 1);
        let interface = &schema.interfaces[0];
        assert_eq!(interface.name, "Node");
        assert_eq!(interface.description, Some("An object with a globally unique ID".to_string()));
        assert_eq!(interface.fields.len(), 1);
        assert_eq!(interface.fields[0].name, "id");
        assert_eq!(interface.fields[0].field_type, "ID");
        assert!(!interface.fields[0].nullable);
    }

    #[test]
    fn test_parse_type_implements_interface() {
        let json = r#"{
            "types": [{
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "name", "type": "String", "nullable": false}
                ],
                "implements": ["Node"]
            }],
            "queries": [],
            "mutations": [],
            "interfaces": [{
                "name": "Node",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false}
                ]
            }]
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.types.len(), 1);
        assert_eq!(schema.types[0].name, "User");
        assert_eq!(schema.types[0].implements, vec!["Node"]);

        assert_eq!(schema.interfaces.len(), 1);
        assert_eq!(schema.interfaces[0].name, "Node");
    }

    #[test]
    fn test_parse_input_object_with_deprecated_field() {
        let json = r#"{
            "types": [],
            "queries": [],
            "mutations": [],
            "input_types": [{
                "name": "CreateUserInput",
                "fields": [
                    {"name": "email", "type": "String!", "nullable": false},
                    {"name": "name", "type": "String!", "nullable": false},
                    {
                        "name": "username",
                        "type": "String",
                        "nullable": true,
                        "deprecated": {"reason": "Use email as unique identifier instead"}
                    }
                ]
            }]
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        let input = &schema.input_types[0];

        // Check deprecated field
        let username_field = &input.fields[2];
        assert_eq!(username_field.name, "username");
        assert!(username_field.deprecated.is_some());
        assert_eq!(
            username_field.deprecated.as_ref().unwrap().reason,
            Some("Use email as unique identifier instead".to_string())
        );
    }

    #[test]
    fn test_parse_union() {
        let json = r#"{
            "types": [
                {"name": "User", "fields": [{"name": "id", "type": "ID", "nullable": false}]},
                {"name": "Post", "fields": [{"name": "id", "type": "ID", "nullable": false}]}
            ],
            "queries": [],
            "mutations": [],
            "unions": [{
                "name": "SearchResult",
                "member_types": ["User", "Post"],
                "description": "Result from a search query"
            }]
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.unions.len(), 1);
        let union_def = &schema.unions[0];
        assert_eq!(union_def.name, "SearchResult");
        assert_eq!(union_def.member_types, vec!["User", "Post"]);
        assert_eq!(union_def.description, Some("Result from a search query".to_string()));
    }

    #[test]
    fn test_parse_field_with_requires_scope() {
        let json = r#"{
            "types": [{
                "name": "Employee",
                "fields": [
                    {
                        "name": "id",
                        "type": "ID",
                        "nullable": false
                    },
                    {
                        "name": "name",
                        "type": "String",
                        "nullable": false
                    },
                    {
                        "name": "salary",
                        "type": "Float",
                        "nullable": false,
                        "description": "Employee salary - protected field",
                        "requires_scope": "read:Employee.salary"
                    },
                    {
                        "name": "ssn",
                        "type": "String",
                        "nullable": true,
                        "description": "Social Security Number",
                        "requires_scope": "admin"
                    }
                ]
            }],
            "queries": [],
            "mutations": []
        }"#;

        let schema: IntermediateSchema = serde_json::from_str(json).unwrap();
        assert_eq!(schema.types.len(), 1);

        let employee = &schema.types[0];
        assert_eq!(employee.name, "Employee");
        assert_eq!(employee.fields.len(), 4);

        // id - no scope required
        assert_eq!(employee.fields[0].name, "id");
        assert!(employee.fields[0].requires_scope.is_none());

        // name - no scope required
        assert_eq!(employee.fields[1].name, "name");
        assert!(employee.fields[1].requires_scope.is_none());

        // salary - requires specific scope
        assert_eq!(employee.fields[2].name, "salary");
        assert_eq!(employee.fields[2].requires_scope, Some("read:Employee.salary".to_string()));

        // ssn - requires admin scope
        assert_eq!(employee.fields[3].name, "ssn");
        assert_eq!(employee.fields[3].requires_scope, Some("admin".to_string()));
    }
}
