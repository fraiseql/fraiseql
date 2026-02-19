# FraiseQL Elixir

> **100% Feature Parity** with 15 other languages

Declarative, type-safe GraphQL schema authoring for Elixir with advanced authorization and security.

## Features

✅ **Custom Authorization Rules** - Expression-based with context variables
✅ **Role-Based Access Control (RBAC)** - Multiple roles with flexible strategies
✅ **Attribute-Based Access Control (ABAC)** - Conditional attribute evaluation
✅ **Authorization Policies** - Reusable policies (RBAC, ABAC, CUSTOM, HYBRID)
✅ **Caching** - Configurable TTL for authorization decisions
✅ **Audit Logging** - Comprehensive access decision tracking

## Requirements

- Elixir 1.14 or higher
- OTP 24 or higher
- Mix package manager

## Installation

Add to your `mix.exs`:

```elixir
def deps do
  [
    {:fraiseql, "~> 1.0.0"}
  ]
end
```

Then run:

```bash
mix deps.get
```

## Quick Start

```elixir
import FraiseQL.Security

# Custom authorization rule
config = authorize_config(
  rule: "isOwner($context.userId, $field.ownerId)",
  description: "Ensures users can only access their own notes",
  cacheable: true,
  cache_duration_seconds: 300
)

# Role-based access control
rbac_config = role_required_config(
  roles: ["manager", "director"],
  strategy: :any
)

# Authorization policy
policy_config = authz_policy_config("piiAccess",
  type: :rbac,
  rule: "hasRole($context, 'data_manager')",
  cache_duration_seconds: 3600
)

# Using builder pattern
builder_config = authorize_builder()
  |> Map.put(:rule, "isOwner(...)")
  |> Map.put(:description, "Ownership check")
  |> authorize()
```

## Authorization Patterns

### RBAC - Role-Based Access Control

```elixir
policy = authz_policy_config("adminOnly",
  type: :rbac,
  rule: "hasRole($context, 'admin')",
  audit_logging: true
)
```

### ABAC - Attribute-Based Access Control

```elixir
policy = authz_policy_config("secretClearance",
  type: :abac,
  attributes: ["clearance_level >= 3", "background_check == true"],
  description: "Requires top secret clearance"
)
```

### Hybrid Policies

```elixir
policy = authz_policy_config("auditAccess",
  type: :hybrid,
  rule: "hasRole($context, 'auditor')",
  attributes: ["audit_enabled == true"]
)
```

## Configuration Options

### authorize_config

```elixir
authorize_config(
  rule: "string",                    # Rule expression
  policy: "string",                  # Named policy reference
  description: "string",             # Description
  error_message: "string",           # Custom error message
  recursive: false,                  # Apply to nested types
  operations: "string",              # Specific operations
  cacheable: true,                   # Cache decisions
  cache_duration_seconds: 300        # Cache TTL in seconds
)
```

### role_required_config

```elixir
role_required_config(
  roles: ["string"],                 # Required roles
  strategy: :any,                    # :any, :all, :exactly
  hierarchy: false,                  # Role hierarchy
  description: "string",             # Description
  error_message: "string",           # Custom error
  operations: "string",              # Specific operations
  inherit: false,                    # Inherit from parent
  cacheable: true,                   # Cache results
  cache_duration_seconds: 300        # Cache TTL in seconds
)
```

### authz_policy_config

```elixir
authz_policy_config("policyName",
  type: :custom,                     # :rbac, :abac, :custom, :hybrid
  description: "string",             # Description
  rule: "string",                    # Rule expression
  attributes: ["string"],            # ABAC attributes
  cacheable: true,                   # Cache decisions
  cache_duration_seconds: 300,       # Cache TTL
  recursive: false,                  # Apply recursively
  operations: "string",              # Specific operations
  audit_logging: false,              # Log decisions
  error_message: "string"            # Custom error
)
```

## Role Matching Strategies

```elixir
:any         # At least one role
:all         # All roles required
:exactly     # Exactly these roles
```

## Policy Types

```elixir
:rbac        # Role-based
:abac        # Attribute-based
:custom      # Custom rules
:hybrid      # Combined approach
```

## Building & Testing

```bash
# Get dependencies
mix deps.get

# Run tests
mix test

# Run specific test file
mix test test/fraiseql/security_test.exs

# Run with coverage
mix test --cover

# Format code
mix format

# Analyze code with Credo
mix credo suggest
```

## Project Structure

```
fraiseql-elixir/
├── lib/
│   └── fraiseql/
│       └── security.ex             # Main security module
├── test/
│   └── fraiseql/
│       └── security_test.exs       # 44 comprehensive tests
├── mix.exs                         # Mix configuration
├── README.md                       # This file
└── ELIXIR_FEATURE_PARITY.md        # Feature parity status
```

## API Documentation

### authorize_config

Create custom authorization configuration:

```elixir
config = authorize_config(
  rule: "isOwner($context.userId, $resource.ownerId)",
  description: "Ownership check"
)
```

### role_required_config

Create RBAC configuration:

```elixir
config = role_required_config(
  roles: ["manager", "director"],
  strategy: :any
)
```

### authz_policy_config

Create authorization policy:

```elixir
policy = authz_policy_config("piiAccess",
  type: :rbac,
  rule: "hasRole($context, 'data_manager')"
)
```

### Builder Pattern

All configuration types support functional builders:

```elixir
config = authorize_builder()
  |> Map.put(:rule, "isOwner($context.userId, $field.ownerId)")
  |> Map.put(:description, "Ownership check")
  |> Map.put(:cacheable, true)
  |> Map.put(:cache_duration_seconds, 300)
  |> authorize()

rbac_config = role_required_builder()
  |> Map.put(:roles, ["manager", "director"])
  |> Map.put(:strategy, :any)
  |> Map.put(:description, "Manager access")
  |> build_roles()

policy_config = authz_policy_builder("piiAccess")
  |> Map.put(:type, :rbac)
  |> Map.put(:rule, "hasRole($context, 'data_manager')")
  |> Map.put(:cache_duration_seconds, 3600)
  |> Map.put(:audit_logging, true)
  |> build_policy()
```

### Helper Functions

```elixir
# Convert strategy atom to string
strategy_to_string(:any)        # "any"
strategy_to_string(:all)        # "all"
strategy_to_string(:exactly)    # "exactly"

# Convert policy type atom to string
policy_type_to_string(:rbac)    # "rbac"
policy_type_to_string(:abac)    # "abac"
policy_type_to_string(:custom)  # "custom"
policy_type_to_string(:hybrid)  # "hybrid"
```

## Type Specs

All functions include type specifications for compile-time type checking:

```elixir
@type role_match_strategy :: :any | :all | :exactly
@type authz_policy_type :: :rbac | :abac | :custom | :hybrid

@spec authorize_config(keyword()) :: authorize_config()
@spec role_required_config(keyword()) :: role_required_config()
@spec authz_policy_config(String.t(), keyword()) :: authz_policy_config()
```

Check types with:

```bash
mix dialyzer
```

## Pattern Matching

Leverage Elixir's pattern matching with authorization configs:

```elixir
# Destructure configuration
%{rule: rule, cacheable: cacheable} = authorize_config(
  rule: "test_rule",
  cacheable: true
)

# Pattern match in function
def handle_config(%{type: :rbac} = config) do
  # Handle RBAC policies
end

def handle_config(%{type: :abac} = config) do
  # Handle ABAC policies
end
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
| Dart | 30/30 ✅ |
| **Elixir** | **30/30** ✅ |

## Documentation

- [ELIXIR_FEATURE_PARITY.md](./ELIXIR_FEATURE_PARITY.md) - Feature parity status
- [Elixir Documentation](https://elixir-lang.org/)
- [Mix Build Tool](https://hexdocs.pm/mix/)
- [ExUnit Testing](https://hexdocs.pm/ex_unit/)

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
- [FraiseQL Dart](../fraiseql-dart/)

---

**Phase 20** - Elixir Feature Parity - Security Extensions ✅

All 30 features implemented with 100% parity across 16 languages.
