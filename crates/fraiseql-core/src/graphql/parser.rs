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

    /// The requested `operationName` matches no operation in the document
    /// (GraphQL § 6.1 *`GetOperation`*).
    #[error("Unknown operation named '{0}'")]
    UnknownOperation(String),

    /// The document defines more than one operation and the request named none
    /// (GraphQL § 6.1 *`GetOperation`*).
    #[error(
        "Document defines {0} operations but no `operationName` was provided — the request must \
         name the operation to execute"
    )]
    AmbiguousOperation(usize),

    /// Selection set has no fields.
    #[error("No fields in selection set")]
    EmptySelection,

    /// GraphQL value nesting exceeds the allowed depth limit.
    #[error("GraphQL value nesting exceeds maximum depth ({0} levels)")]
    ValueNestingTooDeep(usize),
}

impl GraphQLParseError {
    /// Whether this is a *request* error about which operation to run, rather
    /// than a syntax error in the document.
    ///
    /// The distinction is what the client sees: a document that does not parse
    /// is a `PARSE_ERROR`, but `operationName: "Nope"` against a document that
    /// parses perfectly is a `VALIDATION_ERROR`. Reporting the second as a
    /// parse failure sends the client hunting for a syntax mistake that is not
    /// there.
    #[must_use]
    pub const fn is_operation_selection(&self) -> bool {
        matches!(self, Self::UnknownOperation(_) | Self::AmbiguousOperation(_))
    }
}

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
    parse_query_with_operation_name(source, None)
}

/// Parse a GraphQL document and select the operation to execute, per GraphQL
/// § 6.1 *`GetOperation`*.
///
/// - `Some(name)` selects the operation with that name, and fails if no operation carries it.
/// - `None` selects the document's only operation, and fails if the document defines more than one.
///
/// [`parse_query`] is this function with `None`.
///
/// # Why this is not "take the first operation"
///
/// The previous behaviour took the first operation in the document and ignored
/// `operationName` entirely. A client sending a two-operation document and
/// naming the second received the **first** one's data, under HTTP 200, with a
/// perfectly plausible body — the same silent-wrong-answer failure mode as
/// #1154. Naming an operation that does not exist, and sending a
/// multi-operation document with no name at all, were both accepted the same
/// way.
///
/// # Errors
///
/// - [`GraphQLParseError::Syntax`] — the document does not parse.
/// - [`GraphQLParseError::MissingOperation`] — the document defines no operation.
/// - [`GraphQLParseError::UnknownOperation`] — `operation_name` matches nothing.
/// - [`GraphQLParseError::AmbiguousOperation`] — several operations, none named.
pub fn parse_query_with_operation_name(
    source: &str,
    operation_name: Option<&str>,
) -> Result<ParsedQuery, GraphQLParseError> {
    // Through the guarded seam, not `query::parse_query` directly: the parser can
    // panic on a client-controlled document (#976), and this entry point is public.
    let doc: Document<String> = crate::graphql::complexity::parse_graphql_document(source)
        .map_err(|e| GraphQLParseError::Syntax(e.to_string()))?;

    let operation = select_operation(&doc, operation_name)?;
    parse_selected_operation(operation, &doc, source)
}

/// Choose the operation to execute from a parsed document (GraphQL § 6.1).
fn select_operation<'d>(
    doc: &'d Document<'d, String>,
    operation_name: Option<&str>,
) -> Result<&'d OperationDefinition<'d, String>, GraphQLParseError> {
    let mut operations = doc.definitions.iter().filter_map(|def| match def {
        query::Definition::Operation(op) => Some(op),
        query::Definition::Fragment(_) => None,
    });

    let Some(requested) = operation_name else {
        // Anonymous request: legal only when the document is unambiguous.
        let first = operations.next().ok_or(GraphQLParseError::MissingOperation)?;
        let extra = operations.count();
        if extra > 0 {
            return Err(GraphQLParseError::AmbiguousOperation(extra + 1));
        }
        return Ok(first);
    };

    let mut saw_any = false;
    for op in operations {
        saw_any = true;
        if operation_definition_name(op).is_some_and(|name| name == requested) {
            return Ok(op);
        }
    }

    if saw_any {
        Err(GraphQLParseError::UnknownOperation(requested.to_string()))
    } else {
        Err(GraphQLParseError::MissingOperation)
    }
}

/// The declared name of an operation, if it has one.
///
/// A bare selection set (`{ users { id } }`) is anonymous by construction and
/// can never be selected by name.
fn operation_definition_name<'a>(
    operation: &'a OperationDefinition<'a, String>,
) -> Option<&'a str> {
    match operation {
        OperationDefinition::Query(q) => q.name.as_deref(),
        OperationDefinition::Mutation(m) => m.name.as_deref(),
        OperationDefinition::Subscription(s) => s.name.as_deref(),
        OperationDefinition::SelectionSet(_) => None,
    }
}

/// Build a [`ParsedQuery`] from an operation the caller has **already selected**.
///
/// The seam for callers that do their own operation selection and must not have
/// it silently redone. `routes/subscriptions.rs` is the motivating case: it
/// filters for the single *subscription* operation and rejects documents with
/// more than one, so routing it back through [`parse_query`] would validate the
/// document's *first* operation — which on a document mixing `query Q {…}` with
/// `subscription S {…}` is the wrong one.
///
/// # Errors
///
/// - [`GraphQLParseError::EmptySelection`] — the operation selects no field.
/// - [`GraphQLParseError::ValueNestingTooDeep`] — an argument value nests too deeply.
pub fn parse_selected_operation(
    operation: &OperationDefinition<String>,
    doc: &Document<String>,
    source: &str,
) -> Result<ParsedQuery, GraphQLParseError> {
    let (operation_type, operation_name, root_field, selections, variables) =
        extract_operation(operation)?;

    // Fragments are document-scoped, not operation-scoped: an operation may
    // spread any fragment the document defines, so the whole set travels with
    // every selected operation.
    let fragments = extract_fragments(doc)?;

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

/// Serialize a GraphQL value to the shared `value_json` representation.
///
/// Delegates to [`crate::graphql::value_json::encode`], which uses `serde_json`
/// rather than hand-rolled escaping and tags variable references out of band.
/// A value too deeply nested to serialize yields `"null"` — the same conservative
/// fallback the previous depth guard produced, and the depth cap is shared.
pub(crate) fn serialize_value(value: &query::Value<String>) -> String {
    crate::graphql::value_json::encode(value).unwrap_or_else(|_| "null".to_string())
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

/// Map a parse failure onto the error kind the client should see.
///
/// A document that does not parse is a parse error. A document that parses but
/// names an operation that is not there — or names none when it must — is a
/// *validation* error: the syntax is fine, the request is wrong.
#[must_use]
pub fn operation_selection_error(e: &GraphQLParseError) -> crate::error::FraiseQLError {
    if e.is_operation_selection() {
        crate::error::FraiseQLError::Validation {
            message: e.to_string(),
            path:    Some("operationName".to_string()),
        }
    } else {
        crate::error::FraiseQLError::Parse {
            message:  e.to_string(),
            location: "query".to_string(),
        }
    }
}
