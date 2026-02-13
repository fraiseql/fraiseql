<!-- Skip to main content -->
---

title: CompiledSchema Specification
description: The **CompiledSchema** is the compiled output from the authoring layer. It contains all information the Rust runtime needs to execute GraphQL queries without fu
keywords: ["format", "compliance", "schema", "protocol", "specification", "standard"]
tags: ["documentation", "reference"]
---

# CompiledSchema Specification

**Version:** 1.0
**Status:** Draft
**Format:** JSON

---

## 1. Overview

The **CompiledSchema** is the compiled output from the authoring layer. It contains all information the Rust runtime needs to execute GraphQL queries without further interpretation.

### Key properties

- Contains NO executable code
- Fully serializable to JSON
- Database-agnostic (capability manifest applies operators)
- Versioned for compatibility tracking
- Self-contained (no external references)

---

## 2. Top-Level Structure

```json
<!-- Code example in JSON -->
{
  "version": "1.0",
  "metadata": { ... },
  "types": [ ... ],
  "queries": [ ... ],
  "mutations": [ ... ],
  "bindings": { ... },
  "authorization": { ... },
  "capabilities": { ... }
}
```text
<!-- Code example in TEXT -->

---

## 3. Metadata

```json
<!-- Code example in JSON -->
{
  "metadata": {
    "name": "acme-api",
    "description": "ACME Corp public API",
    "version": "2.1.0",
    "schemaVersion": "1.0",
    "compiledAt": "2026-01-11T15:30:00Z",
    "author": "api-team",
    "databaseTarget": "postgresql",
    "databaseVersion": "14+",
    "extensions": [
      {
        "name": "pgvector",
        "version": "0.4.0",
        "required": true
      },
      {
        "name": "postgis",
        "version": "3.3.0",
        "required": false
      }
    ],
    "features": {
      "arrow": true,
      "federation": false,
      "subscriptions": true,
      "softDelete": true
    },
    "compatibility": {
      "minRuntimeVersion": "0.1.0",
      "maxRuntimeVersion": "0.999.0"
    }
  }
}
```text
<!-- Code example in TEXT -->

### Fields

- `version` — CompiledSchema format version
- `name` — Schema identifier
- `schemaVersion` — API semantic version
- `databaseTarget` — Target database (postgresql, sqlite, etc.)
- `extensions` — Required/optional DB extensions with versions
- `features` — Feature flags (Arrow, federation, subscriptions)
- `compatibility` — Runtime version constraints

---

## 4. Types

### 4.1 Type Definitions

```json
<!-- Code example in JSON -->
{
  "types": [
    {
      "name": "User",
      "kind": "object",
      "description": "A user account",
      "fields": [
        {
          "name": "id",
          "type": {
            "kind": "scalar",
            "name": "ID",
            "nonNull": true
          },
          "description": "Unique identifier"
        },
        {
          "name": "email",
          "type": {
            "kind": "scalar",
            "name": "String",
            "nonNull": true
          }
        },
        {
          "name": "posts",
          "type": {
            "kind": "list",
            "elementType": {
              "kind": "object",
              "name": "Post",
              "nonNull": true
            },
            "nonNull": false
          }
        },
        {
          "name": "createdAt",
          "type": {
            "kind": "scalar",
            "name": "DateTime",
            "nonNull": true
          }
        }
      ],
      "authorization": {
        "requiresAuth": true,
        "rules": [
          {
            "level": "type",
            "condition": { "role": "admin" }
          }
        ]
      }
    }
  ]
}
```text
<!-- Code example in TEXT -->

### 4.2 Type Kinds

| Kind | Example | Notes |
|------|---------|-------|
| `scalar` | `ID`, `String`, `Int`, `DateTime` | Built-in or custom |
| `object` | `User`, `Post` | Composed of fields |
| `input` | `UserWhereInput`, `CreateUserInput` | Mutation/filter input |
| `enum` | `UserRole`, `OrderStatus` | Fixed value set |
| `union` | `SearchResult` | One of multiple types |
| `interface` | `Node` | Shared field contract |

### 4.3 Scalar Types

Built-in scalars:

```json
<!-- Code example in JSON -->
{
  "scalars": [
    {
      "name": "ID",
      "description": "Unique identifier (UUID)",
      "coerceInput": "uuid",
      "coerceOutput": "string"
    },
    {
      "name": "String",
      "coerceInput": "string",
      "coerceOutput": "string"
    },
    {
      "name": "Int",
      "coerceInput": "int32",
      "coerceOutput": "int32"
    },
    {
      "name": "Float",
      "coerceInput": "float64",
      "coerceOutput": "float64"
    },
    {
      "name": "Boolean",
      "coerceInput": "boolean",
      "coerceOutput": "boolean"
    },
    {
      "name": "DateTime",
      "coerceInput": "datetime_rfc3339",
      "coerceOutput": "datetime_rfc3339"
    },
    {
      "name": "Date",
      "coerceInput": "date_iso8601",
      "coerceOutput": "date_iso8601"
    },
    {
      "name": "JSON",
      "coerceInput": "json",
      "coerceOutput": "json"
    },
    {
      "name": "UUID",
      "coerceInput": "uuid",
      "coerceOutput": "string"
    },
    {
      "name": "Decimal",
      "coerceInput": "decimal",
      "coerceOutput": "string"
    }
  ]
}
```text
<!-- Code example in TEXT -->

Custom scalars:

```json
<!-- Code example in JSON -->
{
  "name": "Email",
  "description": "Valid email address",
  "coerceInput": "email_validation",
  "coerceOutput": "string",
  "validation": {
    "pattern": "^[^@]+@[^@]+\\.[^@]+$"
  }
}
```text
<!-- Code example in TEXT -->

### 4.4 Input Types

```json
<!-- Code example in JSON -->
{
  "name": "UserWhereInput",
  "kind": "input",
  "fields": [
    {
      "name": "id",
      "type": { "kind": "scalar", "name": "IDFilter" }
    },
    {
      "name": "email",
      "type": { "kind": "scalar", "name": "StringFilter" }
    },
    {
      "name": "posts",
      "type": { "kind": "input", "name": "PostWhereInput" }
    },
    {
      "name": "_and",
      "type": {
        "kind": "list",
        "elementType": {
          "kind": "input",
          "name": "UserWhereInput",
          "nonNull": true
        }
      }
    },
    {
      "name": "_or",
      "type": {
        "kind": "list",
        "elementType": {
          "kind": "input",
          "name": "UserWhereInput",
          "nonNull": true
        }
      }
    },
    {
      "name": "_not",
      "type": { "kind": "input", "name": "UserWhereInput" }
    }
  ]
}
```text
<!-- Code example in TEXT -->

### 4.5 Filter Input Types

```json
<!-- Code example in JSON -->
{
  "name": "StringFilter",
  "kind": "input",
  "operators": [
    {
      "name": "_eq",
      "inputType": { "kind": "scalar", "name": "String" },
      "sqlOperator": "=",
      "supported": ["postgresql", "sqlite"]
    },
    {
      "name": "_neq",
      "inputType": { "kind": "scalar", "name": "String" },
      "sqlOperator": "!=",
      "supported": ["postgresql", "sqlite"]
    },
    {
      "name": "_like",
      "inputType": { "kind": "scalar", "name": "String" },
      "sqlOperator": "LIKE",
      "supported": ["postgresql", "sqlite"]
    },
    {
      "name": "_ilike",
      "inputType": { "kind": "scalar", "name": "String" },
      "sqlOperator": "ILIKE",
      "supported": ["postgresql"]
    },
    {
      "name": "_regex",
      "inputType": { "kind": "scalar", "name": "String" },
      "sqlOperator": "~",
      "supported": ["postgresql"]
    },
    {
      "name": "_in",
      "inputType": {
        "kind": "list",
        "elementType": { "kind": "scalar", "name": "String", "nonNull": true }
      },
      "sqlOperator": "IN",
      "supported": ["postgresql", "sqlite"]
    },
    {
      "name": "_nin",
      "inputType": {
        "kind": "list",
        "elementType": { "kind": "scalar", "name": "String", "nonNull": true }
      },
      "sqlOperator": "NOT IN",
      "supported": ["postgresql", "sqlite"]
    },
    {
      "name": "_is_null",
      "inputType": { "kind": "scalar", "name": "Boolean" },
      "sqlOperator": "IS NULL / IS NOT NULL",
      "supported": ["postgresql", "sqlite"]
    }
  ]
}
```text
<!-- Code example in TEXT -->

---

## 5. Queries

```json
<!-- Code example in JSON -->
{
  "queries": [
    {
      "name": "users",
      "description": "List all users with optional filtering",
      "returnType": {
        "kind": "list",
        "elementType": {
          "kind": "object",
          "name": "User",
          "nonNull": true
        }
      },
      "arguments": [
        {
          "name": "where",
          "type": { "kind": "input", "name": "UserWhereInput" },
          "description": "Filter criteria"
        },
        {
          "name": "orderBy",
          "type": {
            "kind": "list",
            "elementType": {
              "kind": "input",
              "name": "UserOrderByInput",
              "nonNull": true
            }
          }
        },
        {
          "name": "limit",
          "type": { "kind": "scalar", "name": "Int" },
          "defaultValue": 100
        },
        {
          "name": "offset",
          "type": { "kind": "scalar", "name": "Int" },
          "defaultValue": 0
        }
      ],
      "authorization": {
        "requiresAuth": true,
        "rules": [
          {
            "level": "query",
            "condition": { "role": ["admin", "viewer"] }
          }
        ]
      },
      "binding": {
        "type": "view",
        "view": "v_user",
        "dataColumn": "data"
      }
    }
  ]
}
```text
<!-- Code example in TEXT -->

---

## 6. Mutations

```json
<!-- Code example in JSON -->
{
  "mutations": [
    {
      "name": "createUser",
      "description": "Create a new user",
      "inputType": { "kind": "input", "name": "CreateUserInput" },
      "returnType": { "kind": "object", "name": "User", "nonNull": true },
      "authorization": {
        "requiresAuth": true,
        "rules": [
          {
            "level": "mutation",
            "condition": { "role": "admin" }
          }
        ]
      },
      "binding": {
        "type": "procedure",
        "procedure": "fn_create_user",
        "inputMapping": {
          "email": "email_param",
          "name": "name_param"
        },
        "outputMapping": {
          "id": "created_id",
          "email": "created_email"
        }
      }
    }
  ]
}
```text
<!-- Code example in TEXT -->

---

## 7. Bindings

Bindings connect GraphQL types to database resources:

```json
<!-- Code example in JSON -->
{
  "bindings": {
    "User": {
      "type": "view",
      "view": "v_user",
      "dataColumn": "data",
      "idColumns": ["id"],
      "filterColumns": [
        {
          "name": "user_id",
          "path": "user",
          "type": "UUID"
        },
        {
          "name": "items__product__category_id",
          "path": "items.product.category",
          "type": "UUID[]"
        }
      ]
    },
    "Post": {
      "type": "view",
      "view": "v_post",
      "dataColumn": "data",
      "idColumns": ["id"],
      "filterColumns": [
        {
          "name": "user_id",
          "path": "user",
          "type": "UUID"
        }
      ]
    },
    "CreateUserInput": {
      "type": "procedure",
      "procedure": "fn_create_user",
      "parameterMapping": {
        "email": "email",
        "name": "name",
        "password": "password_hash"
      },
      "returnMapping": {
        "id": "created_id",
        "email": "created_email",
        "createdAt": "created_at"
      }
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Binding fields

- `view` — PostgreSQL view to query
- `dataColumn` — Column containing JSONB projection
- `idColumns` — Columns used for WHERE clauses (public IDs)
- `filterColumns` — Denormalized filter columns for deep paths
- `procedure` — PostgreSQL function for mutations
- `parameterMapping` — Input → procedure parameter mapping

---

## 8. Authorization

```json
<!-- Code example in JSON -->
{
  "authorization": {
    "authContextType": {
      "name": "AuthContext",
      "fields": [
        {
          "name": "subject",
          "type": { "kind": "scalar", "name": "String", "nonNull": true },
          "description": "Authenticated user ID"
        },
        {
          "name": "roles",
          "type": {
            "kind": "list",
            "elementType": { "kind": "scalar", "name": "String", "nonNull": true }
          },
          "description": "Assigned roles"
        },
        {
          "name": "tenant_id",
          "type": { "kind": "scalar", "name": "String" },
          "description": "Multi-tenant identifier"
        }
      ]
    },
    "rules": [
      {
        "id": "auth_user_only",
        "description": "Requires authenticated user",
        "target": {
          "type": "query",
          "name": "me"
        },
        "condition": {
          "requiresAuth": true
        }
      },
      {
        "id": "auth_admin_role",
        "description": "Requires admin role",
        "target": {
          "type": "mutation",
          "name": "createUser"
        },
        "condition": {
          "role": "admin"
        }
      },
      {
        "id": "auth_tenant_scoped",
        "description": "Scoped by tenant_id claim",
        "target": {
          "type": "type",
          "name": "Tenant"
        },
        "condition": {
          "claim": "tenant_id",
          "dbParameterName": "app.tenant_id"
        }
      }
    ]
  }
}
```text
<!-- Code example in TEXT -->

---

## 9. Capabilities

References the database capability manifest:

```json
<!-- Code example in JSON -->
{
  "capabilities": {
    "databaseTarget": "postgresql",
    "supportedOperators": {
      "StringFilter": [
        "_eq", "_neq", "_like", "_ilike", "_regex",
        "_in", "_nin", "_is_null"
      ],
      "IntFilter": [
        "_eq", "_neq", "_lt", "_gt", "_lte", "_gte",
        "_in", "_nin", "_is_null"
      ],
      "IDArrayFilter": [
        "_contains", "_contained_by", "_overlaps",
        "_is_empty", "_is_null"
      ],
      "JSONBFilter": [
        "_contains", "_contained_by", "_has_key",
        "_has_any", "_has_all", "_is_null"
      ],
      "VectorFilter": [
        "_cosine_distance", "_l2_distance", "_inner_product"
      ]
    },
    "logicalOperators": ["_and", "_or", "_not"]
  }
}
```text
<!-- Code example in TEXT -->

---

## 10. Example: Complete Minimal Schema

```json
<!-- Code example in JSON -->
{
  "version": "1.0",
  "metadata": {
    "name": "blog-api",
    "version": "1.0.0",
    "schemaVersion": "1.0",
    "compiledAt": "2026-01-11T00:00:00Z",
    "databaseTarget": "postgresql",
    "features": {
      "arrow": false,
      "federation": false,
      "subscriptions": false
    }
  },
  "types": [
    {
      "name": "User",
      "kind": "object",
      "fields": [
        { "name": "id", "type": { "kind": "scalar", "name": "ID", "nonNull": true } },
        { "name": "email", "type": { "kind": "scalar", "name": "String", "nonNull": true } },
        { "name": "posts", "type": { "kind": "list", "elementType": { "kind": "object", "name": "Post", "nonNull": true } } }
      ]
    },
    {
      "name": "Post",
      "kind": "object",
      "fields": [
        { "name": "id", "type": { "kind": "scalar", "name": "ID", "nonNull": true } },
        { "name": "title", "type": { "kind": "scalar", "name": "String", "nonNull": true } },
        { "name": "author", "type": { "kind": "object", "name": "User", "nonNull": true } }
      ]
    },
    {
      "name": "UserWhereInput",
      "kind": "input",
      "fields": [
        { "name": "id", "type": { "kind": "scalar", "name": "IDFilter" } },
        { "name": "email", "type": { "kind": "scalar", "name": "StringFilter" } },
        { "name": "_and", "type": { "kind": "list", "elementType": { "kind": "input", "name": "UserWhereInput", "nonNull": true } } }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "returnType": { "kind": "list", "elementType": { "kind": "object", "name": "User", "nonNull": true } },
      "arguments": [
        { "name": "where", "type": { "kind": "input", "name": "UserWhereInput" } }
      ],
      "binding": {
        "type": "view",
        "view": "v_user",
        "dataColumn": "data"
      }
    }
  ],
  "mutations": [],
  "bindings": {
    "User": {
      "type": "view",
      "view": "v_user",
      "dataColumn": "data"
    }
  },
  "authorization": {
    "authContextType": {
      "name": "AuthContext",
      "fields": []
    },
    "rules": []
  }
}
```text
<!-- Code example in TEXT -->

---

## 11. Validation Rules

The CompiledSchema must satisfy:

1. **Type closure** — All referenced types must be defined
2. **Binding coverage** — All types with queries/mutations must have bindings
3. **Field consistency** — Fields in types must match view columns
4. **Authorization validity** — Auth rules reference defined auth context fields
5. **Capability compliance** — Used operators must be in supported list
6. **Unique names** — No duplicate type/query/mutation names

---

## 12. Runtime Guarantees

The Rust runtime assumes:

1. **Schema is valid** — Compiler validated all rules
2. **Bindings exist** — All referenced views/procedures exist
3. **Operators are supported** — Database can execute all operators
4. **Auth context matches** — Runtime provides auth context matching schema
5. **No mutations during execution** — Schema doesn't change mid-query

---

## 13. Version Strategy

CompiledSchema versions follow semver:

- **Major** — Breaking changes (type removal, field removal, operator removal)
- **Minor** — Additive changes (new types, new fields, new operators)
- **Patch** — Internal changes (description updates, metadata)

Runtime enforces compatibility via `metadata.compatibility.minRuntimeVersion`.

---
