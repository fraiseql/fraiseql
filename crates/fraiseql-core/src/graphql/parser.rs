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
pub(crate) const MAX_SERIALIZE_DEPTH: usize = 64;

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
        // `Arc<str>` is the same one-allocation cost as `String::from(&str)` at
        // construction time, but downstream clones of `ParsedQuery` (notably in
        // the parse cache and during fragment resolution) become atomic
        // ref-count bumps instead of full string copies.
        source: std::sync::Arc::from(source),
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
pub(crate) fn serialize_value(value: &query::Value<String>) -> String {
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
