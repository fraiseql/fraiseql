# AutoFraiseQL

AutoFraiseQL is FraiseQL's automatic GraphQL schema generation from PostgreSQL database schemas. It introspects your database and generates a complete GraphQL API without manual schema definition.

## ✨ Key Features

### 🔄 Automatic Schema Generation
- **Database-First**: Define your API in PostgreSQL, get GraphQL automatically
- **Zero Boilerplate**: No manual GraphQL schema files to maintain
- **Type Safety**: Full TypeScript/Python type generation included

### 📝 PostgreSQL Comments → GraphQL Descriptions
AutoFraiseQL automatically converts PostgreSQL object comments into GraphQL schema descriptions:

- **View comments** → GraphQL type descriptions
- **Function comments** → GraphQL mutation descriptions
- **Composite type comments** → GraphQL input type descriptions
- **Column comments** → Future GraphQL field descriptions (infrastructure ready)

```sql
-- Add comments to your database objects
COMMENT ON VIEW app.v_user_profile IS 'User profile data with contact information';
COMMENT ON FUNCTION app.fn_create_user(text, text) IS 'Creates a new user account';

-- Get rich GraphQL documentation automatically
type UserProfile {
  """User profile data with contact information"""
  # ... fields
}

type Mutation {
  createUser(input: CreateUserInput!): UserPayload
}
```

### 🎯 Smart Introspection
- **Pattern-Based Discovery**: Automatically finds views (`v_*`), functions (`fn_*`), and types
- **Schema-Aware**: Respects PostgreSQL schemas for multi-tenant applications
- **Performance Optimized**: Efficient queries with minimal database load

### 🔧 Enterprise-Ready
- **Multi-Tenant**: Schema-based tenant isolation
- **Security**: Built-in authentication and authorization
- **Monitoring**: Comprehensive metrics and health checks
- **Production**: Battle-tested in high-traffic applications

## 🚀 Quick Start

### 1. Define Your Database Schema

```sql
-- Create a users table
CREATE TABLE users (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  email text NOT NULL UNIQUE,
  name text NOT NULL,
  created_at timestamptz DEFAULT now()
);

-- Create a view with a comment
CREATE VIEW app.v_user_profile AS
SELECT id, email, name, created_at FROM users;

COMMENT ON VIEW app.v_user_profile IS 'User profile data with contact information';

-- Create a function with a comment
CREATE FUNCTION app.fn_create_user(email text, name text)
RETURNS jsonb
LANGUAGE plpgsql
AS $$
BEGIN
  INSERT INTO users (email, name) VALUES (email, name)
  RETURNING row_to_json(users.*)::jsonb;
END;
$$;

COMMENT ON FUNCTION app.fn_create_user(text, text) IS 'Creates a new user account with email verification';
```

### 2. AutoFraiseQL Generates

```python
# Automatic GraphQL schema
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
  email: String!
  name: String!
}

type UserPayload {
  success: UserProfile
  failure: ValidationError
}
```

### 3. Use in Your Application

```python
from fraiseql import FraiseQL

app = FraiseQL()

# GraphQL API is automatically available at /graphql
# Complete with descriptions from your PostgreSQL comments!
```

## 📚 Documentation

- **[PostgreSQL Comments Guide](postgresql-comments.md)** - How to use database comments for GraphQL documentation
- **[Getting Started](../getting-started/quickstart.md)** - 5-minute setup guide
- **[Core Concepts](../core/concepts-glossary.md)** - Understanding FraiseQL's architecture
- **[API Reference](../api-reference/)** - Complete API documentation

## 🎯 Use Cases

### API Documentation
Keep your GraphQL API documentation in sync with your database schema:

```sql
COMMENT ON VIEW reporting.v_monthly_revenue IS
'Monthly revenue breakdown by product category.
Excludes cancelled orders and test accounts.
Updated daily at 02:00 UTC.';
```

### Multi-Team Collaboration
Database comments serve as the single source of truth for API contracts:

```sql
COMMENT ON FUNCTION api.fn_user_login(email text, password_hash text) IS
'Authenticates a user and returns a session token.
Rate limited to 5 attempts per minute per IP.
Returns JWT token valid for 24 hours.';
```

### Schema Evolution
Comments help track API changes and maintain backward compatibility:

```sql
COMMENT ON VIEW api.v_user_public IS
'Public user data for profiles and search.
Deprecated: Use v_user_profile instead.
Will be removed in API v2.0.';
```

## 🔧 Configuration

AutoFraiseQL is configured through environment variables and database schema conventions:

```bash
# Database connection
DATABASE_URL=postgresql://user:pass@localhost:5432/myapp

# Schema discovery
FRAISEQL_SCHEMAS=public,app,api
FRAISEQL_VIEW_PATTERN=v_%
FRAISEQL_FUNCTION_PATTERN=fn_%
```

## 🚀 Performance

- **Sub-millisecond introspection**: Schema discovery takes < 1ms
- **Zero runtime overhead**: Generated code is pure Python
- **Connection pooling**: Efficient database connection management
- **Caching**: Built-in query result caching

## 🏗️ Architecture

```
PostgreSQL Schema ── Introspection ──→ GraphQL Schema
     ↓                        ↓              ↓
  Comments ──────────────→ Descriptions ──→ Documentation
     ↓                        ↓              ↓
  Views ──────────────────→ Types ─────────→ Queries
     ↓                        ↓              ↓
 Functions ───────────────→ Mutations ─────→ API
```

## 📈 Monitoring

AutoFraiseQL provides comprehensive observability:

- **Schema change detection**: Automatic cache invalidation on schema changes
- **Performance metrics**: Query execution times and cache hit rates
- **Health checks**: Database connectivity and schema validation
- **Error tracking**: Detailed error reporting with context

## 🤝 Contributing

AutoFraiseQL is part of the FraiseQL framework. See the main [contributing guide](../../CONTRIBUTING.md) for development setup and contribution guidelines.

## 📄 License

AutoFraiseQL is licensed under the same terms as FraiseQL. See [LICENSE](../../LICENSE) for details.
