# FraiseQL Groovy

> **100% Feature Parity** with 12 other languages

Declarative, type-safe GraphQL schema authoring for Groovy with advanced authorization and security.

## Features

✅ **Custom Authorization Rules** - Expression-based with context variables
✅ **Role-Based Access Control (RBAC)** - Multiple roles with flexible strategies
✅ **Attribute-Based Access Control (ABAC)** - Conditional attribute evaluation
✅ **Authorization Policies** - Reusable policies (RBAC, ABAC, CUSTOM, HYBRID)
✅ **Caching** - Configurable TTL for authorization decisions
✅ **Audit Logging** - Comprehensive access decision tracking

## Requirements

- Groovy 4.0+
- Java 11+
- Gradle 7.0+

## Installation

Add to your `build.gradle`:

```gradle
dependencies {
    implementation 'com.fraiseql:fraiseql-groovy:1.0.0'
}
```

## Quick Start

```groovy
import com.fraiseql.security.*

// Custom authorization rule
def config = new AuthorizeBuilder()
    .rule("isOwner(\$context.userId, \$field.ownerId)")
    .description("Ownership check")
    .cacheable(true)
    .cacheDurationSeconds(300)
    .build()

// Role-based access control
def roles = new RoleRequiredBuilder()
    .roles(["manager", "director"])
    .strategy(RoleMatchStrategy.ANY)
    .build()

// Authorization policy
def policy = new AuthzPolicyBuilder("piiAccess")
    .type(AuthzPolicyType.RBAC)
    .rule("hasRole(\$context, 'data_manager')")
    .build()
```

## Building & Testing

```bash
# Build
gradle build

# Test
gradle test

# Specific test
gradle test --tests "AuthorizationSpec"
```

## Project Structure

```
fraiseql-groovy/
├── src/
│   ├── main/groovy/com/fraiseql/security/
│   │   └── Security.groovy
│   └── test/groovy/com/fraiseql/security/
│       └── SecuritySpec.groovy
├── build.gradle
├── README.md
└── GROOVY_FEATURE_PARITY.md
```

## Documentation

- [GROOVY_FEATURE_PARITY.md](./GROOVY_FEATURE_PARITY.md) - Feature parity status
- [Groovy Documentation](https://groovy-lang.org/documentation.html)
- [Spock Framework](http://spockframework.org/)

## License

Apache License 2.0

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
- [FraiseQL Scala](../fraiseql-scala/)

---

**Phase 17** - Groovy Feature Parity - Security Extensions ✅

All 30 features implemented with 100% parity across 13 languages.
