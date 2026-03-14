# FraiseQL C# SDK

C# schema authoring SDK for [FraiseQL](https://github.com/fraiseql/fraiseql) v2.
Produces `schema.json` consumed by the `fraiseql compile` Rust CLI.

There is no runtime FFI. C# and Rust communicate only through JSON on disk.

---

## Installation

Add the library to your project:

```bash
dotnet add package FraiseQL
```

Install the CLI tool globally:

```bash
dotnet tool install -g FraiseQL.Tool
```

---

## Quick Start

### 1. Define types with attributes

```csharp
using FraiseQL.Attributes;

[GraphQLType(Name = "Author", SqlSource = "v_author", Description = "A blog author")]
public class Author
{
    [GraphQLField(Type = "ID", Nullable = false)]
    public int Id { get; set; }

    [GraphQLField(Nullable = false)]
    public string Name { get; set; } = string.Empty;

    [GraphQLField(Nullable = true)]
    public string? Bio { get; set; }
}
```

### 2. Register a query and export

```csharp
using FraiseQL.Builders;
using FraiseQL.Export;
using FraiseQL.Registry;

SchemaRegistry.Instance.Register(typeof(Author));

QueryBuilder.Query("authors")
    .ReturnType("Author")
    .ReturnsList()
    .SqlSource("v_author")
    .Register();

MutationBuilder.Mutation("createAuthor")
    .ReturnType("Author")
    .SqlSource("fn_create_author")
    .Operation("CREATE")
    .Argument("name", "String")
    .Register();

SchemaExporter.ExportToFile("schema.json");
```

### 3. Compile with fraiseql

```bash
fraiseql compile schema.json
```

---

## Attribute Reference

### `[GraphQLType]`

| Property | Type | Description |
|----------|------|-------------|
| `Name` | `string` | GraphQL type name (defaults to C# class name) |
| `SqlSource` | `string` | Backing SQL view (e.g. `v_author`) |
| `Description` | `string` | Optional schema description |
| `IsInput` | `bool` | Mark as GraphQL input type |
| `Relay` | `bool` | Enable Relay cursor-based pagination |
| `IsError` | `bool` | Mark as a mutation error variant |

### `[GraphQLField]`

| Property | Type | Description |
|----------|------|-------------|
| `Type` | `string?` | Explicit GraphQL type (overrides auto-detection) |
| `Nullable` | `bool` | Whether the field is nullable |
| `Description` | `string?` | Optional field description |
| `Resolver` | `string?` | Custom resolver name |
| `Scope` | `string?` | Required OAuth scope |
| `Scopes` | `string[]?` | Multiple required scopes |

### C# to GraphQL type auto-detection

| C# Type | GraphQL Type |
|---------|-------------|
| `int`, `long`, `short` | `Int` |
| `int?`, `long?` | `Int` (nullable) |
| `float`, `double`, `decimal` | `Float` |
| `bool` | `Boolean` |
| `Guid` | `ID` |
| `string` | `String` |
| `string?` | `String` (nullable) |
| `DateTime`, `DateTimeOffset` | `String` (ISO 8601) |

---

## QueryBuilder / MutationBuilder Reference

### QueryBuilder

```csharp
QueryBuilder.Query("authors")
    .ReturnType("Author")          // required
    .ReturnsList(true)             // default: false
    .Nullable(false)               // default: false
    .SqlSource("v_author")         // required
    .Argument("id", "ID")          // optional, repeatable
    .CacheTtlSeconds(300)          // optional
    .Description("List authors")   // optional
    .Register();                   // adds to SchemaRegistry
```

### MutationBuilder

```csharp
MutationBuilder.Mutation("createAuthor")
    .ReturnType("Author")          // required
    .SqlSource("fn_create_author") // required
    .Operation("CREATE")           // required: CREATE|UPDATE|DELETE|CUSTOM
    .Argument("name", "String")    // optional, repeatable
    .Description("Create author")  // optional
    .Register();                   // adds to SchemaRegistry
```

---

## Fluent SchemaBuilder API

For teams that prefer a code-first approach without decorating domain model classes:

```csharp
using FraiseQL.Builders;

new SchemaBuilder()
    .Type("Author", t => t
        .SqlSource("v_author")
        .Description("A blog author")
        .Field("id", "ID", nullable: false)
        .Field("name", "String", nullable: false)
        .Field("bio", "String", nullable: true))
    .Query("authors", q => q
        .ReturnType("Author")
        .ReturnsList()
        .SqlSource("v_author"))
    .Mutation("createAuthor", m => m
        .ReturnType("Author")
        .SqlSource("fn_create_author")
        .Operation("CREATE")
        .Argument("name", "String"))
    .ExportToFile("schema.json");
```

`SchemaBuilder.ToSchema()` merges fluent registrations with anything already in
`SchemaRegistry.Instance`. Fluent registrations win on name conflicts.

---

## CLI Usage

The `fraiseql` dotnet tool scans a compiled assembly for `[GraphQLType]`-annotated types
and exports `schema.json` automatically.

```bash
# Build your project first
dotnet build

# Export schema from the compiled assembly
fraiseql export bin/Debug/net8.0/MyProject.dll --output schema.json

# Compact (non-indented) output
fraiseql export bin/Debug/net8.0/MyProject.dll --output schema.json --compact
```

---

## JSON Output Format

`SchemaExporter.Export()` produces the intermediate schema format consumed by
`fraiseql compile`:

```json
{
  "version": "2.0.0",
  "types": [
    {
      "name": "Author",
      "sql_source": "v_author",
      "description": "A blog author",
      "fields": [
        { "name": "id", "type": "ID", "nullable": false },
        { "name": "name", "type": "String", "nullable": false },
        { "name": "bio", "type": "String", "nullable": true }
      ]
    }
  ],
  "queries": [
    {
      "name": "authors",
      "return_type": "Author",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_author",
      "arguments": []
    }
  ],
  "mutations": [
    {
      "name": "createAuthor",
      "return_type": "Author",
      "sql_source": "fn_create_author",
      "operation": "insert",
      "arguments": [
        { "name": "name", "type": "String", "nullable": false }
      ]
    }
  ]
}
```

Key invariants:
- All keys are snake_case (`sql_source`, `return_type`, `returns_list`)
- `description` and `cache_ttl_seconds` are omitted when not set
- `arguments` is always an array (empty `[]` when no arguments)

---

## Integration with fraiseql compile

After exporting `schema.json`, pass it to the FraiseQL Rust compiler:

```bash
fraiseql compile schema.json --output schema.compiled.json
```

The compiled schema is loaded by the FraiseQL Rust server at startup.

---

## Requirements

- .NET 8.0 or later
- C# 12

---

## License

MIT
