# FraiseQL Dart

> **100% Feature Parity** with 14 other languages

Declarative, type-safe GraphQL schema authoring for Dart with advanced authorization and security.

## Features

✅ **Custom Authorization Rules** - Expression-based with context variables
✅ **Role-Based Access Control (RBAC)** - Multiple roles with flexible strategies
✅ **Attribute-Based Access Control (ABAC)** - Conditional attribute evaluation
✅ **Authorization Policies** - Reusable policies (RBAC, ABAC, CUSTOM, HYBRID)
✅ **Caching** - Configurable TTL for authorization decisions
✅ **Audit Logging** - Comprehensive access decision tracking

## Requirements

- Dart SDK 3.0.0 or higher
- Pub package manager

## Installation

Add to your `pubspec.yaml`:

```yaml
dependencies:
  fraiseql_dart: ^1.0.0
```

Then run:

```bash
dart pub get
```

## Quick Start

```dart
import 'package:fraiseql_dart/fraiseql_security.dart';

// Custom authorization rule
final config = AuthorizeConfig(
  rule: 'isOwner($context.userId, $field.ownerId)',
  description: 'Ensures users can only access their own notes',
  cacheable: true,
  cacheDurationSeconds: 300,
);

// Role-based access control
final rbacConfig = RoleRequiredConfig(
  roles: ['manager', 'director'],
  strategy: RoleMatchStrategy.any,
);

// Authorization policy
final policyConfig = AuthzPolicyConfig(
  'piiAccess',
  type: AuthzPolicyType.rbac,
  rule: 'hasRole($context, \'data_manager\')',
  cacheDurationSeconds: 3600,
);

// Using builder pattern
final builderConfig = AuthorizeBuilder()
    .rule('isOwner(...)')
    .description('Ownership check')
    .build();
```

## Authorization Patterns

### RBAC - Role-Based Access Control

```dart
final policy = AuthzPolicyConfig(
  'adminOnly',
  type: AuthzPolicyType.rbac,
  rule: 'hasRole($context, \'admin\')',
  auditLogging: true,
);
```

### ABAC - Attribute-Based Access Control

```dart
final policy = AuthzPolicyConfig(
  'secretClearance',
  type: AuthzPolicyType.abac,
  attributes: ['clearance_level >= 3', 'background_check == true'],
  description: 'Requires top secret clearance',
);
```

### Hybrid Policies

```dart
final policy = AuthzPolicyConfig(
  'auditAccess',
  type: AuthzPolicyType.hybrid,
  rule: 'hasRole($context, \'auditor\')',
  attributes: ['audit_enabled == true'],
);
```

## Configuration Options

### AuthorizeConfig

```dart
AuthorizeConfig(
  rule: 'string',                    // Rule expression
  policy: 'string',                  // Named policy reference
  description: 'string',             // Description
  errorMessage: 'string',            // Custom error message
  recursive: false,                  // Apply to nested types
  operations: 'string',              // Specific operations
  cacheable: true,                   // Cache decisions
  cacheDurationSeconds: 300,         // Cache TTL in seconds
)
```

### RoleRequiredConfig

```dart
RoleRequiredConfig(
  roles: ['string'],                 // Required roles
  strategy: RoleMatchStrategy.any,   // any, all, exactly
  hierarchy: false,                  // Role hierarchy
  description: 'string',             // Description
  errorMessage: 'string',            // Custom error
  operations: 'string',              // Specific operations
  inherit: false,                    // Inherit from parent
  cacheable: true,                   // Cache results
  cacheDurationSeconds: 300,         // Cache TTL in seconds
)
```

### AuthzPolicyConfig

```dart
AuthzPolicyConfig(
  'policyName',
  type: AuthzPolicyType.custom,      // rbac, abac, custom, hybrid
  description: 'string',             // Description
  rule: 'string',                    // Rule expression
  attributes: ['string'],            // ABAC attributes
  cacheable: true,                   // Cache decisions
  cacheDurationSeconds: 300,         // Cache TTL
  recursive: false,                  // Apply recursively
  operations: 'string',              // Specific operations
  auditLogging: false,               // Log decisions
  errorMessage: 'string',            // Custom error
)
```

## Role Matching Strategies

```dart
RoleMatchStrategy.any         // At least one role
RoleMatchStrategy.all         // All roles required
RoleMatchStrategy.exactly     // Exactly these roles
```

## Policy Types

```dart
AuthzPolicyType.rbac          // Role-based
AuthzPolicyType.abac          // Attribute-based
AuthzPolicyType.custom        // Custom rules
AuthzPolicyType.hybrid        // Combined approach
```

## Building & Testing

```bash
# Get dependencies
dart pub get

# Run tests
dart test

# Run specific test file
dart test test/fraiseql_security_test.dart

# Run with coverage
dart test --coverage=coverage

# Format code
dart format .

# Analyze code
dart analyze
```

## Project Structure

```
fraiseql-dart/
├── lib/
│   └── fraiseql_security.dart      # Main security module
├── test/
│   └── fraiseql_security_test.dart # 44 comprehensive tests
├── pubspec.yaml                    # Package manifest
├── README.md                       # This file
└── DART_FEATURE_PARITY.md          # Feature parity status
```

## API Documentation

### AuthorizeConfig

Create custom authorization configuration:

```dart
final config = AuthorizeConfig(
  rule: 'isOwner(\$context.userId, \$resource.ownerId)',
  description: 'Ownership check',
);
```

### RoleRequiredConfig

Create RBAC configuration:

```dart
final config = RoleRequiredConfig(
  roles: ['manager', 'director'],
  strategy: RoleMatchStrategy.any,
);
```

### AuthzPolicyConfig

Create authorization policy:

```dart
final policy = AuthzPolicyConfig(
  'piiAccess',
  type: AuthzPolicyType.rbac,
  rule: 'hasRole(\$context, \'data_manager\')',
);
```

### Builder Pattern

All configuration classes support fluent builders:

```dart
final config = AuthorizeBuilder()
    .rule('isOwner(\$context.userId, \$field.ownerId)')
    .description('Ownership check')
    .cacheable(true)
    .cacheDurationSeconds(300)
    .build();

final rbacConfig = RoleRequiredBuilder()
    .roles(['manager', 'director'])
    .strategy(RoleMatchStrategy.any)
    .description('Manager access')
    .build();

final policyConfig = AuthzPolicyBuilder('piiAccess')
    .type(AuthzPolicyType.rbac)
    .rule('hasRole(\$context, \'data_manager\')')
    .cacheDurationSeconds(3600)
    .auditLogging(true)
    .build();
```

## Serialization

All configuration classes implement `toMap()` for JSON serialization:

```dart
final config = AuthorizeConfig(
  rule: 'isOwner(\$context.userId, \$resource.ownerId)',
  description: 'Ownership check',
);

final map = config.toMap();
// {
//   'rule': 'isOwner(...)',
//   'policy': '',
//   'description': 'Ownership check',
//   'errorMessage': '',
//   'recursive': false,
//   'operations': '',
//   'cacheable': true,
//   'cacheDurationSeconds': 300,
// }
```

## Equality and Hashing

All configuration classes implement `==` operator and `hashCode`:

```dart
final config1 = AuthorizeConfig(rule: 'test_rule');
final config2 = AuthorizeConfig(rule: 'test_rule');

assert(config1 == config2);  // true
assert(config1.hashCode == config2.hashCode);  // true
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
| Scala | 30/30 ✅ |
| Groovy | 30/30 ✅ |
| Clojure | 30/30 ✅ |
| **Dart** | **30/30** ✅ |

## Documentation

- [DART_FEATURE_PARITY.md](./DART_FEATURE_PARITY.md) - Feature parity status
- [Dart Documentation](https://dart.dev/guides)
- [Pub Package Manager](https://pub.dev/)

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
- [FraiseQL Groovy](../fraiseql-groovy/)
- [FraiseQL Clojure](../fraiseql-clojure/)

---

**Phase 19** - Dart Feature Parity - Security Extensions ✅

All 30 features implemented with 100% parity across 15 languages.
