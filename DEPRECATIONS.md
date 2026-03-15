# Deprecations

This document tracks deprecated APIs and their migration paths. Deprecated items follow a two-release deprecation cycle: deprecated in version N, removed in version N+2 (or next major).

## Policy

1. **Deprecation**: item is marked `#[deprecated]` with a `since` version and `note` pointing to the replacement. Existing code continues to work.
2. **Migration window**: at least one minor release between deprecation and removal.
3. **Removal**: item is deleted in the target version. The `CHANGELOG.md` entry notes the removal.

## Currently Deprecated

| Item | Since | Replacement | Removal Target |
|------|-------|-------------|----------------|
| `PoolTuningConfig` | v2.0.1 | `PoolPressureMonitorConfig` | v3.0 |

### `PoolTuningConfig` (v2.0.1)

**Location**: `crates/fraiseql-server/src/config/pool_tuning.rs`

**Why deprecated**: Renamed to `PoolPressureMonitorConfig` to better reflect its purpose (monitoring and recommending pool size changes, not resizing at runtime).

**Migration**: Replace `PoolTuningConfig` with `PoolPressureMonitorConfig` in your configuration code. The field names and semantics are identical.

## Previously Removed

_None yet._
