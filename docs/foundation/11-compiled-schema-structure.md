<!-- Skip to main content -->
---
title: 2.6: Compiled Schema Structure
description: The **compiled schema** (`schema.compiled.json`) is the artifact produced by the FraiseQL compiler. It contains the complete GraphQL schema in a form optimized 
keywords: ["query-execution", "schema", "data-planes", "graphql", "compilation", "architecture"]
tags: ["documentation", "reference"]
---

# 2.6: Compiled Schema Structure

## Overview

The **compiled schema** (`schema.compiled.json`) is the artifact produced by the FraiseQL compiler. It contains the complete GraphQL schema in a form optimized for runtime execution: no Python/TypeScript code, no type annotations, no decorators—just pure JSON containing all type definitions, operations, and metadata needed to execute queries.

This is FraiseQL's "binary interface" between compilation and runtime:

- **Input**: `schema.json` (from Python/TypeScript authoring)
- **Process**: Compilation validates, resolves, optimizes
- **Output**: `schema.compiled.json` (loaded directly by Rust server)

### Compilation Flow

```text
<!-- Code example in TEXT -->
┌─────────────────────┐
│ Python/TypeScript   │
│ @FraiseQL.type      │  ← Schema definitions
│ @FraiseQL.query     │
│ @FraiseQL.mutation  │
└──────────┬──────────┘
           │
           ↓ (generates)
┌─────────────────────┐
│ schema.json         │  ← Python output (raw definitions)
│ (input to compiler) │
└──────────┬──────────┘
           │
           ↓ (FraiseQL-cli compile)
┌─────────────────────┐
│ schema.compiled.json │  ← Rust loads this
│ (optimized for      │
│  runtime execution) │
└──────────┬──────────┘
           │
           ↓ (loaded by)
┌─────────────────────┐
│ FraiseQL-server     │
│ Execute queries     │
└─────────────────────┘
```text
<!-- Code example in TEXT -->

---

## Top-Level Schema Structure

The compiled schema is a JSON object with these top-level keys:

```json
<!-- Code example in JSON -->
{
  "types": [...],              // GraphQL object types
  "enums": [...],              // GraphQL enum types
  "input_types": [...],        // GraphQL input object types
  "interfaces": [...],         // GraphQL interface definitions
  "unions": [...],             // GraphQL union types
  "queries": [...],            // Root Query operations
  "mutations": [...],          // Root Mutation operations
  "subscriptions": [...],      // Root Subscription operations (future)
  "directives": [...],         // Custom directives
  "fact_tables": {...},        // Analytics fact table metadata
  "observers": [...],          // Database change observers
  "federation": {...},         // Apollo Federation v2 metadata (optional)
  "schema_sdl": "..."          // Raw GraphQL SDL (optional, for introspection)
}
```text
<!-- Code example in TEXT -->

### Schema Statistics

```json
<!-- Code example in JSON -->
{
  "types": 45,           // 45 object types defined
  "enums": 8,            // 8 enum types
  "input_types": 12,     // 12 input object types
  "interfaces": 2,       // 2 interface definitions
  "unions": 1,           // 1 union definition
  "queries": 23,         // 23 root queries
  "mutations": 18,       // 18 root mutations
  "subscriptions": 0,    // 0 subscriptions
  "total_fields": 256,   // Across all types
  "total_operations": 41 // queries + mutations + subscriptions
}
```text
<!-- Code example in TEXT -->

---

## Type Definitions

### Structure

Each type definition contains:

```json
<!-- Code example in JSON -->
{
  "name": "User",                          // Type name (GraphQL)
  "sql_source": "v_user",                  // Table/view name in database
  "jsonb_column": "data",                  // JSONB column (for flexible fields)
  "fields": [...],                         // Field definitions
  "description": "A user in the system",   // Optional description
  "sql_projection_hint": {...},            // Optional optimization hint
  "implements": ["Node"]                   // Interfaces implemented
}
```text
<!-- Code example in TEXT -->

### Complete Type Example

```json
<!-- Code example in JSON -->
{
  "name": "Post",
  "sql_source": "v_post",
  "jsonb_column": "data",
  "description": "A published blog post",
  "fields": [
    {
      "name": "id",
      "field_type": "ID!",
      "nullable": false,
      "description": "Unique identifier",
      "sql_column": "pk_post_id"
    },
    {
      "name": "title",
      "field_type": "String!",
      "nullable": false,
      "description": "Post title",
      "sql_column": "title"
    },
    {
      "name": "content",
      "field_type": "String!",
      "nullable": false,
      "description": "Post content",
      "sql_column": "content"
    },
    {
      "name": "author",
      "field_type": "User!",
      "nullable": false,
      "description": "Post author",
      "relationship": {
        "type": "one-to-one",
        "foreign_key": "fk_user",
        "target_type": "User"
      }
    },
    {
      "name": "createdAt",
      "field_type": "DateTime!",
      "nullable": false,
      "sql_column": "created_at"
    }
  ],
  "implements": ["Node"],
  "sql_projection_hint": {
    "database": "postgresql",
    "projection_template": "jsonb_build_object('id', pk_post_id, 'title', title, 'content', content)",
    "estimated_reduction_percent": 35
  }
}
```text
<!-- Code example in TEXT -->

### Field Definitions

Each field contains:

```json
<!-- Code example in JSON -->
{
  "name": "title",                    // Field name (camelCase in GraphQL)
  "field_type": "String!",            // Type with modifiers (! for non-null, [] for list)
  "nullable": false,                  // Is field nullable?
  "description": "Post title",        // Optional description
  "sql_column": "title",              // Which database column (if direct)
  "deprecation": {                    // Optional deprecation info
    "reason": "Use 'heading' instead"
  },
  "relationship": {                   // Optional for relationship fields
    "type": "one-to-many",
    "foreign_key": "fk_post_id",
    "target_type": "Comment"
  }
}
```text
<!-- Code example in TEXT -->

---

## Query Definitions

### Structure

Each query contains:

```json
<!-- Code example in JSON -->
{
  "name": "posts",                    // Query name
  "return_type": "Post",              // Type returned
  "returns_list": true,               // Does it return a list?
  "nullable": false,                  // Is return value nullable?
  "arguments": [...],                 // Query parameters
  "sql_source": "v_post",             // Optional direct table source
  "description": "Get all posts",     // Optional description
  "auto_params": {                    // Auto-wired filtering parameters
    "has_where": true,
    "has_order_by": true,
    "has_limit": true,
    "has_offset": true
  },
  "deprecation": null                 // Deprecation info if applicable
}
```text
<!-- Code example in TEXT -->

### Complete Query Example

```json
<!-- Code example in JSON -->
{
  "name": "posts",
  "return_type": "Post",
  "returns_list": true,
  "nullable": false,
  "arguments": [
    {
      "name": "authorId",
      "arg_type": "Int",
      "nullable": true,
      "description": "Filter by author"
    },
    {
      "name": "published",
      "arg_type": "Boolean",
      "nullable": false,
      "default_value": true,
      "description": "Filter by publication status"
    },
    {
      "name": "limit",
      "arg_type": "Int",
      "nullable": false,
      "default_value": 10,
      "description": "Maximum results to return"
    },
    {
      "name": "offset",
      "arg_type": "Int",
      "nullable": false,
      "default_value": 0,
      "description": "Number of results to skip"
    }
  ],
  "sql_source": "v_post",
  "description": "Get posts with filtering and pagination",
  "auto_params": {
    "has_where": true,
    "has_order_by": true,
    "has_limit": true,
    "has_offset": true
  }
}
```text
<!-- Code example in TEXT -->

### Query Argument Details

Each argument specifies:

```json
<!-- Code example in JSON -->
{
  "name": "authorId",         // Argument name
  "arg_type": "Int",          // Type (scalar, list, input type)
  "nullable": true,           // Can be null?
  "default_value": null,      // Default if not provided
  "description": "...",       // Documentation
  "validation": {             // Optional validation rules
    "min": 0,
    "max": 2147483647
  }
}
```text
<!-- Code example in TEXT -->

---

## Mutation Definitions

### Structure

Each mutation contains:

```json
<!-- Code example in JSON -->
{
  "name": "createPost",               // Mutation name
  "return_type": "Post",              // Type returned
  "arguments": [...],                 // Input parameters
  "description": "Create a blog post", // Documentation
  "operation": "Custom",              // Operation type: Insert, Update, Delete, Custom
  "deprecation": null                 // Deprecation info
}
```text
<!-- Code example in TEXT -->

### Complete Mutation Example

```json
<!-- Code example in JSON -->
{
  "name": "createPost",
  "return_type": "Post",
  "arguments": [
    {
      "name": "input",
      "arg_type": "CreatePostInput!",
      "nullable": false,
      "description": "Post creation input"
    }
  ],
  "description": "Create a new blog post",
  "operation": "Custom"
}
```text
<!-- Code example in TEXT -->

### Input Type (for Mutations)

```json
<!-- Code example in JSON -->
{
  "name": "CreatePostInput",
  "fields": [
    {
      "name": "title",
      "field_type": "String!",
      "description": "Post title",
      "nullable": false
    },
    {
      "name": "content",
      "field_type": "String!",
      "description": "Post content",
      "nullable": false
    },
    {
      "name": "authorId",
      "field_type": "Int!",
      "description": "Author ID",
      "nullable": false
    },
    {
      "name": "tags",
      "field_type": "[String]",
      "description": "Associated tags",
      "nullable": true
    }
  ],
  "description": "Input for post creation"
}
```text
<!-- Code example in TEXT -->

---

## Enum Definitions

```json
<!-- Code example in JSON -->
{
  "name": "PostStatus",
  "description": "Status of a post",
  "values": [
    {
      "name": "DRAFT",
      "description": "Post is in draft"
    },
    {
      "name": "PUBLISHED",
      "description": "Post is published"
    },
    {
      "name": "ARCHIVED",
      "description": "Post is archived"
    }
  ]
}
```text
<!-- Code example in TEXT -->

---

## Real-World Example: Blog Platform

### Complete Schema (E-commerce Example)

```json
<!-- Code example in JSON -->
{
  "types": [
    {
      "name": "User",
      "sql_source": "v_user",
      "jsonb_column": "data",
      "description": "A user account",
      "fields": [
        {
          "name": "id",
          "field_type": "ID!",
          "nullable": false,
          "sql_column": "pk_user"
        },
        {
          "name": "email",
          "field_type": "String!",
          "nullable": false,
          "sql_column": "email"
        },
        {
          "name": "name",
          "field_type": "String!",
          "nullable": false,
          "sql_column": "name"
        },
        {
          "name": "role",
          "field_type": "UserRole!",
          "nullable": false,
          "sql_column": "role"
        },
        {
          "name": "posts",
          "field_type": "[Post!]!",
          "nullable": false,
          "relationship": {
            "type": "one-to-many",
            "foreign_key": "fk_user",
            "target_type": "Post"
          }
        },
        {
          "name": "createdAt",
          "field_type": "DateTime!",
          "nullable": false,
          "sql_column": "created_at"
        }
      ]
    },
    {
      "name": "Post",
      "sql_source": "v_post",
      "jsonb_column": "data",
      "description": "A blog post",
      "fields": [
        {
          "name": "id",
          "field_type": "ID!",
          "nullable": false,
          "sql_column": "pk_post_id"
        },
        {
          "name": "title",
          "field_type": "String!",
          "nullable": false,
          "sql_column": "title"
        },
        {
          "name": "content",
          "field_type": "String!",
          "nullable": false,
          "sql_column": "content"
        },
        {
          "name": "author",
          "field_type": "User!",
          "nullable": false,
          "relationship": {
            "type": "one-to-one",
            "foreign_key": "fk_user",
            "target_type": "User"
          }
        },
        {
          "name": "status",
          "field_type": "PostStatus!",
          "nullable": false,
          "sql_column": "status"
        },
        {
          "name": "createdAt",
          "field_type": "DateTime!",
          "nullable": false,
          "sql_column": "created_at"
        }
      ]
    }
  ],
  "enums": [
    {
      "name": "UserRole",
      "values": [
        {"name": "ADMIN"},
        {"name": "EDITOR"},
        {"name": "VIEWER"}
      ]
    },
    {
      "name": "PostStatus",
      "values": [
        {"name": "DRAFT"},
        {"name": "PUBLISHED"},
        {"name": "ARCHIVED"}
      ]
    }
  ],
  "input_types": [
    {
      "name": "CreatePostInput",
      "description": "Input for creating a post",
      "fields": [
        {
          "name": "title",
          "field_type": "String!",
          "nullable": false
        },
        {
          "name": "content",
          "field_type": "String!",
          "nullable": false
        },
        {
          "name": "authorId",
          "field_type": "Int!",
          "nullable": false
        }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_user",
      "arguments": [
        {
          "name": "limit",
          "arg_type": "Int",
          "nullable": false,
          "default_value": 20
        },
        {
          "name": "offset",
          "arg_type": "Int",
          "nullable": false,
          "default_value": 0
        },
        {
          "name": "role",
          "arg_type": "UserRole",
          "nullable": true
        }
      ],
      "description": "Get list of users",
      "auto_params": {
        "has_where": true,
        "has_order_by": true,
        "has_limit": true,
        "has_offset": true
      }
    },
    {
      "name": "user",
      "return_type": "User",
      "returns_list": false,
      "nullable": true,
      "arguments": [
        {
          "name": "id",
          "arg_type": "Int!",
          "nullable": false
        }
      ],
      "description": "Get a single user by ID"
    },
    {
      "name": "posts",
      "return_type": "Post",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_post",
      "arguments": [
        {
          "name": "authorId",
          "arg_type": "Int",
          "nullable": true
        },
        {
          "name": "status",
          "arg_type": "PostStatus",
          "nullable": true
        },
        {
          "name": "limit",
          "arg_type": "Int",
          "nullable": false,
          "default_value": 20
        },
        {
          "name": "offset",
          "arg_type": "Int",
          "nullable": false,
          "default_value": 0
        }
      ],
      "description": "Get posts with filtering",
      "auto_params": {
        "has_where": true,
        "has_order_by": true,
        "has_limit": true,
        "has_offset": true
      }
    }
  ],
  "mutations": [
    {
      "name": "createPost",
      "return_type": "Post",
      "arguments": [
        {
          "name": "input",
          "arg_type": "CreatePostInput!",
          "nullable": false
        }
      ],
      "description": "Create a new post",
      "operation": "Custom"
    },
    {
      "name": "publishPost",
      "return_type": "Post",
      "arguments": [
        {
          "name": "id",
          "arg_type": "Int!",
          "nullable": false
        }
      ],
      "description": "Publish a post",
      "operation": "Custom"
    }
  ],
  "subscriptions": [],
  "directives": [],
  "fact_tables": {},
  "observers": []
}
```text
<!-- Code example in TEXT -->

---

## Using Compiled Schema in Rust

### Loading the Schema

```rust
<!-- Code example in RUST -->
use fraiseql_core::schema::CompiledSchema;
use std::fs;

// Load from file
let json = fs::read_to_string("schema.compiled.json")?;
let schema = CompiledSchema::from_json(&json)?;

// Validate schema for consistency
schema.validate()?;

// Use schema for operations
let posts_query = schema.find_query("posts");
let user_type = schema.find_type("User");
```text
<!-- Code example in TEXT -->

### Introspection Operations

```rust
<!-- Code example in RUST -->
use fraiseql_core::schema::CompiledSchema;

let schema = load_schema()?;

// Find a query by name
if let Some(query) = schema.find_query("posts") {
    println!("Query: {}", query.name);
    println!("Returns: {}", query.return_type);
    println!("Returns list: {}", query.returns_list);
    for arg in &query.arguments {
        println!("  Arg: {} : {}", arg.name, arg.arg_type);
    }
}

// Find a type by name
if let Some(user_type) = schema.find_type("User") {
    println!("Type: {}", user_type.name);
    println!("SQL source: {}", user_type.sql_source);
    for field in &user_type.fields {
        println!("  Field: {} : {}", field.name, field.field_type);
    }
}

// List all queries
for query in &schema.queries {
    println!("Query: {}", query.name);
}

// List all types
for type_def in &schema.types {
    println!("Type: {}", type_def.name);
}

// Get statistics
println!("Total operations: {}", schema.operation_count());
println!("Total types: {}", schema.types.len());
```text
<!-- Code example in TEXT -->

### Validating Against Schema

```rust
<!-- Code example in RUST -->
// Validate that a GraphQL query is valid against the schema
pub fn validate_query(schema: &CompiledSchema, query_name: &str) -> Result<()> {
    // 1. Check query exists
    let query = schema.find_query(query_name)
        .ok_or_else(|| format!("Unknown query: {}", query_name))?;

    // 2. Check return type exists
    schema.find_type(query.return_type.as_str())
        .ok_or_else(|| format!("Unknown type: {}", query.return_type))?;

    // 3. Check all arguments reference valid types
    for arg in &query.arguments {
        if !is_valid_type_reference(schema, &arg.arg_type) {
            return Err(format!("Invalid argument type: {}", arg.arg_type));
        }
    }

    Ok(())
}

fn is_valid_type_reference(schema: &CompiledSchema, type_ref: &str) -> bool {
    // Strip modifiers: "String!" → "String", "[User]" → "User"
    let base_type = type_ref
        .trim_end_matches('!')
        .trim_start_matches('[')
        .trim_end_matches(']');

    schema.find_type(base_type).is_some()
        || schema.find_enum(base_type).is_some()
        || is_builtin_scalar(base_type)
}

fn is_builtin_scalar(name: &str) -> bool {
    matches!(name, "String" | "Int" | "Float" | "Boolean" | "ID" | "DateTime" | "JSON" | "UUID")
}
```text
<!-- Code example in TEXT -->

---

## Schema Size and Performance

### Typical Schema Sizes

| Schema Complexity | Types | Operations | Size (JSON) | Size (gzipped) |
|-------------------|-------|-----------|-------------|----------------|
| Simple (blog) | 5 | 12 | 25 KB | 4 KB |
| Medium (SaaS) | 30 | 50 | 180 KB | 24 KB |
| Large (enterprise) | 100 | 200 | 650 KB | 85 KB |
| Massive (federation) | 500 | 1000+ | 3.2 MB | 320 KB |

**Note:** Even massive schemas are small in terms of disk/memory usage.

### Loading Performance

```text
<!-- Code example in TEXT -->
Schema Deserialization Benchmark
─────────────────────────────────
Small schema (25 KB):     < 1ms
Medium schema (180 KB):   2-3ms
Large schema (650 KB):    8-12ms
Massive schema (3.2 MB):  40-60ms

Memory footprint: ~2-3x JSON size (after deserialization)
```text
<!-- Code example in TEXT -->

### Schema Caching Strategy

**Typical deployment:**

```rust
<!-- Code example in RUST -->
// Server startup (once)
let schema = CompiledSchema::from_json(&fs::read_to_string("schema.compiled.json")?)?;
let schema = Arc::new(schema);  // Immutable, shared across threads

// In request handlers (per request)
let schema = Arc::clone(&schema);  // Clone Arc (cheap pointer copy)
let query_def = schema.find_query("posts")?;
```text
<!-- Code example in TEXT -->

Cost per request: Just a pointer lookup (O(1) amortized).

---

## Evolution and Versioning

### Schema Version Tracking

Optional metadata field in compiled schema:

```json
<!-- Code example in JSON -->
{
  "schema_version": "1.2.3",
  "compiled_at": "2026-01-29T15:30:00Z",
  "compiler_version": "2.0.0",
  "graphql_spec_version": "2021-06",
  "...": "rest of schema"
}
```text
<!-- Code example in TEXT -->

### Backwards Compatibility

When schema changes (new types, queries added):

```text
<!-- Code example in TEXT -->
Old Client           Server
(schema v1)          (schema v2)
    │                  │
    │ GET /graphql?schema_version=1
    │─────────────────→│
    │                  │ Returns only v1 compatible response
    │ 200 OK           │
    │←─────────────────│
```text
<!-- Code example in TEXT -->

FraiseQL can serve multiple schema versions in a single deployment by maintaining a registry of compiled schemas.

---

## Related Topics

- **2.1: Compilation Pipeline** - How schema.json becomes schema.compiled.json
- **2.2: Query Execution Model** - How runtime uses compiled schema
- **2.4: Type System** - Type definitions and inference
- **2.5: Error Handling & Validation** - Validation against schema
- **2.7: Performance Characteristics** - Schema lookup performance

---

## Summary

The compiled schema is a **pure-data representation** of the GraphQL schema optimized for runtime execution:

- **Input**: Python/TypeScript definitions converted to `schema.json`
- **Process**: Compilation validates, resolves, optimizes
- **Output**: `schema.compiled.json` (immutable, language-agnostic)
- **Usage**: Loaded once at server startup, used for query validation and execution
- **Performance**: Microsecond lookup times for type/query resolution

The schema's JSON structure makes it language-agnostic: Python, TypeScript, Go, Rust, or any language with JSON parsing can load and use it. FraiseQL uses this to enable true polyglot development while maintaining compile-time safety guarantees.

Key insight: **The compiled schema makes the implicit explicit.** Type information, relationships, and operations that were scattered across Python decorators are now consolidated into a single, queryable structure that the runtime can efficiently consume.
