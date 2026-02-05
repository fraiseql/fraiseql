# FraiseQL Kotlin

> **100% Feature Parity** with Python, TypeScript, Java, Go, PHP, Node.js, and Ruby

Declarative, type-safe GraphQL schema authoring for Kotlin with advanced authorization and security.

## Features

### Authorization & Security (NEW in Phase 12)

✅ **Custom Authorization Rules** - Expression-based authorization with context variables
✅ **Role-Based Access Control (RBAC)** - Multiple roles with flexible matching strategies
✅ **Attribute-Based Access Control (ABAC)** - Conditional attribute evaluation
✅ **Authorization Policies** - Reusable policies (RBAC, ABAC, CUSTOM, HYBRID)
✅ **Caching** - Configurable TTL for authorization decisions
✅ **Audit Logging** - Comprehensive access decision tracking

### 100% Feature Parity

All 30 core features available across 8 languages:

- Type system (6 features)
- Operations (7 features)
- Field metadata (4 features)
- Analytics (5 features)
- Security (3 features)
- Observers (5 features)

## Requirements

- Kotlin 1.9.20+
- Java 11+
- Gradle 7.0+

## Installation

Add to your `build.gradle.kts`:

```kotlin
dependencies {
    implementation("com.fraiseql:fraiseql-kotlin:1.0.0")
}

repositories {
    mavenCentral()
}
```

## Quick Start

### Custom Authorization Rules

```kotlin
import com.fraiseql.security.*

val config = AuthorizeBuilder()
    .rule("isOwner(\$context.userId, \$field.ownerId)")
    .description("Ensures users can only access their own notes")
    .cacheable(true)
    .cacheDurationSeconds(300)
    .build()

// Or using annotations
@Authorize(
    rule = "isOwner(\$context.userId, \$field.ownerId)",
    description = "Ensures users can only access their own notes"
)
class ProtectedNote {
    val id: Int = 0
    val content: String = ""
    val ownerId: String = ""
}
```

### Role-Based Access Control

```kotlin
val config = RoleRequiredBuilder()
    .roles("manager", "director")
    .strategy(RoleMatchStrategy.Any)
    .description("Managers and directors can view salaries")
    .build()

// Or using annotations
@RoleRequired(
    roles = ["manager", "director"],
    strategy = "any",
    description = "Managers and directors can view salaries"
)
class SalaryData {
    val employeeId: String = ""
    val salary: Double = 0.0
}
```

### Authorization Policies

```kotlin
val policy = AuthzPolicyBuilder("piiAccess")
    .type(AuthzPolicyType.Rbac)
    .rule("hasRole(\$context, 'data_manager') OR hasScope(\$context, 'read:pii')")
    .description("Access to Personally Identifiable Information")
    .cacheable(true)
    .auditLogging(true)
    .build()

// Or using annotations
@AuthzPolicy(
    name = "piiAccess",
    type = "rbac",
    rule = "hasRole(\$context, 'data_manager') OR hasScope(\$context, 'read:pii')",
    description = "Access to Personally Identifiable Information"
)
class Customer {
    val id: String = ""
    val name: String = ""
    val email: String = ""
}
```

## Authorization Patterns

### RBAC - Role-Based Access Control

```kotlin
val adminPolicy = AuthzPolicyBuilder("adminOnly")
    .type(AuthzPolicyType.Rbac)
    .rule("hasRole(\$context, 'admin')")
    .auditLogging(true)
    .build()
```

### ABAC - Attribute-Based Access Control

```kotlin
val clearancePolicy = AuthzPolicyBuilder("secretClearance")
    .type(AuthzPolicyType.Abac)
    .attributes("clearance_level >= 3", "background_check == true")
    .description("Requires top secret clearance")
    .build()
```

### Hybrid Policies

```kotlin
val auditPolicy = AuthzPolicyBuilder("auditAccess")
    .type(AuthzPolicyType.Hybrid)
    .rule("hasRole(\$context, 'auditor')")
    .attributes("audit_enabled == true")
    .description("Role and attribute-based access")
    .build()
```

## Configuration Options

### AuthorizeBuilder

```kotlin
AuthorizeBuilder()
    .rule(string)                      // Rule expression
    .policy(string)                    // Named policy reference
    .description(string)               // Description
    .errorMessage(string)              // Custom error message
    .recursive(boolean)                // Apply to nested types
    .operations(string)                // Specific operations
    .cacheable(boolean)                // Cache decisions
    .cacheDurationSeconds(int)         // Cache TTL
    .build()
```

### RoleRequiredBuilder

```kotlin
RoleRequiredBuilder()
    .roles(*strings)                   // Required roles (varargs)
    .rolesArray(list)                  // Roles from list
    .strategy(strategy)                // ANY, ALL, EXACTLY
    .hierarchy(boolean)                // Role hierarchy
    .description(string)               // Description
    .errorMessage(string)              // Custom error
    .operations(string)                // Specific operations
    .inherit(boolean)                  // Inherit from parent
    .cacheable(boolean)                // Cache results
    .cacheDurationSeconds(int)         // Cache TTL
    .build()
```

### AuthzPolicyBuilder

```kotlin
AuthzPolicyBuilder(name)
    .description(string)               // Description
    .rule(string)                      // Rule expression
    .attributes(*strings)              // ABAC attributes (varargs)
    .attributesArray(list)             // Attributes from list
    .type(type)                        // RBAC/ABAC/CUSTOM/HYBRID
    .cacheable(boolean)                // Cache decisions
    .cacheDurationSeconds(int)         // Cache TTL
    .recursive(boolean)                // Apply recursively
    .operations(string)                // Specific operations
    .auditLogging(boolean)             // Log decisions
    .errorMessage(string)              // Custom error
    .build()
```

## Role Matching Strategies

```kotlin
RoleMatchStrategy.Any       // At least one role
RoleMatchStrategy.All       // All roles required
RoleMatchStrategy.Exactly   // Exactly these roles
```

## Policy Types

```kotlin
AuthzPolicyType.Rbac       // Role-based
AuthzPolicyType.Abac       // Attribute-based
AuthzPolicyType.Custom     // Custom rules
AuthzPolicyType.Hybrid     // Combined approach
```

## Building & Testing

### Build the project

```bash
./gradlew build
```

### Run tests

```bash
./gradlew test
```

### Run specific test

```bash
./gradlew test --tests "*AuthorizationTest"
```

### Build with coverage

```bash
./gradlew test jacocoTestReport
```

## Project Structure

```
fraiseql-kotlin/
├── src/
│   ├── main/kotlin/com/fraiseql/security/
│   │   └── Security.kt          # Security module
│   └── test/kotlin/com/fraiseql/security/
│       ├── AuthorizationTest.kt
│       ├── RoleBasedAccessControlTest.kt
│       ├── AttributeBasedAccessControlTest.kt
│       └── AuthzPolicyTest.kt
├── build.gradle.kts
├── settings.gradle.kts
├── README.md
└── KOTLIN_FEATURE_PARITY.md
```

## API Documentation

### AuthorizeBuilder

Fluent API for custom authorization rules:

```kotlin
AuthorizeBuilder()
    .rule(rule: String)
    .policy(policy: String)
    .description(description: String)
    .errorMessage(msg: String)
    .recursive(flag: Boolean)
    .operations(ops: String)
    .cacheable(flag: Boolean)
    .cacheDurationSeconds(duration: Int)
    .build(): AuthorizeConfig
```

### RoleRequiredBuilder

Fluent API for RBAC rules:

```kotlin
RoleRequiredBuilder()
    .roles(vararg roles: String)
    .rolesArray(roles: List<String>)
    .strategy(strat: RoleMatchStrategy)
    .hierarchy(flag: Boolean)
    .description(desc: String)
    .errorMessage(msg: String)
    .operations(ops: String)
    .inherit(flag: Boolean)
    .cacheable(flag: Boolean)
    .cacheDurationSeconds(duration: Int)
    .build(): RoleRequiredConfig
```

### AuthzPolicyBuilder

Fluent API for authorization policies:

```kotlin
AuthzPolicyBuilder(name: String)
    .description(desc: String)
    .rule(rule: String)
    .attributes(vararg attrs: String)
    .attributesArray(attrs: List<String>)
    .type(type: AuthzPolicyType)
    .cacheable(flag: Boolean)
    .cacheDurationSeconds(duration: Int)
    .recursive(flag: Boolean)
    .operations(ops: String)
    .auditLogging(flag: Boolean)
    .errorMessage(msg: String)
    .build(): AuthzPolicyConfig
```

## Feature Parity

100% feature parity across all authoring languages:

| Language | Type System | Operations | Metadata | Analytics | Security | Observers | Total |
|----------|-------------|-----------|----------|-----------|----------|-----------|-----------|
| Python | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| TypeScript | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Java | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Go | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| PHP | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Node.js | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Ruby | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| **Kotlin** | **6/6** | **7/7** | **4/4** | **5/5** | **3/3** | **5/5** | **30/30** ✅ |

## Documentation

- [KOTLIN_FEATURE_PARITY.md](./KOTLIN_FEATURE_PARITY.md) - Complete feature parity status
- [Kotlin Documentation](https://kotlinlang.org/) - Language documentation
- [JUnit5 Documentation](https://junit.org/junit5/) - Testing framework

## License

Apache License 2.0

## Contributing

Contributions are welcome! Please ensure:

- All tests pass: `./gradlew test`
- Code follows Kotlin style guide
- Tests have good coverage

## See Also

- [FraiseQL Python](../fraiseql-python/)
- [FraiseQL TypeScript](../fraiseql-typescript/)
- [FraiseQL Java](../fraiseql-java/)
- [FraiseQL Go](../fraiseql-go/)
- [FraiseQL PHP](../fraiseql-php/)
- [FraiseQL Node.js](../fraiseql-nodejs/)
- [FraiseQL Ruby](../fraiseql-ruby/)

---

**Phase 12** - Kotlin Feature Parity - Security Extensions ✅

All 30 features implemented with 100% parity across 8 languages.
