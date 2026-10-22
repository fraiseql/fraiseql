//! # FraiseQL v2: a compiled GraphQL execution engine over PostgreSQL
//!
//! Schemas are authored in Python or `TypeScript` and exported as `schema.json`.
//! `fraiseql-cli compile` validates them and binds each query to the PostgreSQL view
//! it reads, producing `schema.compiled.json`. The server loads that file and, per
//! request, builds the SQL for the requested selection and runs it as a prepared
//! statement.
//!
//! This crate re-exports the workspace crates behind features.
//!
//! ## Quick start
//!
//! ```rust
//! use fraiseql::CompiledSchema;
//!
//! let json = r#"{"types": [], "queries": [], "mutations": [], "subscriptions": []}"#;
//! let schema = CompiledSchema::from_json(json, false)?;
//! assert!(schema.queries.is_empty());
//! # Ok::<(), fraiseql::FraiseQLError>(())
//! ```
//!
//! ## Architecture
//!
//! ```text
//! Authoring               Compilation              Runtime
//! (Python/TS)            (Rust)                   (Rust)
//!     ↓                      ↓                        ↓
//! schema.json    +    fraiseql.toml      →    schema.compiled.json    →    GraphQL Server
//! (types)                 (config)           (types + config + bindings)   (builds SQL per request)
//! ```
//!
//! ## Features
//!
//! PostgreSQL support is always compiled in; there are no default features.
//!
//! - `server`: the HTTP server (`fraiseql-server`); `rest`, `mcp`, `metrics`, `sources`, `inbound`,
//!   `inbound-email`, `cdc-outbound` and the `functions-*` features enable its optional transports
//!   and subsystems.
//! - `observers`, `observers-cache`: post-mutation observers.
//! - `arrow`: Apache Arrow Flight.
//! - `wire`: the `fraiseql-wire` streaming client.
//! - `federation`: Apollo Federation subgraph support.
//! - `cli`: the compiler CLI.
//! - `full`: `server`, `observers`, `observers-cache`, `arrow`, `wire`, `cli`, `federation`;
//!   `release-full` is the feature set of the published `-full` binary.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// Re-export core types
#[cfg(feature = "arrow")]
pub use fraiseql_arrow as arrow;
#[cfg(feature = "cli")]
pub use fraiseql_cli as cli;
// Re-export core modules for namespaced access
#[cfg(feature = "federation")]
pub use fraiseql_core::federation;
pub use fraiseql_core::{
    CompiledSchema, FraiseQLError, Result, TenantContext, apq, cache, compiler, db, graphql,
    runtime, schema, security, tenancy, validation,
};
// Re-export error types. The canonical workspace error is `FraiseQLError`
// (re-exported via `fraiseql_core` above). `ConfigError` and `FileError` are
// the only domain-specific error vocabularies owned directly by
// `fraiseql-error`; subsystem error types (auth, webhook, observer) live in
// their own crates and compose into `FraiseQLError::{Auth, Webhook, Observer}`
// via `From` impls.
pub use fraiseql_error::{ConfigError, FileError};
#[cfg(feature = "observers")]
pub use fraiseql_observers as observers;
// Conditional re-exports (feature-gated)
#[cfg(feature = "server")]
pub use fraiseql_server as server;
#[cfg(feature = "wire")]
pub use fraiseql_wire as wire;

// Prelude module for convenient imports
pub mod prelude {
    //! Convenience re-exports for writing a complete FraiseQL application.
    //!
    //! Import with: `use fraiseql::prelude::*;`
    //!
    //! Provides:
    //! - Core schema types: [`CompiledSchema`]
    //! - Error handling: [`FraiseQLError`], [`Result`]
    //! - Database abstraction: [`DatabaseAdapter`], [`db`] module
    //! - GraphQL execution: [`Executor`]
    //! - GraphQL parsing: [`ParsedQuery`], [`parse_query`]
    //! - Tenancy: [`TenantContext`], [`Tenant`]
    //! - Observer system (feature `observers`): `EntityEvent`, `EventKind`, `ObserverExecutor`
    //! - HTTP server (feature `server`): `Server`, `ServerConfig`

    // Error handling
    // Database access
    // GraphQL parsing
    // Runtime executor
    // Tenancy support
    pub use fraiseql_core::{
        db,
        graphql::{ParsedQuery, parse_query},
        prelude::DatabaseAdapter,
        runtime::Executor,
        tenancy::TenantContext as Tenant,
    };

    // Optional: Observer system
    #[cfg(feature = "observers")]
    pub use crate::observers::{EntityEvent, EventKind, ObserverExecutor};
    // Optional: Server components
    #[cfg(feature = "server")]
    pub use crate::server::{Server, ServerConfig};
    // Core types
    pub use crate::{CompiledSchema, TenantContext};
    pub use crate::{FraiseQLError, Result};
}

/// FraiseQL version string
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// The documentation's Rust examples are compiled as doctests: a ```rust block in one
// of these files claims to compile, and `cargo test --doc` holds it to that. A block
// that only illustrates is fenced ```rust,ignore or ```text. `tools/check-docs-rust-
// examples.py` fails when a file with a ```rust block is missing from this list.
#[cfg(doctest)]
mod docs_examples {
    #[doc = include_str!("../../../docs/architecture/functions.md")]
    struct ArchitectureFunctions;
    #[doc = include_str!("../../../docs/architecture/mutation-response.md")]
    struct ArchitectureMutationResponse;
    #[doc = include_str!("../../../docs/architecture/overview.md")]
    struct ArchitectureOverview;
    #[doc = include_str!("../../../docs/architecture/wire-protocol.md")]
    struct ArchitectureWireProtocol;
    #[doc = include_str!("../../../docs/auth/identity-store.md")]
    struct AuthIdentityStore;
    #[doc = include_str!("../../../docs/auth/local-password.md")]
    struct AuthLocalPassword;
    #[doc = include_str!("../../../docs/auth/saml-sso.md")]
    struct AuthSamlSso;
    #[doc = include_str!("../../../docs/features/mutation-timing.md")]
    struct FeaturesMutationTiming;
    #[doc = include_str!("../../../docs/fuzzing.md")]
    struct Fuzzing;
    #[doc = include_str!("../../../docs/guides/field-authorization.md")]
    struct GuidesFieldAuthorization;
    #[doc = include_str!("../../../docs/guides/operation-authorization.md")]
    struct GuidesOperationAuthorization;
    #[doc = include_str!("../../../docs/guides/typed-clients.md")]
    struct GuidesTypedClients;
    #[doc = include_str!("../../../docs/linting.md")]
    struct Linting;
    #[doc = include_str!("../../../docs/modules/fact-table.md")]
    struct ModulesFactTable;
    #[doc = include_str!("../../../docs/modules/window-functions.md")]
    struct ModulesWindowFunctions;
    #[doc = include_str!("../../../docs/value-proposition.md")]
    struct ValueProposition;
}
