# FraiseQL C# SDK Reference

**Status**: Production-Ready | **C# Version**: 11+ | **.NET**: 8.0+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL C# SDK. Provides record types, nullable reference types, and modern async patterns for defining type-safe GraphQL APIs. C# authoring only—compiles to optimized SQL, no runtime FFI or native bindings.

## Installation

```bash
# Via NuGet Package Manager
Install-Package FraiseQL

# Via dotnet CLI (recommended)
dotnet add package FraiseQL

# Via Package Reference (.csproj)
<ItemGroup>
  <PackageReference Include="FraiseQL" Version="2.0.*" />
</ItemGroup>
```

**Requirements**:

- .NET 8.0 or later
- C# 11+ (record types, required properties, file-scoped namespaces)
- Nullable reference types enabled: `#nullable enable` in `.csproj` or `Directory.Build.props`
- Visual Studio 2022 / JetBrains Rider recommended

**Recommended Project Setup**:

```xml
<!-- YourProject.csproj -->
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <Nullable>enable</Nullable>
    <LangVersion>latest</LangVersion>
    <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="FraiseQL" Version="2.0.*" />
    <PackageReference Include="FraiseQL.SourceGenerators" Version="2.0.*" />
  </ItemGroup>
</Project>
```

---

## Quick Reference Table

| Feature | Method / Attribute | Purpose | Async |
|---------|------------------|---------|-------|
| **Types** | `[FraiseQLType]` record | GraphQL object types | — |
| **Queries** | `[Query]` method | Read operations (SELECT) | ✓ Task-based |
| **Mutations** | `[Mutation]` method | Write operations (INSERT/UPDATE/DELETE) | ✓ Task-based |
| **Fact Tables** | `[FactTable]` record | Analytics tables (OLAP) | — |
| **Aggregates** | `[AggregateQuery]` method | GROUP BY aggregations | ✓ async/await |
| **Field Metadata** | `[RequiresScope]`, `[Deprecated]` | Field-level features | — |
| **Input Types** | `record` with `init` | Structured parameters | — |
| **Enums** | `enum` | GraphQL enumeration | — |
| **Validation** | `[Validates]` method | Field-level validation | ✓ async |
| **Schema Export** | `FraiseQL.ExportSchema()` | Generate schema.json | ✓ async |

---

## Type System

### 1. Record Types and Nullable Reference Types

C# records provide immutable types with automatic equality and ToString():

```csharp
#nullable enable

namespace MySchema;

/// <summary>Represents a user in the system.</summary>
[FraiseQLType]
public record User(
    int Id,
    string Name,
    string Email,
    bool IsActive,
    string? MiddleName = null
);

// With named constructor (optional):
[FraiseQLType]
public record User
{
    public int Id { get; init; }
    public string Name { get; init; }
    public string Email { get; init; }
    public bool IsActive { get; init; }
    public string? MiddleName { get; init; }
}

// Equivalent types in GraphQL:
// type User {
//   id: ID!
//   name: String!
//   email: Email!
//   isActive: Boolean!
//   middleName: String
// }
```

### 2. Nullable Reference Types (`#nullable enable`)

Modern C# distinguishes between nullable and non-nullable references:

```csharp
#nullable enable

[FraiseQLType]
public record Article
{
    public int Id { get; init; }           // Non-nullable: String!
    public string Title { get; init; }     // Non-nullable: String!
    public string? Content { get; init; }  // Nullable: String (GraphQL null)
    public Author? Author { get; init; }   // Nullable object: Author
};

// Auto-maps to GraphQL:
// type Article {
//   id: ID!
//   title: String!
//   content: String
//   author: Author
// }
```

### 3. Generic Record Types

```csharp
#nullable enable

[FraiseQLType]
public record Page<T>(
    IReadOnlyList<T> Items,
    int TotalCount,
    int PageNumber,
    int PageSize
)
    where T : notnull;

// Usage:
[FraiseQLType]
public record UserPage : Page<User>;
```

### 4. Complex Nested Types

```csharp
#nullable enable

[FraiseQLType]
public record User
{
    public int Id { get; init; }
    public string Name { get; init; }
    public Address Address { get; init; }
    public IReadOnlyList<Post> Posts { get; init; }
};

[FraiseQLType]
public record Address
{
    public string Street { get; init; }
    public string City { get; init; }
    public string PostalCode { get; init; }
    public string? ApartmentNumber { get; init; }
};

[FraiseQLType]
public record Post
{
    public int Id { get; init; }
    public string Title { get; init; }
    public DateTime CreatedAt { get; init; }
};
```

---

## Operations

### Query Operations (async/await)

```csharp
#nullable enable

[FraiseQLSchema]
public static class UserQueries
{
    /// <summary>Get all users with pagination.</summary>
    [Query(SqlSource = "v_users")]
    public static async Task<IReadOnlyList<User>> GetUsers(
        int limit = 10,
        int offset = 0,
        CancellationToken cancellationToken = default
    )
    {
        // Implementation compiled to SQL at build-time
        // Runtime receives optimized prepared statement
        await Task.CompletedTask; // Placeholder
        return [];
    }

    /// <summary>Get a single user by ID.</summary>
    [Query(SqlSource = "v_users")]
    public static async Task<User?> GetUserById(
        int id,
        CancellationToken cancellationToken = default
    ) =>
        await Task.FromResult<User?>(null);

    /// <summary>Search users by name.</summary>
    [Query(SqlSource = "v_users", RequiresScope = "read:users")]
    public static async Task<IReadOnlyList<User>> SearchUsers(
        string query,
        CancellationToken cancellationToken = default
    ) =>
        await Task.FromResult([]);
}
```

### Mutation Operations (async/await)

```csharp
#nullable enable

[FraiseQLSchema]
public static class UserMutations
{
    /// <summary>Create a new user.</summary>
    [Mutation(SqlSource = "users")]
    public static async Task<User> CreateUser(
        CreateUserInput input,
        CancellationToken cancellationToken = default
    )
    {
        // FraiseQL generates INSERT statement
        // Type-safe: CreateUserInput validates at compile-time
        return await Task.FromResult(
            new User(
                Id: 1,
                Name: input.Name,
                Email: input.Email,
                IsActive: true,
                MiddleName: input.MiddleName
            )
        );
    }

    /// <summary>Update an existing user.</summary>
    [Mutation(SqlSource = "users")]
    public static async Task<User?> UpdateUser(
        int id,
        UpdateUserInput input,
        CancellationToken cancellationToken = default
    )
    {
        // FraiseQL generates UPDATE statement
        return await Task.FromResult<User?>(null);
    }

    /// <summary>Delete a user (soft-delete).</summary>
    [Mutation(SqlSource = "users", RequiresScope = "delete:users")]
    public static async Task<bool> DeleteUser(
        int id,
        CancellationToken cancellationToken = default
    ) =>
        await Task.FromResult(true);
}

// Input types (records for mutations)
[FraiseQLInput]
public record CreateUserInput(
    string Name,
    string Email,
    string? MiddleName = null
);

[FraiseQLInput]
public record UpdateUserInput
{
    public string? Name { get; init; }
    public string? Email { get; init; }
};
```

### Query Builders (LINQ Integration)

```csharp
#nullable enable

[FraiseQLSchema]
public static class AdvancedQueries
{
    /// <summary>Complex query with filtering and sorting.</summary>
    [Query(SqlSource = "v_users")]
    public static async IAsyncEnumerable<User> SearchUsersAdvanced(
        string? nameFilter = null,
        bool? isActive = null,
        string sortBy = "name",
        [EnumeratorCancellation] CancellationToken cancellationToken = default
    )
    {
        var query = GetAllUsers();

        // Compile-time filtering
        if (!string.IsNullOrEmpty(nameFilter))
        {
            query = query.Where(u => u.Name.Contains(nameFilter));
        }

        if (isActive.HasValue)
        {
            query = query.Where(u => u.IsActive == isActive.Value);
        }

        // Compile-time sorting
        query = sortBy switch
        {
            "email" => query.OrderBy(u => u.Email),
            "active" => query.OrderByDescending(u => u.IsActive),
            _ => query.OrderBy(u => u.Name),
        };

        foreach (var user in query)
        {
            yield return user;
        }

        await Task.CompletedTask; // Satisfy async requirement
    }

    private static IQueryable<User> GetAllUsers() => throw new NotImplementedException();
}
```

---

## Advanced Features

### Fact Tables (OLAP Analytics)

```csharp
#nullable enable

/// <summary>Analytics fact table for sales events.</summary>
[FactTable(SqlSource = "fact_sales")]
public record SalesFact
{
    /// <summary>Dimension: Date of sale.</summary>
    [Dimension]
    public DateTime Date { get; init; }

    /// <summary>Dimension: Product category.</summary>
    [Dimension]
    public string Category { get; init; }

    /// <summary>Dimension: Geographic region.</summary>
    [Dimension]
    public string Region { get; init; }

    /// <summary>Measure: Revenue in cents (avoid floats).</summary>
    [Measure]
    public decimal Revenue { get; init; }

    /// <summary>Measure: Unit quantity.</summary>
    [Measure]
    public int Quantity { get; init; }

    /// <summary>Measure: Average order value.</summary>
    [Measure]
    public decimal AverageOrderValue { get; init; }
}

[FraiseQLSchema]
public static class AnalyticsQueries
{
    /// <summary>Aggregate sales by category and region.</summary>
    [AggregateQuery(SqlSource = "fact_sales")]
    public static async Task<IReadOnlyList<SalesAggregate>> GetSalesByDimensions(
        DateTime? startDate = null,
        DateTime? endDate = null,
        CancellationToken cancellationToken = default
    ) => await Task.FromResult([]);

    [FraiseQLType]
    public record SalesAggregate
    {
        public string Category { get; init; }
        public string Region { get; init; }
        public decimal TotalRevenue { get; init; }
        public int TotalQuantity { get; init; }
    }
}
```

### Role-Based Access Control (RBAC)

```csharp
#nullable enable

[FraiseQLSchema]
public static class SecureQueries
{
    /// <summary>
    /// Get users—only accessible with "read:users" scope.
    /// Results filtered by user's organization.
    /// </summary>
    [Query(SqlSource = "v_users")]
    [RequiresScope("read:users")]
    [Audit(LogLevel = AuditLevel.Info, IncludePii = true)]
    public static async Task<IReadOnlyList<User>> GetUsersSecure(
        int limit = 10,
        CancellationToken cancellationToken = default
    ) => await Task.FromResult([]);

    /// <summary>Get sensitive user data (admin only).</summary>
    [Query(SqlSource = "v_users_sensitive")]
    [RequiresScope("admin:read")]
    [RateLimitPerUser(requestsPerMinute: 10)]
    public static async Task<User?> GetSensitiveUserData(
        int userId,
        CancellationToken cancellationToken = default
    ) => await Task.FromResult<User?>(null);

    /// <summary>Export users to external system (super-admin only).</summary>
    [Mutation(SqlSource = "users")]
    [RequiresScope("super:admin")]
    [Audit(LogLevel = AuditLevel.Critical)]
    public static async Task<ExportResult> ExportUsers(
        ExportFormat format = ExportFormat.Csv,
        CancellationToken cancellationToken = default
    ) => await Task.FromResult(new ExportResult());

    public enum ExportFormat { Csv, Json, Parquet }

    [FraiseQLType]
    public record ExportResult
    {
        public string ExportId { get; init; }
        public int RecordCount { get; init; }
        public DateTime CompletedAt { get; init; }
    }
}
```

### Field-Level Metadata and Validation

```csharp
#nullable enable

[FraiseQLType]
public record UserProfile
{
    [FraiseQLField(Description = "User's unique identifier")]
    public int Id { get; init; }

    [FraiseQLField(Description = "User's full name")]
    [Validates(ValidationType.StringLength, MinLength = 1, MaxLength = 256)]
    public string Name { get; init; }

    [FraiseQLField(Description = "User's email (confidential)")]
    [Validates(ValidationType.Email)]
    [FieldEncryption(Algorithm = "AES-256-GCM")]
    public string Email { get; init; }

    [Deprecated("Use 'birthDate' instead", RemovalDate = "2026-12-31")]
    public DateTime? DateOfBirth { get; init; }

    [FraiseQLField(Description = "Date of birth (ISO 8601)")]
    public DateOnly? BirthDate { get; init; }
}

[FraiseQLSchema]
public static class Validators
{
    /// <summary>Custom validation for user emails.</summary>
    [Validates(typeof(UserProfile), nameof(UserProfile.Email))]
    public static async Task<ValidationResult> ValidateUserEmail(
        string email,
        CancellationToken cancellationToken = default
    )
    {
        if (!email.Contains("@"))
        {
            return new ValidationResult(
                IsValid: false,
                ErrorMessage: "Email must contain '@' symbol"
            );
        }

        return new ValidationResult(IsValid: true);
    }

    [FraiseQLType]
    public record ValidationResult(bool IsValid, string? ErrorMessage = null);
}
```

---

## Scalar Types

### C# ↔ GraphQL Type Mappings

| C# Type | GraphQL Type | Notes |
|---------|-------------|-------|
| `int` | `Int!` | 32-bit signed integer |
| `long` | `Long!` | 64-bit signed integer (custom scalar) |
| `decimal` | `Decimal!` | Fixed-point (preferred for money) |
| `double` | `Float!` | 64-bit IEEE floating-point |
| `float` | `Float!` | 32-bit floating-point (not recommended) |
| `string` | `String!` | UTF-16 string |
| `string?` | `String` | Nullable string |
| `bool` | `Boolean!` | True/false |
| `DateTime` | `DateTime!` | ISO 8601 timestamp (custom scalar) |
| `DateOnly` | `Date!` | ISO 8601 date (custom scalar) |
| `TimeOnly` | `Time!` | ISO 8601 time (custom scalar) |
| `Guid` | `UUID!` | RFC 4122 UUID (custom scalar) |
| `byte[]` | `Base64!` | Base64-encoded bytes (custom scalar) |
| `IReadOnlyList<T>` | `[T!]!` | Non-null list of non-null T |
| `List<T>` | `[T]` | Nullable list of nullable T |
| `IReadOnlyDictionary<K, V>` | `JSON!` | Serialized to JSON (custom scalar) |
| `record` / `class` | Named type | Nested object type |
| `enum` | Enum type | GraphQL enumeration |

### Custom Scalar Examples

```csharp
#nullable enable

/// <summary>ISO 8601 timestamp (FraiseQL custom scalar).</summary>
[CustomScalar("DateTime")]
public readonly record struct FraiseQLDateTime
{
    public required DateTime Value { get; init; }

    public static explicit operator FraiseQLDateTime(DateTime dt) =>
        new() { Value = dt };

    public static implicit operator DateTime(FraiseQLDateTime fdt) =>
        fdt.Value;
}

// Usage in schema:
[FraiseQLType]
public record Event
{
    public int Id { get; init; }
    public FraiseQLDateTime OccurredAt { get; init; }  // Serialized as DateTime
    public FraiseQLDateTime? CompletedAt { get; init; }  // Nullable DateTime
}
```

---

## Schema Export

### Export Workflow

```csharp
// Program.cs
using FraiseQL;

namespace MySchema;

var schemaGenerator = new FraiseQLSchemaGenerator();

// Automatic discovery from attributes
await schemaGenerator.ExportSchemaAsync(
    outputPath: "schema.json",
    includeComments: true,
    cancellationToken: CancellationToken.None
);

Console.WriteLine("✓ Generated schema.json");
```

### Project Integration (.csproj)

```xml
<!-- YourProject.csproj -->
<Target Name="GenerateFraiseQLSchema" BeforeTargets="Build">
  <Exec Command="dotnet run --project ./Schema/SchemaGenerator.csproj" />
</Target>
```

### Dependency Injection Integration

```csharp
// Startup.cs / Program.cs (.NET 6+ minimal hosting)
using FraiseQL;
using Microsoft.Extensions.DependencyInjection;

var services = new ServiceCollection();

// Register FraiseQL schema and server
services.AddFraiseQL(options =>
{
    options.SchemaPath = "./schema.compiled.json";
    options.PoolSize = 20;
    options.QueryTimeout = TimeSpan.FromSeconds(30);
});

services.AddScoped<IUserRepository, UserRepository>();
services.AddScoped<IAnalyticsRepository, AnalyticsRepository>();

var serviceProvider = services.BuildServiceProvider();

// Load and initialize
var fraiseQL = serviceProvider.GetRequiredService<IFraiseQLServer>();
await fraiseQL.InitializeAsync();
```

---

## Type Mapping

### GraphQL ↔ C# Conversion Rules

**Input → Record Destructuring:**

```csharp
#nullable enable

// GraphQL Input:
// input CreateUserInput {
//   name: String!
//   email: String!
//   middleName: String
// }

// C# Record (auto-generated or manual):
[FraiseQLInput]
public record CreateUserInput(
    string Name,
    string Email,
    string? MiddleName = null
);

// Deserialization (automatic via source generators):
// JSON: {"name":"Alice","email":"alice@example.com"}
// → CreateUserInput("Alice", "alice@example.com", null)
```

**Output → Record Serialization:**

```csharp
#nullable enable

// C# Record:
[FraiseQLType]
public record User(int Id, string Name, string? MiddleName);

// Serialization → GraphQL Response:
var user = new User(Id: 1, Name: "Alice", MiddleName: null);
// JSON: {"id":1,"name":"Alice","middleName":null}
```

**List Handling:**

```csharp
#nullable enable

// GraphQL Field:
// users: [User!]!

// C# Property (read-only required):
[FraiseQLType]
public record UserList
{
    public required IReadOnlyList<User> Users { get; init; }
}

// Serialization:
var list = new UserList { Users = [user1, user2] };
// JSON: {"users":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}]}
```

---

## Common Patterns

### CRUD Operations

```csharp
#nullable enable
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;

[FraiseQLSchema]
public static class CrudPatterns
{
    // CREATE
    [Mutation(SqlSource = "users")]
    public static async Task<User> Create(
        CreateUserInput input,
        CancellationToken ct = default
    ) => await Task.FromResult(new User(
        Id: 1,
        Name: input.Name,
        Email: input.Email,
        IsActive: true
    ));

    // READ (single)
    [Query(SqlSource = "v_users")]
    public static async Task<User?> ReadById(
        int id,
        CancellationToken ct = default
    ) => await Task.FromResult<User?>(null);

    // READ (list with pagination)
    [Query(SqlSource = "v_users")]
    public static async Task<IReadOnlyList<User>> ReadPaginated(
        int pageNumber = 1,
        int pageSize = 20,
        CancellationToken ct = default
    ) => await Task.FromResult((IReadOnlyList<User>)[]);

    // UPDATE
    [Mutation(SqlSource = "users")]
    public static async Task<User?> Update(
        int id,
        UpdateUserInput input,
        CancellationToken ct = default
    ) => await Task.FromResult<User?>(null);

    // DELETE
    [Mutation(SqlSource = "users")]
    public static async Task<bool> Delete(
        int id,
        CancellationToken ct = default
    ) => await Task.FromResult(true);
}
```

### Pagination Pattern

```csharp
#nullable enable

[FraiseQLType]
public record PaginationInfo(
    int PageNumber,
    int PageSize,
    int TotalCount
)
{
    public int TotalPages => (TotalCount + PageSize - 1) / PageSize;
    public bool HasNextPage => PageNumber < TotalPages;
    public bool HasPreviousPage => PageNumber > 1;
}

[FraiseQLType]
public record UserPageResult(
    IReadOnlyList<User> Items,
    PaginationInfo Pagination
);

[FraiseQLSchema]
public static class PaginationQueries
{
    [Query(SqlSource = "v_users")]
    public static async Task<UserPageResult> GetUserPage(
        int pageNumber = 1,
        int pageSize = 20,
        CancellationToken ct = default
    )
    {
        var items = await Task.FromResult((IReadOnlyList<User>)[]);
        var pagination = new PaginationInfo(
            PageNumber: pageNumber,
            PageSize: pageSize,
            TotalCount: 100
        );
        return new UserPageResult(items, pagination);
    }
}
```

### Filtering and Sorting

```csharp
#nullable enable

public enum SortOrder { Ascending, Descending }

[FraiseQLInput]
public record UserFilter
{
    public string? NameContains { get; init; }
    public bool? IsActive { get; init; }
    public DateOnly? CreatedAfter { get; init; }
}

[FraiseQLInput]
public record UserSort
{
    public required string Field { get; init; }  // "name", "email", "createdAt"
    public SortOrder Order { get; init; } = SortOrder.Ascending;
}

[FraiseQLSchema]
public static class FilteredQueries
{
    [Query(SqlSource = "v_users")]
    public static async Task<IReadOnlyList<User>> SearchUsers(
        UserFilter? filter = null,
        UserSort? sort = null,
        int limit = 10,
        CancellationToken ct = default
    ) => await Task.FromResult([]);
}
```

---

## Error Handling

### Exception Handling

```csharp
#nullable enable

using FraiseQL.Exceptions;

[FraiseQLSchema]
public static class SafeOperations
{
    [Mutation(SqlSource = "users")]
    public static async Task<User> CreateUserSafe(
        CreateUserInput input,
        CancellationToken ct = default
    )
    {
        try
        {
            // FraiseQL validates at compile-time
            // Runtime errors bubble up with rich context
            return await CreateUser(input, ct);
        }
        catch (FraiseQLValidationException ex)
        {
            // Field-level validation failed
            throw new OperationException($"Validation failed: {ex.Message}", ex);
        }
        catch (FraiseQLDatabaseException ex)
        {
            // Database constraint violation, connection error, etc.
            throw new OperationException($"Database error: {ex.Code}", ex);
        }
        catch (OperationCanceledException)
        {
            throw; // Propagate cancellation
        }
    }
}

// Custom exception wrapper
public class OperationException : Exception
{
    public OperationException(string message, Exception? innerException = null)
        : base(message, innerException)
    {
    }
}
```

### Result Pattern (Functional Error Handling)

```csharp
#nullable enable

/// <summary>Functional result type for safe operations.</summary>
public abstract record Result<T>
{
    public sealed record Success(T Value) : Result<T>;
    public sealed record Failure(string Error, Exception? Exception = null) : Result<T>;
}

[FraiseQLSchema]
public static class RobustOperations
{
    [Query(SqlSource = "v_users")]
    public static async Task<Result<User>> GetUserSafe(
        int id,
        CancellationToken ct = default
    )
    {
        try
        {
            var user = await GetUserById(id, ct);
            return user is not null
                ? new Result<User>.Success(user)
                : new Result<User>.Failure("User not found");
        }
        catch (Exception ex)
        {
            return new Result<User>.Failure($"Error: {ex.Message}", ex);
        }
    }

    private static async Task<User?> GetUserById(int id, CancellationToken ct)
        => await Task.FromResult<User?>(null);
}
```

---

## Testing

### xUnit Integration

```csharp
#nullable enable

using Xunit;
using FraiseQL;
using System.Threading.Tasks;

public class UserQueriesTests
{
    private readonly IFraiseQLServer _server;

    public UserQueriesTests()
    {
        // Dependency injection in xUnit
        _server = new MockFraiseQLServer();
    }

    [Fact]
    public async Task GetUsers_ReturnsNonEmptyList()
    {
        // Arrange
        var expectedCount = 10;

        // Act
        var result = await UserQueries.GetUsers(limit: expectedCount);

        // Assert
        Assert.NotEmpty(result);
        Assert.True(result.Count <= expectedCount);
    }

    [Theory]
    [InlineData(0)]
    [InlineData(-1)]
    public async Task GetUsers_WithInvalidLimit_ThrowsArgumentException(int limit)
    {
        // Act & Assert
        await Assert.ThrowsAsync<ArgumentException>(
            () => UserQueries.GetUsers(limit)
        );
    }

    [Fact]
    public async Task CreateUser_WithValidInput_ReturnsUser()
    {
        // Arrange
        var input = new CreateUserInput("Alice", "alice@example.com");

        // Act
        var result = await UserMutations.CreateUser(input);

        // Assert
        Assert.NotNull(result);
        Assert.Equal("Alice", result.Name);
        Assert.Equal("alice@example.com", result.Email);
    }
}

/// <summary>Mock server for testing (compile-time generated).</summary>
public class MockFraiseQLServer : IFraiseQLServer
{
    public Task InitializeAsync() => Task.CompletedTask;

    public Task<GraphQLResponse> ExecuteAsync(
        string query,
        Dictionary<string, object?>? variables = null,
        CancellationToken ct = default
    ) => Task.FromResult(new GraphQLResponse());
}
```

### Integration Testing with Test Containers

```csharp
#nullable enable

using Testcontainers.PostgreSql;
using Xunit;

public class IntegrationTests : IAsyncLifetime
{
    private readonly PostgreSqlContainer _postgres = new PostgreSqlBuilder()
        .WithImage("postgres:15")
        .Build();

    public async Task InitializeAsync()
    {
        await _postgres.StartAsync();
        // Seed test data
    }

    public async Task DisposeAsync()
    {
        await _postgres.StopAsync();
    }

    [Fact]
    public async Task EndToEnd_CreateAndRetrieveUser()
    {
        // Arrange
        var connectionString = _postgres.GetConnectionString();
        var server = new FraiseQLServer(connectionString);
        await server.InitializeAsync();

        // Act
        var created = await UserMutations.CreateUser(
            new CreateUserInput("Test User", "test@example.com")
        );
        var retrieved = await UserQueries.GetUserById(created.Id);

        // Assert
        Assert.NotNull(retrieved);
        Assert.Equal("Test User", retrieved.Name);
    }
}
```

---

## See Also

- **[Python SDK Reference](./python-reference.md)** - Python authoring interface
- **[TypeScript SDK Reference](./typescript-reference.md)** - TypeScript authoring interface
- **[Go SDK Reference](./go-reference.md)** - Go runtime and server integration
- **[Rust Core](../../architecture/README.md)** - FraiseQL compiler and runtime
- **[Security Configuration](../../integrations/authentication/README.md)** - RBAC and authentication
- **[Deployment Guide](../../operations/configuration.md)** - Docker, Kubernetes, cloud platforms
- **[Performance Tuning](../../operations/performance-tuning-runbook.md)** - Benchmarking and optimization

---

## Troubleshooting

### Common Setup Issues

#### NuGet Package Issues

**Issue**: `NU1101: Unable to find package FraiseQL`

**Solution**:

```xml
<!-- .csproj -->
<ItemGroup>
  <PackageReference Include="FraiseQL" Version="2.0.0" />
</ItemGroup>
```

```bash
dotnet add package FraiseQL --version 2.0.0
```

#### Assembly Loading

**Issue**: `FileLoadException: Could not load file or assembly`

**Solution - Check version**:

```bash
dotnet list package --outdated
dotnet restore
dotnet clean && dotnet build
```

#### .NET Version Mismatch

**Issue**: `This package requires .NET 6.0 or higher`

**Check version** (6.0+ required):

```bash
dotnet --version
```

**Update .csproj**:

```xml
<TargetFramework>net8.0</TargetFramework>
```

#### Package Source Issues

**Issue**: `The nuget source is unreachable`

**Configure package source**:

```bash
dotnet nuget add source https://api.nuget.org/v3/index.json -n nuget.org
dotnet nuget list source
```

---

### Type System Issues

#### Nullable Reference Type Issues

**Issue**: `CS8600: Converting null literal or possible null value to non-nullable reference type`

**Enable nullable reference types**:

```xml
<PropertyGroup>
  <Nullable>enable</Nullable>
  <ImplicitUsings>enable</ImplicitUsings>
</PropertyGroup>
```

**Use correct nullability**:

```csharp
// ❌ Wrong - implicit non-null
[FraiseQLType]
public class User
{
    public string Email { get; set; }  // Implicitly required
}

// ✅ Correct - explicit nullability
[FraiseQLType]
public class User
{
    public string Email { get; set; }  // Non-null
    public string? MiddleName { get; set; }  // Nullable
}
```

#### Generic Type Issues

**Issue**: `CS0311: The type 'T' cannot be used as type parameter`

**Solution - Use concrete types**:

```csharp
// ❌ Won't work - generics
[FraiseQLType]
public class Box<T>
{
    public T Value { get; set; }
}

// ✅ Use concrete types
[FraiseQLType]
public class UserBox
{
    public User Value { get; set; }
}
```

#### Dynamic Type Issues

**Issue**: `Cannot cast dynamic to IGraphQLType`

**Solution - Use static types**:

```csharp
// ❌ Don't use dynamic
var result = (dynamic)FraiseQL.Execute(query);

// ✅ Use typed results
var result = FraiseQL.Execute<QueryResult>(query);

// Or cast after
var result = FraiseQL.Execute(query) as QueryResult;
```

#### Attributes Issues

**Issue**: `CS0246: The type or namespace name 'FraiseQLType' could not be found`

**Verify using statements**:

```csharp
using FraiseQL;
using FraiseQL.Attributes;

[FraiseQLType]
public class User { }
```

---

### Runtime Errors

#### Task/Async Issues

**Issue**: `InvalidOperationException: 'await' requires async method`

**Solution - Use async/await properly**:

```csharp
// ❌ Wrong - not async
public QueryResult Execute(string query)
{
    var result = await FraiseQL.ExecuteAsync(query);  // ERROR
    return result;
}

// ✅ Correct
public async Task<QueryResult> ExecuteAsync(string query)
{
    var result = await FraiseQL.ExecuteAsync(query);
    return result;
}

// In controllers
[HttpPost("/graphql")]
public async Task<IActionResult> GraphQL([FromBody] GraphQLRequest request)
{
    var result = await FraiseQL.ExecuteAsync(request.Query);
    return Ok(result);
}
```

#### Reflection Issues

**Issue**: `MissingMethodException: Method not found`

**Solution - Use proper reflection**:

```csharp
// ✅ Get property correctly
var propertyInfo = typeof(User).GetProperty("Email",
    System.Reflection.BindingFlags.IgnoreCase |
    System.Reflection.BindingFlags.Public |
    System.Reflection.BindingFlags.Instance);

if (propertyInfo != null)
{
    var value = propertyInfo.GetValue(user);
}
```

#### Dependency Injection Issues

**Issue**: `InvalidOperationException: Unable to resolve service for type`

**Register dependencies**:

```csharp
// Startup.cs or Program.cs
services.AddSingleton<IFraiseQLServer>(sp =>
    FraiseQLServer.FromCompiled("schema.compiled.json")
);

services.AddScoped<IGraphQLService, GraphQLService>();
```

**Use in controller**:

```csharp
[ApiController]
[Route("api")]
public class GraphQLController : ControllerBase
{
    private readonly IFraiseQLServer _server;

    public GraphQLController(IFraiseQLServer server)
    {
        _server = server;
    }

    [HttpPost("graphql")]
    public async Task<IActionResult> Execute([FromBody] GraphQLRequest request)
    {
        var result = await _server.ExecuteAsync(request.Query);
        return Ok(result);
    }
}
```

#### Entity Framework Issues

**Issue**: `DbUpdateException: An error occurred while updating the entries`

**Solution - Use SQL views/functions only, not EF**:

```csharp
// FraiseQL works with SQL views, not EF entities
// Don't mix EF with FraiseQL schema

// ✅ Correct - SQL views for FraiseQL
CREATE VIEW v_users AS SELECT id, name, email FROM users;

// Use in FraiseQL schema
@FraiseQL.query(sql_source = "v_users")
public User[] GetUsers() { return new User[0]; }
```

---

### Performance Issues

#### Slow Startup

**Issue**: Application takes >10 seconds to start**

**Pre-compile schema**:

```bash
# Use FraiseQL-cli
FraiseQL-cli compile schema.json FraiseQL.toml

# Load pre-compiled (faster)
var server = FraiseQLServer.FromCompiled("schema.compiled.json");
```

#### Large Assembly Size

**Issue**: DLL is >100MB

**Enable trimming**:

```xml
<PropertyGroup>
  <PublishTrimmed>true</PublishTrimmed>
  <PublishReadyToRun>true</PublishReadyToRun>
</PropertyGroup>
```

```bash
dotnet publish -c Release
```

#### Connection Pool Exhaustion

**Issue**: `InvalidOperationException: Timeout expired`

**Increase pool size**:

```csharp
var options = new DbContextOptionsBuilder<MyDbContext>()
    .UseSqlServer(
        connectionString,
        opts => opts.CommandTimeout(60)
    )
    .Build();

// Or via connection string
"Server=...;Max Pool Size=50;Min Pool Size=5;"
```

#### Memory Usage in Long-Running Services

**Issue**: Memory grows over time

**Implement cleanup**:

```csharp
public class GraphQLService : IDisposable
{
    private readonly IFraiseQLServer _server;
    private readonly CancellationTokenSource _cts;

    public GraphQLService(IFraiseQLServer server)
    {
        _server = server;
        _cts = new CancellationTokenSource();
    }

    public async Task<QueryResult> ExecuteAsync(string query)
    {
        return await _server.ExecuteAsync(query, _cts.Token);
    }

    public void Dispose()
    {
        _cts?.Dispose();
        _server?.Dispose();
    }
}
```

---

### Debugging Techniques

#### Enable Logging

**Setup logging**:

```csharp
services.AddLogging(builder =>
{
    builder.AddConsole();
    builder.AddDebug();
    builder.SetMinimumLevel(LogLevel.Debug);
});

// Or in appsettings.json
{
  "Logging": {
    "LogLevel": {
      "FraiseQL": "Debug",
      "Default": "Information"
    }
  }
}
```

#### Debug Output

```csharp
System.Diagnostics.Debug.WriteLine($"Query: {query}");
System.Diagnostics.Debug.WriteLine($"Result: {result}");
```

#### Visual Studio Debugger

1. Set breakpoint (Ctrl+B)
2. Debug → Debug (F5)
3. Step over/into (F10/F11)
4. Watch window (Ctrl+Alt+W)

#### Unit Testing

```csharp
[TestClass]
public class GraphQLTests
{
    private IFraiseQLServer _server;

    [TestInitialize]
    public void Setup()
    {
        _server = FraiseQLServer.FromCompiled("schema.compiled.json");
    }

    [TestMethod]
    public async Task TestQuery()
    {
        var result = await _server.ExecuteAsync("{ user(id: 1) { id } }");
        Assert.IsNotNull(result);
        Assert.IsNotNull(result.Data);
    }
}
```

#### Profiling

**Use Visual Studio Profiler**:

1. Debug → Performance Profiler
2. Select CPU Usage
3. Run app
4. Analyze results

---

### Getting Help

#### GitHub Issues

Provide:

1. .NET version: `dotnet --version`
2. Visual Studio version (if applicable)
3. FraiseQL version: `dotnet list package`
4. Minimal reproducible example
5. Full exception + stack trace

**Template**:

```markdown
**Environment**:
- .NET: 8.0
- Visual Studio: 2022
- FraiseQL: 2.0.0

**Issue**:
[Describe]

**Code**:
[Minimal example]

**Error**:
[Full exception]
```

#### Community Channels

- **GitHub Discussions**: Q&A
- **Stack Overflow**: Tag with `csharp`, `FraiseQL`, `graphql`
- **Reddit**: r/dotnet

---

## See Also

- **[Java SDK Reference](./java-reference.md)** - Java authoring interface
- **[TypeScript SDK Reference](./typescript-reference.md)** - TypeScript authoring interface
- **[Go SDK Reference](./go-reference.md)** - Go runtime and server integration
- **[Rust Core](../../architecture/README.md)** - FraiseQL compiler and runtime
- **[Security Configuration](../../integrations/authentication/README.md)** - RBAC and authentication
- **[Deployment Guide](../../operations/configuration.md)** - Docker, Kubernetes, cloud platforms
- **[Performance Tuning](../../operations/performance-tuning-runbook.md)** - Benchmarking and optimization

---

**FraiseQL Community Edition** — Modern, compiled GraphQL execution for .NET
