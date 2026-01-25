# Phase 11.2: Critical - SQL Injection via JSON Path Fix

**Priority**: 🔴 CRITICAL
**CVSS Score**: 9.2
**Effort**: 4 hours
**Duration**: 1-2 days
**Status**: [ ] Not Started

---

## Objective

Eliminate SQL injection vulnerability in JSON path construction by properly escaping field names and validating against schema.

---

## Success Criteria

- [ ] JSON field names escaped in all SQL generation
- [ ] Schema validation added for field names
- [ ] Unit tests verify escaping works
- [ ] SQL injection tests confirm defense
- [ ] All integration tests pass
- [ ] No performance regression
- [ ] Zero clippy warnings

---

## Vulnerability Details

**Location**: `crates/fraiseql-core/src/db/where_sql_generator.rs:88-102`

**Current Code**:
```rust
fn build_json_path(path: &[String]) -> String {
    if path.len() == 1 {
        format!("data->>'{}'", path[0])  // ❌ NO ESCAPING
    } else {
        let nested = &path[..path.len() - 1];
        let last = &path[path.len() - 1];
        let nested_path = nested.join(",");
        format!("data#>'{{{}}}'->>'{}'", nested_path, last)  // ❌ NO ESCAPING
    }
}
```

**Attack**: Field name like `field'); DROP TABLE users; --` could execute multiple statements

---

## Implementation Plan

### TDD Cycle 1: Basic Field Name Escaping

#### RED: Write test for SQL injection
```rust
#[test]
fn test_sql_injection_in_field_name_fails() {
    let malicious_path = vec!["field'); DROP TABLE users; --".to_string()];
    let sql = build_json_path(&malicious_path);

    // SQL should have escaped quotes, preventing injection
    assert!(!sql.contains("DROP TABLE"));
    assert!(sql.contains("''"));  // Escaped quotes
}

#[test]
fn test_normal_field_name_works() {
    let path = vec!["user_name".to_string()];
    let sql = build_json_path(&path);

    assert!(sql.contains("user_name"));
    assert!(sql.contains("data->>"));
}
```

#### GREEN: Implement basic escaping
```rust
fn escape_field_name(name: &str) -> String {
    // SQL escape: ' becomes ''
    name.replace("'", "''")
}

fn build_json_path(path: &[String]) -> String {
    if path.len() == 1 {
        let escaped = escape_field_name(&path[0]);
        format!("data->>'{}' ", escaped)
    } else {
        let nested = &path[..path.len() - 1];
        let last = escape_field_name(&path[path.len() - 1]);
        let escaped_nested: Vec<String> = nested
            .iter()
            .map(|p| escape_field_name(p))
            .collect();
        let nested_path = escaped_nested.join(",");
        format!("data#>'{{{}}}'->>'{}'", nested_path, last)
    }
}
```

#### REFACTOR: Use PostgreSQL quote_ident
```rust
fn build_json_path(path: &[String]) -> String {
    if path.len() == 1 {
        // Use PostgreSQL's built-in escaping
        format!("data->>quote_ident('{}')", escape_field_name(&path[0]))
    } else {
        // Nested path with escaping
        let escaped_nested: Vec<String> = path[..(path.len() - 1)]
            .iter()
            .map(|p| escape_field_name(p))
            .collect();

        let last = escape_field_name(&path[path.len() - 1]);
        let nested_path = escaped_nested.join(",");

        format!(
            "data#>'{{{}}}'->>quote_ident('{}')",
            nested_path, last
        )
    }
}
```

#### CLEANUP
- [ ] Remove debug code
- [ ] Verify escaping handles edge cases
- [ ] Check for performance impact

---

### TDD Cycle 2: Schema-Based Validation

#### RED: Write test for schema validation
```rust
#[test]
fn test_field_name_validated_against_schema() {
    let schema = Schema::from_json(r#"{
        "types": {
            "User": {
                "fields": {
                    "id": "ID",
                    "name": "String",
                    "email": "String"
                }
            }
        }
    }"#).unwrap();

    // Valid field names should work
    assert!(validate_field_name("name", &schema, "User").is_ok());
    assert!(validate_field_name("email", &schema, "User").is_ok());

    // Invalid field names should fail
    assert!(validate_field_name("nonexistent", &schema, "User").is_err());
    assert!(validate_field_name("'; DROP TABLE --", &schema, "User").is_err());
}
```

#### GREEN: Add schema validation
```rust
pub fn validate_field_name(
    name: &str,
    schema: &Schema,
    type_name: &str,
) -> Result<()> {
    // Check field exists in schema
    let type_def = schema.get_type(type_name)?;
    if !type_def.has_field(name) {
        return Err(Error::ValidationError(
            format!("Field '{}' not found in type '{}'", name, type_name)
        ));
    }

    // Check for suspicious patterns (defense in depth)
    if name.contains("--") || name.contains("/*") || name.contains("*/") {
        return Err(Error::ValidationError(
            format!("Invalid field name: {}", name)
        ));
    }

    Ok(())
}

fn build_json_path_safe(
    path: &[String],
    schema: &Schema,
    type_name: &str,
) -> Result<String> {
    // Validate all path elements
    for (i, element) in path.iter().enumerate() {
        validate_field_name(element, schema, type_name)?;
    }

    // Escape and build SQL (now safe)
    let escaped_path: Vec<String> = path
        .iter()
        .map(|p| escape_field_name(p))
        .collect();

    // Build JSON path with escaping
    Ok(if escaped_path.len() == 1 {
        format!("data->>'{}' ", escaped_path[0])
    } else {
        let last = escaped_path[escaped_path.len() - 1].clone();
        let nested = escaped_path[..escaped_path.len() - 1].join(",");
        format!("data#>'{{{}}}'->>'{}'", nested, last)
    })
}
```

#### REFACTOR: Integrate into query builder
```rust
pub struct JsonPathBuilder {
    schema: Arc<Schema>,
    type_name: String,
}

impl JsonPathBuilder {
    pub fn new(schema: Arc<Schema>, type_name: String) -> Self {
        Self { schema, type_name }
    }

    pub fn build(&self, path: &[String]) -> Result<String> {
        build_json_path_safe(path, &self.schema, &self.type_name)
    }
}
```

#### CLEANUP
- [ ] Remove temporary validation functions
- [ ] Ensure error messages are clear
- [ ] Check performance impact of validation

---

### TDD Cycle 3: Integration Testing

#### RED: Write integration test
```rust
#[tokio::test]
async fn test_sql_injection_through_graphql_fails() {
    let db = setup_test_db().await;
    let schema = db.schema();

    // Try to inject SQL through GraphQL
    let query = json!({
        "query": r#"query { users(where: {
            "field'); DROP TABLE users; --": {eq: "value"}
        }) { id } }"#
    });

    let result = execute_graphql(&db, query, schema).await;

    // Should fail with validation error
    assert!(result.is_err());
    assert!(result.err().unwrap().to_string().contains("Invalid field"));
}

#[tokio::test]
async fn test_normal_query_still_works() {
    let db = setup_test_db().await;
    let schema = db.schema();

    let query = json!({
        "query": r#"query { users(where: {name: {eq: "John"}}) { id name } }"#
    });

    let result = execute_graphql(&db, query, schema).await;
    assert!(result.is_ok());
}
```

#### GREEN: Ensure integration works
```rust
// In where_sql_generator.rs
pub fn generate_where_clause(
    conditions: &WhereConditions,
    schema: &Schema,
    type_name: &str,
) -> Result<String> {
    // Validate field names before building SQL
    for condition in conditions.iter() {
        validate_field_names(condition, schema, type_name)?;
    }

    // Now build SQL safely
    build_where_clause_sql(conditions)
}
```

#### REFACTOR: Create comprehensive validation module
```rust
// New module: validation/field_validator.rs
pub mod field_validator {
    use crate::schema::Schema;
    use crate::error::Result;

    pub fn validate_json_path(
        path: &[String],
        schema: &Schema,
        type_name: &str,
    ) -> Result<()> {
        for element in path {
            validate_field_name(element, schema, type_name)?;
        }
        Ok(())
    }

    fn validate_field_name(
        name: &str,
        schema: &Schema,
        type_name: &str,
    ) -> Result<()> {
        // Implementation
    }
}
```

#### CLEANUP
- [ ] All tests passing
- [ ] No console debug output
- [ ] Clippy warnings addressed

---

## Files to Modify

1. **`crates/fraiseql-core/src/db/where_sql_generator.rs`**
   - Escape field names
   - Add schema validation
   - Create JsonPathBuilder

2. **`crates/fraiseql-core/src/validation/field_validator.rs`** (new)
   - Centralized field validation
   - Reusable validation functions

3. **`crates/fraiseql-core/src/db/mod.rs`**
   - Export new validation module
   - Integration points

---

## Tests to Create

```rust
#[cfg(test)]
mod sql_injection_tests {
    use super::*;

    // Basic escaping tests
    #[test]
    fn test_single_quote_escaped() { }

    #[test]
    fn test_double_dash_in_field_name() { }

    #[test]
    fn test_comment_syntax_in_field_name() { }

    // Schema validation tests
    #[test]
    fn test_valid_field_passes_validation() { }

    #[test]
    fn test_nonexistent_field_fails_validation() { }

    #[test]
    fn test_malicious_field_name_fails() { }

    // Integration tests
    #[tokio::test]
    async fn test_sql_injection_attack_fails() { }

    #[tokio::test]
    async fn test_normal_queries_unaffected() { }

    #[tokio::test]
    async fn test_nested_json_paths_safe() { }

    #[tokio::test]
    async fn test_special_characters_handled() { }
}
```

---

## Example Queries

### Before (Vulnerable)
```rust
// Input: field'; DROP TABLE users; --
// Generated SQL: data->'field'; DROP TABLE users; --'
// Result: SQL injection possible
```

### After (Secure)
```rust
// Input: field'; DROP TABLE users; --
// Validation: Field not in schema → REJECTED
// If somehow bypassed:
// Escaping: data->''field''; DROP TABLE users; --''
// Result: No injection (escaped as literal string)
```

---

## Performance Impact

**Expected**: Negligible
- Schema validation: <1ms per field (cached)
- Field name escaping: O(n) where n = field name length (typically <50 chars)
- Escape characters: 1-5 extra characters per field name

---

## Rollback Plan

```bash
# Revert to previous version
git revert <commit-hash>

# Verify schema validation is removed
grep -r "validate_field_name" src/

# Servers will accept any field names again (but still escaped)
```

---

## Commit Message Template

```
fix(security-11.2): Prevent SQL injection in JSON path construction

## Changes
- Escape single quotes in JSON field names
- Add schema-based field name validation
- Reject suspicious field names (SQL syntax)
- Create FieldValidator module for reusable validation

## Vulnerability Addressed
CVSS 9.2 - SQL injection via JSON path

## Verification
✅ All escaping tests pass
✅ SQL injection attempts fail
✅ Normal queries unaffected
✅ Integration tests pass
✅ Clippy clean
```

---

## Dependencies Added

```toml
# No new external dependencies
# Uses existing schema and error types
```

---

## Documentation Updates

### SECURITY.md
```markdown
## SQL Injection Prevention

All database field names are:
1. Validated against the schema
2. Escaped using SQL quote escaping
3. Checked for SQL syntax patterns

This prevents injection attacks through field names.
```

---

## Phase Status

**Ready**: ✅ Implementation plan complete
**Next**: BEGIN TDD CYCLE 1 - Write SQL injection test

---

**Review**: [Pending approval]
**Reviewed By**: [Awaiting]
**Approved**: [Awaiting]
