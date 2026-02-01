# Phase 24: CLI Integration & Schema Compilation

## Objective

Fix schema merger bug and fully integrate TOML-based configuration throughout the compilation pipeline, restoring all features (security, observers, analytics, caching, federation) that were removed from SDKs in Phase 23.

## Background

**Phase 23 Status**: All 13 Tier 2 language SDKs refactored to export minimal `types.json`
- Security/observers/analytics/caching code removed from SDKs
- Features now should be defined in `fraiseql.toml` instead
- **Problem**: CLI merger and configuration integration incomplete

**Phase 24 Goal**: Complete the TOML-based workflow by:
1. **Fix schema merger bug** (types as map instead of array)
2. **Implement full TOML config parsing** (all sections)
3. **Integrate security/observers/analytics** at compile time
4. **Enable environment variable overrides** for production
5. **Create end-to-end integration tests** (16 languages)

## Known Issues to Fix

### 1. Schema Merger Bug
**Current**: `types.json` from language SDKs is array, but merger treats it as object/map
**Impact**: Types not properly merged into compiled schema
**Fix**: Update merger to handle array format from SDKs, convert to proper structure

### 2. TOML Config Incomplete
**Current**: `fraiseql.toml` schema partially defined
**Missing**:
- Complete security section (rules, policies, field auth, enterprise features)
- Observers configuration (handlers, webhooks)
- Analytics fact tables
- Caching rules
- Federation settings
**Fix**: Define complete TOML schema matching all operational features

### 3. Runtime Configuration Loading
**Current**: Server loads compiled schema but doesn't extract security/observers config
**Missing**: Runtime initialization of security policies, observers, rate limiting
**Fix**: Server should initialize from security/observers sections of compiled schema

### 4. Environment Variable Overrides
**Current**: No way to override TOML config at runtime
**Missing**: Support for env vars like `FRAISEQL_SECURITY_RATE_LIMIT_ENABLED`
**Fix**: Load compiled schema, then override with environment variables

## TDD Cycles

### Cycle 1: Fix Schema Merger (RED → GREEN → REFACTOR → CLEANUP)

**RED**: Write integration test that verifies:
- types.json array with 3 types
- fraiseql.toml with security/queries
- Merged schema has correct array structure for types/queries
- Security config properly embedded

**GREEN**: Fix merger.rs to:
1. Parse types.json as array
2. Build intermediate schema with arrays (not objects)
3. Convert types array to structured format expected by IntermediateSchema

**REFACTOR**: Clean up merger logic, add helper functions

**CLEANUP**: Run clippy, add documentation, commit

### Cycle 2: Implement TOML Security Config

**RED**: Write test for complete security section parsing:
- Rules with caching
- Policies (RBAC/ABAC)
- Field-level authorization
- Enterprise features (rate limiting, audit logging, etc.)

**GREEN**: Implement TomlSchema security parsing

**REFACTOR**: Extract security validation, improve error messages

**CLEANUP**: Lint, document, commit

### Cycle 3: Implement Full Config Loading

**RED**: Write test for complete fraiseql.toml loading:
- Schema metadata
- Database settings
- Types/queries/mutations
- Federation
- Security
- Observers
- Caching
- Analytics

**GREEN**: Implement TomlSchema::from_file with all sections

**REFACTOR**: Extract validation logic

**CLEANUP**: Lint, commit

### Cycle 4: Implement Environment Variable Overrides

**RED**: Write test showing env vars override compiled schema:
- `FRAISEQL_SECURITY_RATE_LIMIT_ENABLED` overrides TOML setting
- `FRAISEQL_DATABASE_URL` overrides compiled URL
- Invalid env vars logged but don't crash

**GREEN**: Implement env override logic in compile command

**REFACTOR**: Extract into separate module

**CLEANUP**: Lint, document, commit

### Cycle 5: Create Integration Tests (All 16 Languages)

**RED**: Write integration test that:
- Exports types.json from each language SDK
- Creates fraiseql.toml with security/queries/mutations
- Compiles schema
- Verifies output contains all types/queries/security/observers/analytics
- Validates compiled schema can be loaded by server

**GREEN**: Run against all 16 languages, fix any issues

**REFACTOR**: Extract test helpers, parameterize

**CLEANUP**: Lint, commit

### Cycle 6: Fix Runtime Security Loading

**RED**: Write test showing server loads security config from compiled schema:
- Server initializes rate limiting from config
- Server sets up audit logging
- Server loads authorization policies

**GREEN**: Implement in fraiseql-server

**REFACTOR**: Extract configuration initialization

**CLEANUP**: Lint, document, commit

## Success Criteria

- [ ] Schema merger correctly handles types.json arrays
- [ ] All TOML config sections parse successfully
- [ ] Compiled schema includes security/observers/analytics from TOML
- [ ] Environment variables successfully override compiled settings
- [ ] All 16 language SDKs integrate end-to-end
- [ ] Server initializes all features from compiled schema
- [ ] Integration tests pass for all language combinations
- [ ] Zero clippy warnings in all modified code
- [ ] All features restored (security, observers, analytics, caching, federation)

## Deliverables

### Core Fixes
- Fixed merger.rs (types as array)
- Complete TomlSchema implementation
- Environment variable override system
- Runtime configuration loading

### Integration
- Integration test suite (16 languages)
- End-to-end examples (Python + TOML + CLI + Server)
- Updated documentation

### Quality
- 100% linting pass
- All tests passing
- No TODO markers remaining

## Status

[ ] Not Started | [ ] In Progress | [ ] Complete

**Current**: Starting Phase 24

---

**Last Updated**: February 1, 2026
**Version**: 1.0-starting
