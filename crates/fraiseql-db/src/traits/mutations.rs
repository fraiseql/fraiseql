//! Mutation support marker trait.
//!
//! [`SupportsMutations`] is a compile-time marker that gates mutation dispatch.

use super::DatabaseAdapter;

/// Marker trait for database adapters that support stored-procedure mutations.
///
/// # Role: documentation, generic bound, and compile-time enforcement
///
/// This trait serves three purposes:
/// 1. **Documentation**: it makes write-capable adapters self-describing at the type level.
/// 2. **Generic bounds**: code that only accepts write-capable adapters can constrain on `A:
///    SupportsMutations` (e.g., `CachedDatabaseAdapter<A: SupportsMutations>`).
/// 3. **Compile-time enforcement**: `Executor<A>::execute_mutation()` is only available when `A:
///    SupportsMutations`. Attempting to call it with `SqliteAdapter` produces a compiler error
///    (`error[E0277]: SqliteAdapter does not implement SupportsMutations`).
///
/// The `execute()` method (which accepts raw GraphQL strings) still performs a runtime
/// `supports_mutations()` check because it cannot know the operation type at compile time.
/// For direct mutation dispatch, prefer `execute_mutation()` to get compile-time safety.
///
/// # Which adapters implement this?
///
/// | Adapter | Implements |
/// |---------|-----------|
/// | `PostgresAdapter` | ✅ Yes |
/// | `MySqlAdapter` | ✅ Yes |
/// | `SqlServerAdapter` | ✅ Yes |
/// | `SqliteAdapter` | ❌ No — SQLite does not support stored-function mutations |
/// | `FraiseWireAdapter` | ❌ No — read-only wire protocol |
/// | `CachedDatabaseAdapter<A>` | ✅ When `A: SupportsMutations` |
pub trait SupportsMutations: DatabaseAdapter {}
