# Phase 6: TOML Configuration Parser

## Objective
Parse and validate TOML configuration files for database connections, features, runtime settings, and caching policies.
(Note: TOML is for configuration only, not schema definition. Schemas are provided separately in any language.)

## Success Criteria
- [ ] Database connection strings parsed from TOML
- [ ] Feature flags properly loaded from configuration
- [ ] Runtime settings (caching, timeouts, etc.) validated
- [ ] Multiple database configurations supported
- [ ] TOML parsing errors reported clearly
- [ ] `cargo clippy -p fraiseql-cli` clean
- [ ] `cargo test -p fraiseql-cli` passes

## Configuration Sections (Example)

```toml
[database.primary]
url = "postgresql://user:pass@localhost/fraiseql"
pool_size = 20

[database.replica]
url = "postgresql://user:pass@replica/fraiseql"
pool_size = 10

[features]
enable_arrow = true
enable_caching = true
enable_subscriptions = true

[cache]
backend = "redis"
redis_url = "redis://localhost:6379"
ttl_seconds = 3600

[runtime]
query_timeout_ms = 30000
max_batch_size = 1000
```

## TDD Cycles

### Cycle 1: Parse Database Configuration

**File**: `crates/fraiseql-cli/src/config/toml_config.rs` (or similar)

- **RED**: Write test expecting database config from TOML
- **GREEN**: Implement TOML parsing for database section
- **REFACTOR**: Support multiple database configurations
- **CLEANUP**: Test validation and error handling, commit

### Cycle 2: Parse Feature Flags and Runtime Settings

**File**: `crates/fraiseql-cli/src/config/toml_config.rs`

- **RED**: Write test for feature and runtime config
- **GREEN**: Parse features and runtime settings sections
- **REFACTOR**: Add defaults for missing values
- **CLEANUP**: Test all configuration combinations, commit

### Cycle 3: Configuration Validation and Testing

**File**: `crates/fraiseql-cli/tests/config_integration.rs`

- **RED**: Write comprehensive test matrix
- **GREEN**: Verify configuration is properly loaded and validated
- **REFACTOR**: Add edge cases (missing sections, invalid values)
- **CLEANUP**: All tests pass, commit

## Dependencies
- None (independent of all other phases)

## Status
[ ] Not Started
