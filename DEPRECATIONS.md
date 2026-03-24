# Deprecations

## Policy

- Deprecated items are marked with `#[deprecated(since = "X.Y.Z", note = "...")]`
- Minimum 2 minor versions before removal
- Migration guide provided for each deprecation
- `cargo-semver-checks` enforces no accidental removals in CI

## Active Deprecations

### `PoolTuningConfig` type alias

- **Deprecated since**: 2.0.1
- **Removal target**: 2.3.0
- **Migration**: Use `PoolPressureMonitorConfig` (same type, renamed for clarity — pool monitoring is recommendation-only, not auto-tuning)
- **File**: `crates/fraiseql-server/src/config/pool_tuning.rs`

## Removed (Historical)

(None yet — first public release)
