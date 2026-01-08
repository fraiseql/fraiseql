//! GraphQL Query Parser Layer
//!
//! This module extracts GraphQL parsing from the monolithic engine into a dedicated layer.
//! Responsibilities:
//! - Parse GraphQL query/mutation strings into AST
//! - Validate query structure
//! - Extract field selections, arguments, variables
//! - Return structured ParsedQuery for downstream planning

use crate::api::error::ApiError;
use graphql_parser::query::{self, OperationDefinition, Selection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents the type of GraphQL operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationType {
    Query,
    Mutation,
    Subscription,
}

/// Represents a field directive (e.g., @skip, @include)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDirective {
    pub name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}

/// Represents a variable definition (e.g., $id: ID!)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableDefinition {
    pub var_type: String,
    pub default_value: Option<serde_json::Value>,
}

/// Represents an argument value (can be literal or variable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArgumentValue {
    String(String),
    Int(i64),
    Float(f64),
    Boolean(bool),
    Null,
    Variable(String),
    List(Vec<ArgumentValue>),
    Object(HashMap<String, ArgumentValue>),
}

impl ArgumentValue {
    /// Convert ArgumentValue to serde_json::Value for easier handling downstream
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            ArgumentValue::String(s) => serde_json::json!(s),
            ArgumentValue::Int(i) => serde_json::json!(i),
            ArgumentValue::Float(f) => serde_json::json!(f),
            ArgumentValue::Boolean(b) => serde_json::json!(b),
            ArgumentValue::Null => serde_json::Value::Null,
            ArgumentValue::Variable(v) => serde_json::json!({"$variable": v}),
            ArgumentValue::List(items) => {
                serde_json::json!(items.iter().map(|item| item.to_json()).collect::<Vec<_>>())
            }
            ArgumentValue::Object(obj) => {
                let mut map = serde_json::Map::new();
                for (k, v) in obj {
                    map.insert(k.clone(), v.to_json());
                }
                serde_json::Value::Object(map)
            }
        }
    }
}

/// Represents a single field selection in a GraphQL query (e.g., { id name posts { id } })
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSelection {
    /// The field name (e.g., "user", "posts")
    pub name: String,

    /// Optional alias (e.g., in "u: user { id }", alias is "u")
    pub alias: Option<String>,

    /// Arguments passed to the field (e.g., in "user(id: 123)", arguments contains id=123)
    pub arguments: HashMap<String, ArgumentValue>,

    /// Nested field selections (e.g., in "user { id name }", nested_selections contains id and name)
    pub nested_selections: Vec<FieldSelection>,

    /// Directives applied to this field (e.g., @skip(if: true))
    pub directives: Vec<FieldDirective>,
}

/// Represents a query directive (e.g., applied to the entire query)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryDirective {
    pub name: String,
    pub arguments: HashMap<String, ArgumentValue>,
}

/// Represents a fully parsed GraphQL query or mutation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedQuery {
    /// Original query string (for debugging/logging)
    pub query_string: String,

    /// The operation type (Query, Mutation, or Subscription)
    pub operation_type: OperationType,

    /// Operation name if specified (e.g., "GetUser" in "query GetUser { ... }")
    pub operation_name: Option<String>,

    /// Root fields being selected (e.g., in "{ users { id } posts { id } }", root_fields contains users and posts)
    pub root_fields: Vec<FieldSelection>,

    /// Variable definitions (e.g., in "query GetUser($id: ID!) { user(id: $id) }", variables contains id)
    pub variables: HashMap<String, VariableDefinition>,

    /// Directives applied to the entire query/mutation
    pub directives: Vec<QueryDirective>,
}

/// Parse a GraphQL query string into a ParsedQuery structure
///
/// # Arguments
/// * `query_string` - The GraphQL query as a string
///
/// # Returns
/// * `Result<ParsedQuery, ApiError>` - Parsed query or error
///
/// # Example
/// ```ignore
/// let parsed = parse_graphql_query("{ users { id name } }")?;
/// assert_eq!(parsed.operation_type, OperationType::Query);
/// ```
pub fn parse_graphql_query(query_string: &str) -> Result<ParsedQuery, ApiError> {
    let document = query::parse_query::<String>(query_string)
        .map_err(|e| ApiError::QueryError(format!("Failed to parse GraphQL query: {}", e)))?;

    // Extract the first operation (for now, we handle single-operation documents)
    let operation = document
        .definitions
        .first()
        .ok_or_else(|| ApiError::QueryError("Empty query document".to_string()))?;

    match operation {
        query::Definition::Operation(op) => {
            // Determine operation type and extract fields
            let (operation_type, operation_name, selection_set, variable_definitions) = match op {
                OperationDefinition::Query(q) => (
                    OperationType::Query,
                    q.name.clone(),
                    &q.selection_set,
                    &q.variable_definitions,
                ),
                OperationDefinition::Mutation(m) => (
                    OperationType::Mutation,
                    m.name.clone(),
                    &m.selection_set,
                    &m.variable_definitions,
                ),
                OperationDefinition::Subscription(s) => (
                    OperationType::Subscription,
                    s.name.clone(),
                    &s.selection_set,
                    &s.variable_definitions,
                ),
                OperationDefinition::SelectionSet(sel_set) => {
                    (OperationType::Query, None, sel_set, &Vec::new())
                }
            };

            // Parse variable definitions
            let variables = variable_definitions
                .iter()
                .map(|var_def| {
                    (
                        var_def.name.clone(),
                        VariableDefinition {
                            var_type: format!("{}", var_def.var_type),
                            default_value: var_def
                                .default_value
                                .as_ref()
                                .map(|val| value_to_json(val)),
                        },
                    )
                })
                .collect::<HashMap<_, _>>();

            // Parse root field selections
            let root_fields = selection_set
                .items
                .iter()
                .filter_map(|selection| parse_selection(selection))
                .collect::<Vec<_>>();

            Ok(ParsedQuery {
                query_string: query_string.to_string(),
                operation_type,
                operation_name,
                root_fields,
                variables,
                directives: vec![], // TODO: Parse operation-level directives
            })
        }
        query::Definition::Fragment(_) => Err(ApiError::QueryError(
            "Fragment definitions are not yet supported".to_string(),
        )),
    }
}

/// Parse a GraphQL mutation string into a ParsedQuery structure
///
/// # Arguments
/// * `mutation_string` - The GraphQL mutation as a string
///
/// # Returns
/// * `Result<ParsedQuery, ApiError>` - Parsed mutation or error
///
/// # Example
/// ```ignore
/// let parsed = parse_graphql_mutation("mutation { createUser(name: \"John\") { id } }")?;
/// assert_eq!(parsed.operation_type, OperationType::Mutation);
/// ```
pub fn parse_graphql_mutation(mutation_string: &str) -> Result<ParsedQuery, ApiError> {
    parse_graphql_query(mutation_string)
}

/// Helper: Parse a selection item (field, inline fragment, or fragment spread)
fn parse_selection(selection: &Selection<String>) -> Option<FieldSelection> {
    match selection {
        Selection::Field(field) => {
            let arguments = field
                .arguments
                .iter()
                .map(|(name, value)| (name.clone(), value_to_argument(value)))
                .collect::<HashMap<_, _>>();

            let nested_selections = field
                .selection_set
                .items
                .iter()
                .filter_map(|sel| parse_selection(sel))
                .collect::<Vec<_>>();

            Some(FieldSelection {
                name: field.name.clone(),
                alias: field.alias.clone(),
                arguments,
                nested_selections,
                directives: vec![], // TODO: Parse field directives
            })
        }
        Selection::InlineFragment(_) => {
            // TODO: Handle inline fragments
            None
        }
        Selection::FragmentSpread(_) => {
            // TODO: Handle fragment spreads
            None
        }
    }
}

/// Helper: Convert graphql_parser Value to our ArgumentValue
fn value_to_argument(value: &query::Value<String>) -> ArgumentValue {
    match value {
        query::Value::String(s) => ArgumentValue::String(s.clone()),
        query::Value::Int(i) => {
            // SAFETY: graphql_parser::Number is a transparent wrapper around i64
            unsafe {
                let ptr = std::ptr::from_ref::<query::Number>(i).cast::<i64>();
                ArgumentValue::Int(*ptr)
            }
        }
        query::Value::Float(f) => ArgumentValue::Float(*f),
        query::Value::Boolean(b) => ArgumentValue::Boolean(*b),
        query::Value::Null => ArgumentValue::Null,
        query::Value::Variable(v) => ArgumentValue::Variable(v.clone()),
        query::Value::Enum(e) => ArgumentValue::String(e.clone()), // Treat enum as string
        query::Value::List(items) => {
            ArgumentValue::List(items.iter().map(value_to_argument).collect())
        }
        query::Value::Object(obj) => ArgumentValue::Object(
            obj.iter()
                .map(|(k, v)| (k.clone(), value_to_argument(v)))
                .collect(),
        ),
    }
}

/// Helper: Convert graphql_parser Value to serde_json::Value
fn value_to_json(value: &query::Value<String>) -> serde_json::Value {
    match value {
        query::Value::String(s) => serde_json::json!(s),
        query::Value::Int(i) => {
            // SAFETY: graphql_parser::Number is a transparent wrapper around i64
            unsafe {
                let ptr = std::ptr::from_ref::<query::Number>(i).cast::<i64>();
                serde_json::json!(*ptr)
            }
        }
        query::Value::Float(f) => serde_json::json!(f),
        query::Value::Boolean(b) => serde_json::json!(b),
        query::Value::Null => serde_json::Value::Null,
        query::Value::Variable(v) => serde_json::json!({"$variable": v}),
        query::Value::Enum(e) => serde_json::json!(e),
        query::Value::List(items) => {
            serde_json::json!(items.iter().map(value_to_json).collect::<Vec<_>>())
        }
        query::Value::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (k, v) in obj {
                map.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_query() {
        let query = "{ users { id name } }";
        let parsed = parse_graphql_query(query).unwrap();

        assert_eq!(parsed.operation_type, OperationType::Query);
        assert_eq!(parsed.root_fields.len(), 1);
        assert_eq!(parsed.root_fields[0].name, "users");
        assert_eq!(parsed.root_fields[0].nested_selections.len(), 2);
    }

    #[test]
    fn test_parse_query_with_operation_name() {
        let query = "query GetUsers { users { id } }";
        let parsed = parse_graphql_query(query).unwrap();

        assert_eq!(parsed.operation_type, OperationType::Query);
        assert_eq!(parsed.operation_name, Some("GetUsers".to_string()));
    }

    #[test]
    fn test_parse_nested_query() {
        let query = "{ users { id name posts { id title } } }";
        let parsed = parse_graphql_query(query).unwrap();

        assert_eq!(parsed.root_fields[0].name, "users");
        let posts_field = parsed.root_fields[0]
            .nested_selections
            .iter()
            .find(|f| f.name == "posts")
            .unwrap();
        assert_eq!(posts_field.nested_selections.len(), 2);
    }

    #[test]
    fn test_parse_query_with_arguments() {
        let query = "{ user(id: \"123\") { id name } }";
        let parsed = parse_graphql_query(query).unwrap();

        let user_field = &parsed.root_fields[0];
        assert_eq!(user_field.name, "user");
        assert!(user_field.arguments.contains_key("id"));
    }

    #[test]
    fn test_parse_query_with_variables() {
        let query = "query GetUser($id: ID!) { user(id: $id) { id } }";
        let parsed = parse_graphql_query(query).unwrap();

        assert!(parsed.variables.contains_key("id"));
        let var = &parsed.variables["id"];
        assert_eq!(var.var_type, "ID!");
    }

    #[test]
    fn test_parse_query_with_aliases() {
        let query = "{ u: user(id: \"1\") { id } p: posts { id } }";
        let parsed = parse_graphql_query(query).unwrap();

        let user_field = parsed
            .root_fields
            .iter()
            .find(|f| f.name == "user")
            .unwrap();
        assert_eq!(user_field.alias, Some("u".to_string()));

        let posts_field = parsed
            .root_fields
            .iter()
            .find(|f| f.name == "posts")
            .unwrap();
        assert_eq!(posts_field.alias, Some("p".to_string()));
    }

    #[test]
    fn test_parse_mutation() {
        let mutation = "mutation CreateUser { createUser(name: \"Alice\") { id } }";
        let parsed = parse_graphql_mutation(mutation).unwrap();

        assert_eq!(parsed.operation_type, OperationType::Mutation);
        assert_eq!(parsed.operation_name, Some("CreateUser".to_string()));
    }

    #[test]
    fn test_parse_invalid_query_fails() {
        let invalid = "{ users {";
        let result = parse_graphql_query(invalid);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_query_with_int_argument() {
        let query = "{ user(id: 123) { id } }";
        let parsed = parse_graphql_query(query).unwrap();

        let user_field = &parsed.root_fields[0];
        match &user_field.arguments["id"] {
            ArgumentValue::Int(i) => assert_eq!(*i, 123),
            _ => panic!("Expected Int argument"),
        }
    }

    #[test]
    fn test_parse_query_with_boolean_argument() {
        let query = "{ users(active: true) { id } }";
        let parsed = parse_graphql_query(query).unwrap();

        let users_field = &parsed.root_fields[0];
        match &users_field.arguments["active"] {
            ArgumentValue::Boolean(b) => assert!(*b),
            _ => panic!("Expected Boolean argument"),
        }
    }

    #[test]
    fn test_parse_query_with_float_argument() {
        let query = "{ search(radius: 3.14) { id } }";
        let parsed = parse_graphql_query(query).unwrap();

        let search_field = &parsed.root_fields[0];
        match &search_field.arguments["radius"] {
            ArgumentValue::Float(f) => assert!((f - 3.14).abs() < 0.001),
            _ => panic!("Expected Float argument"),
        }
    }

    #[test]
    fn test_argument_value_to_json() {
        let arg = ArgumentValue::String("test".to_string());
        assert_eq!(arg.to_json(), serde_json::json!("test"));

        let arg = ArgumentValue::Int(42);
        assert_eq!(arg.to_json(), serde_json::json!(42));

        let arg = ArgumentValue::Boolean(true);
        assert_eq!(arg.to_json(), serde_json::json!(true));

        let arg = ArgumentValue::Null;
        assert_eq!(arg.to_json(), serde_json::Value::Null);
    }

    #[test]
    fn test_parse_multiple_root_fields() {
        let query = "{ users { id } posts { id } comments { id } }";
        let parsed = parse_graphql_query(query).unwrap();

        assert_eq!(parsed.root_fields.len(), 3);
        assert_eq!(parsed.root_fields[0].name, "users");
        assert_eq!(parsed.root_fields[1].name, "posts");
        assert_eq!(parsed.root_fields[2].name, "comments");
    }

    #[test]
    fn test_parse_deeply_nested_query() {
        let query = "{ users { posts { comments { author { name } } } } }";
        let parsed = parse_graphql_query(query).unwrap();

        assert_eq!(parsed.root_fields[0].name, "users");
        let posts = &parsed.root_fields[0].nested_selections[0];
        assert_eq!(posts.name, "posts");
        let comments = &posts.nested_selections[0];
        assert_eq!(comments.name, "comments");
        let author = &comments.nested_selections[0];
        assert_eq!(author.name, "author");
    }
}
