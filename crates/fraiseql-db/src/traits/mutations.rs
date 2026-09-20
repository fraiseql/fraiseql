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
///    SupportsMutations`. Attempting to call it with `FraiseWireAdapter` produces a compiler error
///    (`error[E0277]: FraiseWireAdapter does not implement SupportsMutations`). A `compile_fail`
///    doctest on that impl block pins it, paired with a positive twin so the witness failing to
///    resolve goes red rather than quietly green.
///
/// The `execute()` method (which accepts raw GraphQL strings) still performs a runtime
/// `supports_mutations()` check because it cannot know the operation type at compile time.
/// For direct mutation dispatch, prefer `execute_mutation()` to get compile-time safety.
///
/// # ⚠ Implementing this marker obliges you to override `supports_mutations()` too
///
/// [`DatabaseAdapter::supports_mutations`] must return `true` as well. The two are separate
/// gates answering at different times — this one at compile time, that one at runtime — and
/// **both default to refusing**. An adapter that implements only this one is write-capable to
/// the type system and read-only to the runtime guard. Rust cannot derive one from the other
/// without specialization, so the pairing is stated rather than enforced.
///
/// Stating rather than enforcing is tolerable here only because **both ways of getting it
/// wrong fail safe.** Marker without the override: the runtime guard refuses, so no write
/// happens. Override without the marker: the typed write entries are not callable, and the
/// `execute()` path the adapter can still reach runs the full chokepoint. Neither omission
/// grants anything — which is the property that was missing while the default was `true`.
///
/// # Which adapters implement this?
///
/// | Adapter | Implements |
/// |---------|-----------|
/// | `PostgresAdapter` | ✅ Yes |
/// | `FraiseWireAdapter` | ❌ No — read-only wire protocol |
/// | `CachedDatabaseAdapter<A>` | ✅ When `A: SupportsMutations` |
///
/// `MySqlAdapter`, `SqlServerAdapter` and `SqliteAdapter` were listed here until #374
/// removed the non-PostgreSQL backends.
pub trait SupportsMutations: DatabaseAdapter {}
