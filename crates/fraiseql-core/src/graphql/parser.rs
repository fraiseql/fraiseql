//! Selecting the operation to execute from a parsed document (§ 6.1), as a [`ParsedQuery`].
//!
//! The document itself is read once by [`document::parse`](crate::graphql::document::parse),
//! through the request's shared parse
//! ([`parse_graphql_document`](crate::graphql::parse_graphql_document)).

use crate::graphql::{
    document::{Document, Operation},
    types::ParsedQuery,
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
    // Through the request's shared parse: one document per request, whichever stage asks.
    let doc = crate::graphql::complexity::parse_graphql_document(source)
        .map_err(|e| GraphQLParseError::Syntax(e.to_string()))?;

    let operation = select_operation(&doc, operation_name)?;
    parse_selected_operation(operation, &doc, source)
}

/// Choose the operation to execute from a parsed document (GraphQL § 6.1).
fn select_operation<'d>(
    doc: &'d Document,
    operation_name: Option<&str>,
) -> Result<&'d Operation, GraphQLParseError> {
    let mut operations = doc.operations.iter();

    let Some(requested) = operation_name else {
        // Anonymous request: legal only when the document is unambiguous.
        let first = operations.next().ok_or(GraphQLParseError::MissingOperation)?;
        let extra = operations.count();
        if extra > 0 {
            return Err(GraphQLParseError::AmbiguousOperation(extra + 1));
        }
        return Ok(first);
    };

    if doc.operations.is_empty() {
        return Err(GraphQLParseError::MissingOperation);
    }
    // A bare selection set (`{ users { id } }`) is anonymous by construction and can never
    // be selected by name.
    operations
        .find(|op| op.name.as_deref() == Some(requested))
        .ok_or_else(|| GraphQLParseError::UnknownOperation(requested.to_string()))
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
/// [`GraphQLParseError::EmptySelection`] — the operation selects no field.
pub fn parse_selected_operation(
    operation: &Operation,
    doc: &Document,
    source: &str,
) -> Result<ParsedQuery, GraphQLParseError> {
    // Get root field name (first field in selection set)
    let root_field = operation
        .selections
        .first()
        .map(|s| s.name.clone())
        .ok_or(GraphQLParseError::EmptySelection)?;

    Ok(ParsedQuery {
        operation_type: operation.kind.as_str().to_string(),
        operation_name: operation.name.clone(),
        root_field,
        selections: operation.selections.clone(),
        variables: operation.variables.clone(),
        // Fragments are document-scoped, not operation-scoped: an operation may
        // spread any fragment the document defines, so the whole set travels with
        // every selected operation.
        fragments: doc.fragments.clone(),
        // `Arc<str>` is the same one-allocation cost as `String::from(&str)` at
        // construction time, but downstream clones of `ParsedQuery` (notably in
        // the parse cache and during fragment resolution) become atomic
        // ref-count bumps instead of full string copies.
        source: std::sync::Arc::from(source),
    })
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
