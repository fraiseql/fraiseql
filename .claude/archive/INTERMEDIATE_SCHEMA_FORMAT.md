# FraiseQL Intermediate Schema Format Specification

**Version**: 2.0.0
**Purpose**: Language-agnostic schema representation for cross-language compilation
**Audience**: Language library authors, CLI developers

## Overview

The intermediate schema format (`schema.json`) is the universal bridge between language-specific authoring libraries and the FraiseQL compiler. All language libraries (Python, TypeScript, Go, Ruby, etc.) must output this exact format.

## Design Principles

1. **Language-Agnostic**: No bias toward any programming language
2. **Simple**: Easy to generate from any language
3. **Validated**: CLI validates structure before compilation
4. **Extensible**: Can add fields without breaking existing parsers
5. **Documented**: Clear specification for library authors

## Schema Structure

```json
{
  "version": "2.0.0",
  "types": [ /* TypeDefinition[] */ ],
  "queries": [ /* QueryDefinition[] */ ],
  "mutations": [ /* MutationDefinition[] */ ],
  "fact_tables": [ /* FactTableDefinition[] */ ],
  "aggregate_queries": [ /* AggregateQueryDefinition[] */ ]
}
```

### Top-Level Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `version` | string | No | Schema format version (default: "2.0.0") |
| `types` | array | Yes | GraphQL object types |
| `queries` | array | Yes | GraphQL query operations |
| `mutations` | array | No | GraphQL mutation operations |
| `fact_tables` | array | No | Analytics fact table definitions |
| `aggregate_queries` | array | No | Analytics aggregate query definitions |

## TypeDefinition

Represents a GraphQL object type.

```json
{
  "name": "User",
  "fields": [
    {
      "name": "id",
      "type": "Int",
      "nullable": false
    },
    {
      "name": "email",
      "type": "String",
      "nullable": false
    },
    {
      "name": "bio",
      "type": "String",
      "nullable": true
    }
  ],
  "description": "User account"
}
```

### Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Type name (PascalCase recommended) |
| `fields` | array | Yes | List of fields |
| `description` | string | No | Type description from docstring |

### FieldDefinition

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Field name (camelCase recommended) |
| `type` | string | Yes | GraphQL type name |
| `nullable` | boolean | Yes | Whether field can be null |

**Note**: Use `type`, not `field_type`. The CLI will normalize.

### Supported Type Names

**Scalars**:

- `Int` - 32-bit integer
- `Float` - Double-precision float
- `String` - UTF-8 string
- `Boolean` - true/false
- `ID` - Unique identifier (string or int)

**Custom Types**: Any defined `TypeDefinition.name`

## QueryDefinition

Represents a GraphQL query operation.

```json
{
  "name": "users",
  "return_type": "User",
  "returns_list": true,
  "nullable": false,
  "arguments": [
    {
      "name": "limit",
      "type": "Int",
      "nullable": false,
      "default": 10
    },
    {
      "name": "offset",
      "type": "Int",
      "nullable": false,
      "default": 0
    }
  ],
  "description": "Get users with pagination",
  "sql_source": "v_user",
  "auto_params": {
    "limit": true,
    "offset": true,
    "where": true,
    "order_by": true
  }
}
```

### Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Query name (camelCase) |
| `return_type` | string | Yes | Return type name |
| `returns_list` | boolean | Yes | Whether query returns array |
| `nullable` | boolean | Yes | Whether result can be null |
| `arguments` | array | No | Query arguments |
| `description` | string | No | Query description |
| `sql_source` | string | No | SQL table/view name |
| `auto_params` | object | No | Auto-generated parameters |

### ArgumentDefinition

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Argument name (camelCase) |
| `type` | string | Yes | GraphQL type name |
| `nullable` | boolean | Yes | Whether argument is optional |
| `default` | any | No | Default value (must match type) |

## MutationDefinition

Represents a GraphQL mutation operation.

```json
{
  "name": "createUser",
  "return_type": "User",
  "returns_list": false,
  "nullable": false,
  "arguments": [
    {
      "name": "name",
      "type": "String",
      "nullable": false
    },
    {
      "name": "email",
      "type": "String",
      "nullable": false
    }
  ],
  "description": "Create a new user",
  "sql_source": "fn_create_user",
  "operation": "CREATE"
}
```

### Fields

Same as `QueryDefinition` plus:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `operation` | string | No | Operation type: CREATE, UPDATE, DELETE, CUSTOM |

## FactTableDefinition (Analytics)

Represents an analytics fact table.

```json
{
  "table_name": "tf_sales",
  "measures": [
    {
      "name": "revenue",
      "sql_type": "Float",
      "nullable": false
    },
    {
      "name": "quantity",
      "sql_type": "Int",
      "nullable": false
    }
  ],
  "dimensions": {
    "name": "data",
    "paths": [
      {
        "name": "category",
        "json_path": "data->>'category'",
        "data_type": "text"
      }
    ]
  },
  "denormalized_filters": [
    {
      "name": "customer_id",
      "sql_type": "Text",
      "indexed": true
    },
    {
      "name": "occurred_at",
      "sql_type": "Timestamp",
      "indexed": true
    }
  ]
}
```

### Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `table_name` | string | Yes | SQL table name (must start with `tf_`) |
| `measures` | array | Yes | Aggregatable numeric columns |
| `dimensions` | object | Yes | JSONB dimension column metadata |
| `denormalized_filters` | array | Yes | Indexed filter columns |

## Language-Specific Mapping Guide

### Python (Current)

**Already using intermediate format** ✅

```python
# Python uses standard field names
{
  "name": "id",
  "type": "Int",  # ✅ Correct
  "nullable": false
}
```

### TypeScript (Future)

**Should output intermediate format**:

```typescript
// TypeScript decorator
@fraiseql.type
class User {
  id: number;     // → type: "Int"
  name: string;   // → type: "String"
  email?: string; // → type: "String", nullable: true
}

// Outputs schema.json:
{
  "fields": [
    {"name": "id", "type": "Int", "nullable": false},
    {"name": "name", "type": "String", "nullable": false},
    {"name": "email", "type": "String", "nullable": true}
  ]
}
```

**Type Mappings**:

- `number` → `Int` or `Float` (infer from usage or annotation)
- `string` → `String`
- `boolean` → `Boolean`
- `T | null` or `T?` → `nullable: true`
- `T[]` → Use in `returns_list` context

### Go (Future)

**Should output intermediate format**:

```go
// Go struct tags
type User struct {
    ID    int    `fraiseql:"id,type=Int"`
    Name  string `fraiseql:"name,type=String"`
    Email *string `fraiseql:"email,type=String,nullable"`
}

// Outputs schema.json:
{
  "fields": [
    {"name": "id", "type": "Int", "nullable": false},
    {"name": "name", "type": "String", "nullable": false},
    {"name": "email", "type": "String", "nullable": true}
  ]
}
```

**Type Mappings**:

- `int`, `int32` → `Int`
- `int64` → `Int` (or custom `BigInt` scalar)
- `float32`, `float64` → `Float`
- `string` → `String`
- `bool` → `Boolean`
- `*T` (pointer) → `nullable: true`

### Ruby (Future)

**Should output intermediate format**:

```ruby
# Ruby DSL
class User < Fraiseql::Type
  field :id, type: Integer, nullable: false
  field :name, type: String, nullable: false
  field :email, type: String, nullable: true
end

# Outputs schema.json (same format)
```

### C# (Future)

**Should output intermediate format**:

```csharp
[FraiseQL.Type]
public class User {
    [Field(Type = "Int")]
    public int Id { get; set; }

    [Field(Type = "String")]
    public string Name { get; set; }

    [Field(Type = "String", Nullable = true)]
    public string? Email { get; set; }
}
```

## CLI Normalization

The CLI (`fraiseql-cli compile`) accepts the intermediate format and:

1. **Validates** structure matches specification
2. **Normalizes** field names (e.g., `type` → `field_type` internally if needed)
3. **Resolves** type references
4. **Optimizes** for runtime execution
5. **Outputs** `schema.compiled.json` (Rust-specific internal format)

### Normalization Rules

| Intermediate | Internal (Rust) | Notes |
|--------------|-----------------|-------|
| `type` | `field_type` | Avoids Rust keyword |
| `nullable` | `nullable` | No change |
| Field order | Sorted | Consistent output |

## Validation Rules

The CLI validates:

1. ✅ All `return_type` references exist in `types`
2. ✅ All `arguments[].type` are valid scalar or defined types
3. ✅ No circular type references
4. ✅ Fact tables have valid `tf_*` prefix
5. ✅ Measures are numeric types (`Int`, `Float`)
6. ✅ Required fields are present

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 2.0.0 | 2026-01-12 | Initial intermediate format specification |

## FAQ

### Why not use GraphQL SDL?

GraphQL SDL doesn't capture SQL mappings (`sql_source`), fact table metadata, or auto-params configuration. We need more than SDL provides.

### Why `type` instead of `field_type`?

`type` is universal across all languages. The CLI handles Rust keyword conflicts internally.

### Can I add custom fields?

Yes! Unknown fields are ignored by the CLI (forward compatibility). Document custom fields in your library.

### What about subscriptions?

Subscriptions will be added in a future version of this spec.

## Reference Implementation

See `fraiseql-python` package for a complete reference implementation:

- `/home/lionel/code/fraiseql/fraiseql-python/src/fraiseql/`

## See Also

- [Python Library Documentation](../fraiseql-python/README.md)
- [CLI Documentation](../crates/fraiseql-cli/README.md)
- [Architecture Overview](../README.md)
