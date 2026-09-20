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
/// 3. **Compile-time enforcement**: a write-capable `Executor` can only be *constructed* from an
///    adapter carrying this marker. `Executor::new` and `Executor::with_config` are bounded on it;
///    `Executor::read_only` is not. Attempting `Executor::new` with `FraiseWireAdapter` produces a
///    compiler error (`error[E0277]: FraiseWireAdapter does not implement SupportsMutations`). A
///    `compile_fail` doctest on the constructor pins it, paired with a positive twin so the witness
///    failing to resolve goes red rather than quietly green.
///
///    The enforcement point is the constructor rather than the write entries because the
///    executor no longer names its adapter type — there is no `A` on `execute_mutation()` to
///    bound. What the entries consult instead is the write handle the bounded constructor
///    resolved, which is a strictly wider net: it covers every entry on every transport, and
///    it is the only thing that can also speak for `supports_mutations()`.
///
/// Every write path — `execute()` with a raw GraphQL string, and the typed entries alike —
/// resolves the same write handle before dispatching, so the choice of entry point no longer
/// changes which gate applies.
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
/// happens, on every entry, because the write handle is resolved from both gates at
/// construction and there is no dispatch without it. Override without the marker: a
/// write-capable executor cannot be constructed over the adapter at all. Neither omission
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
