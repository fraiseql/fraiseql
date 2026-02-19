# FraiseQL Scala

> **100% Feature Parity** with 11 other languages

Declarative, type-safe GraphQL schema authoring for Scala with advanced authorization and security.

## Features

### Authorization & Security (NEW in Phase 16)

✅ **Custom Authorization Rules** - Expression-based authorization with context variables
✅ **Role-Based Access Control (RBAC)** - Multiple roles with flexible matching strategies
✅ **Attribute-Based Access Control (ABAC)** - Conditional attribute evaluation
✅ **Authorization Policies** - Reusable policies (RBAC, ABAC, CUSTOM, HYBRID)
✅ **Caching** - Configurable TTL for authorization decisions
✅ **Audit Logging** - Comprehensive access decision tracking

### 100% Feature Parity

All 30 core features available across 12 languages:

- Type system (6 features)
- Operations (7 features)
- Field metadata (4 features)
- Analytics (5 features)
- Security (3 features)
- Observers (5 features)

## Requirements

- Scala 2.13+
- SBT 1.8+
- Java 11+

## Installation

Add to your `build.sbt`:

```scala
libraryDependencies += "com.fraiseql" %% "fraiseql-scala" % "1.0.0"
```

## Quick Start

### Custom Authorization Rules

```scala
import com.fraiseql.security._

// Using builder
val config = new AuthorizeBuilder()
  .withRule("isOwner($context.userId, $field.ownerId)")
  .withDescription("Ensures users can only access their own notes")
  .withCacheable(true)
  .withCacheDurationSeconds(300)
  .build()
```

### Role-Based Access Control

```scala
// Using builder
val config = new RoleRequiredBuilder()
  .withRoles(List("manager", "director"))
  .withStrategy(RoleMatchStrategy.Any)
  .withDescription("Managers and directors can view salaries")
  .build()
```

### Authorization Policies

```scala
// Using builder
val policy = new AuthzPolicyBuilder("piiAccess")
  .withType(AuthzPolicyType.Rbac)
  .withRule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
  .withDescription("Access to Personally Identifiable Information")
  .withCacheable(true)
  .withAuditLogging(true)
  .build()
```

## Authorization Patterns

### RBAC - Role-Based Access Control

```scala
val adminPolicy = new AuthzPolicyBuilder("adminOnly")
  .withType(AuthzPolicyType.Rbac)
  .withRule("hasRole($context, 'admin')")
  .withAuditLogging(true)
  .build()
```

### ABAC - Attribute-Based Access Control

```scala
val clearancePolicy = new AuthzPolicyBuilder("secretClearance")
  .withType(AuthzPolicyType.Abac)
  .withAttributes(List("clearance_level >= 3", "background_check == true"))
  .withDescription("Requires top secret clearance")
  .build()
```

### Hybrid Policies

```scala
val auditPolicy = new AuthzPolicyBuilder("auditAccess")
  .withType(AuthzPolicyType.Hybrid)
  .withRule("hasRole($context, 'auditor')")
  .withAttributes(List("audit_enabled == true"))
  .withDescription("Role and attribute-based access")
  .build()
```

## Configuration Options

### AuthorizeBuilder

```scala
new AuthorizeBuilder()
  .withRule(string)                      // Rule expression
  .withPolicy(string)                    // Named policy reference
  .withDescription(string)               // Description
  .withErrorMessage(string)              // Custom error message
  .withRecursive(boolean)                // Apply to nested types
  .withOperations(string)                // Specific operations
  .withCacheable(boolean)                // Cache decisions
  .withCacheDurationSeconds(int)         // Cache TTL
  .build()
```

### RoleRequiredBuilder

```scala
new RoleRequiredBuilder()
  .withRoles(List[String])               // Required roles
  .withStrategy(RoleMatchStrategy)       // Any, All, Exactly
  .withHierarchy(boolean)                // Role hierarchy
  .withDescription(string)               // Description
  .withErrorMessage(string)              // Custom error
  .withOperations(string)                // Specific operations
  .withInherit(boolean)                  // Inherit from parent
  .withCacheable(boolean)                // Cache results
  .withCacheDurationSeconds(int)         // Cache TTL
  .build()
```

### AuthzPolicyBuilder

```scala
new AuthzPolicyBuilder(name)
  .withType(AuthzPolicyType)             // Rbac/Abac/Custom/Hybrid
  .withDescription(string)               // Description
  .withRule(string)                      // Rule expression
  .withAttributes(List[String])          // ABAC attributes
  .withCacheable(boolean)                // Cache decisions
  .withCacheDurationSeconds(int)         // Cache TTL
  .withRecursive(boolean)                // Apply recursively
  .withOperations(string)                // Specific operations
  .withAuditLogging(boolean)             // Log decisions
  .withErrorMessage(string)              // Custom error
  .build()
```

## Role Matching Strategies

```scala
RoleMatchStrategy.Any        // At least one role
RoleMatchStrategy.All        // All roles required
RoleMatchStrategy.Exactly    // Exactly these roles
```

## Policy Types

```scala
AuthzPolicyType.Rbac         // Role-based
AuthzPolicyType.Abac         // Attribute-based
AuthzPolicyType.Custom       // Custom rules
AuthzPolicyType.Hybrid       // Combined approach
```

## Building & Testing

### Build the project

```bash
sbt build
```

### Run tests

```bash
sbt test
```

### Run specific test

```bash
sbt "test-only com.fraiseql.security.AuthorizationSpec"
```

## Project Structure

```
fraiseql-scala/
├── src/
│   ├── main/scala/com/fraiseql/security/
│   │   └── Security.scala               # Main security module
│   └── test/scala/com/fraiseql/security/
│       └── SecuritySpec.scala           # 44 comprehensive tests
├── build.sbt                            # SBT manifest
├── README.md                            # This file
└── SCALA_FEATURE_PARITY.md             # Feature parity status
```

## API Documentation

### AuthorizeBuilder

Fluent API for custom authorization rules:

```scala
new AuthorizeBuilder()
  .withRule(rule: String)
  .withPolicy(policy: String)
  .withDescription(desc: String)
  .withErrorMessage(msg: String)
  .withRecursive(flag: Boolean)
  .withOperations(ops: String)
  .withCacheable(flag: Boolean)
  .withCacheDurationSeconds(duration: Int)
  .build(): AuthorizeConfig
```

### RoleRequiredBuilder

Fluent API for RBAC rules:

```scala
new RoleRequiredBuilder()
  .withRoles(roles: List[String])
  .withStrategy(strat: RoleMatchStrategy)
  .withHierarchy(flag: Boolean)
  .withDescription(desc: String)
  .withErrorMessage(msg: String)
  .withOperations(ops: String)
  .withInherit(flag: Boolean)
  .withCacheable(flag: Boolean)
  .withCacheDurationSeconds(duration: Int)
  .build(): RoleRequiredConfig
```

### AuthzPolicyBuilder

Fluent API for authorization policies:

```scala
new AuthzPolicyBuilder(name: String)
  .withType(type_: AuthzPolicyType)
  .withDescription(desc: String)
  .withRule(rule: String)
  .withAttributes(attrs: List[String])
  .withCacheable(flag: Boolean)
  .withCacheDurationSeconds(duration: Int)
  .withRecursive(flag: Boolean)
  .withOperations(ops: String)
  .withAuditLogging(flag: Boolean)
  .withErrorMessage(msg: String)
  .build(): AuthzPolicyConfig
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
| Swift | 30/30 ✅ |
| **Scala** | **30/30** ✅ |

## Documentation

- [SCALA_FEATURE_PARITY.md](./SCALA_FEATURE_PARITY.md) - Complete feature parity status
- [Scala Documentation](https://docs.scala-lang.org/) - Language documentation
- [SBT Guide](https://www.scala-sbt.org/1.x/docs/) - Build tool documentation

## License

Apache License 2.0

## Contributing

Contributions are welcome! Please ensure:

- All tests pass: `sbt test`
- Code follows Scala style guidelines
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
- [FraiseQL Swift](../fraiseql-swift/)

---

**Phase 16** - Scala Feature Parity - Security Extensions ✅

All 30 features implemented with 100% parity across 12 languages.
