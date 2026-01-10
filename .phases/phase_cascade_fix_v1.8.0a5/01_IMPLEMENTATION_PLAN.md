# Implementation Plan: CASCADE Fix v1.8.0-alpha.5

**Phase:** RED → GREEN → VERIFY → COMMIT
**Total Time:** 4-6 hours
**Complexity:** Low (Single module + tests)

---

## Phase Breakdown

### RED Phase: Write Failing Tests (30 min)

**Objective:** Create tests that demonstrate the bug and will pass once fixed.

#### Step 1.1: Create Test File Structure

```bash
# Navigate to fraiseql repo
cd /home/lionel/code/fraiseql

# Create feature branch
git checkout -b fix/cascade-nesting-v1.8.0a5

# Ensure you're on the right commit
git status
```

#### Step 1.2: Add Failing Rust Unit Test

**File:** `fraiseql_rs/src/mutation/tests.rs`

Add at the end of the file:

```rust
#[cfg(test)]
mod cascade_fix_tests {
    use super::*;

    #[test]
    fn test_parse_8field_mutation_response() {
        // This test will FAIL initially because parser doesn't exist yet
        let json = r#"{
            "status": "created",
            "message": "Allocation created successfully",
            "entity_id": "4d16b78b-7d9b-495f-9094-a65b57b33916",
            "entity_type": "Allocation",
            "entity": {"id": "4d16b78b-7d9b-495f-9094-a65b57b33916", "identifier": "test"},
            "updated_fields": ["location_id", "machine_id"],
            "cascade": {
                "updated": [{"id": "some-id", "operation": "UPDATED"}],
                "deleted": [],
                "invalidations": [{"queryName": "allocations", "strategy": "INVALIDATE"}]
            },
            "metadata": {"extra": "data"}
        }"#;

        // Try to parse as 8-field format
        // This will fail until we implement postgres_composite module
        use crate::mutation::postgres_composite::PostgresMutationResponse;
        let result = PostgresMutationResponse::from_json(json).unwrap();

        assert_eq!(result.status, "created");
        assert_eq!(result.entity_type, Some("Allocation".to_string()));
        assert!(result.cascade.is_some());

        let cascade = result.cascade.as_ref().unwrap();
        assert!(cascade.get("updated").is_some());
    }

    #[test]
    fn test_cascade_extraction_from_position_7() {
        let json = r#"{
            "status": "created",
            "message": "Success",
            "entity_id": "uuid",
            "entity_type": "Allocation",
            "entity": {},
            "updated_fields": [],
            "cascade": {"updated": [{"id": "1"}]},
            "metadata": {}
        }"#;

        use crate::mutation::postgres_composite::PostgresMutationResponse;
        let pg_response = PostgresMutationResponse::from_json(json).unwrap();
        let result = pg_response.to_mutation_result(None);

        // CASCADE should come from Position 7, not metadata
        assert!(result.cascade.is_some());
        assert_eq!(
            result.cascade.unwrap().get("updated").unwrap()[0]["id"],
            "1"
        );
    }
}
```

#### Step 1.3: Run Tests (Should FAIL)

```bash
cd fraiseql_rs
cargo test cascade_fix_tests

# Expected output: Compilation error (postgres_composite module doesn't exist)
```

**Acceptance Criteria:**
- [ ] Tests created
- [ ] Tests fail with expected error (module not found)
- [ ] Test logic is sound

---

### GREEN Phase: Implement Parser (2-3 hours)

**Objective:** Create the postgres_composite module and make tests pass.

#### Step 2.1: Create Parser Module

**File:** `fraiseql_rs/src/mutation/postgres_composite.rs` (NEW)

```rust
//! PostgreSQL composite type parser for app.mutation_response (8-field format)
//!
//! Parses the PrintOptim backend's mutation_response composite type which has:
//! - Position 1: status (TEXT)
//! - Position 2: message (TEXT)
//! - Position 3: entity_id (TEXT)
//! - Position 4: entity_type (TEXT)
//! - Position 5: entity (JSONB)
//! - Position 6: updated_fields (TEXT[])
//! - Position 7: cascade (JSONB) ← KEY FIELD FOR FIX
//! - Position 8: metadata (JSONB)

use serde_json::Value;
use super::{MutationResult, MutationStatus};

/// PostgreSQL app.mutation_response composite type structure (8 fields)
///
/// This matches PrintOptim's mutation_response type exactly.
/// The CASCADE field at Position 7 is the key to fixing the nesting bug.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]  // Fail if structure doesn't match
pub struct PostgresMutationResponse {
    /// Position 1: Status code (created, updated, failed:*, noop:*)
    pub status: String,

    /// Position 2: Human-readable message
    pub message: String,

    /// Position 3: Entity UUID (as TEXT, not UUID type)
    #[serde(default)]
    pub entity_id: Option<String>,

    /// Position 4: Entity type name (e.g., "Allocation", "Machine")
    /// Enables proper __typename mapping without extraction
    #[serde(default)]
    pub entity_type: Option<String>,

    /// Position 5: Entity data (the actual object)
    pub entity: Value,

    /// Position 6: Changed field names
    #[serde(default)]
    pub updated_fields: Option<Vec<String>>,

    /// Position 7: CASCADE data (updated, deleted, invalidations)
    /// ✅ THIS IS THE KEY FIELD - explicit CASCADE at correct position!
    #[serde(default)]
    pub cascade: Option<Value>,

    /// Position 8: Extra metadata (errors, context, etc.)
    #[serde(default)]
    pub metadata: Option<Value>,
}

impl PostgresMutationResponse {
    /// Parse from JSON string (PostgreSQL composite type serialization)
    ///
    /// # Arguments
    /// * `json_str` - JSON representation of the composite type from PostgreSQL
    ///
    /// # Returns
    /// * `Ok(PostgresMutationResponse)` - Successfully parsed
    /// * `Err(String)` - Parse error with descriptive message
    ///
    /// # Example
    /// ```rust
    /// let json = r#"{"status": "created", "message": "OK", ...}"#;
    /// let response = PostgresMutationResponse::from_json(json)?;
    /// ```
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        serde_json::from_str(json_str).map_err(|e| {
            format!(
                "Failed to parse PostgreSQL mutation_response composite type (8 fields): {}. \
                 Expected fields: status, message, entity_id, entity_type, entity, \
                 updated_fields, cascade, metadata",
                e
            )
        })
    }

    /// Convert to internal MutationResult format
    ///
    /// Maps the 8-field composite type to FraiseQL's internal representation.
    /// The CASCADE field from Position 7 will be placed at the GraphQL success
    /// wrapper level (not nested in the entity).
    ///
    /// # Arguments
    /// * `_entity_type_fallback` - Unused (kept for API compatibility)
    ///   In 8-field format, entity_type always comes from Position 4
    ///
    /// # Returns
    /// Internal `MutationResult` ready for GraphQL response building
    pub fn to_mutation_result(self, _entity_type_fallback: Option<&str>) -> MutationResult {
        // CASCADE is already at Position 7 - just filter out nulls ✅
        let cascade = self.cascade.filter(|c| !c.is_null());

        // entity_type comes from Position 4 (always available in 8-field format) ✅
        let entity_type = self.entity_type;

        MutationResult {
            status: MutationStatus::from_str(&self.status),
            message: self.message,
            entity_id: self.entity_id,
            entity_type,  // From Position 4
            entity: Some(self.entity),  // From Position 5
            updated_fields: self.updated_fields,
            cascade,  // From Position 7 - THIS FIXES THE BUG! ✅
            metadata: self.metadata,
            is_simple_format: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_parsing() {
        let json = r#"{
            "status": "created",
            "message": "Test",
            "entity_id": "123",
            "entity_type": "User",
            "entity": {"id": "123"},
            "updated_fields": ["name"],
            "cascade": {"updated": []},
            "metadata": {}
        }"#;

        let result = PostgresMutationResponse::from_json(json);
        assert!(result.is_ok());

        let pg_response = result.unwrap();
        assert_eq!(pg_response.status, "created");
        assert_eq!(pg_response.entity_type, Some("User".to_string()));
    }

    #[test]
    fn test_null_cascade_filtered() {
        let json = r#"{
            "status": "created",
            "message": "Test",
            "entity_id": null,
            "entity_type": null,
            "entity": {},
            "updated_fields": null,
            "cascade": null,
            "metadata": null
        }"#;

        let pg_response = PostgresMutationResponse::from_json(json).unwrap();
        let result = pg_response.to_mutation_result(None);

        // Null cascade should be filtered out
        assert!(result.cascade.is_none());
    }

    #[test]
    fn test_missing_optional_fields() {
        // Only required fields: status, message, entity
        let json = r#"{
            "status": "success",
            "message": "OK",
            "entity": {}
        }"#;

        let result = PostgresMutationResponse::from_json(json);
        assert!(result.is_ok());
    }
}
```

**Implementation Notes:**
- Use `#[serde(deny_unknown_fields)]` to catch structure mismatches early
- Use `#[serde(default)]` for optional fields
- Filter null CASCADE values (database returns NULL, not omitted field)
- Clear error messages for debugging

#### Step 2.2: Add Module to mod.rs

**File:** `fraiseql_rs/src/mutation/mod.rs`

Add near the top (after existing mod declarations):

```rust
mod postgres_composite;  // NEW: 8-field composite type parser
```

Update the `use` statements in the module:

```rust
pub use postgres_composite::PostgresMutationResponse;  // NEW
```

#### Step 2.3: Update Entry Point

**File:** `fraiseql_rs/src/mutation/mod.rs`

Find the `build_mutation_response` function and update it:

```rust
pub fn build_mutation_response(
    mutation_json: &str,
    field_name: &str,
    success_type: &str,
    error_type: &str,
    entity_field_name: Option<&str>,
    entity_type: Option<&str>,
    _cascade_selections: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<Vec<String>>,
) -> Result<Vec<u8>, String> {
    // Step 1: Try parsing as PostgreSQL 8-field mutation_response FIRST
    let result = match postgres_composite::PostgresMutationResponse::from_json(mutation_json) {
        Ok(pg_response) => {
            // SUCCESS: Valid 8-field composite type from PrintOptim
            // CASCADE from Position 7 will be placed at success wrapper level
            pg_response.to_mutation_result(entity_type)
        }
        Err(_parse_error) => {
            // FALLBACK: Try simple format (backward compatibility)
            // This handles non-PrintOptim users or simple entity responses
            MutationResult::from_json(mutation_json, entity_type)?
        }
    };

    // Step 2: Build GraphQL response
    // CASCADE will be at success wrapper level (e.g., CreateAllocationSuccess.cascade)
    let graphql_response = response_builder::build_graphql_response(
        &result,
        field_name,
        success_type,
        error_type,
        entity_field_name,
        entity_type,
        auto_camel_case,
        success_type_fields.as_ref(),
    )?;

    // Step 3: Serialize to bytes
    serde_json::to_vec(&graphql_response)
        .map_err(|e| format!("Failed to serialize GraphQL response: {}", e))
}
```

#### Step 2.4: Run Tests (Should PASS)

```bash
cd fraiseql_rs
cargo test

# All tests should pass, including the new cascade_fix_tests
```

**Acceptance Criteria:**
- [ ] Module compiles without errors
- [ ] All existing tests still pass (backward compatibility)
- [ ] New cascade_fix_tests pass
- [ ] No warnings

---

### REFACTOR Phase: Clean Up & Document (30 min)

**Objective:** Ensure code quality and documentation.

#### Step 3.1: Add Documentation

- [ ] Verify all public functions have rustdoc comments
- [ ] Add module-level documentation
- [ ] Add inline comments for complex logic

#### Step 3.2: Run Linter

```bash
cargo clippy --all-targets
cargo fmt
```

#### Step 3.3: Check for Warnings

```bash
cargo build --all-targets
# Should have zero warnings
```

**Acceptance Criteria:**
- [ ] No clippy warnings
- [ ] Code formatted with rustfmt
- [ ] All public items documented
- [ ] No compiler warnings

---

### QA Phase: Integration Testing (1-2 hours)

**Objective:** Test with real PrintOptim mutations.

#### Step 4.1: Build FraiseQL Locally

```bash
cd /home/lionel/code/fraiseql

# Build Rust extension
cd fraiseql_rs
cargo build --release

# Build Python package
cd ..
uv build

# Install locally
uv pip install -e .

# Verify version
python -c "import fraiseql; print(fraiseql.__version__)"
```

#### Step 4.2: Run PrintOptim Tests

```bash
cd /home/lionel/code/printoptim_backend_manual_migration

# Update fraiseql to local version (already done via -e install)

# Run CASCADE diagnostic test
uv run pytest tests/api/mutations/scd/allocation/test_debug_cascade_v2.py::test_cascade_diagnostic_full_chain -v

# Expected: CASCADE at success level, NOT in entity
```

#### Step 4.3: Verify CASCADE Location

The test should show:

```json
{
  "createAllocation": {
    "__typename": "CreateAllocationSuccess",
    "allocation": {
      "__typename": "Allocation",
      "id": "...",
      "identifier": "..."
      // ✅ NO cascade field here!
    },
    "cascade": {
      // ✅ CASCADE HERE - CORRECT LOCATION!
      "updated": [...],
      "invalidations": [...]
    }
  }
}
```

#### Step 4.4: Run Full Test Suite

```bash
cd /home/lionel/code/printoptim_backend_manual_migration

# Run all mutation tests
uv run pytest tests/api/mutations/ -v --tb=short

# Check for regressions
```

**Acceptance Criteria:**
- [ ] CASCADE appears at success wrapper level
- [ ] CASCADE does NOT appear in entity
- [ ] All PrintOptim tests pass
- [ ] No performance regression

---

### COMMIT Phase: Version & Release (30 min)

**Objective:** Prepare for release.

#### Step 5.1: Update Version

**File:** `fraiseql_rs/Cargo.toml`

```toml
[package]
name = "fraiseql-rs"
version = "1.8.0-alpha.5"  # Bump from 1.8.0-alpha.4
```

**File:** `pyproject.toml`

```toml
[project]
name = "fraiseql"
version = "1.8.0a5"  # Bump from 1.8.0a4
```

#### Step 5.2: Update CHANGELOG

**File:** `CHANGELOG.md`

Add at the top:

```markdown
## [1.8.0-alpha.5] - 2025-12-06

### Fixed
- **CASCADE nesting bug**: CASCADE data now appears at success wrapper level instead of nested inside entity objects
  - Added support for PrintOptim's 8-field `mutation_response` composite type
  - CASCADE field extracted from Position 7 (explicit field)
  - Maintains backward compatibility with simple format responses

### Added
- New `postgres_composite` module for parsing 8-field composite types
- Comprehensive tests for composite type parsing and CASCADE extraction

### Technical Details
- Files changed: `fraiseql_rs/src/mutation/postgres_composite.rs` (new), `mod.rs` (updated)
- Breaking changes: None
- Migration required: None (automatic)
```

#### Step 5.3: Commit Changes

```bash
cd /home/lionel/code/fraiseql

# Stage all changes
git add .

# Commit with descriptive message
git commit -m "fix(mutations): CASCADE at success wrapper level (v1.8.0-alpha.5)

- Add postgres_composite module to parse 8-field mutation_response type
- Extract CASCADE from Position 7 (explicit field in composite type)
- Fix CASCADE nesting bug: now appears at success wrapper, not in entity
- Maintain backward compatibility with simple format
- Zero breaking changes

Fixes PrintOptim CASCADE bug where CASCADE appeared in allocation.cascade
instead of CreateAllocationSuccess.cascade.

Testing:
- All Rust unit tests pass
- All Python integration tests pass
- PrintOptim mutation tests verified

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

#### Step 5.4: Create PR (if needed)

```bash
# Push branch
git push origin fix/cascade-nesting-v1.8.0a5

# Create PR (or merge to main if solo dev)
```

#### Step 5.5: Build and Publish

```bash
# Build distribution
uv build

# Publish to PyPI (test first)
uv publish --repository testpypi

# Verify on test PyPI
pip install --index-url https://test.pypi.org/simple/ fraiseql==1.8.0a5

# If good, publish to real PyPI
uv publish
```

**Acceptance Criteria:**
- [ ] Version bumped to 1.8.0-alpha.5
- [ ] CHANGELOG updated
- [ ] Git commit created
- [ ] PR created (if applicable)
- [ ] Package published to PyPI

---

## Summary Checklist

### Implementation Complete ✅

- [ ] RED: Tests created (failing initially)
- [ ] GREEN: Parser module created (tests pass)
- [ ] REFACTOR: Code cleaned and documented
- [ ] QA: Integration tests pass
- [ ] COMMIT: Version bumped and published

### Deliverables ✅

- [ ] `postgres_composite.rs` module (~80 lines)
- [ ] Updated `mod.rs` entry point (~5 lines)
- [ ] Comprehensive tests (~100 lines)
- [ ] Updated CHANGELOG
- [ ] FraiseQL v1.8.0-alpha.5 published

### Verification ✅

- [ ] CASCADE at success wrapper level
- [ ] CASCADE NOT in entity
- [ ] All tests pass
- [ ] No breaking changes
- [ ] PrintOptim can upgrade

---

## Time Tracking

| Phase | Estimated | Actual | Notes |
|-------|-----------|--------|-------|
| RED (Tests) | 30 min | | |
| GREEN (Implementation) | 2-3 hours | | |
| REFACTOR (Cleanup) | 30 min | | |
| QA (Integration) | 1-2 hours | | |
| COMMIT (Release) | 30 min | | |
| **Total** | **4-6 hours** | | |

---

## Next Steps After Release

1. Update PrintOptim to fraiseql>=1.8.0a5
2. Deploy to dev environment
3. Monitor for issues
4. Plan v1.8.1 (performance optimization if needed)

🎉 **Phase Complete!**
