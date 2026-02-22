# fraiseql-rust

RBAC and authorization primitives for FraiseQL schema authoring in Rust.

This crate provides the security layer used when defining FraiseQL schemas:
role-based access control, attribute-based policies, field-level scopes, and
scope format validation. It is **not** a full schema authoring SDK — type and
query definition are handled by the other language SDKs (Python, TypeScript,
Go, Java, PHP).

## Usage

```rust
use fraiseql_rust::{
    AuthorizeBuilder, RoleRequiredBuilder, RoleMatchStrategy,
    AuthzPolicyBuilder, AuthzPolicyType,
    Field, validate_scope,
};

// Field-level scope requirement
let field = Field::new("salary", "Int")
    .with_requires_scope(Some("read:User.salary".to_string()));

// Role-based access control
let admin_only = RoleRequiredBuilder::new()
    .roles(vec!["admin"])
    .strategy(RoleMatchStrategy::Any)
    .cacheable(true)
    .build();

// Custom authorization rule
let ownership_check = AuthorizeBuilder::new()
    .rule("isOwner($context.userId, $field.ownerId)")
    .description("Owner-only access")
    .build();

// ABAC policy
let policy = AuthzPolicyBuilder::new("content_access")
    .policy_type(AuthzPolicyType::ResourceBased)
    .condition("$context.subscription == 'premium'")
    .build();

// Scope validation
assert!(validate_scope("read:User.email").is_ok());
assert!(validate_scope("invalid").is_err());
```

## What's included

| Module | Contents |
|--------|----------|
| `authorization` | `AuthorizeConfig`, `AuthorizeBuilder` — custom rule expressions |
| `roles` | `RoleRequiredConfig`, `RoleRequiredBuilder`, `RoleMatchStrategy` — RBAC |
| `policies` | `AuthzPolicyConfig`, `AuthzPolicyBuilder`, `AuthzPolicyType` — ABAC |
| `field` | `Field` — field definition with scope metadata |
| `schema` | `SchemaRegistry`, `validate_scope`, `ScopeValidationError` |

## Full authoring SDKs

For defining types, queries, and mutations use one of the production-ready SDKs:

- [fraiseql-python](../fraiseql-python) — reference implementation
- [fraiseql-typescript](../fraiseql-typescript)
- [fraiseql-java](../fraiseql-java)
- [fraiseql-php](../fraiseql-php)
- [fraiseql-go](../fraiseql-go)
