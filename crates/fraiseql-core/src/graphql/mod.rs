//! GraphQL parsing and query processing.
//!
//! This module provides runtime GraphQL query handling:
//! - Query parsing via `graphql-parser` crate
//! - Fragment resolution and expansion
//! - Directive evaluation (@skip, @include)
//! - Fragment cycle detection
//!
//! # Architecture
//!
//! ```text
//! Incoming GraphQL Query
//!         │
//!         ▼
//!   ┌─────────────┐
//!   │   Parser    │  ← graphql-parser crate
//!   └──────┬──────┘
//!          │ ParsedQuery
//!          ▼
//!   ┌─────────────┐
//!   │  Fragment   │  ← Cycle detection
//!   │  Validator  │
//!   └──────┬──────┘
//!          │
//!          ▼
//!   ┌───────────────────────────────┐
//!   │        selection_set          │  ← the one routine every entry point
//!   │                               │    uses: `/graphql`, the multi-root
//!   │  Fragment Resolver            │    fan-out, `node(id:)`, mutations
//!   │    ← spread expansion, with   │
//!   │      the spread's directives  │
//!   │      carried onto the fields  │
//!   │      it contributes (#826)    │
//!   │             │                 │
//!   │             ▼                 │
//!   │  Directive Evaluator          │
//!   │    ← @skip/@include           │
//!   └──────────────┬────────────────┘
//!          │ Final field list
//!          ▼
//!     SQL Generation
//! ```
//!
//! Expansion runs first, but it is **not** allowed to discard what it expands:
//! a spread's own `@skip`/`@include` travels onto every field the spread
//! contributes, so the evaluator downstream still sees it. Expansion depends
//! only on the document and is therefore cacheable; directive evaluation needs
//! the request's variables and is not.
//!
//! # Example
//!
//! ```no_run
//! // Requires: fraiseql_core graphql module (internal types).
//! use fraiseql_core::graphql::{parse_query, FragmentResolver, DirectiveEvaluator};
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let query = r#"
//!     fragment UserFields on User { id name }
//!     query { users { ...UserFields } }
//! "#;
//!
//! let parsed = parse_query(query)?;
//! let resolver = FragmentResolver::new(&parsed.fragments);
//! let resolved = resolver.resolve_spreads(&parsed.selections)?;
//! # Ok(())
//! # }
//! ```

// ============================================================================
// Module declarations
// ============================================================================

/// GraphQL AST types for query representation.
pub mod types;

/// GraphQL query parsing wrapper.
pub mod parser;

/// The `value_json` encoding shared by every consumer of an inline argument.
pub mod value_json;

/// Fragment resolution and expansion.
pub mod fragment_resolver;

/// Directive evaluation (@skip, @include).
pub mod directive_evaluator;

/// Fragment cycle detection.
pub mod fragments;

/// The shared selection-resolution routine used by every entry point.
pub mod selection_set;

/// Query complexity analysis and `DoS` prevention.
pub mod complexity;

/// Field-level RBAC directive (@require_permission).
pub mod require_permission_directive;

// ============================================================================
// Re-exports for convenient access
// ============================================================================

pub use complexity::{
    ComplexityConfig, ComplexityValidationError, DEFAULT_MAX_ALIASES, MAX_VARIABLES_COUNT,
    QueryMetrics, RequestValidator, estimate_query_cost, parse_graphql_document,
};
pub use directive_evaluator::{
    CustomDirectiveEvaluator, DirectiveError, DirectiveEvaluator, DirectiveHandler,
    DirectiveResult, EvaluationContext, OperationType,
};
pub use fragment_resolver::{FragmentError, FragmentResolver};
pub use fragments::FragmentGraph;
pub use parser::parse_query;
pub use require_permission_directive::RequirePermissionDirective;
pub use selection_set::SelectionError;
pub use types::{
    Directive, FieldSelection, FragmentDefinition, GraphQLArgument, GraphQLType, ParsedQuery,
    VariableDefinition,
};

// ============================================================================
// Test modules
// ============================================================================

#[cfg(test)]
mod tests;
