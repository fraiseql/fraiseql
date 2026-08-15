//! MCP Resources and Prompts, derived from the compiled schema (#967).
//!
//! Both are *advertisements*: a Resource says "there is something readable at
//! this URI", a Prompt says "here is a sentence that drives that operation".
//! Neither carries any authority of its own — reading a Resource runs the same
//! operation through the same seam a `tools/call` does, so authentication,
//! tenant dispatch, RLS and the field gates are the tool path's, not a second
//! copy of them.
//!
//! # One allowlist, three surfaces
//!
//! Tools, Resources and Prompts are all built from
//! [`exposed_operations`] — the single source of
//! truth for what `[mcp]`'s `include`/`exclude`/`read_only` makes reachable. That
//! is not tidiness: a Resource list built from `schema.queries` directly would
//! advertise the URIs of operations the allowlist withholds, which is an
//! existence oracle for exactly the names an operator chose to hide, and a
//! `read_resource` that resolved against a *different* set than `tools/call`
//! would be a second door around it (#808).

use std::fmt::Write as _;

use fraiseql_core::schema::{CompiledSchema, McpConfig};
use rmcp::model::{
    Annotated, Prompt, PromptArgument, PromptMessage, PromptMessageRole, RawResource,
    RawResourceTemplate, Resource, ResourceTemplate,
};

use super::tools::{ExposedOperation, exposed_operations};

/// The URI scheme and path prefix a query Resource is published under.
const QUERY_URI_PREFIX: &str = "fraiseql://query/";

/// The MIME type every FraiseQL Resource returns.
const RESOURCE_MIME: &str = "application/json";

/// Every exposed **query** as a readable Resource.
///
/// Mutations are excluded by construction, and not because `read_only` might be
/// set: `resources/read` is a GET in every client that speaks MCP, and a client
/// that reads a resource has not asked to change anything. A mutation advertised
/// as a Resource would be a write behind a verb that promises a read.
///
/// A query with no `description` is still listed. The issue's sketch said
/// "each compiled query **with a description**", but withholding the others makes
/// the Resource list a function of documentation quality — an agent then cannot
/// see an operation that `tools/list` advertises, for no reason it can act on.
/// The description is used when present and the name stands in when it is not.
#[must_use]
pub fn schema_to_resources(schema: &CompiledSchema, config: &McpConfig) -> Vec<Resource> {
    exposed_operations(schema, config)
        .into_iter()
        .filter_map(|(display, op)| match op {
            ExposedOperation::Query(q) => Some((display, q)),
            ExposedOperation::Mutation(_) => None,
        })
        .map(|(display, q)| {
            Annotated::new(
                RawResource {
                    uri:         format!("{QUERY_URI_PREFIX}{display}"),
                    name:        display.clone(),
                    title:       Some(display.clone()),
                    description: Some(q.description.clone().unwrap_or_else(|| {
                        format!("Rows of {} returned by the '{display}' query", q.return_type)
                    })),
                    mime_type:   Some(RESOURCE_MIME.to_string()),
                    size:        None,
                    icons:       None,
                    meta:        None,
                },
                None,
            )
        })
        .collect()
}

/// Resource *templates* for the parameterised reads.
///
/// One `similarity-search` template per exposed query whose return type declares
/// a vector field (#386), so a RAG client can discover that the operation takes
/// an embedding rather than having to infer it from the tool's JSON schema.
///
/// A template is a discovery aid, not a second execution path: its URI resolves
/// through the same `read_resource` as any other, so nothing about the gate
/// changes.
#[must_use]
pub fn schema_to_resource_templates(
    schema: &CompiledSchema,
    config: &McpConfig,
) -> Vec<ResourceTemplate> {
    exposed_operations(schema, config)
        .into_iter()
        .filter_map(|(display, op)| match op {
            ExposedOperation::Query(q) => Some((display, q)),
            ExposedOperation::Mutation(_) => None,
        })
        .filter(|(_, q)| returns_a_vector_type(schema, &q.return_type))
        .map(|(display, q)| {
            Annotated::new(
                RawResourceTemplate {
                    uri_template: format!("{QUERY_URI_PREFIX}{display}{{?nearest,limit}}"),
                    name:         format!("{display} similarity-search"),
                    title:        Some(format!("{display} (similarity search)")),
                    description:  Some(format!(
                        "Nearest-neighbour search over {}'s vector field. Pass `nearest` (the \
                         query embedding) and an optional `limit` as tool arguments; the same \
                         operation is callable as the '{display}' tool.",
                        q.return_type
                    )),
                    mime_type:    Some(RESOURCE_MIME.to_string()),
                    icons:        None,
                },
                None,
            )
        })
        .collect()
}

/// Whether `type_name` declares at least one vector-typed field.
fn returns_a_vector_type(schema: &CompiledSchema, type_name: &str) -> bool {
    schema.types.iter().filter(|t| t.name == type_name).any(|t| {
        t.fields.iter().any(|f| {
            matches!(
                f.field_type,
                fraiseql_core::schema::FieldType::Vector
                    | fraiseql_core::schema::FieldType::HalfVector
                    | fraiseql_core::schema::FieldType::SparseVector
                    | fraiseql_core::schema::FieldType::BitVector
            )
        })
    })
}

/// The operation name a `fraiseql://query/{name}` URI addresses.
///
/// `None` for any other URI — including one that merely starts with the scheme,
/// so a client cannot reach an operation by dressing a name up as a path. The
/// caller still resolves the returned name through the allowlist, so this is a
/// parser and not an authorization decision.
#[must_use]
pub fn query_name_from_uri(uri: &str) -> Option<&str> {
    let name = uri.strip_prefix(QUERY_URI_PREFIX)?;
    // One path segment, no traversal, no query string: the name is looked up in
    // the exposed set by exact match, so anything else is a caller error rather
    // than something to normalise into a match.
    if name.is_empty() || name.contains('/') || name.contains('?') || name.contains('#') {
        return None;
    }
    Some(name)
}

/// Every exposed operation as a Prompt.
///
/// A Prompt here is the operation's own description rendered as an instruction,
/// with one argument per operation argument. Mutations **are** included — unlike
/// Resources — because a prompt is a sentence, not an execution: getting it
/// changes nothing, and an agent asked to write data needs the writing operation
/// described. Whether it can then *call* that operation is the tool allowlist's
/// decision, and `read_only` has already removed mutations from the exposed set
/// if the operator said so.
#[must_use]
pub fn schema_to_prompts(schema: &CompiledSchema, config: &McpConfig) -> Vec<Prompt> {
    exposed_operations(schema, config)
        .into_iter()
        .map(|(display, op)| {
            let arguments: Vec<PromptArgument> = op
                .arguments()
                .into_iter()
                .map(|arg| {
                    let mut a = PromptArgument::new(arg.name.clone()).with_required(!arg.nullable);
                    if let Some(ref d) = arg.description {
                        a = a.with_description(d.clone());
                    }
                    a
                })
                .collect();
            Prompt::new(
                display.clone(),
                Some(prompt_description(&display, op)),
                if arguments.is_empty() {
                    None
                } else {
                    Some(arguments)
                },
            )
            .with_title(display)
        })
        .collect()
}

/// The human-readable line a Prompt advertises.
fn prompt_description(display: &str, op: ExposedOperation<'_>) -> String {
    let authored = match op {
        ExposedOperation::Query(q) => q.description.clone(),
        ExposedOperation::Mutation(m) => m.description.clone(),
    };
    authored.unwrap_or_else(|| {
        if op.is_mutation() {
            format!("Perform the '{display}' operation on {}", op.return_type())
        } else {
            format!("Read {} using the '{display}' query", op.return_type())
        }
    })
}

/// Render a Prompt into the message an agent receives.
///
/// Substitutes the supplied arguments into a single user-role message. Returns
/// `None` for a name the allowlist does not expose — deliberately
/// indistinguishable from "no such operation", for the same reason
/// [`resolve_tool`](super::tools::resolve_tool) is: an error separating the two
/// is an existence oracle for the names an operator chose to hide.
#[must_use]
pub fn render_prompt(
    name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    schema: &CompiledSchema,
    config: &McpConfig,
) -> Option<(String, Vec<PromptMessage>)> {
    let op = super::tools::resolve_tool(name, schema, config)?;
    let description = prompt_description(name, op);

    let mut text = format!("{description}\n\nCall the '{name}' tool");
    if let Some(args) = arguments.filter(|a| !a.is_empty()) {
        text.push_str(" with:\n");
        // Sorted so the rendered prompt is stable across runs — a `HashMap`
        // iteration order would make the same request produce a different
        // message each time, which is the kind of nondeterminism that makes an
        // agent's behaviour irreproducible.
        let mut keys: Vec<&String> = args.keys().collect();
        keys.sort();
        for key in keys {
            let _ = writeln!(text, "  {key} = {}", args[key]);
        }
    } else {
        text.push('.');
    }

    Some((description, vec![PromptMessage::new_text(PromptMessageRole::User, text)]))
}
