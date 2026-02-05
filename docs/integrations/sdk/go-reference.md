# FraiseQL Go SDK Reference

**Status**: Production-Ready | **Go Version**: 1.22+ | **SDK Version**: 2.0.0+
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Go SDK. This guide covers the Go authoring interface for building type-safe GraphQL APIs with struct tags, builder patterns, and idiomatic Go.

## Quick Start

```bash
# Installation
go get github.com/fraiseql/fraiseql-go

# Or with Go workspaces
go get -u github.com/fraiseql/fraiseql-go@latest
```

**Requirements**:
- Go 1.22 or later
- Standard library (no external runtime dependencies)
- Optional: `github.com/fraiseql/fraiseql-go/tools` for code generation

**First Schema** (30 seconds):

```go
package main

import (
	"github.com/fraiseql/fraiseql-go/fraiseql"
)

type User struct {
	ID    int    `fraiseql:"id,type=Int"`
	Name  string `fraiseql:"name,type=String"`
	Email string `fraiseql:"email,type=String"`
}

func init() {
	fraiseql.NewQuery("users").
		ReturnType(User{}).
		ReturnsArray(true).
		Config(map[string]interface{}{
			"sql_source": "v_users",
		}).
		Arg("limit", "Int", 10).
		Description("Get all users").
		Register()
}

func main() {
	if err := fraiseql.ExportSchema("schema.json"); err != nil {
		panic(err)
	}
}
```

Export and deploy:

```bash
go run main.go                              # Generates schema.json
fraiseql-cli compile schema.json
fraiseql-server --schema schema.compiled.json
```

---

## Quick Reference Table

| Feature | Builder | Purpose | Returns |
|---------|---------|---------|---------|
| **Types** | `RegisterTypes()` | GraphQL object types | JSON schema |
| **Queries** | `NewQuery()` | Read operations (SELECT) | Single or list |
| **Mutations** | `NewMutation()` | Write operations (INSERT/UPDATE/DELETE) | Type result |
| **Fact Tables** | `NewFactTable()` | Analytics tables (OLAP) | Aggregation schema |
| **Aggregate Queries** | `NewAggregateQueryConfig()` | Analytics queries | Aggregated results |
| **Field Metadata** | Struct tags | Column definitions | Type metadata |

---

## Type System

### Struct Tags

Define GraphQL fields using Go struct tags in the `fraiseql` namespace:

```go
type Product struct {
	ID       int     `fraiseql:"id,type=Int"`
	Name     string  `fraiseql:"name,type=String"`
	Price    float64 `fraiseql:"price,type=Float"`
	InStock  bool    `fraiseql:"in_stock,type=Boolean"`
	IsActive *bool   `fraiseql:"is_active,type=Boolean,nullable=true"`
}
```

**Tag Format**: `fraiseql:"<field_name>,type=<type>[,nullable=<true|false>]"`

**Parameters**:
- `field_name`: GraphQL field name (maps snake_case to camelCase)
- `type`: GraphQL scalar or custom type (required)
- `nullable`: Allow null values (optional, inferred from pointer types)

### Type Mapping: Go ↔ GraphQL

| Go Type | GraphQL Type | Nullable | Notes |
|---------|-------------|----------|-------|
| `int` | `Int` | No | 32/64-bit signed |
| `*int` | `Int` | Yes | Pointer for nullability |
| `int32` | `Int` | No | Explicit 32-bit |
| `int64` | `Int` | No | Explicit 64-bit |
| `float64` | `Float` | No | IEEE 754 |
| `*float64` | `Float` | Yes | Nullable float |
| `string` | `String` | No | UTF-8 text |
| `*string` | `String` | Yes | Nullable string |
| `bool` | `Boolean` | No | True/False |
| `*bool` | `Boolean` | Yes | Nullable bool |
| `[]T` | `[T]` | No | Non-nullable list |
| `*[]T` | `[T]` | Yes | Nullable list |
| `time.Time` | `String` | No | ISO 8601 format |
| `*time.Time` | `String` | Yes | Nullable time |
| Custom struct | Custom Type | No | Registered type |
| `*CustomType` | Custom Type | Yes | Nullable type |

### Advanced Type Patterns

```go
// With nested types
type Address struct {
	Street    string `fraiseql:"street,type=String"`
	City      string `fraiseql:"city,type=String"`
	State     string `fraiseql:"state,type=String"`
	PostalCode string `fraiseql:"postal_code,type=String"`
}

type Company struct {
	ID          int       `fraiseql:"id,type=Int"`
	Name        string    `fraiseql:"name,type=String"`
	Address     Address   `fraiseql:"address,type=Address"`
	Employees   []User    `fraiseql:"employees,type=[User]"`
	Description *string   `fraiseql:"description,type=String,nullable=true"`
}

// Type registration
func init() {
	fraiseql.RegisterTypes(User{}, Address{}, Company{})
}
```

---

## Operations

### Query Builder

Define read-only operations mapping to SQL views:

```go
fraiseql.NewQuery("users").
	ReturnType(User{}).
	ReturnsArray(true).
	Nullable(false).
	Config(map[string]interface{}{
		"sql_source": "v_users",
		"auto_params": map[string]bool{
			"limit":  true,
			"offset": true,
		},
	}).
	Arg("limit", "Int", 10).
	Arg("offset", "Int", 0).
	Arg("active", "Boolean", nil).
	Description("Get all users with pagination").
	Register()
```

**Builder Methods**:
- `ReturnType(type)` - Return type (required)
- `ReturnsArray(bool)` - Returns list (default: false)
- `Nullable(bool)` - Result nullable (default: false)
- `Config(map)` - SQL configuration
- `Arg(name, type, default)` - Add argument
- `Description(string)` - GraphQL description
- `Register()` - Finalize and register

**Examples**:

```go
// Single result
fraiseql.NewQuery("user").
	ReturnType(User{}).
	Config(map[string]interface{}{"sql_source": "v_user_by_id"}).
	Arg("id", "Int", nil).
	Description("Get user by ID").
	Register()

// List with defaults
fraiseql.NewQuery("users").
	ReturnType(User{}).
	ReturnsArray(true).
	Config(map[string]interface{}{"sql_source": "v_users"}).
	Arg("limit", "Int", 20).
	Arg("offset", "Int", 0).
	Description("Paginated user list").
	Register()

// With filters
fraiseql.NewQuery("searchUsers").
	ReturnType(User{}).
	ReturnsArray(true).
	Config(map[string]interface{}{"sql_source": "v_search_users"}).
	Arg("name", "String", nil).
	Arg("email", "String", nil).
	Arg("isActive", "Boolean", true).
	Description("Search users").
	Register()
```

### Mutation Builder

Define write operations mapping to SQL functions:

```go
fraiseql.NewMutation("createUser").
	ReturnType(User{}).
	Config(map[string]interface{}{
		"sql_source": "fn_create_user",
		"operation":  "CREATE",
	}).
	Arg("name", "String", nil).
	Arg("email", "String", nil).
	Description("Create new user").
	Register()
```

**Builder Methods** (same as Query):
- `ReturnType(type)` - Required
- `Config(map)` - SQL config with `operation` field
- `Arg(name, type, default)` - Arguments
- `Description(string)` - Docs

**Examples**:

```go
// Create
fraiseql.NewMutation("createUser").
	ReturnType(User{}).
	Config(map[string]interface{}{
		"sql_source": "fn_create_user",
		"operation":  "CREATE",
	}).
	Arg("name", "String", nil).
	Arg("email", "String", nil).
	Register()

// Update
fraiseql.NewMutation("updateUser").
	ReturnType(User{}).
	Config(map[string]interface{}{
		"sql_source": "fn_update_user",
		"operation":  "UPDATE",
	}).
	Arg("id", "Int", nil).
	Arg("email", "String", nil).
	Register()

// Delete
fraiseql.NewMutation("deleteUser").
	ReturnType(BoolResult{}).
	Config(map[string]interface{}{
		"sql_source": "fn_delete_user",
		"operation":  "DELETE",
	}).
	Arg("id", "Int", nil).
	Register()
```

---

## Advanced Features

### Fact Tables (Analytics)

Define OLAP tables with measures and dimensions:

```go
fraiseql.NewFactTable("sales").
	TableName("tf_sales").
	Measure("revenue", "sum", "avg", "max").
	Measure("quantity", "sum", "count", "avg").
	Dimension("category", "data->>'category'", "text").
	Dimension("region", "data->>'region'", "text").
	Dimension("year_month", "date_trunc('month', occurred_at)::text", "text").
	Config(map[string]interface{}{
		"denormalized_columns": []string{"customer_id", "created_at"},
	}).
	Description("Sales fact table for OLAP analysis").
	Register()
```

**Builder Methods**:
- `TableName(string)` - Database table name (required)
- `Measure(name, aggs...)` - Numeric aggregatable field
- `Dimension(name, path, dataType)` - Dimension with JSON path
- `Config(map)` - Custom configuration
- `Description(string)` - Docs
- `Register()` - Finalize

### Aggregate Query

```go
fraiseql.NewAggregateQueryConfig("salesByCategory").
	FactTableName("sales").
	AutoGroupBy(true).
	AutoAggregates(true).
	Description("Sales aggregated by category").
	Register()
```

**Builder Methods**:
- `FactTableName(string)` - Reference fact table (required)
- `AutoGroupBy(bool)` - Enable auto GROUP BY
- `AutoAggregates(bool)` - Enable auto aggregates
- `Config(map)` - Custom config
- `Register()` - Finalize

### Struct Field Metadata

Use struct tags for additional field configuration:

```go
type SalesMetrics struct {
	ID        int64     `fraiseql:"id,type=Int"`
	Revenue   float64   `fraiseql:"revenue,type=Float,measure=sum;avg;max"`
	Quantity  int       `fraiseql:"quantity,type=Int,measure=sum;count"`
	CreatedAt time.Time `fraiseql:"created_at,type=String"`
}
```

---

## Scalar Types

FraiseQL Go SDK maps to standard Go types:

| Go Type | GraphQL Type | Example |
|---------|-------------|---------|
| `int`, `int32`, `int64` | `Int` | `42` |
| `float64` | `Float` | `3.14` |
| `string` | `String` | `"hello"` |
| `bool` | `Boolean` | `true` |
| `time.Time` | `String` (ISO 8601) | `"2026-02-05T14:30:00Z"` |
| `*int` | `Int` (nullable) | `nil` or `42` |
| `[]string` | `[String!]!` | `[]string{"a", "b"}` |
| `*[]int` | `[Int]` (nullable) | `nil` or list |

---

## Schema Export

### Programmatic Export

```go
package main

import (
	"log"
	"github.com/fraiseql/fraiseql-go/fraiseql"
)

func init() {
	// Define types and operations
	fraiseql.RegisterTypes(User{}, Post{})
	// ... queries and mutations ...
}

func main() {
	// Export to file
	if err := fraiseql.ExportSchema("schema.json"); err != nil {
		log.Fatal(err)
	}

	// Or export to string
	schema, err := fraiseql.ExportSchemaString()
	if err != nil {
		log.Fatal(err)
	}
	log.Println(schema)
}
```

### CLI Workflow

```bash
# 1. Run Go program to generate schema.json
go run main.go

# 2. Compile with fraiseql-cli
fraiseql-cli compile schema.json

# 3. Deploy compiled schema
fraiseql-server --schema schema.compiled.json
```

---

## Type Mapping Reference

### Nullability Rules

| Go | GraphQL | Meaning |
|----|---------|---------|
| `int` | `Int!` | Required, non-null |
| `*int` | `Int` | Optional, nullable |
| `[]int` | `[Int!]!` | Required list of non-null |
| `[]*int` | `[Int]!` | Required list of nullable |
| `*[]int` | `[Int!]` | Nullable list |

### Container Types

```go
// List of ints
type Result struct {
	IDs []int `fraiseql:"ids,type=[Int]"`
}

// Nullable list
type Optional struct {
	Items *[]string `fraiseql:"items,type=[String],nullable=true"`
}

// Nested objects
type Nested struct {
	User User   `fraiseql:"user,type=User"`
	Tags []string `fraiseql:"tags,type=[String]"`
}
```

---

## Common Patterns

### CRUD Operations

```go
type Todo struct {
	ID    int    `fraiseql:"id,type=Int"`
	Title string `fraiseql:"title,type=String"`
	Done  bool   `fraiseql:"done,type=Boolean"`
}

func init() {
	// Create
	fraiseql.NewMutation("createTodo").
		ReturnType(Todo{}).
		Config(map[string]interface{}{"sql_source": "fn_create_todo", "operation": "CREATE"}).
		Arg("title", "String", nil).
		Register()

	// Read
	fraiseql.NewQuery("todo").
		ReturnType(Todo{}).
		Config(map[string]interface{}{"sql_source": "v_todo_by_id"}).
		Arg("id", "Int", nil).
		Register()

	// Update
	fraiseql.NewMutation("updateTodo").
		ReturnType(Todo{}).
		Config(map[string]interface{}{"sql_source": "fn_update_todo", "operation": "UPDATE"}).
		Arg("id", "Int", nil).
		Arg("done", "Boolean", nil).
		Register()

	// Delete
	fraiseql.NewMutation("deleteTodo").
		ReturnType(BoolResult{}).
		Config(map[string]interface{}{"sql_source": "fn_delete_todo", "operation": "DELETE"}).
		Arg("id", "Int", nil).
		Register()
}
```

### Pagination Pattern

```go
type PageInfo struct {
	HasNext     bool `fraiseql:"has_next,type=Boolean"`
	HasPrevious bool `fraiseql:"has_previous,type=Boolean"`
	TotalCount  int  `fraiseql:"total_count,type=Int"`
	PageSize    int  `fraiseql:"page_size,type=Int"`
}

type UserConnection struct {
	Items    []User   `fraiseql:"items,type=[User]"`
	PageInfo PageInfo `fraiseql:"page_info,type=PageInfo"`
}

func init() {
	fraiseql.NewQuery("users").
		ReturnType(UserConnection{}).
		Config(map[string]interface{}{"sql_source": "v_users_paginated"}).
		Arg("limit", "Int", 20).
		Arg("offset", "Int", 0).
		Register()
}
```

### Error Handling in Go

```go
func initSchema() error {
	if err := fraiseql.RegisterTypes(User{}, Post{}); err != nil {
		return fmt.Errorf("failed to register types: %w", err)
	}

	if err := fraiseql.ExportSchema("schema.json"); err != nil {
		return fmt.Errorf("failed to export schema: %w", err)
	}

	return nil
}

func main() {
	if err := initSchema(); err != nil {
		log.Fatalf("initialization failed: %v", err)
	}
}
```

---

## Testing

### Table-Driven Tests

```go
func TestQueryRegistration(t *testing.T) {
	tests := []struct {
		name    string
		query   string
		expects string
	}{
		{"users query", "users", "users"},
		{"user by id", "user", "user"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			schema, _ := fraiseql.ExportSchemaString()
			if !contains(schema, tt.expects) {
				t.Errorf("expected %s in schema", tt.expects)
			}
		})
	}
}
```

### Schema Validation

```go
func TestSchemaValidity(t *testing.T) {
	schema, err := fraiseql.ExportSchemaString()
	if err != nil {
		t.Fatalf("export failed: %v", err)
	}

	var data map[string]interface{}
	if err := json.Unmarshal([]byte(schema), &data); err != nil {
		t.Fatalf("invalid JSON schema: %v", err)
	}

	if _, ok := data["types"]; !ok {
		t.Error("schema missing 'types' field")
	}
	if _, ok := data["queries"]; !ok {
		t.Error("schema missing 'queries' field")
	}
}
```

---

## Error Handling

Common errors and recovery patterns:

```go
import "github.com/fraiseql/fraiseql-go/fraiseql"

// Type registration errors
if err := fraiseql.RegisterTypes(MyType{}); err != nil {
	// Handle duplicate type, invalid struct, etc.
	log.Fatal("Type registration failed:", err)
}

// Export errors
if err := fraiseql.ExportSchema("schema.json"); err != nil {
	// Handle file I/O, validation errors
	log.Fatal("Schema export failed:", err)
}

// Query builder errors (builder pattern validates at Register)
fraiseql.NewQuery("users").
	ReturnType(User{}). // Must be registered type
	Config(map[string]interface{}{"sql_source": "v_users"}).
	Register() // Panics if validation fails
```

---

## Best Practices

### Type Definition

1. **Export types publicly**: Use PascalCase (`User` not `user`)
2. **Use struct tags consistently**: Always include field name and type
3. **Group related types**: Keep domain types together
4. **Document with comments**: Becomes GraphQL descriptions

```go
// User represents a system user account
type User struct {
	ID    int    `fraiseql:"id,type=Int"`
	Name  string `fraiseql:"name,type=String"`
	Email string `fraiseql:"email,type=String"`
}
```

### Builder Pattern

1. **Chain methods for clarity**: Improves readability
2. **Always set required fields**: `ReturnType`, `sql_source`
3. **Call `Register()` last**: Finalizes builder
4. **Use init() for registration**: Runs at package load time

```go
func init() {
	fraiseql.NewQuery("users").
		ReturnType(User{}).
		ReturnsArray(true).
		Config(map[string]interface{}{"sql_source": "v_users"}).
		Arg("limit", "Int", 20).
		Description("Get paginated users").
		Register()
}
```

### Concurrency

Go SDK is thread-safe for schema export during `init()`:

```go
// Safe to call from goroutines after init completes
go func() {
	schema, _ := fraiseql.ExportSchemaString()
	// Process schema
}()
```

---

## See Also

- **Architecture Guide**: [FraiseQL Architecture Principles](../../guides/ARCHITECTURE_PRINCIPLES.md)
- **Python SDK**: [Python Reference](./python-reference.md)
- **TypeScript SDK**: [TypeScript Reference](./typescript-reference.md)
- **Database Patterns**: [SQL View & Function Patterns](../../guides/database-patterns.md)
- **Analytics Guide**: [Fact Tables & OLAP](../../guides/analytics-olap.md)
- **GitHub**: [fraiseql-go repository](https://github.com/fraiseql/fraiseql-go)

---

**Status**: ✅ Production Ready
**Last Updated**: 2026-02-05
**Maintained By**: FraiseQL Community
**License**: MIT
