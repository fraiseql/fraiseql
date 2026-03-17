# Deprecations

This document tracks deprecated APIs and their migration paths. Deprecated items follow a two-release deprecation cycle: deprecated in version N, removed in version N+2 (or next major).

## Policy

1. **Deprecation**: item is marked `#[deprecated]` with a `since` version and `note` pointing to the replacement. Existing code continues to work.
2. **Migration window**: at least one minor release between deprecation and removal.
3. **Removal**: item is deleted in the target version. The `CHANGELOG.md` entry notes the removal.

## Currently Deprecated

| Item | Crate | Since | Replacement | Removal Target |
|------|-------|-------|-------------|----------------|
| `PoolTuningConfig` | `fraiseql-server` | v2.0.1 | `PoolPressureMonitorConfig` | v3.0 |
| `DatabaseAdapter` (trait) | `fraiseql-arrow` | v2.2.0 | `ArrowDatabaseAdapter` | v2.4.0 |
| `EventStorage` (trait) | `fraiseql-arrow` | v2.2.0 | `ArrowEventStorage` | v2.4.0 |
| `Sanitizable` (trait) | `fraiseql-auth` | v2.2.0 | `Sanitize` | v2.4.0 |
| `AuditableResult` (trait) | `fraiseql-auth` | v2.2.0 | `AuditExt` | v2.4.0 |
| `MutationCapable` (trait) | `fraiseql-db` | v2.2.0 | `SupportsMutations` | v2.4.0 |

### `PoolTuningConfig` (v2.0.1)

**Location**: `crates/fraiseql-server/src/config/pool_tuning.rs`

**Why deprecated**: Renamed to `PoolPressureMonitorConfig` to better reflect its purpose (monitoring and recommending pool size changes, not resizing at runtime).

**Migration**: Replace `PoolTuningConfig` with `PoolPressureMonitorConfig` in your configuration code. The field names and semantics are identical.

### v2.2.0 Trait Aliases

These traits are zero-content aliases created during the crate extraction refactor. They exist solely for backward compatibility.

**`DatabaseAdapter` → `ArrowDatabaseAdapter`** (`fraiseql-arrow`)
Replace `impl DatabaseAdapter for T` with `impl ArrowDatabaseAdapter for T`.

**`EventStorage` → `ArrowEventStorage`** (`fraiseql-arrow`)
Replace `impl EventStorage for T` with `impl ArrowEventStorage for T`.

**`Sanitizable` → `Sanitize`** (`fraiseql-auth`)
Replace `impl Sanitizable for T` with `impl Sanitize for T`.

**`AuditableResult` → `AuditExt`** (`fraiseql-auth`)
Replace `impl AuditableResult<T, E> for T` with `impl AuditExt<T, E> for T`.

**`MutationCapable` → `SupportsMutations`** (`fraiseql-db`)
Replace `impl MutationCapable for T` with `impl SupportsMutations for T`.

## Previously Removed

_None yet._
