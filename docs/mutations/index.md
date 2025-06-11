# Mutations

FraiseQL implements a powerful PostgreSQL function-based mutation system that keeps business logic in the database while providing type-safe GraphQL mutations.

## Overview

Instead of writing mutation resolvers in Python, FraiseQL:
1. Calls PostgreSQL functions that contain your business logic
2. Automatically parses the results into typed Success/Error responses
3. Handles complex object instantiation from JSONB data

## Key Benefits

- **Single Source of Truth**: All business logic lives in PostgreSQL
- **Better Performance**: Single database round-trip per mutation
- **Type Safety**: Automatic conversion between JSONB and Python types
- **Rich Responses**: Return complex objects without N+1 queries
- **Transactional**: Full ACID guarantees from PostgreSQL

## Quick Example

```python
# Python: Define the mutation types
@fraiseql.mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError
```

```sql
-- PostgreSQL: Implement the logic
CREATE FUNCTION graphql.create_user(input_data JSONB)
RETURNS mutation_result AS $$
BEGIN
    -- Your business logic here
    -- Return standardized result
END;
$$ LANGUAGE plpgsql;
```

## Documentation

- [PostgreSQL Function-Based Mutations](./postgresql-function-based.md) - Complete guide to the mutation system
- [Migration Guide](./migration-guide.md) - How to migrate from manual resolvers

## Design Philosophy

FraiseQL's mutation system follows these principles:

1. **Database-First**: PostgreSQL is the best place for business logic
2. **Type-Safe**: Strong typing from database to GraphQL
3. **Flexible**: Support complex return types with nested objects
4. **Simple**: Minimal boilerplate, maximum functionality

## Comparison with Traditional Approaches

| Aspect | Traditional (Strawberry/GraphQL) | FraiseQL |
|--------|----------------------------------|-----------|
| Business Logic | Python resolvers | PostgreSQL functions |
| Database Calls | Multiple (N+1 risk) | Single round-trip |
| Type Safety | Manual mapping | Automatic from JSONB |
| Transaction Handling | Manual in Python | Native PostgreSQL |
| Testing | Mock database | Test functions directly |
| Code Volume | ~100 lines per mutation | ~20 lines per mutation |

## Next Steps

- Read the [full documentation](./postgresql-function-based.md)
- See the [blog API example](/examples/blog_api) for a complete implementation
- Check the [migration guide](./migration-guide.md) if moving from another system
