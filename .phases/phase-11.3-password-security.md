# Phase 11.3: High - Password Memory Security

**Priority**: 🟠 HIGH
**CVSS Score**: 8.1
**Effort**: 3 hours
**Duration**: 1 day
**Status**: [ ] Not Started

---

## Objective

Prevent plaintext password exposure in memory by automatically zeroing password memory on drop and removing passwords from error messages.

---

## Success Criteria

- [ ] zeroize crate added and integrated
- [ ] All password fields use Zeroizing<String>
- [ ] Passwords automatically zeroed on drop
- [ ] Error messages don't contain passwords
- [ ] Tests verify memory zeroing
- [ ] No performance regression
- [ ] Zero clippy warnings

---

## Vulnerability Details

**Location**: `crates/fraiseql-wire/src/client/connection_string.rs:158-164`

**Risk**: Rust String doesn't zero memory on drop. With memory access (RCE, VM escape), attacker can recover plaintext passwords from heap.

---

## Implementation Plan

### TDD Cycle 1: Add zeroize Dependency

#### RED: Write test that fails without zeroize
```rust
#[test]
fn test_password_not_stored_as_plain_string() {
    // This should fail with plain String
    let password = String::from("super_secret_123");
    assert!(!is_zeroized(&password));  // Will fail
}
```

#### GREEN: Add zeroize to Cargo.toml
```toml
[dependencies]
zeroize = { version = "1.6", features = ["std", "derive"] }
```

Add to code:
```rust
use zeroize::Zeroizing;

let password = Zeroizing::new(password_string);
// Password will be zeroed on drop
```

#### REFACTOR: Create password wrapper type
```rust
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone)]
pub struct Password(Zeroizing<String>);

impl Password {
    pub fn new(pass: String) -> Self {
        Password(Zeroizing::new(pass))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for Password {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Password(***)")
    }
}
```

#### CLEANUP
- [ ] Verify zeroize works
- [ ] Check dependency versions
- [ ] Clippy passes

---

### TDD Cycle 2: Update Connection String Handling

#### RED: Write test for secure password storage
```rust
#[test]
fn test_credentials_struct_uses_zeroizing() {
    let creds = DbCredentials {
        username: "admin".to_string(),
        password: Password::new("secret123".to_string()),
    };

    // Password should be Zeroizing type
    assert_eq!(creds.username, "admin");
    // creds.password is secure type
}

#[test]
fn test_password_not_in_debug_output() {
    let creds = DbCredentials {
        username: "admin".to_string(),
        password: Password::new("secret123".to_string()),
    };

    let debug_str = format!("{:?}", creds);
    assert!(!debug_str.contains("secret123"));
    assert!(debug_str.contains("***"));
}
```

#### GREEN: Update DbCredentials struct
```rust
#[derive(Clone)]
pub struct DbCredentials {
    pub username: String,
    pub password: Password,
}

impl Debug for DbCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbCredentials")
            .field("username", &self.username)
            .field("password", &"***")
            .finish()
    }
}
```

#### REFACTOR: Implement secure parsing
```rust
impl DbCredentials {
    pub fn from_connection_string(conn_str: &str) -> Result<Self> {
        // Parse connection string
        let (user, pass) = parse_auth(conn_str)?;

        Ok(DbCredentials {
            username: user,
            password: Password::new(pass),
            // password is now Zeroizing type
        })
    }

    pub fn connect_string(&self) -> String {
        // Never include password in string representation
        format!("postgres://{}@host/db", self.username)
    }
}
```

#### CLEANUP
- [ ] All tests pass
- [ ] No password leaks in error messages
- [ ] Check all usages of password field

---

### TDD Cycle 3: Error Message Sanitization

#### RED: Write test for password not in errors
```rust
#[test]
fn test_connection_error_hides_password() {
    let creds = DbCredentials {
        username: "admin".to_string(),
        password: Password::new("secret123".to_string()),
    };

    let result = connect_to_db(&creds);
    match result {
        Err(e) => {
            let error_msg = e.to_string();
            assert!(!error_msg.contains("secret123"));
            assert!(!error_msg.contains(&creds.password.as_str()));
        }
        _ => (),
    }
}
```

#### GREEN: Add error sanitization
```rust
pub enum DbError {
    ConnectionFailed { reason: String },
    AuthenticationFailed,
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DbError::ConnectionFailed { reason } => {
                // Sanitize reason - remove sensitive data
                let sanitized = reason
                    .replace("[sensitive]", "***")
                    .replace("password", "***");
                write!(f, "Connection failed: {}", sanitized)
            }
            DbError::AuthenticationFailed => {
                write!(f, "Authentication failed")
            }
        }
    }
}

// Helper function
fn sanitize_error_message(msg: &str) -> String {
    msg.replace("password", "***")
        .replace("secret", "***")
        .replace("token", "***")
}
```

#### REFACTOR: Create error context wrapper
```rust
pub struct SecureDbError {
    // Internal error with full details
    internal: DbError,
    // External error safe to show users
    public: String,
}

impl SecureDbError {
    pub fn connection_failed(internal_reason: &str) -> Self {
        SecureDbError {
            internal: DbError::ConnectionFailed {
                reason: internal_reason.to_string(),
            },
            public: "Connection failed - check configuration".to_string(),
        }
    }

    pub fn public_message(&self) -> &str {
        &self.public
    }
}
```

#### CLEANUP
- [ ] Verify errors don't leak passwords
- [ ] Check server logs for sensitive data
- [ ] Clippy passes

---

## Files to Modify

1. **`Cargo.toml`**
   - Add zeroize dependency

2. **`crates/fraiseql-wire/src/types.rs`**
   - Create Password wrapper type
   - Create DbCredentials struct

3. **`crates/fraiseql-wire/src/client/connection_string.rs`**
   - Use Password type
   - Remove plaintext password storage

4. **`crates/fraiseql-core/src/error.rs`**
   - Add error sanitization
   - Remove password from error messages

---

## Tests to Create

```rust
#[cfg(test)]
mod password_security_tests {
    use super::*;

    // Zeroizing tests
    #[test]
    fn test_password_uses_zeroizing() { }

    #[test]
    fn test_password_dropped_properly() { }

    #[test]
    fn test_password_not_cloned_unprotected() { }

    // Debug output tests
    #[test]
    fn test_password_debug_hides_value() { }

    #[test]
    fn test_credentials_debug_hides_password() { }

    // Error message tests
    #[test]
    fn test_error_messages_sanitized() { }

    #[test]
    fn test_connection_error_no_password() { }

    // Integration tests
    #[test]
    fn test_connection_with_secure_password() { }
}
```

---

## Dependencies Added

```toml
zeroize = { version = "1.6", features = ["std", "derive"] }
```

---

## Configuration

No config changes needed. Just ensure connection strings are in environment variables:

```bash
# ✅ Good: Password in env var, not in config file
export DATABASE_URL="postgres://user:pass@host/db"

# ❌ Avoid: Password in config file
# config.toml: database_url = "postgres://user:pass@..."
```

---

## Performance Impact

**Expected**: Negligible to positive
- Zeroizing<String> same performance as String
- Memory zeroing: implicit on drop (no explicit call)
- Encoding/decoding: same as before

---

## Commit Message Template

```
fix(security-11.3): Secure password memory management

## Changes
- Add zeroize crate for secure memory handling
- Create Password wrapper type with auto-zeroing
- Replace plaintext String passwords with Zeroizing<String>
- Sanitize error messages to hide passwords
- Update Debug output to hide password values

## Vulnerability Addressed
CVSS 8.1 - Plaintext password storage in memory

## Verification
✅ Password memory tests pass
✅ Error message sanitization works
✅ No password in logs/errors
✅ Clippy clean
```

---

## Phase Status

**Ready**: ✅ Implementation plan complete
**Next**: BEGIN TDD CYCLE 1 - Add zeroize dependency

---

**Review**: [Pending approval]
**Reviewed By**: [Awaiting]
**Approved**: [Awaiting]
