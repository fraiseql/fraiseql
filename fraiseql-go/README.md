# FraiseQL v2 - Go Schema Authoring

> Compiled GraphQL execution engine - Schema authoring in Go

FraiseQL v2 is a high-performance GraphQL engine that compiles schemas at build-time for zero-cost query execution. This package provides **schema authoring in Go** that generates JSON schemas consumed by the Rust compiler.

**Key Principle**: Go is for **authoring only** - no runtime FFI, no language bindings. Just pure JSON generation.

## Architecture

```
Go Code (struct tags + builders)
         ↓
    schema.json
         ↓
 fraiseql-cli compile
         ↓
 schema.compiled.json
         ↓
 Rust Runtime (fraiseql-server)
```

## Installation

```bash
go get github.com/fraiseql/fraiseql-go
```

**Requirements**: Go 1.22+

## Quick Start

### 1. Define Types

```go
package main

import "github.com/fraiseql/fraiseql-go/fraiseql"

type User struct {
    ID        int    `fraiseql:"id,type=Int"`
    Name      string `fraiseql:"name,type=String"`
    Email     string `fraiseql:"email,type=String"`
    CreatedAt string `fraiseql:"created_at,type=String"`
}
```

### 2. Define Queries

```go
func init() {
    fraiseql.NewQuery("users").
        ReturnType(User{}).
        ReturnsArray(true).
        Config(map[string]interface{}{
            "sql_source": "v_user",
            "auto_params": map[string]bool{
                "limit":  true,
                "offset": true,
            },
        }).
        Arg("limit", "Int", 10).
        Arg("offset", "Int", 0).
        Description("Get all users with pagination").
        Register()
}
```

### 3. Define Mutations

```go
func init() {
    fraiseql.NewMutation("createUser").
        ReturnType(User{}).
        Config(map[string]interface{}{
            "sql_source": "fn_create_user",
            "operation": "CREATE",
        }).
        Arg("name", "String", nil).
        Arg("email", "String", nil).
        Description("Create a new user").
        Register()
}
```

### 4. Export Schema

```go
func main() {
    if err := fraiseql.RegisterTypes(User{}); err != nil {
        log.Fatal(err)
    }

    if err := fraiseql.ExportSchema("schema.json"); err != nil {
        log.Fatal(err)
    }
}
```

### 5. Compile Schema

```bash
# Compile schema.json to optimized schema.compiled.json
fraiseql-cli compile schema.json -o schema.compiled.json

# Start server with compiled schema
fraiseql-server --schema schema.compiled.json
```

## Type System

FraiseQL supports the following Go types:

| Go Type | GraphQL Type | Nullable |
|---------|-------------|----------|
| `int` | `Int` | No |
| `*int` | `Int` | Yes |
| `int32` | `Int` | No |
| `int64` | `Int` | No |
| `float64` | `Float` | No |
| `*float64` | `Float` | Yes |
| `string` | `String` | No |
| `*string` | `String` | Yes |
| `bool` | `Boolean` | No |
| `*bool` | `Boolean` | Yes |
| `[]T` | `[T]` | No |
| `*[]T` | `[T]` | Yes |
| `time.Time` | `String` | No |
| `*time.Time` | `String` | Yes |
| Custom struct | Custom Type | No |
| `*CustomStruct` | Custom Type | Yes |

### Struct Tags

Define field metadata using struct tags:

```go
type User struct {
    ID    int    `fraiseql:"id,type=Int"`
    Name  string `fraiseql:"name,type=String"`
}
```

Tag format: `fraiseql:"<field_name>,type=<graphql_type>,nullable=<true|false>"`

- `field_name`: GraphQL field name (optional, defaults to struct field name)
- `type`: GraphQL type (required)
- `nullable`: Whether field can be null (optional, defaults to false for non-pointer types)

## Features

- **Type-safe**: Go struct definitions map to GraphQL types
- **Database-backed**: Queries map to SQL views, mutations to functions
- **Compile-time**: All validation happens at compile time, zero runtime overhead
- **No FFI**: Pure JSON output, no Go-Rust bindings needed
- **Builder Pattern**: Go-idiomatic API with chainable configuration
- **Struct Introspection**: Automatic field extraction with reflection
- **Analytics**: Fact tables and aggregate queries for OLAP workloads

## Examples

### Basic Schema

See `examples/basic_schema.go` for a complete example with Users and Posts.

Run it:
```bash
cd examples
go run basic_schema.go
```

This generates a `schema.json` that can be compiled with fraiseql-cli.

### Analytics / Fact Tables

See `examples/analytics_schema.go` for a fact table example with measures and dimensions.

## API Reference

### Registry Functions

#### RegisterTypes

Register Go struct types with the schema registry.

```go
err := fraiseql.RegisterTypes(User{}, Post{}, Comment{})
if err != nil {
    log.Fatal(err)
}
```

#### ExportSchema

Export the schema registry to a JSON file.

```go
err := fraiseql.ExportSchema("schema.json")
if err != nil {
    log.Fatal(err)
}
```

### Query Builder

#### NewQuery

Create a new query builder.

```go
qb := fraiseql.NewQuery("users")
```

Methods:
- `ReturnType(any)` - Set the return type (required)
- `ReturnsArray(bool)` - Whether query returns a list (default: false)
- `Nullable(bool)` - Whether result can be null (default: false)
- `Config(map[string]interface{})` - Set configuration (sql_source, auto_params, etc.)
- `Arg(name, graphqlType string, defaultValue interface{}, nullable ...bool)` - Add argument
- `Description(string)` - Set description
- `Register()` - Register the query

Example:
```go
fraiseql.NewQuery("user").
    ReturnType(User{}).
    Config(map[string]interface{}{
        "sql_source": "v_user",
    }).
    Arg("id", "Int", nil).
    Description("Get a user by ID").
    Register()
```

### Mutation Builder

#### NewMutation

Create a new mutation builder (same API as query builder).

```go
fraiseql.NewMutation("createUser").
    ReturnType(User{}).
    Config(map[string]interface{}{
        "sql_source": "fn_create_user",
        "operation": "CREATE",
    }).
    Arg("name", "String", nil).
    Arg("email", "String", nil).
    Register()
```

### Fact Table Builder

For analytics / OLAP workloads:

```go
fraiseql.NewFactTable("sales").
    TableName("tf_sales").
    Measure("revenue", "sum", "avg", "max").
    Measure("quantity", "sum", "count", "avg").
    Measure("cost", "sum", "avg").
    Dimension("category", "data->>'category'", "text").
    Dimension("region", "data->>'region'", "text").
    Dimension("year_month", "date_trunc('month', occurred_at)::text", "text").
    Description("Sales fact table for OLAP analysis").
    Register()
```

Methods:
- `TableName(string)` - Underlying database table name
- `Measure(name string, aggregates ...string)` - Add a measure (specify aggregation functions like "sum", "avg", "count", "min", "max")
- `Dimension(name, jsonPath, dataType string)` - Add a dimension with JSON path and data type
- `Description(string)` - Set description
- `Config(map[string]interface{})` - Set custom configuration
- `Register()` - Register the fact table

### Aggregate Query Builder

```go
fraiseql.NewAggregateQueryConfig("salesByCategory").
    FactTableName("sales").
    AutoGroupBy(true).
    AutoAggregates(true).
    Description("Sales aggregated by category").
    Register()
```

Methods:
- `FactTableName(string)` - Reference the fact table to aggregate
- `AutoGroupBy(bool)` - Enable automatic GROUP BY generation
- `AutoAggregates(bool)` - Enable automatic aggregate function generation
- `Description(string)` - Set description
- `Config(map[string]interface{})` - Set custom configuration
- `Register()` - Register the aggregate query

## Testing

Run the test suite:

```bash
make test
```

Run with verbose output:

```bash
make test-verbose
```

Generate coverage report:

```bash
make coverage
# Opens coverage.html
```

## Development

### Code Quality

Format code:
```bash
make fmt
```

Check formatting:
```bash
make fmt-check
```

Run linter (requires golangci-lint):
```bash
make lint
```

Run go vet:
```bash
make vet
```

### Building

```bash
make build
```

### Running Examples

```bash
make examples
```

## Integration with FraiseQL Ecosystem

After exporting `schema.json`:

1. **Compile schema**: `fraiseql-cli compile schema.json`
2. **Start server**: `fraiseql-server --schema schema.compiled.json`
3. **Send GraphQL queries**: Execute queries against the compiled schema

The Go authoring layer is **completely decoupled from runtime** - it only generates JSON. The Rust compiler and server handle all execution.

## Comparison with Python/TypeScript

| Feature | Go | Python | TypeScript |
|---------|----|---------|----|
| **Type Safety** | Static (compile-time) | Dynamic (runtime hints) | Static (compile-time) |
| **API Style** | Builder pattern + struct tags | Decorators | Decorators |
| **Null Handling** | Pointer types (`*T`) | Union with `None` | `null` |
| **Lists** | `[]T` | `list[T]` | `T[]` |
| **Configuration** | Method chaining | Decorator args | Method registration |
| **Runtime Overhead** | None (build time) | None (build time) | None (build time) |

## License

MIT License - See LICENSE file for details

## Contributing

Contributions welcome! Please open issues and pull requests on GitHub.

---

**FraiseQL v2**: Compiled GraphQL execution engine
- [Official Documentation](https://fraiseql.dev)
- [GitHub](https://github.com/fraiseql/fraiseql)
- [Issue Tracker](https://github.com/fraiseql/fraiseql/issues)
