# FraiseQL Mutations Demo

This demo shows how to use FraiseQL's PostgreSQL function-based mutation system.

## Features Demonstrated

- Type-safe mutations using the `@mutation` decorator
- Automatic resolver generation that calls PostgreSQL functions
- Success/Error union types with proper parsing
- Complex object instantiation from JSONB
- Real-world error handling patterns

## Setup

1. Start the PostgreSQL database:
   ```bash
   docker-compose up -d
   ```

2. Install dependencies (if not already installed):
   ```bash
   pip install psycopg[async]
   ```

3. Run the demo:
   ```bash
   python examples/mutations_demo/demo.py
   ```

## What It Does

The demo performs these operations:

1. **Creates a user** - Shows successful user creation
2. **Handles duplicates** - Demonstrates conflict detection with helpful suggestions
3. **Updates a user** - Shows partial updates with field tracking
4. **Handles not found** - Shows error handling for missing records
5. **Deletes a user** - Demonstrates deletion with data return
6. **Builds GraphQL schema** - Shows how mutations integrate with the schema

## Key Concepts

### PostgreSQL Functions

All mutations are implemented as PostgreSQL functions that:
- Accept JSONB input
- Return a standardized `mutation_result` type
- Handle all business logic in the database
- Provide rich error information

### Type Safety

The FraiseQL mutation classes ensure:
- Input validation through `@fraiseql.input` types
- Success/Error discrimination through union types
- Automatic JSONB to Python object conversion
- Full GraphQL schema integration

### Error Handling

Errors can include:
- Detailed messages
- Related objects (e.g., conflicting user)
- Suggestions (e.g., alternative email)
- Validation errors per field

## Database Schema

The demo uses a simple users table with JSONB storage:

```sql
CREATE TABLE users (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE
);
```

This pattern allows flexible schema evolution while maintaining PostgreSQL's powerful querying capabilities.

## Extending

To add your own mutations:

1. Define input/success/error types
2. Create a PostgreSQL function returning `mutation_result`
3. Use FraiseQLMutation class with matching function names
4. Add to schema with `build_fraiseql_schema()`

The system automatically handles the rest!
