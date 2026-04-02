//! GraphQL query parser using graphql-parser crate.
//!
//! Parses GraphQL query strings into a Rust AST for further processing
//! by fragment resolution and directive evaluation.

use graphql_parser::query::{
    self, Definition, Directive as GraphQLDirective, Document, OperationDefinition, Selection,
};

use crate::graphql::types::{
    Directive, FieldSelection, GraphQLArgument, GraphQLType, ParsedQuery, VariableDefinition,
};

/// Errors that can occur when parsing a GraphQL query.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GraphQLParseError {
    /// Failed to parse GraphQL syntax.
    #[error("Failed to parse GraphQL query: {0}")]
    Syntax(String),

    /// No query or mutation operation found in the document.
    #[error("No query or mutation operation found")]
    MissingOperation,

    /// Selection set has no fields.
    #[error("No fields in selection set")]
    EmptySelection,

    /// GraphQL value nesting exceeds the allowed depth limit.
    #[error("GraphQL value nesting exceeds maximum depth ({0} levels)")]
    ValueNestingTooDeep(usize),
}

/// Maximum nesting depth for `serialize_value` recursion.
///
/// Real-world GraphQL variables rarely exceed 5-10 levels of nesting.  A cap
/// of 64 is generous while preventing stack-exhaustion from a crafted payload
/// like `[[[[…]]]]` with tens-of-thousands of levels.
const MAX_SERIALIZE_DEPTH: usize = 64;

/// Parse GraphQL query string into Rust AST.
///
/// # Errors
///
/// Returns an error if:
/// - GraphQL syntax is invalid or malformed
/// - Query structure is invalid (missing operation, invalid selections)
///
/// # Example
///
/// ```
/// use fraiseql_core::graphql::parse_query;
///
/// let query = "query { users { id name } }";
/// let parsed = parse_query(query).unwrap();
/// assert_eq!(parsed.operation_type, "query");
/// assert_eq!(parsed.root_field, "users");
/// ```
pub fn parse_query(source: &str) -> Result<ParsedQuery, GraphQLParseError> {
    // Use graphql-parser to parse query string
    let doc: Document<String> =
        query::parse_query(source).map_err(|e| GraphQLParseError::Syntax(e.to_string()))?;

    // Extract first operation (ignore multiple operations for now)
    let operation = doc
        .definitions
        .iter()
        .find_map(|def| match def {
            query::Definition::Operation(op) => Some(op),
            query::Definition::Fragment(_) => None,
        })
        .ok_or(GraphQLParseError::MissingOperation)?;

    // Extract operation details
    let (operation_type, operation_name, root_field, selections, variables) =
        extract_operation(operation)?;

    // Extract fragment definitions
    let fragments = extract_fragments(&doc)?;

    Ok(ParsedQuery {
        operation_type,
        operation_name,
        root_field,
        selections,
        variables,
        fragments,
        source: source.to_string(),
    })
}

/// Extract fragment definitions from GraphQL document.
fn extract_fragments(
    doc: &Document<String>,
) -> Result<Vec<crate::graphql::types::FragmentDefinition>, GraphQLParseError> {
    let mut fragments = Vec::new();

    for def in &doc.definitions {
        if let Definition::Fragment(fragment) = def {
            let selections = parse_selection_set(&fragment.selection_set)?;

            // Extract fragment spreads from selections
            let fragment_spreads = extract_fragment_spreads(&fragment.selection_set);

            // Convert type condition to string
            let type_condition = match &fragment.type_condition {
                query::TypeCondition::On(type_name) => type_name.clone(),
            };

            fragments.push(crate::graphql::types::FragmentDefinition {
                name: fragment.name.clone(),
                type_condition,
                selections,
                fragment_spreads,
            });
        }
    }

    Ok(fragments)
}

/// Extract fragment spreads from a selection set.
fn extract_fragment_spreads(selection_set: &query::SelectionSet<String>) -> Vec<String> {
    let mut spreads = Vec::new();

    for selection in &selection_set.items {
        match selection {
            Selection::FragmentSpread(spread) => {
                spreads.push(spread.fragment_name.clone());
            },
            Selection::InlineFragment(inline) => {
                // Inline fragments can also contain spreads
                spreads.extend(extract_fragment_spreads(&inline.selection_set));
            },
            Selection::Field(field) => {
                // Fields can have nested selections with spreads
                spreads.extend(extract_fragment_spreads(&field.selection_set));
            },
        }
    }

    spreads
}

/// Extract operation details from GraphQL operation definition.
fn extract_operation(
    operation: &OperationDefinition<String>,
) -> Result<
    (String, Option<String>, String, Vec<FieldSelection>, Vec<VariableDefinition>),
    GraphQLParseError,
> {
    let operation_type = match operation {
        OperationDefinition::Query(_) | OperationDefinition::SelectionSet(_) => "query",
        OperationDefinition::Mutation(_) => "mutation",
        OperationDefinition::Subscription(_) => "subscription",
    }
    .to_string();

    let (name, selection_set, var_defs) = match operation {
        OperationDefinition::Query(q) => (&q.name, &q.selection_set, &q.variable_definitions),
        OperationDefinition::Mutation(m) => (&m.name, &m.selection_set, &m.variable_definitions),
        OperationDefinition::Subscription(s) => {
            (&s.name, &s.selection_set, &s.variable_definitions)
        },
        OperationDefinition::SelectionSet(sel_set) => (&None, sel_set, &Vec::new()),
    };

    // Parse selection set (recursive)
    let selections = parse_selection_set(selection_set)?;

    // Get root field name (first field in selection set)
    let root_field = selections
        .first()
        .map(|s| s.name.clone())
        .ok_or(GraphQLParseError::EmptySelection)?;

    // Parse variable definitions
    let variables = var_defs
        .iter()
        .map(|var_def| VariableDefinition {
            name:          var_def.name.clone(),
            var_type:      parse_graphql_type(&var_def.var_type),
            default_value: var_def.default_value.as_ref().map(|v| serialize_value(v)),
        })
        .collect();

    Ok((operation_type, name.clone(), root_field, selections, variables))
}

/// Parse GraphQL selection set recursively.
///
/// Handles fields, fragment spreads, and inline fragments.
fn parse_selection_set(
    selection_set: &query::SelectionSet<String>,
) -> Result<Vec<FieldSelection>, GraphQLParseError> {
    let mut fields = Vec::new();

    for selection in &selection_set.items {
        match selection {
            Selection::Field(field) => {
                // Parse field arguments
                let arguments = field
                    .arguments
                    .iter()
                    .map(|(name, value)| GraphQLArgument {
                        name:       name.clone(),
                        value_type: value_type_string(value),
                        value_json: serialize_value(value),
                    })
                    .collect();

                // Parse nested selection set (recursive)
                let nested_fields = parse_selection_set(&field.selection_set)?;

                let directives = field.directives.iter().map(parse_directive).collect();

                fields.push(FieldSelection {
                    name: field.name.clone(),
                    alias: field.alias.clone(),
                    arguments,
                    nested_fields,
                    directives,
                });
            },
            Selection::FragmentSpread(spread) => {
                // Represent fragment spread as a special field with "..." prefix
                // This will be resolved by FragmentResolver
                let directives = spread.directives.iter().map(parse_directive).collect();

                fields.push(FieldSelection {
                    name: format!("...{}", spread.fragment_name),
                    alias: None,
                    arguments: vec![],
                    nested_fields: vec![],
                    directives,
                });
            },
            Selection::InlineFragment(inline) => {
                // Represent inline fragment as special field
                // Type condition is stored in the name
                let type_condition =
                    inline.type_condition.as_ref().map_or_else(String::new, |tc| match tc {
                        query::TypeCondition::On(name) => name.clone(),
                    });

                let nested_fields = parse_selection_set(&inline.selection_set)?;
                let directives = inline.directives.iter().map(parse_directive).collect();

                fields.push(FieldSelection {
                    name: format!("...on {type_condition}"),
                    alias: None,
                    arguments: vec![],
                    nested_fields,
                    directives,
                });
            },
        }
    }

    Ok(fields)
}

/// Get type of GraphQL value for classification.
fn value_type_string(value: &query::Value<String>) -> String {
    match value {
        query::Value::String(_) => "string".to_string(),
        query::Value::Int(_) => "int".to_string(),
        query::Value::Float(_) => "float".to_string(),
        query::Value::Boolean(_) => "boolean".to_string(),
        query::Value::Null => "null".to_string(),
        query::Value::Enum(_) => "enum".to_string(),
        query::Value::List(_) => "list".to_string(),
        query::Value::Object(_) => "object".to_string(),
        query::Value::Variable(_) => "variable".to_string(),
    }
}

/// Serialize GraphQL value to JSON string.
///
/// Returns `None` when the recursion depth exceeds `MAX_SERIALIZE_DEPTH`.
/// The public wrapper `serialize_value` returns a fallback `"null"` in that case;
/// callers that need to surface the error can call `try_serialize_value` directly.
fn serialize_value_inner(value: &query::Value<String>, depth: usize) -> Option<String> {
    if depth > MAX_SERIALIZE_DEPTH {
        return None;
    }

    let s = match value {
        query::Value::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        query::Value::Int(i) => {
            // Use the safe as_i64() method from graphql-parser
            i.as_i64().map_or_else(|| "0".to_string(), |n| n.to_string())
        },
        query::Value::Float(f) => format!("{f}"),
        query::Value::Boolean(b) => b.to_string(),
        query::Value::Null => "null".to_string(),
        query::Value::Enum(e) => format!("\"{e}\""),
        query::Value::List(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(serialize_value_inner(item, depth + 1)?);
            }
            format!("[{}]", parts.join(","))
        },
        query::Value::Object(obj) => {
            let mut pairs = Vec::with_capacity(obj.len());
            for (k, v) in obj {
                let serialized = serialize_value_inner(v, depth + 1)?;
                pairs.push(format!("\"{}\":{serialized}", k));
            }
            format!("{{{}}}", pairs.join(","))
        },
        query::Value::Variable(v) => format!("\"${v}\""),
    };

    Some(s)
}

/// Serialize a GraphQL value to a JSON string.
///
/// Returns `"null"` if the value is nested more than `MAX_SERIALIZE_DEPTH` levels deep,
/// preventing stack exhaustion from adversarially crafted variable payloads.
fn serialize_value(value: &query::Value<String>) -> String {
    serialize_value_inner(value, 0).unwrap_or_else(|| "null".to_string())
}

/// Parse GraphQL directive from graphql-parser Directive.
fn parse_directive(directive: &GraphQLDirective<String>) -> Directive {
    let arguments = directive
        .arguments
        .iter()
        .map(|(name, value)| GraphQLArgument {
            name:       name.clone(),
            value_type: value_type_string(value),
            value_json: serialize_value(value),
        })
        .collect();

    Directive {
        name: directive.name.clone(),
        arguments,
    }
}

/// Parse GraphQL type from graphql-parser Type to our `GraphQLType`.
fn parse_graphql_type(graphql_type: &query::Type<String>) -> GraphQLType {
    match graphql_type {
        query::Type::NamedType(name) => GraphQLType {
            name:          name.clone(),
            nullable:      true, // Named types are nullable by default
            list:          false,
            list_nullable: false,
        },
        query::Type::ListType(inner) => GraphQLType {
            name:          format!("[{}]", parse_graphql_type(inner).name),
            nullable:      true,
            list:          true,
            list_nullable: true, // List items are nullable by default
        },
        query::Type::NonNullType(inner) => {
            let mut parsed = parse_graphql_type(inner);
            parsed.nullable = false;
            if parsed.list {
                parsed.list_nullable = false;
            }
            parsed
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_parse_simple_query() {
        let query = "query { users { id name } }";
        let parsed = parse_query(query).unwrap();

        assert_eq!(parsed.operation_type, "query");
        assert_eq!(parsed.root_field, "users");
        assert_eq!(parsed.selections.len(), 1);
        assert_eq!(parsed.selections[0].nested_fields.len(), 2);
    }

    #[test]
    fn test_parse_query_with_arguments() {
        let query = r#"
            query {
                users(where: {status: "active"}, limit: 10) {
                    id
                    name
                }
            }
        "#;
        let parsed = parse_query(query).unwrap();

        let first_field = &parsed.selections[0];
        assert_eq!(first_field.arguments.len(), 2);
        assert_eq!(first_field.arguments[0].name, "where");
        assert_eq!(first_field.arguments[1].name, "limit");
    }

    #[test]
    fn test_parse_mutation() {
        let query = "mutation { createUser(input: {}) { id } }";
        let parsed = parse_query(query).unwrap();

        assert_eq!(parsed.operation_type, "mutation");
        assert_eq!(parsed.root_field, "createUser");
    }

    #[test]
    fn test_parse_query_with_variables() {
        let query = r"
            query GetUsers($where: UserWhere!) {
                users(where: $where) {
                    id
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        assert_eq!(parsed.variables.len(), 1);
        assert_eq!(parsed.variables[0].name, "where");
    }

    #[test]
    fn test_parse_query_with_integer_argument() {
        let query = r"
            query {
                users(limit: 42, offset: 100) {
                    id
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        let first_field = &parsed.selections[0];
        assert_eq!(first_field.arguments.len(), 2);

        assert_eq!(first_field.arguments[0].name, "limit");
        assert_eq!(first_field.arguments[0].value_type, "int");
        assert_eq!(first_field.arguments[0].value_json, "42");

        assert_eq!(first_field.arguments[1].name, "offset");
        assert_eq!(first_field.arguments[1].value_type, "int");
        assert_eq!(first_field.arguments[1].value_json, "100");
    }

    #[test]
    fn test_parse_query_with_fragment() {
        let query = r"
            fragment UserFields on User {
                id
                name
                email
            }

            query {
                users {
                    ...UserFields
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        // Should have fragment definition
        assert_eq!(parsed.fragments.len(), 1);
        assert_eq!(parsed.fragments[0].name, "UserFields");
        assert_eq!(parsed.fragments[0].type_condition, "User");
        assert_eq!(parsed.fragments[0].selections.len(), 3);

        // Selection should have fragment spread
        assert_eq!(parsed.selections[0].nested_fields.len(), 1);
        assert_eq!(parsed.selections[0].nested_fields[0].name, "...UserFields");
    }

    #[test]
    fn test_parse_query_with_directives() {
        let query = r"
            query($skipEmail: Boolean!) {
                users {
                    id
                    email @skip(if: $skipEmail)
                    name @include(if: true)
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        let user_fields = &parsed.selections[0].nested_fields;
        assert_eq!(user_fields.len(), 3);

        // id has no directives
        assert!(user_fields[0].directives.is_empty());

        // email has @skip
        assert_eq!(user_fields[1].directives.len(), 1);
        assert_eq!(user_fields[1].directives[0].name, "skip");

        // name has @include
        assert_eq!(user_fields[2].directives.len(), 1);
        assert_eq!(user_fields[2].directives[0].name, "include");
    }

    #[test]
    fn test_parse_query_with_alias() {
        let query = r"
            query {
                users {
                    id
                    writer: author {
                        name
                    }
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        let user_fields = &parsed.selections[0].nested_fields;
        assert_eq!(user_fields.len(), 2);

        // Check aliased field
        let aliased_field = &user_fields[1];
        assert_eq!(aliased_field.name, "author");
        assert_eq!(aliased_field.alias, Some("writer".to_string()));
    }

    #[test]
    fn test_parse_inline_fragment() {
        let query = r"
            query {
                users {
                    id
                    ... on Admin {
                        permissions
                    }
                }
            }
        ";
        let parsed = parse_query(query).unwrap();

        let user_fields = &parsed.selections[0].nested_fields;
        assert_eq!(user_fields.len(), 2);

        // Check inline fragment
        assert_eq!(user_fields[1].name, "...on Admin");
        assert_eq!(user_fields[1].nested_fields.len(), 1);
        assert_eq!(user_fields[1].nested_fields[0].name, "permissions");
    }

    // ── serialize_value depth guard ────────────────────────────────────────────

    #[test]
    fn test_serialize_value_flat_list_accepted() {
        // A flat list of scalars is well within the depth limit.
        let value = query::Value::List(vec![
            query::Value::Int(graphql_parser::query::Number::from(1_i32)),
            query::Value::String("hello".to_string()),
            query::Value::Boolean(true),
        ]);
        let result = serialize_value(&value);
        assert_eq!(result, r#"[1,"hello",true]"#);
    }

    #[test]
    fn test_serialize_value_nested_at_limit_accepted() {
        // Build a list nested exactly MAX_SERIALIZE_DEPTH levels — must serialize.
        let mut v: query::Value<String> = query::Value::Boolean(true);
        for _ in 0..MAX_SERIALIZE_DEPTH {
            v = query::Value::List(vec![v]);
        }
        let result = serialize_value(&v);
        // Verify it didn't fall back to "null" — it should contain "true".
        assert!(result.contains("true"), "value at limit should serialize correctly: {result}");
    }

    #[test]
    fn test_serialize_value_exceeds_depth_returns_null() {
        // Build a list nested MAX_SERIALIZE_DEPTH + 1 levels — must return "null".
        let mut v: query::Value<String> = query::Value::Boolean(true);
        for _ in 0..=MAX_SERIALIZE_DEPTH {
            v = query::Value::List(vec![v]);
        }
        let result = serialize_value(&v);
        assert_eq!(result, "null", "over-limit value must fall back to null: {result}");
    }

    #[test]
    fn test_serialize_value_deeply_nested_object_returns_null() {
        // Deeply nested object should also hit the depth cap.
        let mut v: query::Value<String> = query::Value::Boolean(false);
        for i in 0..=MAX_SERIALIZE_DEPTH {
            let mut map = std::collections::BTreeMap::new();
            map.insert(format!("k{i}"), v);
            v = query::Value::Object(map);
        }
        let result = serialize_value(&v);
        assert_eq!(result, "null", "over-limit object must fall back to null: {result}");
    }
}
