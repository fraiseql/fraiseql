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

### Observer pool size inheritance (v2.2.0)

**Behaviour change**: Prior to v2.2.0, the observer pool inherited `pool_min_size` /
`pool_max_size` from the top-level `ServerConfig`. As of v2.2.0, the observer pool
uses its own defaults (`min=2, max=5`) unless explicitly configured via `[observers.pool]`.

**Migration**: If you relied on the observer pool inheriting the application pool size,
add an explicit `[observers.pool]` section to your `fraiseql.toml`:

```toml
[observers.pool]
min_connections = 5   # was: pool_min_size
max_connections = 20  # was: pool_max_size
acquire_timeout_secs = 30
```

This change is intentional: the observer pool serves LISTEN/NOTIFY and metadata
queries — it rarely needs more than 2–5 connections.

### `PoolTuningConfig` (v2.0.1)

**Location**: `crates/fraiseql-server/src/config/pool_tuning.rs`

**Why deprecated**: Renamed to `PoolPressureMonitorConfig` to better reflect its purpose (monitoring and recommending pool size changes, not resizing at runtime).

**Migration**: Replace `PoolTuningConfig` with `PoolPressureMonitorConfig` in your configuration code. The field names and semantics are identical.

## Previously Removed

| Item | Crate | Deprecated In | Replacement | Removed In |
|------|-------|---------------|-------------|------------|
| `DatabaseAdapter` (trait) | `fraiseql-arrow` | — | `ArrowDatabaseAdapter` | v2.1.0 |
| `EventStorage` (trait) | `fraiseql-arrow` | — | `ArrowEventStorage` | v2.1.0 |
| `Sanitizable` (trait) | `fraiseql-auth` | — | `Sanitize` | v2.1.0 |
| `AuditableResult` (trait) | `fraiseql-auth` | — | `AuditExt` | v2.1.0 |
| `MutationCapable` (trait) | `fraiseql-db` | — | `SupportsMutations` | v2.1.0 |

These were zero-content supertrait aliases created during the crate extraction refactor. Removed in v2.1.0 (first public release; no external consumers existed).
