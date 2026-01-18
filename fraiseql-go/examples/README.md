# FraiseQL Go Examples

This directory contains examples demonstrating different use cases of the FraiseQL Go authoring library.

## Examples

### 1. Basic Schema (`basic_schema.go`)

Demonstrates fundamental schema definition with queries and mutations.

**Features:**

- Type definitions with struct tags
- Query builders (list and single-item queries)
- Mutation builders (create, update, delete operations)
- Pagination parameters
- Configuration for SQL sources

**Run:**

```bash
go run basic_schema.go
```

**Output:** `schema.json` with Users and Posts types, 3 queries, and 5 mutations.

### 2. Analytics Schema (`analytics_schema.go`)

Demonstrates OLAP (Online Analytical Processing) schema with fact tables and aggregate queries.

**Features:**

- Fact table definitions with measures and dimensions
- Multiple fact tables (Sales and Events)
- Aggregate queries with automatic grouping and aggregation
- Measure definitions (sum, avg, count, min, max)
- Dimension definitions with SQL paths and data types

**Run:**

```bash
go run analytics_schema.go
```

**Output:** `schema.json` with 2 fact tables (Sales, Events) and 6 aggregate queries for analytics.

### 3. Complete Schema (`complete_schema.go`)

Demonstrates a complete production-like schema combining all features.

**Features:**

- Multiple types (User, Post, Revenue)
- Complete CRUD query and mutation sets
- Fact tables with multiple measures and dimensions
- Multiple aggregate queries for different analysis patterns
- Comprehensive documentation and next steps

**Run:**

```bash
go run complete_schema.go
```

**Output:** `schema.json` with 3 types, 4 queries, 7 mutations, 1 fact table, and 4 aggregate queries.

## Understanding the Examples

### Type Definition Pattern

All examples define Go structs with `fraiseql` struct tags:

```go
type User struct {
    ID        int    `fraiseql:"id,type=Int"`
    Name      string `fraiseql:"name,type=String"`
    Email     string `fraiseql:"email,type=String"`
    CreatedAt string `fraiseql:"created_at,type=String"`
}
```

Struct tag format: `fraiseql:"<field_name>,type=<graphql_type>"`

### Query Builder Pattern

Queries are defined using a fluent builder:

```go
fraiseql.NewQuery("users").
    ReturnType(User{}).
    ReturnsArray(true).
    Config(map[string]interface{}{
        "sql_source": "v_user",
    }).
    Arg("limit", "Int", 10).
    Arg("offset", "Int", 0).
    Description("Get all users").
    Register()
```

### Mutation Builder Pattern

Mutations are defined similarly with operation types:

```go
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
```

### Fact Table Pattern

Fact tables are used for analytics workloads:

```go
fraiseql.NewFactTable("sales").
    TableName("tf_sales").
    Measure("revenue", "sum", "avg", "max").
    Measure("quantity", "sum", "count").
    Dimension("category", "data->>'category'", "text").
    Dimension("region", "data->>'region'", "text").
    Description("Sales fact table for OLAP").
    Register()
```

### Aggregate Query Pattern

Aggregate queries reference fact tables:

```go
fraiseql.NewAggregateQueryConfig("salesByCategory").
    FactTableName("sales").
    AutoGroupBy(true).
    AutoAggregates(true).
    Description("Sales by category").
    Register()
```

## Next Steps

After generating a `schema.json` file:

1. **Validate the schema:**

   ```bash
   fraiseql-cli validate schema.json
   ```

2. **Compile the schema:**

   ```bash
   fraiseql-cli compile schema.json -o schema.compiled.json
   ```

3. **Start a FraiseQL server:**

   ```bash
   fraiseql-server --schema schema.compiled.json --port 8000
   ```

4. **Test with GraphQL:**

   ```bash
   curl -X POST http://localhost:8000/graphql \
     -H "Content-Type: application/json" \
     -d '{"query":"{ users(limit:10) { id name email } }"}'
   ```

## Tips

- **Type Safety**: Always use the correct Go types; they map directly to GraphQL types
- **Nullable Fields**: Use pointers (`*int`, `*string`) for nullable types
- **Field Names**: Override struct field names with the first part of the fraiseql tag
- **SQL Sources**: Configure `sql_source` to point to database views or functions
- **Caching**: Set `sql_source` for queries to enable query caching and optimization
- **Testing**: Run `go run <example>.go` to validate your schema definition

## Common Patterns

### Pagination Query

```go
fraiseql.NewQuery("users").
    ReturnType(User{}).
    ReturnsArray(true).
    Arg("limit", "Int", 10).
    Arg("offset", "Int", 0).
    // ...
```

### Single Item Query

```go
fraiseql.NewQuery("user").
    ReturnType(User{}).
    Arg("id", "Int", nil).
    // ...
```

### Create Mutation

```go
fraiseql.NewMutation("createUser").
    ReturnType(User{}).
    Arg("name", "String", nil).
    Arg("email", "String", nil).
    // ...
```

### Update Mutation

```go
fraiseql.NewMutation("updateUser").
    ReturnType(User{}).
    Arg("id", "Int", nil).
    Arg("name", "String", nil, true).  // nullable
    // ...
```

### Delete Mutation

```go
fraiseql.NewMutation("deleteUser").
    ReturnType(User{}).
    Arg("id", "Int", nil).
    // ...
```

## Troubleshooting

**Schema generation fails:**

- Check that all types are registered with `RegisterTypes()`
- Verify struct field names match the fraiseql tags
- Ensure all GraphQL types are valid

**Empty queries/mutations in schema:**

- Make sure `Register()` is called on all builders
- Verify the builders are defined in `init()` function
- Check for runtime errors in the output

## For More Information

See the main README at `../README.md` for complete API documentation.
