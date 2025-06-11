# LLM-Native Architecture

FraiseQL is designed from the ground up to be **LLM-native** - optimized for AI-assisted development and code generation. Its architecture provides clear contracts, simple patterns, and minimal ambiguity that make it ideal for Large Language Model understanding and generation.

## Design Philosophy

### Two-Language Simplicity

FraiseQL uses only **two languages** throughout the entire stack:

1. **Python** - For type definitions, business logic, and application code
2. **SQL** - For data modeling, queries, and database functions

This eliminates the complexity of multiple DSLs, configuration formats, or templating languages that can confuse both developers and AI systems.

```python
# Pure Python type definitions
@fraiseql.type
class User:
    id: UUID
    name: str = fraise_field(description="User's display name")
    email: str = fraise_field(description="Email address")
```

```sql
-- Pure SQL view definitions
CREATE VIEW v_users AS
SELECT jsonb_build_object(
    'id', id,
    'name', name,
    'email', email
) as data FROM users;
```

### Clear Architectural Boundaries

FraiseQL establishes **explicit boundaries** between different concerns:

- **Database Views** → Data transformation and aggregation
- **Python Types** → GraphQL schema definition
- **PostgreSQL Functions** → Business logic and mutations
- **Repository Pattern** → Data access abstraction

These boundaries create **clear contracts** that LLMs can easily understand and respect.

## LLM-Friendly Patterns

### 1. Explicit Type Contracts

Every component has clear, typed interfaces:

```python
# Input contract
@fraiseql.input
class CreateUserInput:
    email: str
    name: str
    password: str

# Output contract
@fraiseql.type
class User:
    id: UUID
    email: str
    name: str
    created_at: datetime

# Result contract
@fraiseql.result
class CreateUserResult:
    pass

@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"

@fraiseql.failure
class CreateUserError:
    message: str
    code: str
    field_errors: Optional[dict[str, str]] = None
```

### 2. Consistent Naming Conventions

FraiseQL follows **predictable patterns** that LLMs can easily learn:

- **Types**: `User`, `Post`, `Comment` (PascalCase entities)
- **Inputs**: `CreateUserInput`, `UpdatePostInput` (Action + Entity + Input)
- **Results**: `CreateUserResult` (Action + Entity + Result)
- **Success**: `CreateUserSuccess` (Action + Entity + Success)
- **Errors**: `CreateUserError` (Action + Entity + Error)
- **Views**: `v_users`, `v_posts` (v_ prefix + plural snake_case)
- **Functions**: `fn_create_user`, `fn_update_post` (fn_ prefix + action_entity)

### 3. Declarative Configuration

Configuration is declarative and type-safe:

```python
# Application configuration
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post, Comment],
    mutations=[CreateUser, UpdatePost],
    production=False,
    auto_camel_case=True
)

# Field configuration
@fraiseql.type
class Product:
    name: str = fraise_field(
        description="Product name",
        purpose="Display product title to customers"
    )
    price: Decimal = fraise_field(
        description="Price in USD",
        validation=["positive", "max_precision:2"]
    )
```

### 4. Auto-Generation Patterns

FraiseQL minimizes boilerplate through **predictable auto-generation**:

```python
# This minimal definition...
@fraiseql.mutation
class CreateUser:
    input: CreateUserInput
    result: CreateUserResult

# ...auto-generates:
# 1. PostgreSQL function fn_create_user(input_data JSONB)
# 2. Composite type create_user_result
# 3. GraphQL resolver with error handling
# 4. Type conversion and validation logic
```

## AI Code Generation Benefits

### 1. Minimal Context Requirements

LLMs need only understand:
- Python type annotations
- Basic SQL DDL/DML
- FraiseQL decorators (`@fraiseql.type`, `@fraiseql.input`, etc.)
- Naming conventions

### 2. Error-Resistant Patterns

The architecture prevents common mistakes:

```python
# Type system prevents errors
@fraiseql.type
class User:
    id: UUID  # Must be UUID, not string
    email: str = fraise_field(description="Required field description")
    # created_at: datetime  <- Type hint required, will error without it

# Clear result patterns prevent confusion
@fraiseql.success
class CreateUserSuccess:
    user: User  # Must return the created entity

@fraiseql.failure
class CreateUserError:
    message: str  # Must provide error message
    code: str     # Must provide error code
```

### 3. Template-Driven Generation

LLMs can use consistent templates for common patterns:

```python
# Template: CRUD Entity
@fraiseql.type
class {Entity}:
    id: UUID
    {fields}
    created_at: datetime
    updated_at: datetime

@fraiseql.input
class Create{Entity}Input:
    {required_fields}

@fraiseql.result
class Create{Entity}Result:
    pass

@fraiseql.success
class Create{Entity}Success:
    {entity_lower}: {Entity}

@fraiseql.failure
class Create{Entity}Error:
    message: str
    code: str
```

## Database Schema Generation

### View Template Pattern

```sql
-- Template: Entity View
CREATE VIEW v_{entity_plural} AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        {field_mappings},
        'created_at', created_at,
        'updated_at', updated_at
    ) as data
FROM {table_name};
```

### Function Template Pattern

```sql
-- Template: Create Function
CREATE OR REPLACE FUNCTION fn_create_{entity}(input_data JSONB)
RETURNS {entity}_result AS $$
DECLARE
    result {entity}_result;
    new_id UUID;
BEGIN
    -- Validation
    {validation_logic}

    -- Business rules
    {business_logic}

    -- Insert
    INSERT INTO {table_name} (data)
    VALUES (input_data || jsonb_build_object(
        'id', gen_random_uuid(),
        'created_at', NOW(),
        'updated_at', NOW()
    ))
    RETURNING (data->>'id')::UUID INTO new_id;

    -- Success response
    SELECT data INTO result.{entity}_data
    FROM v_{entity_plural}
    WHERE (data->>'id')::UUID = new_id;

    result.success := true;
    RETURN result;
END;
$$ LANGUAGE plpgsql;
```

## LLM Training Compatibility

### Clear Intent Signals

FraiseQL provides unambiguous signals for LLM understanding:

```python
# Intent: Define a GraphQL type
@fraiseql.type
class User:
    pass

# Intent: Define input for mutations
@fraiseql.input
class CreateUserInput:
    pass

# Intent: Define successful result
@fraiseql.success
class CreateUserSuccess:
    pass

# Intent: Define error result
@fraiseql.failure
class CreateUserError:
    pass
```

### Compositional Patterns

Complex features are built from simple, composable patterns:

```python
# Pattern: Paginated Query
@fraiseql.field
async def users(
    self,
    info: fraiseql.Info,
    first: Optional[int] = 20,
    after: Optional[str] = None
) -> Connection[User]:
    repo = CQRSRepository(info.context["db"])
    result = await repo.paginate("v_users", first=first, after=after)
    return Connection[User].from_dict(result)

# Pattern: Filtered Query
@fraiseql.field
async def posts(
    self,
    info: fraiseql.Info,
    published: Optional[bool] = None
) -> list[Post]:
    repo = CQRSRepository(info.context["db"])
    filters = {"is_published": published} if published is not None else {}
    posts_data = await repo.query("v_posts", filters=filters)
    return [Post.from_dict(data) for data in posts_data]
```

## AI Development Workflow

### 1. Requirements → Types

LLM converts natural language requirements into type definitions:

```
"Create a blog system with users, posts, and comments"
↓
@fraiseql.type
class User: ...
@fraiseql.type
class Post: ...
@fraiseql.type
class Comment: ...
```

### 2. Types → Database Schema

LLM generates database views and tables from types:

```python
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str
```

```sql
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL
);

CREATE VIEW v_users AS
SELECT id, jsonb_build_object(
    'id', id,
    'name', data->>'name',
    'email', data->>'email'
) as data FROM users;
```

### 3. Operations → Mutations

LLM creates mutations for business operations:

```
"Users should be able to create posts"
↓
@fraiseql.mutation
class CreatePost:
    input: CreatePostInput
    result: CreatePostResult
```

### 4. Complete Code Generation

LLM can generate entire features with minimal prompting:

```
"Add user authentication with email/password"
↓
- Authentication types and inputs
- Password hashing utilities
- Login/register mutations
- Protected resolver decorators
- Database schema updates
- Complete error handling
```

## Cost-Efficient LLM Development

FraiseQL's architecture significantly **reduces LLM costs** compared to other architectures:

### Lower Token Usage

1. **Minimal Context Window** - Only Python and SQL, no additional DSLs or frameworks
2. **Predictable Patterns** - LLMs can generate code with fewer iterations
3. **Less Debugging** - Type-safe patterns reduce back-and-forth error correction
4. **Reusable Templates** - Common patterns can be cached and reused

### Comparison with Other Architectures

| Architecture | Languages/DSLs | Context Size | Error Rate | Avg. Tokens per Feature |
|-------------|----------------|--------------|------------|------------------------|
| **FraiseQL** | 2 (Python, SQL) | Small | Low | ~500-1000 |
| Traditional GraphQL | 4-6 (JS/TS, GraphQL, SQL, ORM, Config) | Large | Medium | ~2000-4000 |
| Microservices | 5-8 (Multiple languages, APIs, Config) | Very Large | High | ~3000-6000 |
| Low-Code Platforms | 3-5 (Visual DSL, Config, Custom Scripts) | Medium | Medium | ~1500-3000 |

### Cost Reduction Factors

1. **Fewer Generation Attempts**
   - Clear patterns mean LLMs get it right the first time
   - Type safety catches errors before runtime
   - No ambiguous framework conventions to misinterpret

2. **Smaller Prompts**
   ```python
   # FraiseQL prompt (short and clear):
   "Create a User type with email validation"

   # vs Traditional (requires framework context):
   "Create a GraphQL type for User with email validation using Apollo Server,
    TypeORM entities, class-validator decorators, and ensure it works with
    the existing resolver middleware..."
   ```

3. **Direct Implementation**
   - No ORM translation layer to explain
   - No resolver boilerplate to generate
   - No complex configuration files

4. **Efficient Iterations**
   ```python
   # Adding a field in FraiseQL:
   @fraiseql.type
   class User:
       phone: str = fraise_field(description="Phone number")
   ```

   ```sql
   -- Update view to include phone
   CREATE OR REPLACE VIEW v_users AS
   SELECT jsonb_build_object(
       'id', id,
       'name', data->>'name',
       'email', data->>'email',
       'phone', data->>'phone'  -- One line added
   ) as data FROM users;
   ```

   vs Traditional (multiple files, more tokens):
   - Update GraphQL schema
   - Update TypeScript types
   - Update ORM entity
   - Update validation rules
   - Update resolver logic
   - Update database migration
   - Update DTOs/interfaces

### Real-World Cost Example

For a typical blog API with authentication:

- **FraiseQL**: ~3,200 tokens total
  - Type definitions: 500 tokens
  - SQL tables (JSONB): 400 tokens
  - SQL views: 800 tokens
  - SQL functions: 800 tokens
  - Mutations: 700 tokens

- **Traditional Stack**: ~8,000 tokens total
  - GraphQL schemas: 1,000 tokens
  - Resolver boilerplate: 2,000 tokens
  - ORM models: 1,500 tokens
  - Database migrations: 800 tokens
  - Validation logic: 1,000 tokens
  - Auth middleware: 1,500 tokens
  - Configuration: 200 tokens

**Result: 60% reduction in LLM token usage**

### Key Efficiency Differences

1. **FraiseQL's JSONB Advantage**
   - Schema changes often require no table migrations
   - Views handle data transformation declaratively
   - Functions encapsulate business logic in one place

2. **Traditional Stack Overhead**
   - ORM abstraction requires understanding both ORM and SQL
   - Resolver logic scattered across multiple files
   - Type definitions duplicated in multiple languages
   - Complex middleware chains for cross-cutting concerns

## Production Readiness

Despite being LLM-optimized, FraiseQL maintains production-grade features:

- **Type Safety** - Full static type checking
- **Performance** - Direct SQL execution in production mode
- **Security** - Built-in authentication and authorization
- **Scalability** - PostgreSQL-native architecture
- **Monitoring** - Comprehensive logging and metrics
- **Testing** - Complete test generation patterns

## Best Practices for LLM Integration

### 1. Provide Clear Context

```python
# Good: Clear context for LLM
"""
Create a blog API with:
- Users (id, name, email, created_at)
- Posts (id, title, content, author_id, published, created_at)
- Comments (id, content, post_id, author_id, created_at)

Include:
- CRUD operations for all entities
- Authentication for protected operations
- Pagination for listing operations
- Proper error handling with field validation
"""
```

### 2. Use Consistent Patterns

```python
# Establish patterns that LLM can follow
@fraiseql.type
class BlogPost:  # Always use descriptive names
    id: UUID  # Always include ID
    title: str = fraise_field(description="Post title")  # Always describe fields
    created_at: datetime  # Always include timestamps
```

### 3. Leverage Auto-Generation

```python
# Let FraiseQL generate boilerplate
@fraiseql.mutation
class CreateBlogPost:
    input: CreateBlogPostInput
    result: CreateBlogPostResult
    # Everything else is auto-generated
```

FraiseQL's LLM-native architecture makes it the ideal choice for AI-assisted development, enabling rapid generation of production-ready GraphQL APIs with minimal human intervention while maintaining code quality and performance.
