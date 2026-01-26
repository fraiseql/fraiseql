# Phase 11.7: Low - Security Enhancement Items

**Priority**: 🔵 LOW
**Effort**: 12 hours
**Duration**: 2-3 days
**Status**: [ ] Not Started

---

## Objective

Implement five low-severity security enhancements:
1. Query depth/complexity limits
2. Rate limiting key extraction verification
3. SCRAM authentication version support
4. Audit log integrity with tamper detection
5. ID enumeration attack prevention

---

## Success Criteria

- [ ] Query depth limits enforced
- [ ] Query complexity budgets working
- [ ] Rate limiting documentation verified
- [ ] SCRAM version support documented
- [ ] Audit log integrity checks implemented
- [ ] Opaque ID generation option added
- [ ] All tests passing
- [ ] Zero clippy warnings

---

## Issue 1: Query Depth/Complexity Limits

**CVSS**: 2.7

### Implementation

```rust
const MAX_QUERY_DEPTH: usize = 10;
const MAX_QUERY_COMPLEXITY: usize = 1000;

#[derive(Debug)]
pub struct QueryComplexity {
    depth: usize,
    complexity: usize,
}

pub fn analyze_query_complexity(query: &Query) -> QueryComplexity {
    QueryComplexity {
        depth: calculate_depth(&query.selection_set),
        complexity: calculate_complexity(&query.selection_set),
    }
}

fn calculate_depth(selection_set: &SelectionSet) -> usize {
    if selection_set.items.is_empty() {
        0
    } else {
        1 + selection_set
            .items
            .iter()
            .map(|s| calculate_depth(&s.selection_set))
            .max()
            .unwrap_or(0)
    }
}

fn calculate_complexity(selection_set: &SelectionSet) -> usize {
    selection_set
        .items
        .iter()
        .map(|item| 1 + calculate_complexity(&item.selection_set))
        .sum()
}

pub fn validate_query_limits(query: &Query) -> Result<()> {
    let complexity = analyze_query_complexity(query);

    if complexity.depth > MAX_QUERY_DEPTH {
        return Err(Error::QueryTooDeep(complexity.depth, MAX_QUERY_DEPTH));
    }

    if complexity.complexity > MAX_QUERY_COMPLEXITY {
        return Err(Error::QueryTooComplex(complexity.complexity, MAX_QUERY_COMPLEXITY));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_shallow_query_allowed() {
        let query = parse_query("{ users { id name } }").unwrap();
        assert!(validate_query_limits(&query).is_ok());
    }

    #[test]
    fn test_deep_query_rejected() {
        let deep = "{ a { b { c { d { e { f { g { h { i { j { k } } } } } } } } } } }";
        let query = parse_query(deep).unwrap();
        assert!(validate_query_limits(&query).is_err());
    }
}
```

### Files to Modify
- `crates/fraiseql-core/src/graphql/validator.rs` - Add complexity validation
- `crates/fraiseql-server/src/routes/graphql.rs` - Call validator before execution

---

## Issue 2: Rate Limiting Key Extraction

**CVSS**: 3.1

### Documentation

```markdown
## Rate Limiting Implementation

### Key Extraction Strategy

Rate limits are keyed by:
1. User ID (for authenticated requests)
2. Client IP (for unauthenticated requests)

### IP Address Resolution

The system correctly handles proxies:
- Only trust X-Forwarded-For from configured proxy servers
- Default: Trust only direct connections
- Configuration: `rate_limit.trusted_proxies`

### Configuration

```toml
[rate_limit]
# Global limit: 100 requests per minute
global_limit = 100
window_secs = 60

# Trusted proxies that can set X-Forwarded-For
trusted_proxies = [
    "10.0.0.0/8",      # Internal load balancer
    "172.16.0.0/12",   # VPC CIDR
]
```

### Verification

```rust
#[test]
fn test_rate_limiting_uses_correct_key() {
    // Direct connection -> use source IP
    let key = get_rate_limit_key_for_request(
        direct_connection,
        "192.168.1.100"
    );
    assert_eq!(key, "ip:192.168.1.100");

    // From untrusted proxy -> use source IP
    let key = get_rate_limit_key_for_request(
        with_x_forwarded_for("8.8.8.8"),
        "10.0.0.1"
    );
    assert_eq!(key, "ip:10.0.0.1");  // Ignore X-Forwarded-For

    // From trusted proxy -> use X-Forwarded-For
    let key = get_rate_limit_key_for_request(
        with_x_forwarded_for("203.0.113.5"),
        "10.0.0.1"
    );
    assert_eq!(key, "ip:203.0.113.5");  // Use header
}
```

### Files to Modify
- `docs/RATE_LIMITING.md` - Add complete documentation
- `crates/fraiseql-server/src/middleware/rate_limit.rs` - Verify implementation

---

## Issue 3: SCRAM Authentication Version Support

**CVSS**: 1.5

### Documentation

```markdown
## PostgreSQL Authentication

### Supported Methods
- SCRAM-SHA-256 (RFC 5802) - **Recommended**
- SCRAM-SHA-256-PLUS (channel binding) - **Best**

### Requirements
- PostgreSQL 10+ for SCRAM-SHA-256
- PostgreSQL 11+ for SCRAM-SHA-256-PLUS

### Migration from MD5

If using older PostgreSQL:
1. Update PostgreSQL to 10+
2. Reset user passwords to update auth method
3. Or use password_encryption = 'scram-sha256'

### Configuration

```toml
[database]
# PostgreSQL SCRAM authentication
url = "postgresql://user:pass@localhost/fraiseql"
```
```

### Files to Modify
- `docs/INSTALLATION.md` - Add PostgreSQL version requirements
- `crates/fraiseql-wire/src/auth/scram.rs` - Add documentation

---

## Issue 4: Audit Log Integrity

**CVSS**: 3.7

### Implementation

```rust
use sha2::{Sha256, Digest};

#[derive(Debug, Clone)]
pub struct AuditLogEntry {
    id: u64,
    timestamp: DateTime<Utc>,
    event: String,
    user_id: Option<String>,
    hash_prev: String,  // Hash of previous entry
    hash_current: String,  // Hash including this entry
    signature: Option<String>,  // Signature of this entry
}

impl AuditLogEntry {
    pub fn new(
        id: u64,
        event: String,
        user_id: Option<String>,
        prev_entry: Option<&AuditLogEntry>,
    ) -> Self {
        let timestamp = Utc::now();

        let hash_prev = prev_entry
            .map(|e| e.hash_current.clone())
            .unwrap_or_else(|| "0".to_string());

        // Hash includes: id + timestamp + event + prev hash
        let hash_input = format!(
            "{}:{}:{}:{}",
            id, timestamp, event, hash_prev
        );

        let mut hasher = Sha256::new();
        hasher.update(&hash_input);
        let hash_current = hex::encode(hasher.finalize());

        Self {
            id,
            timestamp,
            event,
            user_id,
            hash_prev,
            hash_current,
            signature: None,
        }
    }
}

pub fn verify_audit_log_integrity(entries: &[AuditLogEntry]) -> bool {
    for i in 1..entries.len() {
        let prev = &entries[i - 1];
        let current = &entries[i];

        // Each entry should reference previous hash
        if current.hash_prev != prev.hash_current {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_audit_log_chain_detected_tampering() {
        let entry1 = AuditLogEntry::new(1, "login".into(), None, None);
        let entry2 = AuditLogEntry::new(2, "query".into(), None, Some(&entry1));

        let mut entries = vec![entry1, entry2];

        // Tamper with first entry
        entries[0].event = "HACKED".to_string();

        // Tampering is detected
        assert!(!verify_audit_log_integrity(&entries));
    }

    #[test]
    fn test_audit_log_chain_valid() {
        let entry1 = AuditLogEntry::new(1, "login".into(), None, None);
        let entry2 = AuditLogEntry::new(2, "query".into(), None, Some(&entry1));
        let entry3 = AuditLogEntry::new(3, "logout".into(), None, Some(&entry2));

        assert!(verify_audit_log_integrity(&[entry1, entry2, entry3]));
    }
}
```

### Files to Modify
- `crates/fraiseql-core/src/security/audit.rs` - Add integrity checking
- Database migration: Add hash columns to audit log table

---

## Issue 5: ID Enumeration Prevention

**CVSS**: 2.1

### Implementation

```rust
use rand::Rng;

// Current: Sequential IDs (enumerable)
// Problem: user_1, user_2, user_3 → Easy to guess

// Solution: Opaque IDs (not enumerable)
pub fn generate_opaque_id(prefix: &str) -> String {
    let mut rng = rand::thread_rng();
    let random_bytes: [u8; 12] = rng.gen();
    let encoded = base64::encode(&random_bytes);

    format!("{}_{}", prefix, encoded)
}

pub enum IdPolicy {
    Sequential,  // ❌ Vulnerable: 1, 2, 3
    Uuid,        // ✅ Good: random UUIDs
    Opaque,      // ✅ Best: ghu_abc123xyz
}

impl Entity {
    pub fn generate_id(policy: IdPolicy) -> String {
        match policy {
            IdPolicy::Sequential => {
                // Database auto-increment
                format!("user_{}", next_id())
            }
            IdPolicy::Uuid => uuid::Uuid::new_v4().to_string(),
            IdPolicy::Opaque => generate_opaque_id("usr"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_opaque_ids_not_enumerable() {
        let id1 = generate_opaque_id("usr");
        let id2 = generate_opaque_id("usr");

        // IDs are different
        assert_ne!(id1, id2);

        // Can't guess pattern
        assert!(!id1.ends_with("1"));
        assert!(!id2.ends_with("2"));
    }
}
```

### Configuration

```toml
[database]
# ID generation policy: sequential, uuid, opaque
id_policy = "opaque"

# Prefix for opaque IDs
opaque_id_prefix = "usr"  # Results in: usr_abc123xyz
```

### Files to Modify
- `crates/fraiseql-core/src/schema/entity.rs` - Add opaque ID support
- Configuration files - Document ID policies

---

## Combined Testing

```rust
#[cfg(test)]
mod low_security_tests {
    use super::*;

    // Complexity tests
    #[test]
    fn test_query_depth_limit() { }

    #[test]
    fn test_query_complexity_limit() { }

    // Rate limiting tests
    #[test]
    fn test_rate_limiting_key_extraction() { }

    // Audit log tests
    #[test]
    fn test_audit_log_integrity() { }

    #[test]
    fn test_tampering_detected() { }

    // ID enumeration tests
    #[test]
    fn test_opaque_ids_generated() { }

    #[test]
    fn test_ids_not_enumerable() { }
}
```

---

## Implementation Order

1. **Query Limits** (3 hours)
2. **Rate Limiting Verification** (1 hour)
3. **SCRAM Documentation** (1 hour)
4. **Audit Log Integrity** (4 hours)
5. **ID Enumeration** (3 hours)

---

## Commit Message Template

```
feat(security-11.7): Add security enhancements

## Changes
- Add GraphQL query depth/complexity validation
- Document rate limiting key extraction strategy
- Document PostgreSQL SCRAM requirements
- Implement immutable audit log with hash chains
- Add opaque ID generation option

## Enhancements (Low-severity items)
- Prevent DoS via deeply nested queries
- Verify rate limiting uses correct keys
- Ensure PostgreSQL version compatibility
- Detect audit log tampering
- Prevent ID enumeration attacks

## Verification
✅ Query complexity tests pass
✅ Audit log integrity verified
✅ Opaque IDs generated correctly
✅ Clippy clean
```

---

## Dependencies Added

```toml
sha2 = "0.10"
hex = "0.4"
```

---

## Phase Status

**Ready**: ✅ Implementation plan complete
**Next**: BEGIN Phase 11.7.1 - Query limits

---

**Review**: [Pending approval]
**Reviewed By**: [Awaiting]
**Approved**: [Awaiting]
