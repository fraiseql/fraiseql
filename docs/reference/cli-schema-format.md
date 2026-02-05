# FraiseQL CLI Schema Format Guide

## Overview

The `fraiseql-cli` command-line tool compiles FraiseQL schemas from JSON format into optimized SQL templates and execution plans. This document describes the schema format, compilation process, and usage patterns.

## Schema Format

### Basic Structure

```json
{
  "types": [
    {
      "name": "User",
      "fields": [
        { "name": "id", "type": "Int", "nullable": false },
        { "name": "name", "type": "String", "nullable": false },
        { "name": "email", "type": "String", "nullable": true }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "arguments": [
        { "name": "limit", "type": "Int", "default": 10 }
      ],
      "return_type": "User",
      "return_list": true,
      "sql_source": "v_users"
    }
  ],
  "mutations": [],
  "fact_tables": []
}
```text

### Type Definition

Every type must have:

- `name`: Unique type name
- `fields`: Array of field definitions

Each field has:

- `name`: Field identifier
- `type`: GraphQL type (Int, String, Float, Boolean, or custom type name)
- `nullable`: Whether field can be null (default: true)

### Query Definition

Each query has:

- `name`: Query name (must be unique)
- `arguments`: Array of input arguments (optional)
- `return_type`: Type name or scalar type
- `return_list`: Whether query returns a list (default: false)
- `sql_source`: Database view or function name (required for database queries)

### Mutation Definition

Similar to queries but for write operations:

- `name`: Mutation name
- `arguments`: Input arguments (required)
- `return_type`: Return type
- `sql_source`: Stored procedure or function name

### Fact Table Definition

For analytics schemas:

- `name`: Fact table name (convention: starts with `tf_`)
- `table_name`: Actual SQL table name
- `measures`: List of numeric columns for aggregation
- `dimension_column`: JSONB column name (default: "data")
- `dimension_paths`: Optional array of dimension definitions

## Language Generator Output Formats

All language generators produce compatible JSON schemas:

### Python

```python
@fraiseql.type
class User:
    id: int
    name: str
    email: str | None

@fraiseql.query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    pass

fraiseql.export_schema("schema.json")
```text

Output: Valid `schema.json` for CLI compilation

### TypeScript

```typescript
@Type()
class User {
  id!: number;
  name!: string;
  email?: string;
}

@Query(sql_source = "v_users")
users(limit?: number): User[] { /* ... */ }

ExportSchema("schema.json");
```text

Output: Valid `schema.json` for CLI compilation

### Go

```go
type User struct {
    ID    int    `fraiseql:"id"`
    Name  string `fraiseql:"name"`
    Email *string `fraiseql:"email"`
}

type UserQuery struct {
    Users []User `fraiseql:"query,sql_source=v_users"`
}

ExportSchema("schema.json")
```text

Output: Valid `schema.json` for CLI compilation

## Compilation Process

### Step 1: Validate Schema

The CLI validates:

- All type names are unique
- All referenced types exist
- Required fields are present
- SQL sources are valid identifiers

```bash
fraiseql-cli compile schema.json
```text

Output includes validation warnings and suggestions:

```text
⚠️  Warnings (2):
   Query 'posts' returns a list but has no sql_source
   Query 'users' returns a list but has no sql_source
```text

### Step 2: Generate Compiled Schema

The compiler produces `schema.compiled.json`:

```json
{
  "version": "2.0.0",
  "types": [...],
  "queries": [...],
  "sql_templates": {
    "v_users": "SELECT ... FROM v_users WHERE ...",
    "v_posts": "SELECT ... FROM v_posts WHERE ..."
  },
  "metadata": {
    "generated_at": "2026-01-16T09:59:00Z",
    "source_hash": "abc123"
  }
}
```text

### Step 3: Optimization Suggestions

The compiler provides optimization hints:

```text
📊 Optimization Suggestions:

  Indexes:
  • Query 'posts': List query with arguments benefits from index
    Columns: authorId, published, limit, offset
```text

## Usage Examples

### Basic Schema Compilation

```bash
# Compile schema from Python generator
cd fraiseql-python
python -c "from fraiseql import export_schema; export_schema('schema.json')"
cd ..

# Compile with CLI
fraiseql-cli compile fraiseql-python/schema.json -o schema.compiled.json
```text

### With Custom Output Path

```bash
fraiseql-cli compile schema.json --output compiled.json
```text

### Validate Only (no output)

```bash
fraiseql-cli validate schema.json
```text

## Schema Format Compatibility

All 5 language generators produce compatible schemas:

| Language | Status | Notes |
|----------|--------|-------|
| Python | ✅ Fully compatible | Modern type hints, full feature support |
| TypeScript | ✅ Fully compatible | Decorator support, full feature support |
| Go | ✅ Fully compatible | Struct tags, full feature support |
| Java | ✅ Fully compatible | Annotations, full feature support |
| PHP | ✅ Fully compatible | Attributes, full feature support |

## Common Issues and Solutions

### Issue: "No compiler is provided"

**Cause**: The CLI was not built

**Solution**:

```bash
cargo build --release -p fraiseql-cli
export PATH="$(pwd)/target/release:$PATH"
```text

### Issue: SQL source not recognized

**Cause**: Query references non-existent SQL function/view

**Solution**: Verify database schema or remove `sql_source` for testing

### Issue: Type not found

**Cause**: Mutation or query references undefined type

**Solution**: Add missing type to schema's `types` array

## Runtime Compilation

The compiled schema is consumed by the FraiseQL runtime server:

```bash
fraiseql-server --schema schema.compiled.json --port 4000
```text

The server then:

1. Loads compiled schema
2. Accepts GraphQL queries
3. Executes optimized SQL
4. Returns results

## Best Practices

1. **Keep schemas modular**: Separate concerns by domain
2. **Use meaningful names**: Clear type and query names aid debugging
3. **Include SQL sources**: Essential for production queries
4. **Document complex queries**: Add comments in schema
5. **Version schemas**: Track schema changes in git
6. **Test compilation regularly**: Catch errors early

## Advanced Features

### Analytics (Fact Tables)

```json
{
  "fact_tables": [
    {
      "name": "tf_sales",
      "table_name": "fact_sales",
      "measures": ["revenue", "quantity"],
      "dimension_column": "dimensions"
    }
  ]
}
```text

### Type Extensions

```json
{
  "types": [
    {
      "name": "Post",
      "fields": [...],
      "extends": "Node"
    }
  ]
}
```text

### Subscriptions

```json
{
  "subscriptions": [
    {
      "name": "onUserCreated",
      "return_type": "User",
      "triggers": ["user.created"]
    }
  ]
}
```text

## See Also

- [Language Generators Guide](../guides/language-generators.md)
- [E2E Testing Guide](../guides/development/e2e-testing.md)
- [GraphQL Specification](https://spec.graphql.org)
