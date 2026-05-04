//! Verifies that common FraiseQL types are accessible from the prelude alone.
//!
//! If this test fails to compile, a commonly-needed type was removed from the
//! prelude. This is a compile-time contract test — the test body is intentionally
//! minimal.

/// Verifies that types needed for a complete FraiseQL application are in the prelude.
#[test]
fn prelude_is_usable() {
    use fraiseql::prelude::*;

    // Error handling types are reachable from the prelude.
    let _: fn() -> Result<()> = || Ok(());
    let _: Option<FraiseQLError> = None;

    // DatabaseAdapter trait is reachable (used as a type bound).
    fn _accepts_adapter<A: DatabaseAdapter>(_a: A) {}

    // Core types are reachable.
    let _: Option<CompiledSchema> = None;
    let _: Option<TenantContext> = None;
}
