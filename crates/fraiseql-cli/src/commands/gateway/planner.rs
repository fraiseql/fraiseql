//! Query planner — decomposes a GraphQL operation into subgraph fetches.
//!
//! The planner maps each root field to the subgraph that owns it and produces
//! an ordered list of `SubgraphFetch` steps. For fields that span subgraphs
//! (entity references), the planner emits follow-up `_entities` fetches.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

/// Maximum depth of nested entity resolution to prevent unbounded recursion.
const MAX_ENTITY_DEPTH: usize = 8;

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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    fn make_ownership() -> FieldOwnership {
        let mut fo = FieldOwnership::default();
        fo.insert("users".to_string(), "users-svc".to_string());
        fo.insert("user".to_string(), "users-svc".to_string());
        fo.insert("products".to_string(), "products-svc".to_string());
        fo.insert("orders".to_string(), "orders-svc".to_string());
        fo
    }

    #[test]
    fn test_plan_single_subgraph() {
        let ownership = make_ownership();
        let fields = vec!["users".to_string()];
        let plan = plan_query(&fields, &ownership).unwrap();
        assert_eq!(plan.fetches.len(), 1);
        assert_eq!(plan.fetches[0].subgraph, "users-svc");
        assert!(!plan.fetches[0].is_entity_fetch);
    }

    #[test]
    fn test_plan_groups_same_subgraph() {
        let ownership = make_ownership();
        let fields = vec!["users".to_string(), "user".to_string()];
        let plan = plan_query(&fields, &ownership).unwrap();
        assert_eq!(plan.fetches.len(), 1);
        assert_eq!(plan.fetches[0].subgraph, "users-svc");
    }

    #[test]
    fn test_plan_multiple_subgraphs() {
        let ownership = make_ownership();
        let fields = vec!["users".to_string(), "products".to_string()];
        let plan = plan_query(&fields, &ownership).unwrap();
        assert_eq!(plan.fetches.len(), 2);
        let subgraphs: Vec<&str> = plan.fetches.iter().map(|f| f.subgraph.as_str()).collect();
        assert!(subgraphs.contains(&"users-svc"));
        assert!(subgraphs.contains(&"products-svc"));
    }

    #[test]
    fn test_plan_unknown_field() {
        let ownership = make_ownership();
        let fields = vec!["nonexistent".to_string()];
        let result = plan_query(&fields, &ownership);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlanError::UnknownField { .. }));
    }

    #[test]
    fn test_plan_empty_query() {
        let ownership = make_ownership();
        let result = plan_query(&[], &ownership);
        assert!(matches!(result.unwrap_err(), PlanError::EmptyQuery));
    }

    #[test]
    fn test_entity_fetch_depth_exceeded() {
        let result = plan_entity_fetch("svc", &[], "id name", MAX_ENTITY_DEPTH);
        assert!(matches!(result.unwrap_err(), PlanError::DepthExceeded { .. }));
    }

    #[test]
    fn test_entity_fetch_ok() {
        let reps = vec![serde_json::json!({"__typename": "User", "id": "1"})];
        let fetch = plan_entity_fetch("users-svc", &reps, "name email", 0).unwrap();
        assert!(fetch.is_entity_fetch);
        assert_eq!(fetch.subgraph, "users-svc");
        assert!(fetch.query.contains("_entities"));
    }

    #[test]
    fn test_extract_root_fields_simple() {
        let fields = extract_root_fields("{ users products }");
        assert_eq!(fields, vec!["users", "products"]);
    }

    #[test]
    fn test_extract_root_fields_nested() {
        let fields = extract_root_fields("{ users { id name } products }");
        assert_eq!(fields, vec!["users", "products"]);
    }

    #[test]
    fn test_extract_root_fields_with_args() {
        let fields = extract_root_fields("{ user(id: 1) { name } products }");
        assert_eq!(fields, vec!["user", "products"]);
    }

    #[test]
    fn test_extract_root_fields_named_query() {
        let fields = extract_root_fields("query GetStuff { users orders }");
        assert_eq!(fields, vec!["users", "orders"]);
    }

    #[test]
    fn test_extract_root_fields_empty() {
        let fields = extract_root_fields("no braces here");
        assert!(fields.is_empty());
    }
}
