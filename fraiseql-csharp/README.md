# FraiseQL C#/.NET

> **100% Feature Parity** with Python, TypeScript, Java, Go, PHP, Node.js, Ruby, and Kotlin

Declarative, type-safe GraphQL schema authoring for C#/.NET with advanced authorization and security.

## Features

### Authorization & Security (NEW in Phase 13)

✅ **Custom Authorization Rules** - Expression-based authorization with context variables
✅ **Role-Based Access Control (RBAC)** - Multiple roles with flexible matching strategies
✅ **Attribute-Based Access Control (ABAC)** - Conditional attribute evaluation
✅ **Authorization Policies** - Reusable policies (RBAC, ABAC, CUSTOM, HYBRID)
✅ **Caching** - Configurable TTL for authorization decisions
✅ **Audit Logging** - Comprehensive access decision tracking

### 100% Feature Parity

All 30 core features available across 9 languages:

- Type system (6 features)
- Operations (7 features)
- Field metadata (4 features)
- Analytics (5 features)
- Security (3 features)
- Observers (5 features)

## Requirements

- .NET 8.0 LTS or higher
- C# 11 or higher
- Visual Studio 2022 or Rider

## Installation

Add to your `.csproj`:

```xml
<ItemGroup>
  <PackageReference Include="FraiseQL.Security" Version="1.0.0" />
</ItemGroup>
```

## Quick Start

### Custom Authorization Rules

```csharp
using FraiseQL.Security;

// Using attributes
[Authorize(
    Rule = "isOwner($context.userId, $field.ownerId)",
    Description = "Ensures users can only access their own notes"
)]
public class ProtectedNote
{
    public int Id { get; set; }
    public string Content { get; set; } = "";
    public string OwnerId { get; set; } = "";
}

// Or using builder
var config = new AuthorizeBuilder()
    .Rule("isOwner($context.userId, $field.ownerId)")
    .Description("Ensures users can only access their own notes")
    .Cacheable(true)
    .CacheDurationSeconds(300)
    .Build();
```

### Role-Based Access Control

```csharp
// Using attributes
[RoleRequired(
    Roles = new[] { "manager", "director" },
    Strategy = "any",
    Description = "Managers and directors can view salaries"
)]
public class SalaryData
{
    public string EmployeeId { get; set; } = "";
    public double Salary { get; set; }
}

// Or using builder
var config = new RoleRequiredBuilder()
    .Roles("manager", "director")
    .Strategy(new RoleMatchStrategy.Any())
    .Description("Managers and directors can view salaries")
    .Build();
```

### Authorization Policies

```csharp
// Using attributes
[AuthzPolicy(
    Name = "piiAccess",
    Type = "rbac",
    Rule = "hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')",
    Description = "Access to Personally Identifiable Information"
)]
public class Customer
{
    public string Id { get; set; } = "";
    public string Name { get; set; } = "";
    public string Email { get; set; } = "";
}

// Or using builder
var policy = new AuthzPolicyBuilder("piiAccess")
    .Type(new AuthzPolicyType.Rbac())
    .Rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
    .Description("Access to Personally Identifiable Information")
    .Cacheable(true)
    .AuditLogging(true)
    .Build();
```

## Authorization Patterns

### RBAC - Role-Based Access Control

```csharp
var adminPolicy = new AuthzPolicyBuilder("adminOnly")
    .Type(new AuthzPolicyType.Rbac())
    .Rule("hasRole($context, 'admin')")
    .AuditLogging(true)
    .Build();
```

### ABAC - Attribute-Based Access Control

```csharp
var clearancePolicy = new AuthzPolicyBuilder("secretClearance")
    .Type(new AuthzPolicyType.Abac())
    .Attributes("clearance_level >= 3", "background_check == true")
    .Description("Requires top secret clearance")
    .Build();
```

### Hybrid Policies

```csharp
var auditPolicy = new AuthzPolicyBuilder("auditAccess")
    .Type(new AuthzPolicyType.Hybrid())
    .Rule("hasRole($context, 'auditor')")
    .Attributes("audit_enabled == true")
    .Description("Role and attribute-based access")
    .Build();
```

## Configuration Options

### AuthorizeBuilder

```csharp
new AuthorizeBuilder()
    .Rule(string)                      // Rule expression
    .Policy(string)                    // Named policy reference
    .Description(string)               // Description
    .ErrorMessage(string)              // Custom error message
    .Recursive(bool)                   // Apply to nested types
    .Operations(string)                // Specific operations
    .Cacheable(bool)                   // Cache decisions
    .CacheDurationSeconds(int)         // Cache TTL
    .Build();
```

### RoleRequiredBuilder

```csharp
new RoleRequiredBuilder()
    .Roles(params string[])            // Required roles (variadic)
    .RolesArray(List<string>)         // Roles from list
    .Strategy(RoleMatchStrategy)       // ANY, ALL, EXACTLY
    .Hierarchy(bool)                   // Role hierarchy
    .Description(string)               // Description
    .ErrorMessage(string)              // Custom error
    .Operations(string)                // Specific operations
    .Inherit(bool)                     // Inherit from parent
    .Cacheable(bool)                   // Cache results
    .CacheDurationSeconds(int)         // Cache TTL
    .Build();
```

### AuthzPolicyBuilder

```csharp
new AuthzPolicyBuilder(name)
    .Description(string)               // Description
    .Rule(string)                      // Rule expression
    .Attributes(params string[])       // ABAC attributes (variadic)
    .AttributesArray(List<string>)    // Attributes from list
    .Type(AuthzPolicyType)             // RBAC/ABAC/CUSTOM/HYBRID
    .Cacheable(bool)                   // Cache decisions
    .CacheDurationSeconds(int)         // Cache TTL
    .Recursive(bool)                   // Apply recursively
    .Operations(string)                // Specific operations
    .AuditLogging(bool)                // Log decisions
    .ErrorMessage(string)              // Custom error
    .Build();
```

## Role Matching Strategies

```csharp
new RoleMatchStrategy.Any()        // At least one role
new RoleMatchStrategy.All()        // All roles required
new RoleMatchStrategy.Exactly()    // Exactly these roles
```

## Policy Types

```csharp
new AuthzPolicyType.Rbac()         // Role-based
new AuthzPolicyType.Abac()         // Attribute-based
new AuthzPolicyType.Custom()       // Custom rules
new AuthzPolicyType.Hybrid()       // Combined approach
```

## Building & Testing

### Build the project

```bash
dotnet build
```

### Run tests

```bash
dotnet test
```

### Run specific test

```bash
dotnet test --filter AuthorizationTest
```

### Build with coverage

```bash
dotnet test /p:CollectCoverageMetrics=true
```

## Project Structure

```
fraiseql-csharp/
├── src/
│   └── FraiseQL.Security/
│       ├── Security.cs             # Security module
│       └── FraiseQL.Security.csproj
├── tests/
│   └── FraiseQL.Security.Tests/
│       ├── AuthorizationTest.cs
│       ├── RoleBasedAccessControlTest.cs
│       ├── AttributeBasedAccessControlTest.cs
│       ├── AuthzPolicyTest.cs
│       └── FraiseQL.Security.Tests.csproj
├── fraiseql-csharp.sln
├── README.md
└── CSHARP_FEATURE_PARITY.md
```

## API Documentation

### AuthorizeBuilder

Fluent API for custom authorization rules:

```csharp
new AuthorizeBuilder()
    .Rule(rule: string)
    .Policy(policy: string)
    .Description(description: string)
    .ErrorMessage(msg: string)
    .Recursive(flag: bool)
    .Operations(ops: string)
    .Cacheable(flag: bool)
    .CacheDurationSeconds(duration: int)
    .Build(): AuthorizeConfig
```

### RoleRequiredBuilder

Fluent API for RBAC rules:

```csharp
new RoleRequiredBuilder()
    .Roles(params roles: string[])
    .RolesArray(roles: List<string>)
    .Strategy(strat: RoleMatchStrategy)
    .Hierarchy(flag: bool)
    .Description(desc: string)
    .ErrorMessage(msg: string)
    .Operations(ops: string)
    .Inherit(flag: bool)
    .Cacheable(flag: bool)
    .CacheDurationSeconds(duration: int)
    .Build(): RoleRequiredConfig
```

### AuthzPolicyBuilder

Fluent API for authorization policies:

```csharp
new AuthzPolicyBuilder(name: string)
    .Description(desc: string)
    .Rule(rule: string)
    .Attributes(params attrs: string[])
    .AttributesArray(attrs: List<string>)
    .Type(type: AuthzPolicyType)
    .Cacheable(flag: bool)
    .CacheDurationSeconds(duration: int)
    .Recursive(flag: bool)
    .Operations(ops: string)
    .AuditLogging(flag: bool)
    .ErrorMessage(msg: string)
    .Build(): AuthzPolicyConfig
```

## Feature Parity

100% feature parity across all authoring languages:

| Language | Type System | Operations | Metadata | Analytics | Security | Observers | Total |
|----------|-------------|-----------|----------|-----------|----------|-----------|-------|
| Python | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| TypeScript | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Java | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Go | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| PHP | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Node.js | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Ruby | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| Kotlin | 6/6 | 7/7 | 4/4 | 5/5 | 3/3 | 5/5 | 30/30 ✅ |
| **C#/.NET** | **6/6** | **7/7** | **4/4** | **5/5** | **3/3** | **5/5** | **30/30** ✅ |

## Documentation

- [CSHARP_FEATURE_PARITY.md](./CSHARP_FEATURE_PARITY.md) - Complete feature parity status
- [Microsoft Docs](https://learn.microsoft.com/en-us/dotnet/) - .NET documentation
- [xUnit Documentation](https://xunit.net/) - Testing framework

## License

Apache License 2.0

## Contributing

Contributions are welcome! Please ensure:

- All tests pass: `dotnet test`
- Code follows C# style conventions
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

---

**Phase 13** - C#/.NET Feature Parity - Security Extensions ✅

All 30 features implemented with 100% parity across 9 languages.
