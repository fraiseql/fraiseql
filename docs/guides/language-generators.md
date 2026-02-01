# FraiseQL Language Generators

## Overview

FraiseQL v2 supports schema authoring in **5 programming languages**, all producing compatible JSON schemas that compile to the same optimized execution engine. This document describes each language generator, their features, and how to use them.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  Language Generators                         │
├──────────────────┬──────────────────┬──────────────────┬────┤
│  Python          │  TypeScript      │  Java            │ Go │
│  (decorators)    │  (decorators)    │  (annotations)   │    │
└────────┬─────────┴────────┬─────────┴────────┬────────┴─┬──┘
         │                  │                  │          │
         └──────────────────┼──────────────────┼──────────┘
                            │
                      ┌─────▼──────┐
                      │ schema.json │ ← All produce this format
                      └─────┬──────┘
                            │
                      ┌─────▼──────────────┐
                      │ fraiseql-cli       │
                      │ (compilation)      │
                      └─────┬──────────────┘
                            │
                      ┌─────▼──────────────────┐
                      │ schema.compiled.json   │
                      │ (optimized execution)  │
                      └────────────────────────┘
```

## Status Summary

| Language | Version | Status | Tests | Features |
|----------|---------|--------|-------|----------|
| Python | 2.0.0-a1 | ✅ Ready | 34/34 ✓ | Full support |
| TypeScript | 2.0.0-a1 | ✅ Ready | 10/10 ✓ | Full support |
| Go | 2.0.0-a1 | ✅ Ready | 45+ ✓ | Full support |
| Java | 2.0.0-a1 | ✅ Ready | 6 tests ✓ | Full support |
| PHP | 2.0.0-a1 | ✅ Ready | 15+ ✓ | Full support |

## Python Generator

### Installation

```bash
cd fraiseql-python
pip install -e .
# or with uv:
uv sync
```

### Basic Usage

```python
from fraiseql import (
    type as fraiseql_type,
    query as fraiseql_query,
    mutation as fraiseql_mutation,
    schema as fraiseql_schema,
)

# Define types
@fraiseql_type
class User:
    id: int
    name: str
    email: str | None
    createdAt: str
    isActive: bool

@fraiseql_type
class Post:
    id: int
    title: str
    content: str
    authorId: int
    published: bool

# Define queries
@fraiseql_query(sql_source="v_users")
def users(limit: int = 10, offset: int = 0) -> list[User]:
    """Get all users."""
    pass

@fraiseql_query(sql_source="v_posts")
def posts(
    authorId: int | None = None,
    published: bool | None = None,
    limit: int = 10,
    offset: int = 0
) -> list[Post]:
    """Get posts with filtering."""
    pass

# Define mutations
@fraiseql_mutation(sql_source="fn_create_user")
def createUser(name: str, email: str) -> User:
    """Create a new user."""
    pass

# Export schema
fraiseql_schema.export_schema("schema.json")
```

### Analytics Support

```python
from fraiseql import fact_table, aggregate_query

@fact_table(
    table_name="tf_sales",
    measures=["revenue", "quantity", "cost"],
    dimension_paths=[
        {
            "name": "category",
            "json_path": "dimensions->>'category'",
            "data_type": "text"
        }
    ]
)
class SalesFactTable:
    revenue: float
    quantity: int
    cost: float

@aggregate_query(fact_table="tf_sales")
def salesByCategory(category: str) -> dict:
    """Sales aggregated by category."""
    pass
```

### Features

- ✅ Modern Python 3.10+ type hints
- ✅ Decorator-based schema definition
- ✅ Full analytics support (fact tables, measures)
- ✅ GraphQL type mapping
- ✅ JSON schema export
- ✅ CLI compilation support

### Testing

```bash
cd fraiseql-python
python -m pytest tests/ -v

# E2E test
python -m pytest tests/e2e/python_e2e_test.py -v
```

## TypeScript Generator

### Installation

```bash
cd fraiseql-typescript
npm install
# or
npm ci
```

### Basic Usage

```typescript
import { Type, Query, Mutation, SchemaRegistry, ExportSchema } from "./src/decorators";

// Define types
@Type()
class User {
  id!: number;
  name!: string;
  email?: string;
  createdAt!: string;
  isActive!: boolean;
}

@Type()
class Post {
  id!: number;
  title!: string;
  content!: string;
  authorId!: number;
  published!: boolean;
}

// Define queries
@Query(sql_source = "v_users")
users(limit?: number, offset?: number): User[] {
  return [];
}

@Query(sql_source = "v_posts")
posts(
  authorId?: number,
  published?: boolean,
  limit?: number,
  offset?: number
): Post[] {
  return [];
}

// Define mutations
@Mutation(sql_source = "fn_create_user")
createUser(name: string, email: string): User {
  return new User();
}

// Export schema
ExportSchema("schema.json");
```

### Analytics Support

```typescript
import { FactTable, AggregateQuery } from "./src/decorators";

@FactTable({
  name: "tf_sales",
  measures: ["revenue", "quantity"],
  dimensionColumn: "dimensions"
})
class SalesFactTable {
  revenue!: number;
  quantity!: number;
}

@AggregateQuery(factTable = "tf_sales")
salesByCategory(category: string): Record<string, any> {
  return {};
}
```

### Configuration

Enable experimental decorators in `tsconfig.json`:

```json
{
  "compilerOptions": {
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true,
    "target": "ES2022"
  }
}
```

### Features

- ✅ Full TypeScript type safety
- ✅ Decorator-based schema definition
- ✅ Analytics support
- ✅ Jest testing support
- ✅ JSON schema export
- ✅ CLI compilation support

### Testing

```bash
cd fraiseql-typescript
npm test

# E2E test
npm run example:basic
npm run example:analytics
```

## Go Generator

### Installation

```bash
cd fraiseql-go
go mod download
```

### Basic Usage

```go
package main

import "github.com/fraiseql/fraiseql-go/fraiseql"

// Define types
type User struct {
    ID        int     `fraiseql:"id"`
    Name      string  `fraiseql:"name"`
    Email     *string `fraiseql:"email"`
    CreatedAt string  `fraiseql:"createdAt"`
    IsActive  bool    `fraiseql:"isActive"`
}

type Post struct {
    ID        int    `fraiseql:"id"`
    Title     string `fraiseql:"title"`
    Content   string `fraiseql:"content"`
    AuthorID  int    `fraiseql:"authorId"`
    Published bool   `fraiseql:"published"`
}

// Define schema
type Schema struct {
    Users []User `fraiseql:"query,sql_source=v_users"`
    Posts []Post `fraiseql:"query,sql_source=v_posts"`
}

// Export schema
func main() {
    fraiseql.ExportSchema("schema.json")
}
```

### Features

- ✅ Struct-based type definition
- ✅ Tag-based configuration
- ✅ Nil pointer for nullable fields
- ✅ JSON schema export
- ✅ CLI compilation support
- ✅ High performance

### Testing

```bash
cd fraiseql-go
go test ./fraiseql/... -v

# Run example
go run examples/basic_schema.go
```

## Java Generator

### Installation

```bash
cd fraiseql-java
mvn clean install
```

### Basic Usage

```java
package com.fraiseql.example;

import com.fraiseql.annotations.*;
import java.util.List;

@FraiseQLType
public class User {
    @Field
    private int id;

    @Field
    private String name;

    @Field(nullable = true)
    private String email;

    // Getters/setters...
}

@FraiseQLType
public class Post {
    @Field
    private int id;

    @Field
    private String title;

    @Field(sqlSource = "v_posts")
    private List<Post> posts;
}

public class Schema {
    @Query(sqlSource = "v_users")
    public List<User> users(int limit) {
        return null;
    }

    @Mutation(sqlSource = "fn_create_user")
    public User createUser(String name, String email) {
        return null;
    }
}
```

### Features

- ✅ Annotation-based schema definition
- ✅ Full type safety with generics
- ✅ Stream API integration
- ✅ JSON schema export
- ✅ CLI compilation support

### Testing

```bash
cd fraiseql-java
mvn test
```

## PHP Generator

### Installation

```bash
cd fraiseql-php
composer install
```

### Basic Usage

```php
<?php

namespace FraiseQL\Example;

use FraiseQL\Attributes\Type;
use FraiseQL\Attributes\Field;
use FraiseQL\Attributes\Query;

#[Type]
class User {
    #[Field]
    public int $id;

    #[Field]
    public string $name;

    #[Field(nullable: true)]
    public ?string $email;
}

#[Type]
class Post {
    #[Field]
    public int $id;

    #[Field]
    public string $title;
}

class Schema {
    #[Query(sqlSource: 'v_users')]
    public function users(int $limit = 10): array {
        return [];
    }

    #[Query(sqlSource: 'v_posts')]
    public function posts(int $authorId = null): array {
        return [];
    }
}

// Export schema
(new SchemaExporter())->export('schema.json');
?>
```

### Features

- ✅ PHP 8 Attributes-based schema definition
- ✅ Full type declaration support
- ✅ Nullable type support
- ✅ JSON schema export
- ✅ CLI compilation support

### Testing

```bash
cd fraiseql-php
vendor/bin/phpunit tests/
```

## Schema Generation Workflow

### Step 1: Define Schema in Language

Use language-specific decorators/annotations to define types, queries, and mutations.

### Step 2: Generate JSON

All generators export to `schema.json`:

```python
fraiseql_schema.export_schema("schema.json")
```

### Step 3: Compile with CLI

```bash
fraiseql-cli compile schema.json
```

This produces `schema.compiled.json` with:

- Optimized SQL templates
- Type validation
- Performance suggestions

### Step 4: Deploy Compiled Schema

Use compiled schema with runtime:

```bash
fraiseql-server --schema schema.compiled.json
```

## Feature Comparison

| Feature | Python | TypeScript | Go | Java | PHP |
|---------|--------|------------|----|----- |-----|
| Basic types | ✅ | ✅ | ✅ | ✅ | ✅ |
| Nullable fields | ✅ | ✅ | ✅ | ✅ | ✅ |
| List types | ✅ | ✅ | ✅ | ✅ | ✅ |
| Queries | ✅ | ✅ | ✅ | ✅ | ✅ |
| Mutations | ✅ | ✅ | ✅ | ✅ | ✅ |
| Fact tables | ✅ | ✅ | ⏳ | ⏳ | ⏳ |
| Aggregate queries | ✅ | ✅ | ⏳ | ⏳ | ⏳ |
| Custom scalars | ✅ | ✅ | ✅ | ✅ | ✅ |
| Subscriptions | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |

## Best Practices

### 1. Use Consistent Naming

```python
# Good: Clear, descriptive names
@fraiseql_type
class UserProfile:
    userId: int
    displayName: str

# Avoid: Vague or abbreviated names
@fraiseql_type
class U:
    uid: int
    nm: str
```

### 2. Leverage Type Safety

```typescript
// Good: Full type annotations
@Query(sql_source = "v_users")
users(limit: number, offset: number): User[] {
  return [];
}

// Avoid: Any types
@Query(sql_source = "v_users")
users(limit: any, offset: any): any[] {
  return [];
}
```

### 3. Document Complex Schemas

```go
// Good: Document purpose and constraints
type SalesAnalytics struct {
    // Revenue in cents for precision
    Revenue int `fraiseql:"revenue,description=Revenue in cents"`
    // Aggregated by date
    Date string `fraiseql:"date,description=Date in YYYY-MM-DD format"`
}

// Avoid: Undocumented fields
type SalesData struct {
    Rev int
    D string
}
```

### 4. Test Before Compilation

```bash
# Run language-specific tests first
go test ./fraiseql/...
npm test --prefix fraiseql-typescript

# Then compile
fraiseql-cli compile schema.json
```

## Performance Considerations

- **Python**: Decorator application happens at import time
- **TypeScript**: Metadata stored in memory during execution
- **Go**: Reflection-based, zero runtime cost after initial schema extraction
- **Java**: Annotation processing at compile time
- **PHP**: Reflection-based, attributes extracted at first use

## Troubleshooting

### "Type not found" Error

**Cause**: Schema references undefined type

**Solution**: Ensure all types are decorated/annotated and exported

### "SQL source not valid" Error

**Cause**: sql_source references non-existent database object

**Solution**: Verify database schema or use validation-only mode

### Export produces empty schema

**Cause**: Types not registered with schema registry

**Solution**: Ensure all classes are decorated/annotated and in scope

## Migration Guide

### From REST APIs

1. Define types matching API response structures
2. Map API endpoints to queries/mutations
3. Export schema and compile

### From Other GraphQL Implementations

1. Replicate type definitions in FraiseQL schema
2. Map resolvers to sql_source references
3. Export and compile

## See Also

- [CLI Schema Format Guide](./cli-schema-format.md)
- [E2E Testing Guide](./e2e-testing.md)
- [FraiseQL Architecture](../README.md)
