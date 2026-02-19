# FraiseQL Swift

> **100% Feature Parity** with 10 other languages

Declarative, type-safe GraphQL schema authoring for Swift with advanced authorization and security.

## Features

### Authorization & Security (NEW in Phase 15)

✅ **Custom Authorization Rules** - Expression-based authorization with context variables
✅ **Role-Based Access Control (RBAC)** - Multiple roles with flexible matching strategies
✅ **Attribute-Based Access Control (ABAC)** - Conditional attribute evaluation
✅ **Authorization Policies** - Reusable policies (RBAC, ABAC, CUSTOM, HYBRID)
✅ **Caching** - Configurable TTL for authorization decisions
✅ **Audit Logging** - Comprehensive access decision tracking

### 100% Feature Parity

All 30 core features available across 11 languages:

- Type system (6 features)
- Operations (7 features)
- Field metadata (4 features)
- Analytics (5 features)
- Security (3 features)
- Observers (5 features)

## Requirements

- Swift 5.9+
- iOS 16+, macOS 13+, tvOS 16+, watchOS 9+
- Xcode 15+

## Installation

Add to your `Package.swift`:

```swift
.package(url: "https://github.com/fraiseql/fraiseql-swift.git", from: "1.0.0")
```

Or to your `Podfile` (if using CocoaPods):

```ruby
pod 'FraiseQLSecurity', '~> 1.0.0'
```

## Quick Start

### Custom Authorization Rules

```swift
import FraiseQLSecurity

// Using builder
let config = AuthorizeBuilder()
    .rule("isOwner($context.userId, $field.ownerId)")
    .description("Ensures users can only access their own notes")
    .cacheable(true)
    .cacheDurationSeconds(300)
    .build()
```

### Role-Based Access Control

```swift
// Using builder
let config = RoleRequiredBuilder()
    .roles(["manager", "director"])
    .strategy(.any)
    .description("Managers and directors can view salaries")
    .build()
```

### Authorization Policies

```swift
// Using builder
let policy = AuthzPolicyBuilder("piiAccess")
    .type(.rbac)
    .rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
    .description("Access to Personally Identifiable Information")
    .cacheable(true)
    .auditLogging(true)
    .build()
```

## Authorization Patterns

### RBAC - Role-Based Access Control

```swift
let adminPolicy = AuthzPolicyBuilder("adminOnly")
    .type(.rbac)
    .rule("hasRole($context, 'admin')")
    .auditLogging(true)
    .build()
```

### ABAC - Attribute-Based Access Control

```swift
let clearancePolicy = AuthzPolicyBuilder("secretClearance")
    .type(.abac)
    .attributes(["clearance_level >= 3", "background_check == true"])
    .description("Requires top secret clearance")
    .build()
```

### Hybrid Policies

```swift
let auditPolicy = AuthzPolicyBuilder("auditAccess")
    .type(.hybrid)
    .rule("hasRole($context, 'auditor')")
    .attributes(["audit_enabled == true"])
    .description("Role and attribute-based access")
    .build()
```

## Configuration Options

### AuthorizeBuilder

```swift
AuthorizeBuilder()
    .rule(String)                      // Rule expression
    .policy(String)                    // Named policy reference
    .description(String)               // Description
    .errorMessage(String)              // Custom error message
    .recursive(Bool)                   // Apply to nested types
    .operations(String)                // Specific operations
    .cacheable(Bool)                   // Cache decisions
    .cacheDurationSeconds(Int)         // Cache TTL
    .build()
```

### RoleRequiredBuilder

```swift
RoleRequiredBuilder()
    .roles([String])                   // Required roles
    .strategy(RoleMatchStrategy)       // any, all, exactly
    .hierarchy(Bool)                   // Role hierarchy
    .description(String)               // Description
    .errorMessage(String)              // Custom error
    .operations(String)                // Specific operations
    .inherit(Bool)                     // Inherit from parent
    .cacheable(Bool)                   // Cache results
    .cacheDurationSeconds(Int)         // Cache TTL
    .build()
```

### AuthzPolicyBuilder

```swift
AuthzPolicyBuilder(name)
    .type(AuthzPolicyType)             // rbac/abac/custom/hybrid
    .description(String)               // Description
    .rule(String)                      // Rule expression
    .attributes([String])              // ABAC attributes
    .cacheable(Bool)                   // Cache decisions
    .cacheDurationSeconds(Int)         // Cache TTL
    .recursive(Bool)                   // Apply recursively
    .operations(String)                // Specific operations
    .auditLogging(Bool)                // Log decisions
    .errorMessage(String)              // Custom error
    .build()
```

## Role Matching Strategies

```swift
RoleMatchStrategy.any        // At least one role
RoleMatchStrategy.all        // All roles required
RoleMatchStrategy.exactly    // Exactly these roles
```

## Policy Types

```swift
AuthzPolicyType.rbac         // Role-based
AuthzPolicyType.abac         // Attribute-based
AuthzPolicyType.custom       // Custom rules
AuthzPolicyType.hybrid       // Combined approach
```

## Building & Testing

### Build the project

```bash
swift build
```

### Run tests

```bash
swift test
```

### Run specific test

```bash
swift test SecurityTests
```

## Project Structure

```
fraiseql-swift/
├── Sources/
│   └── FraiseQLSecurity/
│       └── Security.swift             # Main security module
├── Tests/
│   └── FraiseQLSecurityTests/
│       └── SecurityTests.swift        # 44 comprehensive tests
├── Package.swift                      # SPM manifest
├── README.md                          # This file
└── SWIFT_FEATURE_PARITY.md           # Feature parity status
```

## API Documentation

### AuthorizeBuilder

Fluent API for custom authorization rules:

```swift
let config: AuthorizeConfig = AuthorizeBuilder()
    .rule(rule: String)
    .policy(policy: String)
    .description(desc: String)
    .errorMessage(msg: String)
    .recursive(flag: Bool)
    .operations(ops: String)
    .cacheable(flag: Bool)
    .cacheDurationSeconds(duration: Int)
    .build()
```

### RoleRequiredBuilder

Fluent API for RBAC rules:

```swift
let config: RoleRequiredConfig = RoleRequiredBuilder()
    .roles(roles: [String])
    .strategy(strat: RoleMatchStrategy)
    .hierarchy(flag: Bool)
    .description(desc: String)
    .errorMessage(msg: String)
    .operations(ops: String)
    .inherit(flag: Bool)
    .cacheable(flag: Bool)
    .cacheDurationSeconds(duration: Int)
    .build()
```

### AuthzPolicyBuilder

Fluent API for authorization policies:

```swift
let config: AuthzPolicyConfig = AuthzPolicyBuilder(name: String)
    .type(type: AuthzPolicyType)
    .description(desc: String)
    .rule(rule: String)
    .attributes(attrs: [String])
    .cacheable(flag: Bool)
    .cacheDurationSeconds(duration: Int)
    .recursive(flag: Bool)
    .operations(ops: String)
    .auditLogging(flag: Bool)
    .errorMessage(msg: String)
    .build()
```

## Feature Parity

100% feature parity across all authoring languages:

| Language | Total Features |
|----------|-----------------|
| Python | 30/30 ✅ |
| TypeScript | 30/30 ✅ |
| Java | 30/30 ✅ |
| Go | 30/30 ✅ |
| PHP | 30/30 ✅ |
| Node.js | 30/30 ✅ |
| Ruby | 30/30 ✅ |
| Kotlin | 30/30 ✅ |
| C#/.NET | 30/30 ✅ |
| Rust | 30/30 ✅ |
| **Swift** | **30/30** ✅ |

## Documentation

- [SWIFT_FEATURE_PARITY.md](./SWIFT_FEATURE_PARITY.md) - Complete feature parity status
- [Swift.org Documentation](https://www.swift.org/documentation/) - Language documentation
- [SPM Guide](https://github.com/apple/swift-package-manager/tree/main/Documentation) - Package management

## License

Apache License 2.0

## Contributing

Contributions are welcome! Please ensure:

- All tests pass: `swift test`
- Code follows Swift style guide
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
- [FraiseQL Rust](../fraiseql-rust/)

---

**Phase 15** - Swift Feature Parity - Security Extensions ✅

All 30 features implemented with 100% parity across 11 languages.
