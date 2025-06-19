# Code Generation

FraiseQL's code generation tools help you quickly scaffold your application with type-safe code and database migrations.

## Overview

The `fraiseql generate` command provides several generators:

- **Migrations**: Generate SQL migrations from your Python types
- **CRUD Operations**: Generate complete Create, Read, Update, Delete mutations
- **GraphQL Schema**: Export your schema for documentation or tooling

## Migration Generation

Generate database migrations that follow FraiseQL's CQRS pattern:

```bash
fraiseql generate migration User
```

This creates a migration file with:
- JSONB table structure
- Proper indexes for performance
- Updated_at triggers
- Soft delete support
- A view for FraiseQL queries

Example output:

```sql
-- migrations/20240615123045_create_users.sql
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMPTZ
);

-- GIN index for JSONB queries
CREATE INDEX idx_users_data ON users USING gin(data);

-- View for FraiseQL
CREATE OR REPLACE VIEW v_users AS
SELECT
    id,
    data,
    created_at,
    updated_at
FROM users
WHERE deleted_at IS NULL;
```

## CRUD Generation

Generate complete CRUD mutations for any type:

```bash
fraiseql generate crud User
```

This creates:
- Input types for create/update operations
- Success/error result types
- Mutation functions with proper error handling
- Repository integration

Example generated code:

```python
@fraiseql.input
class CreateUserInput:
    """Input for creating a User."""
    email: str = fraise_field(description="Email address")
    name: str = fraise_field(description="Display name")

@fraiseql.mutation
async def create_user(
    input: CreateUserInput,
    repository: CQRSRepository,
) -> UserResult:
    """Create a new User."""
    try:
        result = await repository.create("users", input)
        return UserSuccess(
            user=result,
            message="User created successfully"
        )
    except Exception as e:
        return UserError(
            message=str(e),
            code="CREATE_FAILED"
        )
```

## Schema Generation

Export your complete GraphQL schema:

```bash
fraiseql generate schema --output schema.graphql
```

This is useful for:
- Documentation
- Client code generation
- Schema validation in CI/CD
- GraphQL tooling integration

## Advanced Usage

### Custom Table Names

```bash
fraiseql generate migration BlogPost --table blog_posts
```

### Batch Generation

Generate multiple resources at once:

```bash
# Generate everything for a new entity
fraiseql generate migration User
fraiseql generate crud User
```

### Template Customization

FraiseQL uses Jinja2 templates for code generation. You can customize them by creating a `.fraiseql/templates/` directory in your project.

## Best Practices

1. **Generate First, Customize Second**: Use generators to create the initial structure, then customize the generated code

2. **Review Generated SQL**: Always review migrations before running them in production

3. **Type-First Development**: Define your Python types first, then generate the database schema

4. **Keep Generated Code**: Commit generated code to version control and customize as needed

## Integration with Development Workflow

1. Define your type:
   ```python
   @fraise_type
   class Product:
       id: UUID
       name: str
       price: Decimal
       in_stock: bool
   ```

2. Generate migration:
   ```bash
   fraiseql generate migration Product
   ```

3. Run migration:
   ```bash
   psql $DATABASE_URL -f migrations/*_create_products.sql
   ```

4. Generate CRUD:
   ```bash
   fraiseql generate crud Product
   ```

5. Import and use:
   ```python
   from mutations.product_mutations import (
       create_product,
       update_product,
       delete_product
   )
   ```

## Next Steps

- Explore [custom mutations](../mutations/index.md) for complex business logic
- Read about [performance optimization](../advanced/performance.md) for generated code
