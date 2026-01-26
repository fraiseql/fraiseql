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
//!   ┌─────────────┐
//!   │  Fragment   │  ← Spread expansion
//!   │  Resolver   │
//!   └──────┬──────┘
//!          │ Resolved selections
//!          ▼
//!   ┌─────────────┐
//!   │  Directive  │  ← @skip/@include
//!   │  Evaluator  │
//!   └──────┬──────┘
//!          │ Final field list
//!          ▼
//!     SQL Generation
//! ```
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_core::graphql::{parse_query, FragmentResolver, DirectiveEvaluator};
//!
//! let query = r#"
//!     fragment UserFields on User { id name }
//!     query { users { ...UserFields } }
//! "#;
//!
//! let parsed = parse_query(query)?;
//! let resolver = FragmentResolver::new(&parsed.fragments);
//! let resolved = resolver.resolve_spreads(&parsed.selections)?;
//! ```

// ============================================================================
// Module declarations
// ============================================================================

/// GraphQL AST types for query representation.
pub mod types;

/// GraphQL query parsing wrapper.
pub mod parser;

/// Fragment resolution and expansion.
pub mod fragment_resolver;

/// Directive evaluation (@skip, @include).
pub mod directive_evaluator;

/// Fragment cycle detection.
pub mod fragments;

/// Query complexity analysis and DoS prevention.
pub mod complexity;

// ============================================================================
// Re-exports for convenient access
// ============================================================================

pub use complexity::{ComplexityAnalyzer, ComplexityConfig};
pub use directive_evaluator::{
    CustomDirectiveEvaluator, DirectiveError, DirectiveEvaluator, DirectiveHandler,
    DirectiveResult, EvaluationContext, OperationType,
};
pub use fragment_resolver::{FragmentError, FragmentResolver};
pub use fragments::FragmentGraph;
pub use parser::parse_query;
pub use types::{
    Directive, FieldSelection, FragmentDefinition, GraphQLArgument, GraphQLType, ParsedQuery,
    VariableDefinition,
};
