# FraiseQL Clojure

> **100% Feature Parity** with 13 other languages

Declarative, type-safe GraphQL schema authoring for Clojure with advanced authorization and security.

## Features

✅ **Custom Authorization Rules** - Expression-based with context variables
✅ **Role-Based Access Control (RBAC)** - Multiple roles with flexible strategies
✅ **Attribute-Based Access Control (ABAC)** - Conditional attribute evaluation
✅ **Authorization Policies** - Reusable policies (RBAC, ABAC, CUSTOM, HYBRID)
✅ **Caching** - Configurable TTL for authorization decisions
✅ **Audit Logging** - Comprehensive access decision tracking

## Requirements

- Clojure 1.11+
- Leiningen 2.9+
- Java 11+

## Installation

Add to your `project.clj`:

```clojure
:dependencies [[com.fraiseql/fraiseql-clojure "1.0.0"]]
```

## Quick Start

```clojure
(ns myapp.security
  (:require [fraiseql.security :as security]))

; Custom authorization rule
(let [config (security/authorize-config
               :rule "isOwner($context.userId, $field.ownerId)"
               :description "Ensures users can only access their own notes"
               :cacheable true
               :cache-duration 300)]
  config)

; Role-based access control
(let [config (security/role-required-config
               :roles ["manager" "director"]
               :strategy security/ROLE_MATCH_ANY)]
  config)

; Authorization policy
(let [config (security/authz-policy-config "piiAccess"
               :type security/AUTHZ_POLICY_RBAC
               :rule "hasRole($context, 'data_manager')"
               :cache-duration 3600)]
  config)

; Using builder pattern
(let [config (-> (security/authorize-builder)
                 (assoc :rule "isOwner(...)")
                 (assoc :description "Ownership check")
                 security/authorize)]
  config)
```

## Authorization Patterns

### RBAC - Role-Based Access Control

```clojure
(let [policy (security/authz-policy-config "adminOnly"
               :type security/AUTHZ_POLICY_RBAC
               :rule "hasRole($context, 'admin')"
               :audit-logging true)]
  policy)
```

### ABAC - Attribute-Based Access Control

```clojure
(let [policy (security/authz-policy-config "secretClearance"
               :type security/AUTHZ_POLICY_ABAC
               :attributes ["clearance_level >= 3" "background_check == true"]
               :description "Requires top secret clearance")]
  policy)
```

### Hybrid Policies

```clojure
(let [policy (security/authz-policy-config "auditAccess"
               :type security/AUTHZ_POLICY_HYBRID
               :rule "hasRole($context, 'auditor')"
               :attributes ["audit_enabled == true"])]
  policy)
```

## Configuration Options

### authorize-config

```clojure
(security/authorize-config
  :rule string                    ; Rule expression
  :policy string                  ; Named policy reference
  :description string             ; Description
  :error-message string           ; Custom error message
  :recursive boolean              ; Apply to nested types
  :operations string              ; Specific operations
  :cacheable boolean              ; Cache decisions
  :cache-duration int)            ; Cache TTL in seconds
```

### role-required-config

```clojure
(security/role-required-config
  :roles [strings]                ; Required roles
  :strategy keyword               ; :any, :all, :exactly
  :hierarchy boolean              ; Role hierarchy
  :description string             ; Description
  :error-message string           ; Custom error
  :operations string              ; Specific operations
  :inherit boolean                ; Inherit from parent
  :cacheable boolean              ; Cache results
  :cache-duration int)            ; Cache TTL in seconds
```

### authz-policy-config

```clojure
(security/authz-policy-config name
  :type keyword                   ; :rbac, :abac, :custom, :hybrid
  :description string             ; Description
  :rule string                    ; Rule expression
  :attributes [strings]           ; ABAC attributes
  :cacheable boolean              ; Cache decisions
  :cache-duration int             ; Cache TTL
  :recursive boolean              ; Apply recursively
  :operations string              ; Specific operations
  :audit-logging boolean          ; Log decisions
  :error-message string)          ; Custom error
```

## Role Matching Strategies

```clojure
security/ROLE_MATCH_ANY        ; At least one role
security/ROLE_MATCH_ALL        ; All roles required
security/ROLE_MATCH_EXACTLY    ; Exactly these roles
```

## Policy Types

```clojure
security/AUTHZ_POLICY_RBAC     ; Role-based
security/AUTHZ_POLICY_ABAC     ; Attribute-based
security/AUTHZ_POLICY_CUSTOM   ; Custom rules
security/AUTHZ_POLICY_HYBRID   ; Combined approach
```

## Building & Testing

```bash
# Build
lein compile

# Run tests
lein test

# Run specific test namespace
lein test fraiseql.security-test

# Interactive REPL
lein repl
```

## Project Structure

```
fraiseql-clojure/
├── src/
│   └── fraiseql/
│       └── security.clj          # Main security module
├── test/
│   └── fraiseql/
│       └── security_test.clj     # 44 comprehensive tests
├── project.clj                   # Leiningen manifest
├── README.md                     # This file
└── CLOJURE_FEATURE_PARITY.md    # Feature parity status
```

## API Documentation

### authorize-config

Create custom authorization configuration:

```clojure
(security/authorize-config
  :rule "isOwner($context.userId, $resource.ownerId)"
  :description "Ownership check")
```

### role-required-config

Create RBAC configuration:

```clojure
(security/role-required-config
  :roles ["manager" "director"]
  :strategy security/ROLE_MATCH_ANY)
```

### authz-policy-config

Create authorization policy:

```clojure
(security/authz-policy-config "piiAccess"
  :type security/AUTHZ_POLICY_RBAC
  :rule "hasRole($context, 'data_manager')")
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
| **Clojure** | **30/30** ✅ |

## Documentation

- [CLOJURE_FEATURE_PARITY.md](./CLOJURE_FEATURE_PARITY.md) - Feature parity status
- [Clojure Documentation](https://clojure.org/guides/getting_started)
- [Leiningen Guide](https://leiningen.org/)

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

---

**Phase 18** - Clojure Feature Parity - Security Extensions ✅

All 30 features implemented with 100% parity across 14 languages.
