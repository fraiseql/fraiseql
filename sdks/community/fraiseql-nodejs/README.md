# FraiseQL Node.js

> **100% Feature Parity** with Python, TypeScript, Java, Go, and PHP

Declarative, type-safe GraphQL schema authoring for Node.js with advanced authorization and security.

## Features

### Authorization & Security (NEW in Phase 10)

✅ **Custom Authorization Rules** - Expression-based authorization with context variables
✅ **Role-Based Access Control (RBAC)** - Multiple roles with flexible matching strategies
✅ **Attribute-Based Access Control (ABAC)** - Conditional attribute evaluation
✅ **Authorization Policies** - Reusable policies (RBAC, ABAC, CUSTOM, HYBRID)
✅ **Caching** - Configurable TTL for authorization decisions
✅ **Audit Logging** - Comprehensive access decision tracking

### 100% Feature Parity

All 30 core features available across 6 languages:

- Type system (6 features)
- Operations (7 features)
- Field metadata (4 features)
- Analytics (5 features)
- Security (3 features)
- Observers (5 features)

## Installation

```bash
npm install fraiseql-nodejs
# or
yarn add fraiseql-nodejs
```

## Quick Start

### Custom Authorization Rules

```typescript
import { AuthorizeBuilder } from 'fraiseql-nodejs';

const ownershipCheck = new AuthorizeBuilder()
  .rule("isOwner($context.userId, $field.ownerId)")
  .description("Ensures users can only access their own notes")
  .cacheable(true)
  .cacheDurationSeconds(300)
  .build();
```

### Role-Based Access Control

```typescript
import { RoleRequiredBuilder, RoleMatchStrategy } from 'fraiseql-nodejs';

const salaryAccess = new RoleRequiredBuilder()
  .roles('manager', 'director')
  .strategy(RoleMatchStrategy.ANY)
  .description("Managers and directors can view salaries")
  .build();
```

### Authorization Policies

```typescript
import { AuthzPolicyBuilder, AuthzPolicyType } from 'fraiseql-nodejs';

const piiPolicy = new AuthzPolicyBuilder('piiAccess')
  .type(AuthzPolicyType.RBAC)
  .rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
  .description("Access to Personally Identifiable Information")
  .cacheable(true)
  .auditLogging(true)
  .build();
```

### Using Decorators

```typescript
import { Authorize, RoleRequired, AuthzPolicy, RoleMatchStrategy, AuthzPolicyType } from 'fraiseql-nodejs';

@Authorize({
  rule: "isOwner($context.userId, $field.ownerId)",
  description: "Ownership check"
})
class ProtectedNote {
  id: number;
  content: string;
  ownerId: string;
}

@RoleRequired({
  roles: ['manager', 'director'],
  strategy: RoleMatchStrategy.ANY,
  description: "Management access"
})
class SalaryData {
  employeeId: string;
  salary: number;
}

@AuthzPolicy({
  name: 'piiAccess',
  type: AuthzPolicyType.RBAC,
  rule: "hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')",
  description: "Access to Personally Identifiable Information"
})
class Customer {
  id: string;
  name: string;
  email: string;
}
```

## Authorization Patterns

### RBAC - Role-Based Access Control

```typescript
const adminAccess = new AuthzPolicyBuilder('adminOnly')
  .type(AuthzPolicyType.RBAC)
  .rule("hasRole($context, 'admin')")
  .auditLogging(true)
  .build();
```

### ABAC - Attribute-Based Access Control

```typescript
const secretClearance = new AuthzPolicyBuilder('secretClearance')
  .type(AuthzPolicyType.ABAC)
  .attributes('clearance_level >= 3', 'background_check == true')
  .description('Requires top secret clearance')
  .build();
```

### Hybrid Policies

```typescript
const auditAccess = new AuthzPolicyBuilder('auditAccess')
  .type(AuthzPolicyType.HYBRID)
  .rule("hasRole($context, 'auditor')")
  .attributes('audit_enabled == true')
  .description('Role and attribute-based access')
  .build();
```

## Configuration Options

### AuthorizeConfig

```typescript
{
  rule?: string;                    // Rule expression
  policy?: string;                  // Named policy reference
  description?: string;             // Description
  errorMessage?: string;            // Custom error message
  recursive?: boolean;              // Apply to nested types
  operations?: string;              // Specific operations
  cacheable?: boolean;              // Cache decisions
  cacheDurationSeconds?: number;    // Cache TTL
}
```

### RoleRequiredConfig

```typescript
{
  roles?: string[];                     // Required roles
  strategy?: RoleMatchStrategy;         // ANY, ALL, EXACTLY
  hierarchy?: boolean;                  // Role hierarchy
  description?: string;                 // Description
  errorMessage?: string;                // Custom error
  operations?: string;                  // Specific operations
  inherit?: boolean;                    // Inherit from parent
  cacheable?: boolean;                  // Cache results
  cacheDurationSeconds?: number;        // Cache TTL
}
```

### AuthzPolicyConfig

```typescript
{
  name: string;                         // Policy name (required)
  description?: string;                 // Description
  rule?: string;                        // Rule expression
  attributes?: string[];                // ABAC attributes
  type?: AuthzPolicyType;               // RBAC/ABAC/CUSTOM/HYBRID
  cacheable?: boolean;                  // Cache decisions
  cacheDurationSeconds?: number;        // Cache TTL
  recursive?: boolean;                  // Apply recursively
  operations?: string;                  // Specific operations
  auditLogging?: boolean;               // Log decisions
  errorMessage?: string;                // Custom error
}
```

## Role Matching Strategies

```typescript
enum RoleMatchStrategy {
  ANY = 'any',          // User needs at least one role
  ALL = 'all',          // User needs all roles
  EXACTLY = 'exactly',  // User needs exactly these roles
}
```

## Policy Types

```typescript
enum AuthzPolicyType {
  RBAC = 'rbac',        // Role-based
  ABAC = 'abac',        // Attribute-based
  CUSTOM = 'custom',    // Custom rules
  HYBRID = 'hybrid',    // Combined approach
}
```

## Testing

```bash
# Run tests
npm test

# Watch mode
npm run test:watch

# Coverage report
npm run test:coverage
```

## Building

```bash
# Compile TypeScript
npm run build

# Clean build artifacts
npm run clean
```

## Linting & Formatting

```bash
# Lint code
npm run lint

# Format code
npm run format
```

## API Documentation

### AuthorizeBuilder

Fluent API for custom authorization rules:

```typescript
new AuthorizeBuilder()
  .rule(rule: string)
  .policy(policy: string)
  .description(description: string)
  .errorMessage(errorMessage: string)
  .recursive(recursive: boolean)
  .operations(operations: string)
  .cacheable(cacheable: boolean)
  .cacheDurationSeconds(duration: number)
  .build(): AuthorizeConfig
```

### RoleRequiredBuilder

Fluent API for RBAC rules:

```typescript
new RoleRequiredBuilder()
  .roles(...roles: string[])
  .rolesArray(roles: string[])
  .strategy(strategy: RoleMatchStrategy)
  .hierarchy(hierarchy: boolean)
  .description(description: string)
  .errorMessage(errorMessage: string)
  .operations(operations: string)
  .inherit(inherit: boolean)
  .cacheable(cacheable: boolean)
  .cacheDurationSeconds(duration: number)
  .build(): RoleRequiredConfig
```

### AuthzPolicyBuilder

Fluent API for authorization policies:

```typescript
new AuthzPolicyBuilder(name: string)
  .description(description: string)
  .rule(rule: string)
  .attributes(...attributes: string[])
  .attributesArray(attributes: string[])
  .type(type: AuthzPolicyType)
  .cacheable(cacheable: boolean)
  .cacheDurationSeconds(duration: number)
  .recursive(recursive: boolean)
  .operations(operations: string)
  .auditLogging(auditLogging: boolean)
  .errorMessage(errorMessage: string)
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
| **Node.js** | **6/6** | **7/7** | **4/4** | **5/5** | **3/3** | **5/5** | **30/30** ✅ |

## Documentation

- [NODEJS_FEATURE_PARITY.md](./NODEJS_FEATURE_PARITY.md) - Complete feature parity status
- [TypeScript Documentation](https://www.typescriptlang.org/) - Language documentation
- [Jest Documentation](https://jestjs.io/) - Testing framework

## License

Apache License 2.0

## Contributing

Contributions are welcome! Please ensure:

- All tests pass: `npm test`
- Code is formatted: `npm run format`
- No linting issues: `npm run lint`
- TypeScript is strict: `npm run build`

## See Also

- [FraiseQL Python](../fraiseql-python/)
- [FraiseQL TypeScript](../fraiseql-typescript/)
- [FraiseQL Java](../fraiseql-java/)
- [FraiseQL Go](../fraiseql-go/)
- [FraiseQL PHP](../fraiseql-php/)

---

**Phase 10** - Node.js Feature Parity - Security Extensions ✅

All 30 features implemented with 100% parity across 6 languages.
