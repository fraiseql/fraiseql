# FraiseQL Ruby

> Not conformance-tested; see [sdks/community/README.md](../README.md).

Declarative, type-safe GraphQL schema authoring for Ruby with advanced authorization and security.

## Features

### Authorization & Security

✅ **Custom Authorization Rules** - Expression-based authorization with context variables
✅ **Role-Based Access Control (RBAC)** - Multiple roles with flexible matching strategies
✅ **Attribute-Based Access Control (ABAC)** - Conditional attribute evaluation
✅ **Authorization Policies** - Reusable policies (RBAC, ABAC, CUSTOM, HYBRID)
✅ **Caching** - Configurable TTL for authorization decisions
✅ **Audit Logging** - Comprehensive access decision tracking

## Installation

Add to your Gemfile:

```ruby
gem 'fraiseql-ruby'
```

Then run:

```bash
bundle install
```

Or install directly:

```bash
gem install fraiseql-ruby
```

## Quick Start

### Custom Authorization Rules

```ruby
class ProtectedNote
  include FraiseQL::Security::Authorize

  authorize rule: "isOwner($context.userId, $field.ownerId)",
            description: "Ensures users can only access their own notes"
end

# Or using the builder
config = FraiseQL::Security::AuthorizeBuilder.create
  .rule("isOwner($context.userId, $field.ownerId)")
  .description("Ensures users can only access their own notes")
  .cacheable(true)
  .cache_duration_seconds(300)
  .build
```

### Role-Based Access Control

```ruby
class SalaryData
  include FraiseQL::Security::RoleRequired

  require_role roles: ['manager', 'director'],
               strategy: FraiseQL::Security::RoleMatchStrategy::ANY,
               description: "Managers and directors can view salaries"
end

# Or using the builder
config = FraiseQL::Security::RoleRequiredBuilder.create
  .roles('manager', 'director')
  .strategy(FraiseQL::Security::RoleMatchStrategy::ANY)
  .description("Managers and directors can view salaries")
  .build
```

### Authorization Policies

```ruby
class Customer
  include FraiseQL::Security::AuthzPolicy

  authz_policy name: 'piiAccess',
               type: FraiseQL::Security::AuthzPolicyType::RBAC,
               rule: "hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')",
               description: "Access to Personally Identifiable Information"
end

# Or using the builder
policy = FraiseQL::Security::AuthzPolicyBuilder.create('piiAccess')
  .type(FraiseQL::Security::AuthzPolicyType::RBAC)
  .rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
  .description("Access to Personally Identifiable Information")
  .cacheable(true)
  .audit_logging(true)
  .build
```

## Authorization Patterns

### RBAC - Role-Based Access Control

```ruby
admin_policy = FraiseQL::Security::AuthzPolicyBuilder.create('adminOnly')
  .type(FraiseQL::Security::AuthzPolicyType::RBAC)
  .rule("hasRole($context, 'admin')")
  .audit_logging(true)
  .build
```

### ABAC - Attribute-Based Access Control

```ruby
clearance_policy = FraiseQL::Security::AuthzPolicyBuilder.create('secretClearance')
  .type(FraiseQL::Security::AuthzPolicyType::ABAC)
  .attributes('clearance_level >= 3', 'background_check == true')
  .description('Requires top secret clearance')
  .build
```

### Hybrid Policies

```ruby
audit_policy = FraiseQL::Security::AuthzPolicyBuilder.create('auditAccess')
  .type(FraiseQL::Security::AuthzPolicyType::HYBRID)
  .rule("hasRole($context, 'auditor')")
  .attributes('audit_enabled == true')
  .description('Role and attribute-based access')
  .build
```

## Configuration Options

### AuthorizeBuilder

```ruby
FraiseQL::Security::AuthorizeBuilder.create
  .rule(string)                      # Rule expression
  .policy(string)                    # Named policy reference
  .description(string)               # Description
  .error_message(string)             # Custom error message
  .recursive(boolean)                # Apply to nested types
  .operations(string)                # Specific operations
  .cacheable(boolean)                # Cache decisions
  .cache_duration_seconds(integer)   # Cache TTL
  .build
```

### RoleRequiredBuilder

```ruby
FraiseQL::Security::RoleRequiredBuilder.create
  .roles(*string)                    # Required roles (variadic)
  .roles_array(array)                # Roles from array
  .strategy(strategy)                # ANY, ALL, EXACTLY
  .hierarchy(boolean)                # Role hierarchy
  .description(string)               # Description
  .error_message(string)             # Custom error
  .operations(string)                # Specific operations
  .inherit(boolean)                  # Inherit from parent
  .cacheable(boolean)                # Cache results
  .cache_duration_seconds(integer)   # Cache TTL
  .build
```

### AuthzPolicyBuilder

```ruby
FraiseQL::Security::AuthzPolicyBuilder.create(name)
  .description(string)               # Description
  .rule(string)                      # Rule expression
  .attributes(*string)               # ABAC attributes (variadic)
  .attributes_array(array)           # Attributes from array
  .type(type)                        # RBAC/ABAC/CUSTOM/HYBRID
  .cacheable(boolean)                # Cache decisions
  .cache_duration_seconds(integer)   # Cache TTL
  .recursive(boolean)                # Apply recursively
  .operations(string)                # Specific operations
  .audit_logging(boolean)            # Log decisions
  .error_message(string)             # Custom error
  .build
```

## Role Matching Strategies

```ruby
FraiseQL::Security::RoleMatchStrategy::ANY      # At least one role
FraiseQL::Security::RoleMatchStrategy::ALL      # All roles required
FraiseQL::Security::RoleMatchStrategy::EXACTLY  # Exactly these roles
```

## Policy Types

```ruby
FraiseQL::Security::AuthzPolicyType::RBAC       # Role-based
FraiseQL::Security::AuthzPolicyType::ABAC       # Attribute-based
FraiseQL::Security::AuthzPolicyType::CUSTOM     # Custom rules
FraiseQL::Security::AuthzPolicyType::HYBRID     # Combined approach
```

## Testing

```bash
# Run all tests
bundle exec rspec

# Run with verbose output
bundle exec rspec -fd

# Run specific file
bundle exec rspec spec/authorization_spec.rb

# Run with coverage
bundle exec rspec --coverage
```

## Code Quality

```bash
# Run RuboCop linting
bundle exec rubocop

# Auto-fix issues
bundle exec rubocop -A

# Run full test suite with coverage
bundle exec rake
```

## API Documentation

### AuthorizeBuilder

Fluent API for custom authorization rules:

```ruby
FraiseQL::Security::AuthorizeBuilder.create
  .rule(rule: string)
  .policy(policy: string)
  .description(description: string)
  .error_message(msg: string)
  .recursive(flag: boolean)
  .operations(ops: string)
  .cacheable(flag: boolean)
  .cache_duration_seconds(duration: integer)
  .build
```

### RoleRequiredBuilder

Fluent API for RBAC rules:

```ruby
FraiseQL::Security::RoleRequiredBuilder.create
  .roles(*roles: string)
  .roles_array(roles: array)
  .strategy(strat: strategy)
  .hierarchy(flag: boolean)
  .description(desc: string)
  .error_message(msg: string)
  .operations(ops: string)
  .inherit(flag: boolean)
  .cacheable(flag: boolean)
  .cache_duration_seconds(duration: integer)
  .build
```

### AuthzPolicyBuilder

Fluent API for authorization policies:

```ruby
FraiseQL::Security::AuthzPolicyBuilder.create(name: string)
  .description(desc: string)
  .rule(rule: string)
  .attributes(*attrs: string)
  .attributes_array(attrs: array)
  .type(type: type)
  .cacheable(flag: boolean)
  .cache_duration_seconds(duration: integer)
  .recursive(flag: boolean)
  .operations(ops: string)
  .audit_logging(flag: boolean)
  .error_message(msg: string)
  .build
```

## Conformance

Not conformance-tested; see [sdks/community/README.md](../README.md).

## Documentation

- [Ruby Documentation](https://ruby-doc.org/) - Language documentation
- [RSpec Documentation](https://rspec.info/) - Testing framework

## License

Apache License 2.0

## Contributing

Contributions are welcome! Please ensure:

- All tests pass: `bundle exec rspec`
- Code is clean: `bundle exec rubocop -A`
- Tests have good coverage

## See Also

- [FraiseQL Python](../fraiseql-python/)
- [FraiseQL TypeScript](../fraiseql-typescript/)
- [FraiseQL Java](../fraiseql-java/)
- [FraiseQL Go](../fraiseql-go/)
- [FraiseQL PHP](../fraiseql-php/)
- [FraiseQL Node.js](../fraiseql-nodejs/)
