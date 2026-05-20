//! Query planner — decomposes a GraphQL operation into subgraph fetches.
//!
//! The planner maps each root field to the subgraph that owns it and produces
//! an ordered list of `SubgraphFetch` steps. For fields that span subgraphs
//! (entity references), the planner emits follow-up `_entities` fetches.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

/// Maximum depth of nested entity resolution to prevent unbounded recursion.
pub(crate) const MAX_ENTITY_DEPTH: usize = 8;

/// A query plan ready for execution.
#[derive(Debug, Clone, Serialize)]
pub struct QueryPlan {
    /// Ordered sequence of fetches to execute.
    pub fetches: Vec<SubgraphFetch>,
}

/// A single fetch to a subgraph.
#[derive(Debug, Clone, Serialize)]
pub struct SubgraphFetch {
    /// Name of the target subgraph.
    pub subgraph: String,

    /// GraphQL operation to send.
    pub query: String,

    /// Variables to include.
    pub variables: Value,

    /// Whether this is an `_entities` follow-up fetch.
    pub is_entity_fetch: bool,

    /// Index of the parent fetch whose results feed into this entity fetch.
    /// `None` for root-level fetches.
    pub depends_on: Option<usize>,
}

/// Maps root-level GraphQL field names to the subgraph that owns them.
///
/// Built at gateway startup from the composed schema.
#[derive(Debug, Clone, Default)]
pub struct FieldOwnership {
    /// field_name → subgraph_name
    entries: HashMap<String, String>,
}

impl FieldOwnership {
    /// Register a field as owned by a subgraph.
    pub fn insert(&mut self, field: String, subgraph: String) {
        self.entries.insert(field, subgraph);
    }

    /// Look up the owning subgraph for a field.
    pub fn owner(&self, field: &str) -> Option<&str> {
        self.entries.get(field).map(String::as_str)
    }
}

/// Errors from the query planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// A root field has no owning subgraph.
    UnknownField {
        /// The field name that has no owner.
        field: String,
    },
    /// Entity resolution depth exceeded.
    DepthExceeded {
        /// Current depth.
        depth: usize,
        /// Maximum allowed depth.
        max: usize,
    },
    /// The query body is empty / unparseable at the planner level.
    EmptyQuery,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownField { field } => {
                write!(f, "No subgraph owns root field '{field}'")
            },
            Self::DepthExceeded { depth, max } => {
                write!(f, "Entity resolution depth {depth} exceeds max {max}")
            },
            Self::EmptyQuery => write!(f, "Query is empty"),
        }
    }
}

impl std::error::Error for PlanError {}

/// Plan execution of a GraphQL query across subgraphs.
///
/// For the MVP (Layer 1), this does simple field→subgraph routing: each
/// root field is forwarded to the subgraph that owns it. Fields owned by
/// the same subgraph are grouped into a single fetch.
///
/// # Errors
///
/// Returns `PlanError::UnknownField` if a root field has no owner.
/// Returns `PlanError::EmptyQuery` if no root fields are found.
pub fn plan_query(
    root_fields: &[String],
    ownership: &FieldOwnership,
) -> Result<QueryPlan, PlanError> {
    if root_fields.is_empty() {
        return Err(PlanError::EmptyQuery);
    }

    // Group fields by subgraph
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for field in root_fields {
        let owner = ownership.owner(field).ok_or_else(|| PlanError::UnknownField {
            field: field.clone(),
        })?;
        groups.entry(owner.to_string()).or_default().push(field.clone());
    }

    // Build fetches — one per subgraph
    let fetches = groups
        .into_iter()
        .map(|(subgraph, fields)| {
            let selection = fields.join("\n    ");
            let query = format!("{{\n    {selection}\n}}");
            SubgraphFetch {
                subgraph,
                query,
                variables: Value::Object(serde_json::Map::new()),
                is_entity_fetch: false,
                depends_on: None,
            }
        })
        .collect();

    Ok(QueryPlan { fetches })
}

/// Plan an `_entities` fetch for cross-subgraph entity resolution (Layer 2).
///
/// Given a list of entity representations and the target subgraph, builds
/// the `_entities` query.
///
/// # Errors
///
/// Returns `PlanError::DepthExceeded` if `current_depth` >= `MAX_ENTITY_DEPTH`.
pub fn plan_entity_fetch(
    subgraph: &str,
    representations: &[Value],
    selection: &str,
    current_depth: usize,
) -> Result<SubgraphFetch, PlanError> {
    if current_depth >= MAX_ENTITY_DEPTH {
        return Err(PlanError::DepthExceeded {
            depth: current_depth,
            max: MAX_ENTITY_DEPTH,
        });
    }

    let query = format!(
        "query($representations: [_Any!]!) {{\n    _entities(representations: $representations) {{\n        ... on _ {{\n            {selection}\n        }}\n    }}\n}}"
    );

    let variables = serde_json::json!({
        "representations": representations,
    });

    Ok(SubgraphFetch {
        subgraph: subgraph.to_string(),
        query,
        variables,
        is_entity_fetch: true,
        depends_on: None,
    })
}

/// Extract root-level field names from a simple GraphQL query body.
///
/// This is a lightweight extractor for `{ field1 field2(arg: val) { sub } }`
/// style queries. It returns the top-level field names only.
pub fn extract_root_fields(query: &str) -> Vec<String> {
    let trimmed = query.trim();

    // Strip leading `query ... {` or `mutation ... {`
    let body = if let Some(brace_start) = trimmed.find('{') {
        &trimmed[brace_start + 1..]
    } else {
        return Vec::new();
    };

    // Strip trailing `}`
    let body = if let Some(brace_end) = body.rfind('}') {
        &body[..brace_end]
    } else {
        return Vec::new();
    };

    let mut fields = Vec::new();
    let mut brace_depth: i32 = 0;
    let mut paren_depth: i32 = 0;

    for token in body.split_whitespace() {
        if brace_depth == 0 && paren_depth == 0 {
            // At root level — this is a field name (possibly with args)
            let field_name = token.split('(').next().unwrap_or(token);
            if !field_name.is_empty()
                && field_name != "{"
                && field_name != "}"
                && !field_name.starts_with('#')
                && !field_name.starts_with("...")
            {
                fields.push(field_name.to_string());
            }
        }

        // Track depth for braces and parens
        for ch in token.chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                '(' => paren_depth += 1,
                ')' => paren_depth -= 1,
                _ => {},
            }
        }
    }

    fields
}
