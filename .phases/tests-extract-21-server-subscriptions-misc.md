---
title: Test Extraction — fraiseql-server subscriptions/, observers/, pool/, resilience/
status: planned
---

# Phase 21: `fraiseql-server` — `subscriptions/`, `observers/`, `pool/`, `resilience/`

## Objective

Extract inline tests from four subsystems of `fraiseql-server`.

## Files

### subscriptions/ (6 files)

| File |
|------|
| `subscriptions/broadcast.rs` |
| `subscriptions/event_bridge.rs` |
| `subscriptions/lifecycle.rs` |
| `subscriptions/presence.rs` |
| `subscriptions/protocol.rs` |
| `subscriptions/webhook_lifecycle.rs` |

→ `subscriptions/tests.rs`

### observers/ (4 files)

| File |
|------|
| `observers/config.rs` |
| `observers/repository.rs` |
| `observers/routes.rs` |
| `observers/runtime.rs` |

→ `observers/tests.rs`

### pool/ (1 file)

| File |
|------|
| `pool/auto_tuner.rs` |

→ `pool/tests.rs`

### resilience/ (1 file)

| File |
|------|
| `resilience/backpressure.rs` |

→ `resilience/tests.rs`

### federation/ (2 files)

| File |
|------|
| `federation/circuit_breaker.rs` |
| `federation/health_checker.rs` |

→ `federation/tests.rs`

### mcp/ (2 files)

| File |
|------|
| `mcp/executor.rs` |
| `mcp/tools.rs` |

→ `mcp/tests.rs`

## Steps

For each group above:
1. Create the `tests.rs` with `use super::*` or explicit imports.
2. Add `#[cfg(test)] mod tests;` declaration in the module root.
3. Remove inline blocks from source files.

## Commit

```
refactor(server): extract subscriptions/, observers/, pool/, resilience/ inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-server --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-server --lib
```
