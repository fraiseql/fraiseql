# Phase 11.6: Medium - Data Protection Enhancements

**Priority**: 🟡 MEDIUM
**Effort**: 9 hours
**Duration**: 2-3 days
**Status**: [ ] Not Started

---

## Objective

Address four medium-severity data protection issues:
1. Error message information leakage
2. Field masking incomplete coverage
3. JSON variable ordering in APQ cache
4. Bearer token timing attack

---

## Success Criteria

- [ ] Error messages redacted in REGULATED profile
- [ ] Field masking extended to 30+ patterns
- [ ] JSON variable ordering deterministic
- [ ] Bearer token comparison constant-time
- [ ] All tests passing
- [ ] Zero clippy warnings

---

## Issue 1: Error Message Information Leakage

**CVSS**: 4.3

### Implementation

#### TDD Cycle
```rust
// RED: Test error redaction
#[test]
fn test_errors_redacted_in_regulated_profile() {
    let error = Error::DatabaseError("Column 'user_id' does not exist".into());
    let redacted = error.to_string_in_profile(&SecurityProfile::Regulated);

    assert!(!redacted.contains("Column"));
    assert!(!redacted.contains("user_id"));
    assert!(redacted.contains("Database error"));
}

// GREEN: Implement redaction
pub fn to_string_in_profile(&self, profile: &SecurityProfile) -> String {
    match profile {
        SecurityProfile::Standard => self.to_string(),
        SecurityProfile::Regulated => match self {
            Error::DatabaseError(_) => "Database error occurred".to_string(),
            Error::ValidationError(_) => "Invalid request".to_string(),
            Error::AuthenticationError(_) => "Authentication failed".to_string(),
            _ => "An error occurred".to_string(),
        }
    }
}
```

### Files to Modify
- `crates/fraiseql-core/src/error.rs` - Add profile-aware error formatting
- `crates/fraiseql-server/src/routes/graphql.rs` - Use profile in responses

---

## Issue 2: Field Masking Incomplete Coverage

**CVSS**: 5.2

### Implementation

#### Current Patterns
```rust
"password", "secret", "token", "ssn", "creditcard", "pin"
```

#### Extended Patterns
```rust
const SENSITIVE_FIELD_PATTERNS: &[&str] = &[
    // Authentication
    "password", "secret", "token", "pin",

    // PII
    "ssn", "social_security", "phone", "telephone",
    "address", "zip", "postal", "dob", "birthdate",
    "email", "email_address",

    // Financial
    "creditcard", "credit_card", "account_number",
    "routing", "balance", "salary", "payment",
    "bank_account",

    // Healthcare
    "medical", "health", "diagnosis", "prescription",

    // Employment
    "hire_date", "termination_date",

    // Other PII
    "bio", "biography", "note", "comment",
];

pub fn is_sensitive_field(name: &str) -> bool {
    let lower = name.to_lowercase();
    SENSITIVE_FIELD_PATTERNS.iter().any(|p| lower.contains(p))
}
```

### Files to Modify
- `crates/fraiseql-core/src/security/field_masking.rs` - Extend patterns

---

## Issue 3: JSON Variable Ordering in APQ Cache

**CVSS**: 5.5

### Implementation

```rust
// RED: Test deterministic hashing
#[test]
fn test_apq_hash_deterministic() {
    let vars1 = json!({"z": 1, "a": 2});
    let vars2 = json!({"a": 2, "z": 1});

    assert_eq!(
        hash_query_with_variables("query", &vars1),
        hash_query_with_variables("query", &vars2)
    );
}

// GREEN: Sort JSON keys before hashing
fn sort_json_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();

            for key in keys {
                sorted.insert(key.clone(), sort_json_keys(&map[&key]));
            }

            Value::Object(sorted)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(sort_json_keys).collect())
        }
        other => other.clone(),
    }
}

pub fn hash_query_with_variables(query: &str, variables: &Value) -> String {
    let sorted_vars = sort_json_keys(variables);
    let variables_json = serde_json::to_string(&sorted_vars).unwrap_or_default();
    let combined = format!("{}:{}", sha256_hash(query), variables_json);
    sha256_hash(&combined)
}
```

### Files to Modify
- `crates/fraiseql-core/src/apq/hasher.rs` - Sort JSON before hashing

---

## Issue 4: Bearer Token Timing Attack

**CVSS**: 4.7

### Implementation

```rust
// RED: Test constant-time comparison
#[test]
fn test_token_comparison_constant_time() {
    let valid = "ghu_abcdef123456";

    let short = "xyz";
    let long = "ghu_abcdef123456_with_extra";
    let wrong = "ghu_zzzzzzzzzzzzz";

    let start = Instant::now();
    _ = constant_time_compare(valid, &short);
    let short_time = start.elapsed();

    let start = Instant::now();
    _ = constant_time_compare(valid, &wrong);
    let wrong_time = start.elapsed();

    // Times should be similar (constant-time)
    let ratio = wrong_time.as_nanos() as f64 / short_time.as_nanos() as f64;
    assert!(ratio > 0.8 && ratio < 1.2, "Timing difference: {}", ratio);
}

// GREEN: Use subtle crate
fn constant_time_compare(a: &str, b: &str) -> bool {
    use subtle::ConstantTimeComparison;
    a.ct_eq(b).into()
}
```

### Files to Modify
- `Cargo.toml` - Add `subtle` crate
- `crates/fraiseql-server/src/middleware/auth.rs` - Use subtle for token comparison

---

## Combined Tests

```rust
#[cfg(test)]
mod medium_security_tests {
    use super::*;

    // Error redaction tests
    #[test]
    fn test_error_redaction_database() { }

    #[test]
    fn test_error_redaction_validation() { }

    // Field masking tests
    #[test]
    fn test_extended_sensitive_patterns() { }

    #[test]
    fn test_custom_field_names_masked() { }

    // JSON ordering tests
    #[test]
    fn test_json_ordering_deterministic() { }

    // Timing attack tests
    #[test]
    fn test_constant_time_comparison() { }
}
```

---

## Implementation Order

1. **Error Redaction** (2 hours)
   - Update error types
   - Add profile parameter to response handlers
   - Test error messages

2. **Field Masking** (1 hour)
   - Extend pattern list
   - Add configuration support
   - Test coverage

3. **JSON Ordering** (2 hours)
   - Implement JSON sorting
   - Add determinism tests
   - Verify performance

4. **Timing Attack** (1 hour)
   - Add subtle dependency
   - Replace comparison function
   - Verify timing

5. **Integration Testing** (3 hours)
   - End-to-end error handling
   - Field masking with GraphQL
   - Cache behavior verification

---

## Dependencies Added

```toml
subtle = "2.4"
```

---

## Configuration

```toml
[security]
# Error profile: standard or regulated
profile = "regulated"

# Field masking patterns
sensitive_field_patterns = [
    "password", "secret", "token", "pin",
    # ... extend as needed
]

# Cache TTL for APQ
apq_cache_ttl_secs = 3600
```

---

## Commit Message Template

```
fix(security-11.6): Address medium-severity data protection issues

## Changes
- Implement error message redaction in REGULATED profile
- Extend field masking patterns to 30+ sensitive field types
- Fix JSON variable ordering for deterministic cache keys
- Use constant-time comparison for bearer tokens

## Vulnerabilities Addressed
- CVSS 4.3 - Error message information leakage
- CVSS 5.2 - Field masking incomplete coverage
- CVSS 5.5 - JSON variable ordering cache evasion
- CVSS 4.7 - Bearer token timing attack

## Verification
✅ All error redaction tests pass
✅ Field masking tests pass
✅ JSON ordering deterministic
✅ Token comparison timing constant
✅ Clippy clean
```

---

## Phase Status

**Ready**: ✅ Implementation plan complete
**Next**: BEGIN Phase 11.6.1 - Error redaction

---

**Review**: [Pending approval]
**Reviewed By**: [Awaiting]
**Approved**: [Awaiting]
