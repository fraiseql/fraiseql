# AutoFraiseQL: Automatic GraphQL Schema Generation

AutoFraiseQL is a revolutionary feature that transforms FraiseQL from a "write Python decorators" framework into a "write SQL comments" framework, making it the first truly zero-code GraphQL backend.

## Overview

Instead of writing Python code to define GraphQL schemas, AutoFraiseQL allows you to:

1. **Create database views** with standard naming conventions (e.g., `v_user`, `v_post`)
2. **Add YAML metadata** in PostgreSQL comments using `@fraiseql:type` and `@fraiseql:mutation` annotations
3. **Get complete GraphQL APIs** automatically generated

## Quick Example

```sql
-- Create a view with metadata
CREATE VIEW v_user AS
SELECT id, name, email, created_at
FROM users;

-- Add FraiseQL metadata
COMMENT ON VIEW v_user IS '@fraiseql:type
trinity: true
description: User account information
expose_fields:
  - id
  - name
  - email';
```

This automatically generates:
- `User` GraphQL type
- `user(id)` query
- `users(where, orderBy, limit, offset)` query
- `UserWhereInput` and `UserOrderByInput` types

## Features

- **Zero Python code** required for basic CRUD operations
- **PostgreSQL-native** metadata storage
- **100% backward compatible** with existing FraiseQL applications
- **Type-safe** GraphQL APIs with automatic validation
- **Performance optimized** with intelligent caching

## Getting Started

1. Enable auto-discovery in your FraiseQL app:
   ```python
   app = create_fraiseql_app(database_url, auto_discover=True)
   ```

2. Create views with `@fraiseql:type` annotations

3. Your GraphQL API is ready!

## Documentation Sections

- [Type Annotations](./type-annotations.md) - How to annotate database views
- [Mutation Annotations](./mutation-annotations.md) - How to create mutations from functions
- [Migration Guide](./migration-guide.md) - Moving from manual to auto-discovery
- [Best Practices](./best-practices.md) - Naming conventions and patterns
- [Troubleshooting](./troubleshooting.md) - Common issues and solutions
