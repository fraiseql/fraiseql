# PostgreSQL Comments to GraphQL Descriptions

FraiseQL automatically converts PostgreSQL object comments into GraphQL schema descriptions, providing rich documentation directly from your database schema.

## Overview

When you add comments to PostgreSQL database objects, FraiseQL automatically uses these comments as descriptions in the generated GraphQL schema. This keeps your API documentation in sync with your database schema.

## Supported Comment Types

### 1. View Comments → GraphQL Type Descriptions

PostgreSQL view comments become GraphQL object type descriptions.

```sql
-- Create a view with a comment
CREATE VIEW app.v_user_profile AS
SELECT id, email, name, created_at
FROM users;

-- Add a descriptive comment
COMMENT ON VIEW app.v_user_profile IS 'User profile data with contact information';
```

**Result**: The GraphQL type `UserProfile` will have the description "User profile data with contact information".

### 2. Function Comments → GraphQL Mutation Descriptions

PostgreSQL function comments become GraphQL mutation descriptions.

```sql
-- Create a function with a comment
CREATE FUNCTION app.fn_create_user(email text, name text)
RETURNS jsonb
LANGUAGE plpgsql
AS $$
BEGIN
  -- function implementation
END;
$$;

-- Add a descriptive comment
COMMENT ON FUNCTION app.fn_create_user(text, text) IS 'Creates a new user account with email verification';
```

**Result**: The GraphQL mutation `createUser` will have the description "Creates a new user account with email verification".

### 3. Composite Type Comments → GraphQL Input Type Descriptions

PostgreSQL composite type comments become GraphQL input type descriptions.

```sql
-- Create a composite type for input
CREATE TYPE app.type_create_user_input AS (
  email text,
  name text
);

-- Add a descriptive comment
COMMENT ON TYPE app.type_create_user_input IS 'Input parameters for user creation';
```

**Result**: The GraphQL input type `CreateUserInput` will have the description "Input parameters for user creation".

### 4. Column Comments (Infrastructure Ready)

PostgreSQL column comments are captured during introspection and ready for future field-level GraphQL descriptions.

```sql
-- Add comments to table columns
COMMENT ON COLUMN users.email IS 'Primary email address for authentication';
COMMENT ON COLUMN users.created_at IS 'Account creation timestamp (UTC)';
```

**Status**: Column comments are captured but not yet used in GraphQL field descriptions (planned for future release).

## Priority Hierarchy

When multiple comment sources are available, FraiseQL uses this priority order:

1. **Explicit annotations** (highest priority)
2. **PostgreSQL comments**
3. **Auto-generated descriptions** (lowest priority)

## Examples

### Complete Example

```sql
-- Create user table
CREATE TABLE users (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  email text NOT NULL,
  name text NOT NULL,
  created_at timestamptz DEFAULT now()
);

-- Add column comments
COMMENT ON COLUMN users.email IS 'Primary email address for authentication';
COMMENT ON COLUMN users.name IS 'Full name of the user';
COMMENT ON COLUMN users.created_at IS 'Account creation timestamp (UTC)';

-- Create view with comment
CREATE VIEW app.v_user_profile AS
SELECT id, email, name, created_at FROM users;

COMMENT ON VIEW app.v_user_profile IS 'User profile data with contact information';

-- Create input type with comment
CREATE TYPE app.type_create_user_input AS (
  email text,
  name text
);

COMMENT ON TYPE app.type_create_user_input IS 'Input parameters for user creation';

-- Create function with comment
CREATE FUNCTION app.fn_create_user(input jsonb)
RETURNS jsonb
LANGUAGE plpgsql
AS $$
BEGIN
  -- implementation
END;
$$;

COMMENT ON FUNCTION app.fn_create_user(jsonb) IS 'Creates a new user account with email verification';
```

### Generated GraphQL Schema

```graphql
type UserProfile {
  """User profile data with contact information"""
  id: UUID!
  email: String!
  name: String!
  createdAt: DateTime!
}

type Mutation {
  createUser(input: CreateUserInput!): UserPayload
}

input CreateUserInput {
  """Input parameters for user creation"""
  email: String!
  name: String!
}

type UserPayload {
  success: User
  failure: ValidationError
}
```

## Best Practices

### 1. Use Descriptive Comments

Write clear, concise comments that explain the purpose and usage of database objects:

```sql
-- Good
COMMENT ON VIEW app.v_active_users IS 'Users who have logged in within the last 30 days';

-- Less helpful
COMMENT ON VIEW app.v_active_users IS 'Active users view';
```

### 2. Keep Comments in Sync

Update comments when you modify the underlying database objects:

```sql
-- When changing a view's purpose, update the comment
COMMENT ON VIEW app.v_user_profile IS 'User profile data including contact information and preferences';
```

### 3. Use Consistent Naming

Follow your team's conventions for comment style and content.

### 4. Document Complex Logic

Use comments to explain complex business logic in views and functions:

```sql
COMMENT ON VIEW app.v_user_revenue IS
'Revenue per user calculated from completed orders, including refunds and discounts.
Excludes cancelled orders and test accounts.';
```

## Implementation Details

### Comment Storage

- **Views**: Comments stored in `pg_class` table
- **Functions**: Comments stored in `pg_proc` table
- **Types**: Comments stored in `pg_type` table
- **Columns**: Comments stored in `pg_description` table

### Introspection Process

1. FraiseQL introspects database objects using PostgreSQL system catalogs
2. Comments are retrieved using `obj_description()` and `col_description()` functions
3. Comments are attached to metadata objects during schema generation
4. GraphQL schema generation uses comments as descriptions

### Limitations

- PostgreSQL does not support comments on composite type attributes (`COMMENT ON ATTRIBUTE` syntax)
- Column comments are captured but not yet used for GraphQL field descriptions
- Comments are limited to PostgreSQL's comment length restrictions

## Troubleshooting

### Comments Not Appearing

1. **Check comment syntax**: Ensure you're using correct PostgreSQL comment syntax
2. **Verify permissions**: Make sure the database user can read system catalogs
3. **Check object names**: Ensure schema-qualified names are used correctly

### Debug Commands

```sql
-- Check view comments
SELECT
  c.relname,
  obj_description(c.oid, 'pg_class') as comment
FROM pg_class c
WHERE c.relkind = 'v';

-- Check function comments
SELECT
  p.proname,
  obj_description(p.oid, 'pg_proc') as comment
FROM pg_proc p;

-- Check type comments
SELECT
  t.typname,
  obj_description(t.oid, 'pg_type') as comment
FROM pg_type t;
```

## Future Enhancements

- **Field-level descriptions**: Use column comments for GraphQL field descriptions
- **Enum descriptions**: Support for enum value comments
- **Relationship descriptions**: Automatic descriptions for foreign key relationships
