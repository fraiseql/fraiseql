//! Arrow Flight integration for fraiseql-core.
//!
//! This module provides interfaces for Arrow Flight support.
//! The actual implementation is provided by the fraiseql-arrow crate, which
//! depends on fraiseql-core to avoid circular dependencies.
//!
//! # Architecture
//!
//! - **fraiseql-core**: Provides arrow_executor module (this file) with interfaces
//! - **fraiseql-arrow**: Depends on fraiseql-core[arrow] and implements Arrow Flight server
//!
//! This separation allows:
//! - fraiseql-core to remain independent of fraiseql-arrow
//! - fraiseql-arrow to access `Executor<A>` and query execution
//! - No circular dependencies in the dependency graph
