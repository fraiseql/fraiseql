# FraiseQL Rust

> **100% Feature Parity** with Python, TypeScript, Java, Go, PHP, Node.js, Ruby, Kotlin, and C#/.NET

Declarative, type-safe GraphQL schema authoring for Rust with advanced authorization and security.

## Features

### Authorization & Security (NEW in Phase 14)

✅ **Custom Authorization Rules** - Expression-based authorization with context variables
✅ **Role-Based Access Control (RBAC)** - Multiple roles with flexible matching strategies
✅ **Attribute-Based Access Control (ABAC)** - Conditional attribute evaluation
✅ **Authorization Policies** - Reusable policies (RBAC, ABAC, CUSTOM, HYBRID)
✅ **Caching** - Configurable TTL for authorization decisions
✅ **Audit Logging** - Comprehensive access decision tracking

### 100% Feature Parity

All 30 core features available across 10 languages:

- Type system (6 features)
- Operations (7 features)
- Field metadata (4 features)
- Analytics (5 features)
- Security (3 features)
- Observers (5 features)

## Requirements

- Rust 1.70+ (2021 edition)
- Cargo
- No unsafe code (`unsafe_code = "forbid"`)

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
fraiseql-rust = "1.0.0"
```

## Quick Start

### Custom Authorization Rules

```rust
use fraiseql_rust::AuthorizeBuilder;

// Using builder
let config = AuthorizeBuilder::new()
    .rule("isOwner($context.userId, $field.ownerId)")
    .description("Ensures users can only access their own notes")
    .cacheable(true)
    .cache_duration_seconds(300)
    .build();
```

### Role-Based Access Control

```rust
use fraiseql_rust::{RoleRequiredBuilder, RoleMatchStrategy};

// Using builder
let config = RoleRequiredBuilder::new()
    .roles(vec!["manager", "director"])
    .strategy(RoleMatchStrategy::Any)
    .description("Managers and directors can view salaries")
    .build();
```

### Authorization Policies

```rust
use fraiseql_rust::{AuthzPolicyBuilder, AuthzPolicyType};

// Using builder
let policy = AuthzPolicyBuilder::new("piiAccess")
    .policy_type(AuthzPolicyType::Rbac)
    .rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
    .description("Access to Personally Identifiable Information")
    .cacheable(true)
    .audit_logging(true)
    .build();
```

## Authorization Patterns

### RBAC - Role-Based Access Control

```rust
let admin_policy = AuthzPolicyBuilder::new("adminOnly")
    .policy_type(AuthzPolicyType::Rbac)
    .rule("hasRole($context, 'admin')")
    .audit_logging(true)
    .build();
```

### ABAC - Attribute-Based Access Control

```rust
let clearance_policy = AuthzPolicyBuilder::new("secretClearance")
    .policy_type(AuthzPolicyType::Abac)
    .attributes(vec!["clearance_level >= 3", "background_check == true"])
    .description("Requires top secret clearance")
    .build();
```

### Hybrid Policies

```rust
let audit_policy = AuthzPolicyBuilder::new("auditAccess")
    .policy_type(AuthzPolicyType::Hybrid)
    .rule("hasRole($context, 'auditor')")
    .attributes(vec!["audit_enabled == true"])
    .description("Role and attribute-based access")
    .build();
```

## Configuration Options

### AuthorizeBuilder

```rust
AuthorizeBuilder::new()
    .rule(string)                      // Rule expression
    .policy(string)                    // Named policy reference
    .description(string)               // Description
    .error_message(string)             // Custom error message
    .recursive(bool)                   // Apply to nested types
    .operations(string)                // Specific operations
    .cacheable(bool)                   // Cache decisions
    .cache_duration_seconds(u32)       // Cache TTL
    .build()
```

### RoleRequiredBuilder

```rust
RoleRequiredBuilder::new()
    .roles(vec![...])                  // Required roles
    .roles_vec(Vec<String>)           // Roles from vector
    .strategy(RoleMatchStrategy)       // ANY, ALL, EXACTLY
    .hierarchy(bool)                   // Role hierarchy
    .description(string)               // Description
    .error_message(string)             // Custom error
    .operations(string)                // Specific operations
    .inherit(bool)                     // Inherit from parent
    .cacheable(bool)                   // Cache results
    .cache_duration_seconds(u32)       // Cache TTL
    .build()
```

### AuthzPolicyBuilder

```rust
AuthzPolicyBuilder::new(name)
    .policy_type(AuthzPolicyType)      // RBAC/ABAC/CUSTOM/HYBRID
    .description(string)               // Description
    .rule(string)                      // Rule expression
    .attributes(vec![...])             // ABAC attributes
    .attributes_vec(Vec<String>)      // Attributes from vector
    .cacheable(bool)                   // Cache decisions
    .cache_duration_seconds(u32)       // Cache TTL
    .recursive(bool)                   // Apply recursively
    .operations(string)                // Specific operations
    .audit_logging(bool)               // Log decisions
    .error_message(string)             // Custom error
    .build()
```

## Role Matching Strategies

```rust
RoleMatchStrategy::Any        // At least one role
RoleMatchStrategy::All        // All roles required
RoleMatchStrategy::Exactly    // Exactly these roles
```

## Policy Types

```rust
AuthzPolicyType::Rbac         // Role-based
AuthzPolicyType::Abac         // Attribute-based
AuthzPolicyType::Custom       // Custom rules
AuthzPolicyType::Hybrid       // Combined approach
```

## Building & Testing

### Build the project

```bash
cargo build
```

### Run tests

```bash
cargo test
```

### Run specific test

```bash
cargo test test_simple_authorization_rule
```

### Run with strict linting

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

## Project Structure

```
fraiseql-rust/
├── src/
│   ├── lib.rs                        # Library entry point
│   ├── authorization.rs              # Custom authorization rules
│   ├── roles.rs                      # Role-based access control
│   └── policies.rs                   # Authorization policies
├── tests/
│   └── integration_test.rs           # 44 integration tests
├── Cargo.toml                        # Manifest file
├── README.md                         # This file
└── RUST_FEATURE_PARITY.md           # Feature parity status
```

## API Documentation

### AuthorizeBuilder

Fluent API for custom authorization rules:

```rust
use fraiseql_rust::{AuthorizeBuilder, AuthorizeConfig};

let config: AuthorizeConfig = AuthorizeBuilder::new()
    .rule(rule: impl Into<String>)
    .policy(policy: impl Into<String>)
    .description(desc: impl Into<String>)
    .error_message(msg: impl Into<String>)
    .recursive(flag: bool)
    .operations(ops: impl Into<String>)
    .cacheable(flag: bool)
    .cache_duration_seconds(duration: u32)
    .build();
```

### RoleRequiredBuilder

Fluent API for RBAC rules:

```rust
use fraiseql_rust::{RoleRequiredBuilder, RoleMatchStrategy, RoleRequiredConfig};

let config: RoleRequiredConfig = RoleRequiredBuilder::new()
    .roles(roles: impl IntoIterator<Item = impl Into<String>>)
    .roles_vec(roles: Vec<String>)
    .strategy(strat: RoleMatchStrategy)
    .hierarchy(flag: bool)
    .description(desc: impl Into<String>)
    .error_message(msg: impl Into<String>)
    .operations(ops: impl Into<String>)
    .inherit(flag: bool)
    .cacheable(flag: bool)
    .cache_duration_seconds(duration: u32)
    .build();
```

### AuthzPolicyBuilder

Fluent API for authorization policies:

```rust
use fraiseql_rust::{AuthzPolicyBuilder, AuthzPolicyType, AuthzPolicyConfig};

let config: AuthzPolicyConfig = AuthzPolicyBuilder::new(name: impl Into<String>)
    .policy_type(type_: AuthzPolicyType)
    .description(desc: impl Into<String>)
    .rule(rule: impl Into<String>)
    .attributes(attrs: impl IntoIterator<Item = impl Into<String>>)
    .attributes_vec(attrs: Vec<String>)
    .cacheable(flag: bool)
    .cache_duration_seconds(duration: u32)
    .recursive(flag: bool)
    .operations(ops: impl Into<String>)
    .audit_logging(flag: bool)
    .error_message(msg: impl Into<String>)
    .build();
```

## Feature Parity

100% feature parity across all authoring languages:

| Language | Type System | Operations | Metadata | Analytics | Security | Observers | Total |
|----------|-------------|-----------|----------|-----------|----------|-----------|-------|
| Python | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| TypeScript | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Java | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Go | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| PHP | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Node.js | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Ruby | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Kotlin | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| C#/.NET | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| **Rust** | **6/6** | **7/7** | **4/4** | **5/5** | **3/3** | **5/5** | **30/30** ✅ |

## Documentation

- [RUST_FEATURE_PARITY.md](./RUST_FEATURE_PARITY.md) - Complete feature parity status
- [Rust Book](https://doc.rust-lang.org/book/) - Rust language documentation
- [Cargo Documentation](https://doc.rust-lang.org/cargo/) - Package manager documentation

## License

Apache License 2.0

## Contributing

Contributions are welcome! Please ensure:

- All tests pass: `cargo test`
- Code passes clippy: `cargo clippy --all-targets --all-features`
- No unsafe code (forbidden by lints)
- Tests have good coverage

## See Also

- [FraiseQL Python](../fraiseql-python/)
- [FraiseQL TypeScript](../fraiseql-typescript/)
- [FraiseQL Java](../fraiseql-java/)
- [FraiseQL Go](../fraiseql-go/)
- [FraiseQL PHP](../fraiseql-php/)
- [FraiseQL Node.js](../fraiseql-nodejs/)
- [FraiseQL Ruby](../fraiseql-ruby/)
- [FraiseQL Kotlin](../fraiseql-kotlin/)
- [FraiseQL C#/.NET](../fraiseql-csharp/)

---

**Phase 14** - Rust Feature Parity - Security Extensions ✅

All 30 features implemented with 100% parity across 10 languages.
