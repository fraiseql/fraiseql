//! Intermediate Schema Format
//!
//! Language-agnostic schema representation that all language libraries output.
//! See .`claude/INTERMEDIATE_SCHEMA_FORMAT.md` for full specification.

use serde::{Deserialize, Serialize};

/// Intermediate schema - universal format from all language libraries
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

fn default_version() -> String {
    "2.0.0".to_string()
}

/// Type definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateType {
    /// Type name (e.g., "User")
    pub name: String,

    /// Type fields
    pub fields: Vec<IntermediateField>,

    /// Type description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Interfaces this type implements (GraphQL spec §3.6)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,
}

/// Field definition in intermediate format
///
/// **NOTE**: Uses `type` field (not `field_type`)
/// This is the language-agnostic format. Rust conversion happens in converter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateField {
    /// Field name (e.g., "id")
    pub name: String,

    /// Field type name (e.g., "Int", "String", "User")
    ///
    /// **Language-agnostic**: All languages use "type", not "`field_type`"
    #[serde(rename = "type")]
    pub field_type: String,

    /// Is field nullable?
    pub nullable: bool,

    /// Field description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Applied directives (e.g., @deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directives: Option<Vec<IntermediateAppliedDirective>>,

    /// Scope required to access this field (field-level access control)
    ///
    /// When set, users must have this scope in their JWT to query this field.
    /// Supports patterns like "read:Type.field" or custom scopes like "hr:view_pii".
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "name": "salary",
    ///   "type": "Int",
    ///   "nullable": false,
    ///   "requires_scope": "read:Employee.salary"
    /// }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_scope: Option<String>,
}

// =============================================================================
// Enum Definitions
// =============================================================================

/// GraphQL enum type definition in intermediate format.
///
/// Enums represent a finite set of possible values.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "OrderStatus",
///   "values": [
///     {"name": "PENDING"},
///     {"name": "PROCESSING"},
///     {"name": "SHIPPED", "description": "Package has been shipped"},
///     {"name": "DELIVERED"}
///   ],
///   "description": "Possible states of an order"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateEnum {
    /// Enum type name (e.g., "OrderStatus")
    pub name: String,

    /// Possible values for this enum
    pub values: Vec<IntermediateEnumValue>,

    /// Enum description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A single value within an enum type.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "ACTIVE",
///   "description": "The item is currently active",
///   "deprecated": {"reason": "Use ENABLED instead"}
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateEnumValue {
    /// Value name (e.g., "PENDING")
    pub name: String,

    /// Value description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Deprecation info (if value is deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}

/// Deprecation information for enum values or input fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateDeprecation {
    /// Deprecation reason (what to use instead)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// =============================================================================
// Input Object Definitions
// =============================================================================

/// GraphQL input object type definition in intermediate format.
///
/// Input objects are used for complex query arguments like filters,
/// ordering, and mutation inputs.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "UserFilter",
///   "fields": [
///     {"name": "name", "type": "String", "nullable": true},
///     {"name": "email", "type": "String", "nullable": true},
///     {"name": "active", "type": "Boolean", "nullable": true, "default": true}
///   ],
///   "description": "Filter criteria for users"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateInputObject {
    /// Input object type name (e.g., "UserFilter")
    pub name: String,

    /// Input fields
    pub fields: Vec<IntermediateInputField>,

    /// Input type description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A field within an input object type.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "email",
///   "type": "String!",
///   "description": "User's email address",
///   "default": "user@example.com"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateInputField {
    /// Field name
    pub name: String,

    /// Field type name (e.g., "String!", "[Int]", "UserFilter")
    #[serde(rename = "type")]
    pub field_type: String,

    /// Is field nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Field description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default value (as JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    /// Deprecation info (if field is deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}

/// Query definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateQuery {
    /// Query name (e.g., "users")
    pub name: String,

    /// Return type name (e.g., "User")
    pub return_type: String,

    /// Returns a list?
    #[serde(default)]
    pub returns_list: bool,

    /// Result is nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Query arguments
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Query description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SQL source (table/view name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Auto-generated parameters config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_params: Option<IntermediateAutoParams>,

    /// Deprecation info (from @deprecated directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}

/// Mutation definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateMutation {
    /// Mutation name (e.g., "createUser")
    pub name: String,

    /// Return type name (e.g., "User")
    pub return_type: String,

    /// Returns a list?
    #[serde(default)]
    pub returns_list: bool,

    /// Result is nullable?
    #[serde(default)]
    pub nullable: bool,

    /// Mutation arguments
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Mutation description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SQL source (function name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_source: Option<String>,

    /// Operation type (CREATE, UPDATE, DELETE, CUSTOM)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,

    /// Deprecation info (from @deprecated directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}

// =============================================================================
// Interface Definitions (GraphQL Spec §3.7)
// =============================================================================

/// GraphQL interface type definition in intermediate format.
///
/// Interfaces define a common set of fields that multiple object types can implement.
/// Per GraphQL spec §3.7, interfaces enable polymorphic queries.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "Node",
///   "fields": [
///     {"name": "id", "type": "ID", "nullable": false}
///   ],
///   "description": "An object with a globally unique ID"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateInterface {
    /// Interface name (e.g., "Node")
    pub name: String,

    /// Interface fields (all implementing types must have these fields)
    pub fields: Vec<IntermediateField>,

    /// Interface description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Argument definition in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateArgument {
    /// Argument name
    pub name: String,

    /// Argument type name
    ///
    /// **Language-agnostic**: Uses "type", not "`arg_type`"
    #[serde(rename = "type")]
    pub arg_type: String,

    /// Is argument optional?
    pub nullable: bool,

    /// Default value (JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    /// Deprecation info (from @deprecated directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}

// =============================================================================
// Union Definitions (GraphQL Spec §3.10)
// =============================================================================

/// GraphQL union type definition in intermediate format.
///
/// Unions represent a type that could be one of several object types.
/// Per GraphQL spec §3.10, unions are abstract types with member types.
/// Unlike interfaces, unions don't define common fields.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "SearchResult",
///   "member_types": ["User", "Post", "Comment"],
///   "description": "A result from a search query"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateUnion {
    /// Union type name (e.g., "SearchResult")
    pub name: String,

    /// Member types (object type names that belong to this union)
    pub member_types: Vec<String>,

    /// Union description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Auto-params configuration in intermediate format
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateAutoParams {
    #[serde(default)]
    pub limit:        bool,
    #[serde(default)]
    pub offset:       bool,
    #[serde(rename = "where", default)]
    pub where_clause: bool,
    #[serde(default)]
    pub order_by:     bool,
}

// =============================================================================
// Subscription Definitions
// =============================================================================

/// Subscription definition in intermediate format.
///
/// Subscriptions provide real-time event streams for GraphQL clients.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "orderUpdated",
///   "return_type": "Order",
///   "arguments": [
///     {"name": "orderId", "type": "ID", "nullable": true}
///   ],
///   "topic": "order_events",
///   "filter": {
///     "conditions": [
///       {"argument": "orderId", "path": "$.id"}
///     ]
///   },
///   "description": "Stream of order update events"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateSubscription {
    /// Subscription name (e.g., "orderUpdated")
    pub name: String,

    /// Return type name (e.g., "Order")
    pub return_type: String,

    /// Subscription arguments (for filtering events)
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Subscription description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Event topic to subscribe to (e.g., "order_events")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,

    /// Filter configuration for event matching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<IntermediateSubscriptionFilter>,

    /// Fields to project from event data
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,

    /// Deprecation info (from @deprecated directive)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<IntermediateDeprecation>,
}

/// Subscription filter definition for event matching.
///
/// Maps subscription arguments to JSONB paths in event data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateSubscriptionFilter {
    /// Filter conditions mapping arguments to event data paths
    pub conditions: Vec<IntermediateFilterCondition>,
}

/// A single filter condition for subscription event matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateFilterCondition {
    /// Argument name from subscription arguments
    pub argument: String,

    /// JSON path to the value in event data (e.g., "$.id", "$.order_status")
    pub path: String,
}

// =============================================================================
// Fragment and Directive Definitions (GraphQL Spec §2.9-2.12)
// =============================================================================

/// Fragment definition in intermediate format.
///
/// Fragments are reusable field selections that can be spread into queries.
/// Per GraphQL spec §2.9-2.10, fragments have a type condition and field list.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "UserFields",
///   "on": "User",
///   "fields": ["id", "name", "email"]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateFragment {
    /// Fragment name (e.g., "UserFields")
    pub name: String,

    /// Type condition - the type this fragment applies to (e.g., "User")
    #[serde(rename = "on")]
    pub type_condition: String,

    /// Fields to select (can be field names or nested fragment spreads)
    pub fields: Vec<IntermediateFragmentField>,

    /// Fragment description (from docstring)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Fragment field selection - either a simple field or a nested object/fragment spread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IntermediateFragmentField {
    /// Simple field name (e.g., "id", "name")
    Simple(String),

    /// Complex field with nested selections or directives
    Complex(IntermediateFragmentFieldDef),
}

/// Complex fragment field definition with optional alias, directives, and nested fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateFragmentFieldDef {
    /// Field name (source field in the type)
    pub name: String,

    /// Output alias (optional, per GraphQL spec §2.13)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Nested field selections (for object fields)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<IntermediateFragmentField>>,

    /// Fragment spread (e.g., "...UserFields")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spread: Option<String>,

    /// Applied directives (e.g., @skip, @include)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directives: Option<Vec<IntermediateAppliedDirective>>,
}

/// Directive definition in intermediate format.
///
/// Directives provide a way to describe alternate runtime execution and type validation.
/// Per GraphQL spec §2.12, directives can be applied to various locations.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "auth",
///   "locations": ["FIELD_DEFINITION", "OBJECT"],
///   "arguments": [{"name": "role", "type": "String", "nullable": false}],
///   "description": "Requires authentication with specified role"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateDirective {
    /// Directive name (without @, e.g., "auth", "deprecated")
    pub name: String,

    /// Valid locations where this directive can be applied
    pub locations: Vec<String>,

    /// Directive arguments
    #[serde(default)]
    pub arguments: Vec<IntermediateArgument>,

    /// Whether the directive can be applied multiple times
    #[serde(default)]
    pub repeatable: bool,

    /// Directive description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// An applied directive instance (used on fields, types, etc.).
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "skip",
///   "arguments": {"if": true}
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateAppliedDirective {
    /// Directive name (without @)
    pub name: String,

    /// Directive arguments as key-value pairs
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

// =============================================================================
// Analytics Definitions
// =============================================================================

/// Fact table definition in intermediate format (Analytics)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateFactTable {
    pub table_name:           String,
    pub measures:             Vec<IntermediateMeasure>,
    pub dimensions:           IntermediateDimensions,
    pub denormalized_filters: Vec<IntermediateFilter>,
}

/// Measure column definition
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateMeasure {
    pub name:     String,
    pub sql_type: String,
    pub nullable: bool,
}

/// Dimensions metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateDimensions {
    pub name:  String,
    pub paths: Vec<IntermediateDimensionPath>,
}

/// Dimension path within JSONB
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateDimensionPath {
    pub name:      String,
    /// JSON path (accepts both "`json_path`" and "path" for cross-language compat)
    #[serde(alias = "path")]
    pub json_path: String,
    /// Data type (accepts both "`data_type`" and "type" for cross-language compat)
    #[serde(alias = "type")]
    pub data_type: String,
}

/// Denormalized filter column
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateFilter {
    pub name:     String,
    pub sql_type: String,
    pub indexed:  bool,
}

/// Aggregate query definition (Analytics)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateAggregateQuery {
    pub name:            String,
    pub fact_table:      String,
    pub auto_group_by:   bool,
    pub auto_aggregates: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description:     Option<String>,
}

// =============================================================================
// Observer Definitions
// =============================================================================

/// Observer definition in intermediate format.
///
/// Observers listen to database change events (INSERT/UPDATE/DELETE) and execute
/// actions (webhooks, Slack, email) when conditions are met.
///
/// # Example JSON
///
/// ```json
/// {
///   "name": "onHighValueOrder",
///   "entity": "Order",
///   "event": "INSERT",
///   "condition": "total > 1000",
///   "actions": [
///     {
///       "type": "webhook",
///       "url": "https://api.example.com/orders",
///       "headers": {"Content-Type": "application/json"}
///     },
///     {
///       "type": "slack",
///       "channel": "#sales",
///       "message": "New order: {id}",
///       "webhook_url_env": "SLACK_WEBHOOK_URL"
///     }
///   ],
///   "retry": {
///     "max_attempts": 3,
///     "backoff_strategy": "exponential",
///     "initial_delay_ms": 100,
///     "max_delay_ms": 60000
///   }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntermediateObserver {
    /// Observer name (unique identifier)
    pub name: String,

    /// Entity type to observe (e.g., "Order", "User")
    pub entity: String,

    /// Event type: INSERT, UPDATE, or DELETE
    pub event: String,

    /// Actions to execute when observer triggers
    pub actions: Vec<IntermediateObserverAction>,

    /// Optional condition expression in FraiseQL DSL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,

    /// Retry configuration for action execution
    pub retry: IntermediateRetryConfig,
}

/// Observer action (webhook, Slack, email, etc.).
///
/// Actions are stored as flexible JSON objects since they have different
/// structures based on action type.
pub type IntermediateObserverAction = serde_json::Value;

/// Retry configuration for observer actions.
///
/// # Example JSON
///
/// ```json
/// {
///   "max_attempts": 5,
///   "backoff_strategy": "exponential",
///   "initial_delay_ms": 100,
///   "max_delay_ms": 60000
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntermediateRetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,

    /// Backoff strategy: exponential, linear, or fixed
    pub backoff_strategy: String,

    /// Initial delay in milliseconds
    pub initial_delay_ms: u32,

    /// Maximum delay in milliseconds
    pub max_delay_ms: u32,
}

#[cfg(test)]
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
