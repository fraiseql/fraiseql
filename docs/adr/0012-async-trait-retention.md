# ADR-0012: Retain `#[async_trait]` Until RTN + Send Stabilizes

## Status: Accepted

## Date: 2026-03-10

## Context

FraiseQL uses `#[async_trait]` across ~128 sites to support `dyn Trait` dispatch
on async traits. Native async fn in traits (Rust 1.75+) is not object-safe: you
cannot write `Box<dyn T>` or `Arc<dyn T>` when `T` has `async fn` methods unless
the future type is erased.

Two alternatives were evaluated:

**Option A — Manual `BoxFuture<'_, ..., + Send>`**: Convert each `async fn` in the
trait to `fn foo(&self, ...) -> BoxFuture<'_, Result<...>>` with an explicit `+ Send`
bound, implemented with `Box::pin(async move { ... })` in each impl body. This is
what `#[async_trait]` generates, written by hand. High mechanical effort (~50+ trait
methods across 28 traits), no ergonomic gain.

**Option B — `dynosaur` crate**: A proc-macro that auto-generates a vtable-compatible
wrapper from the trait definition. Attempted and **blocked**: `dynosaur` erases future
types via vtable but does not propagate `+ Send` bounds on the generated futures.
Tokio's multi-threaded runtime requires all spawned futures to be `Send`. Every trait
behind `Arc<dyn T>` in async handlers fails to compile — the generated `DynFoo<_>`
type does not satisfy `Future: Send`.

**Option C — Wait for native dyn-async-trait + Send in std**: Tracked upstream as
RFC 3425 (Return Type Notation). Not yet stable as of Rust 1.88 (current MSRV).

## Decision

Retain `#[async_trait]` across all 28 affected traits until native dyn-async-trait
with `Send` bound support stabilizes in std (RFC 3425 / RTN).

A CI gate (`make lint-async-trait`) tracks the baseline of 128 `#[async_trait]`
usages and will fail if new usages are added without justification. Each existing
site is annotated with:

```rust
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
```

## Consequences

- **Positive**: No migration effort, `#[async_trait]` is correct and well-maintained.
- **Positive**: The CI gate prevents accidental growth of the annotation count.
- **Negative**: Heap allocation per async call on `dyn Trait` dispatch paths (same
  as manual `BoxFuture` — not a regression vs. alternatives).
- **Negative**: Dependency on `async-trait` crate remains until RFC 3425 lands.
- **Future**: When RTN + Send stabilizes, migrate trait-by-trait using the inventory
  in the migration plan (28 traits across 9 crates, ordered by dependency graph).
  Eliminate `async-trait` from workspace dependencies at the end.

## Monitoring

```bash
# Track usage count (baseline: 128 as of v2.1.0)
make lint-async-trait

# Find all sites for migration planning
grep -rn "#\[async_trait\]" crates/ --include="*.rs" | grep -v "tonic::async_trait"
```

## References

- [RFC 3425 — Return Type Notation](https://github.com/rust-lang/rfcs/pull/3425)
- [async-trait crate](https://crates.io/crates/async-trait)
- CI gate: `Makefile` target `lint-async-trait`
