# Phase 5: Developer Experience & Tooling

**Status**: Ready for Implementation
**Created**: 2025-10-24
**Estimated Time**: 2-3 weeks
**Complexity**: Medium-High
**Prerequisites**: Phase 4 completed

---

## 📋 Executive Summary

Phase 5 focuses on improving developer experience through:
1. VS Code extension for FraiseQL
2. Enhanced CLI with better commands
3. Improved error messages and debugging
4. Comprehensive testing guide
5. Development tools and utilities
6. Better logging and observability

**Impact**: Reduce development time by 50%, improve debugging experience, increase developer satisfaction.

---

## 🎯 Objectives

### Primary Goals
- ✅ Reduce time to debug issues from 30min to 5min
- ✅ Provide IDE integration for type safety
- ✅ Make CLI more powerful and intuitive
- ✅ Improve error messages (clear, actionable)
- ✅ Comprehensive testing documentation
- ✅ Better development workflow

### Success Metrics
- VS Code extension: 500+ installs
- CLI satisfaction: 9+/10
- Error resolution time: -80%
- Test coverage: 90%+
- Developer NPS: 50+

---

## 📦 Task Breakdown

---

## Task 1: VS Code Extension

**Priority**: High
**Time**: 1 week
**Complexity**: High

### Features

#### 1. Syntax Highlighting

**File**: `vscode-fraiseql/syntaxes/fraiseql.tmLanguage.json`

```json
{
  "scopeName": "source.fraiseql",
  "patterns": [
    {
      "name": "keyword.decorator.fraiseql",
      "match": "@(type|query|mutation|input|success|failure|authorized)"
    },
    {
      "name": "support.class.fraiseql",
      "match": "\\b(sql_source|jsonb_column|table_view|pk_column)\\b"
    }
  ]
}
```

#### 2. IntelliSense

**Autocomplete for decorators:**
```typescript
// extension.ts
export function activate(context: vscode.ExtensionContext) {
  const provider = vscode.languages.registerCompletionItemProvider(
    'python',
    {
      provideCompletionItems(document, position) {
        const completions = [];

        // @type decorator
        const typeCompletion = new vscode.CompletionItem('@type');
        typeCompletion.kind = vscode.CompletionItemKind.Snippet;
        typeCompletion.insertText = new vscode.SnippetString(
          '@type(sql_source="${1:v_table}", jsonb_column="${2:data}")\n' +
          'class ${3:TypeName}:\n' +
          '    ${4:id}: int\n' +
          '    ${5:name}: str'
        );
        typeCompletion.documentation = new vscode.MarkdownString(
          'Map PostgreSQL view to GraphQL type'
        );
        completions.push(typeCompletion);

        // @query decorator
        const queryCompletion = new vscode.CompletionItem('@query');
        queryCompletion.kind = vscode.CompletionItemKind.Snippet;
        queryCompletion.insertText = new vscode.SnippetString(
          '@query\n' +
          'def ${1:query_name}() -> list[${2:Type}]:\n' +
          '    pass  # Auto-generated'
        );
        completions.push(queryCompletion);

        return completions;
      }
    },
    '@'
  );

  context.subscriptions.push(provider);
}
```

#### 3. Type Checking Integration

**Validate sql_source exists:**
```typescript
async function validateSqlSource(document: vscode.TextDocument) {
  const text = document.getText();
  const typeRegex = /@type\(sql_source="([^"]+)"/g;

  let match;
  while ((match = typeRegex.exec(text)) !== null) {
    const sqlSource = match[1];

    // Check if view exists in database
    const exists = await checkViewExists(sqlSource);

    if (!exists) {
      const diagnostic = new vscode.Diagnostic(
        new vscode.Range(
          document.positionAt(match.index),
          document.positionAt(match.index + match[0].length)
        ),
        `PostgreSQL view "${sqlSource}" does not exist`,
        vscode.DiagnosticSeverity.Error
      );
      diagnostics.push(diagnostic);
    }
  }
}
```

#### 4. GraphQL Schema Preview

**Command**: "FraiseQL: Preview Schema"

```typescript
vscode.commands.registerCommand('fraiseql.previewSchema', async () => {
  const terminal = vscode.window.createTerminal('FraiseQL Schema');
  terminal.sendText('fraiseql generate schema --stdout');
  terminal.show();
});
```

#### 5. Database Connection Manager

**UI for managing database connections:**
```typescript
class DatabaseTreeProvider implements vscode.TreeDataProvider<DatabaseItem> {
  getChildren(element?: DatabaseItem): DatabaseItem[] {
    if (!element) {
      // Root: Show connections
      return this.connections.map(conn => new DatabaseItem(conn));
    } else {
      // Show tables/views
      return this.getTables(element.connection);
    }
  }
}
```

#### 6. Quick Actions

**Hover tooltips:**
- Show view definition when hovering over `sql_source`
- Show GraphQL schema when hovering over type name
- Show function signature when hovering over mutation

**Code actions:**
- "Generate migration for new field"
- "Create PostgreSQL view"
- "Generate test for query"

---

### Implementation Steps

1. **Setup extension project** (1 day)
   ```bash
   npm install -g yo generator-code
   yo code
   # Choose: New Extension (TypeScript)
   ```

2. **Implement syntax highlighting** (1 day)
3. **Add IntelliSense** (2 days)
4. **Integrate type checking** (2 days)
5. **Add commands and UI** (1 day)
6. **Testing and polish** (1 day)

7. **Publish to marketplace:**
   ```bash
   vsce package
   vsce publish
   ```

---

## Task 2: Enhanced CLI

**Priority**: High
**Time**: 4-5 days
**Complexity**: Medium

### New Commands

#### 1. `fraiseql doctor`

**Diagnose common issues:**
```bash
$ fraiseql doctor

✅ Python version: 3.10.5 (OK)
✅ PostgreSQL connection: OK
❌ Missing PostgreSQL extension: uuid-ossp
   Fix: psql -c "CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\""
⚠️  No views found matching pattern v_*
   Check: Do you have JSONB views created?
✅ Rust pipeline: OK (7.3x faster than Python)

Overall: 3 checks passed, 1 warning, 1 error
```

**Implementation:**
```python
import click
from fraiseql.cli import cli

@cli.command()
def doctor():
    """Diagnose common configuration issues."""
    checks = [
        check_python_version(),
        check_database_connection(),
        check_postgresql_extensions(),
        check_views_exist(),
        check_rust_pipeline()
    ]

    passed = sum(1 for c in checks if c.status == 'passed')
    warnings = sum(1 for c in checks if c.status == 'warning')
    errors = sum(1 for c in checks if c.status == 'error')

    click.echo(f"\nOverall: {passed} checks passed, {warnings} warning, {errors} error")
```

---

#### 2. `fraiseql generate`

**Generate code:**
```bash
# Generate TypeScript types
$ fraiseql generate types --output=src/generated/types.ts

# Generate GraphQL schema
$ fraiseql generate schema --output=schema.graphql

# Generate database migration
$ fraiseql generate migration --name="add_user_avatar"

# Generate CRUD for table
$ fraiseql generate crud --table=tb_product --output=app/products.py
```

**Example output for CRUD:**
```python
# Generated by fraiseql generate crud --table=tb_product

from fraiseql import type, query, mutation, input, success

@type(sql_source="v_product", jsonb_column="data")
class Product:
    id: int
    name: str
    price: float

@query
def products() -> list[Product]:
    pass

@query
def product(id: int) -> Product | None:
    pass

@input
class CreateProductInput:
    name: str
    price: float

@success
class ProductCreated:
    product: Product

@mutation
class CreateProduct:
    input: CreateProductInput
    success: ProductCreated
```

---

#### 3. `fraiseql explain`

**Explain query execution:**
```bash
$ fraiseql explain --query="SELECT data FROM v_user LIMIT 10"

PostgreSQL Execution Plan:
┌─────────────────────────────────────────────────┐
│ Limit  (cost=0.00..1.10 rows=10)               │
│   →  Seq Scan on tb_user  (cost=0.00..110.00)  │
└─────────────────────────────────────────────────┘

Execution Time: 2.3ms
Rows Returned: 10

Recommendations:
✅ Query is optimal
💡 Consider adding index on frequently filtered columns
```

---

#### 4. `fraiseql test`

**Run tests with coverage:**
```bash
$ fraiseql test --coverage

Running tests...
✅ test_user_query          PASSED    12ms
✅ test_create_user         PASSED    45ms
✅ test_invalid_input       PASSED    8ms
❌ test_update_user         FAILED    23ms
   AssertionError: Expected 200, got 404

Coverage: 87% (243/280 lines)

Missing coverage:
  app/users.py: lines 45-52 (error handling)
  app/posts.py: lines 89-93 (edge case)
```

---

#### 5. `fraiseql benchmark`

**Benchmark performance:**
```bash
$ fraiseql benchmark --queries=queries.graphql --duration=30s

Running benchmark (30 seconds)...

Results:
  Total requests: 45,230
  Requests/sec:   1,507.7
  Latency P50:    8.2ms
  Latency P95:    24.6ms
  Latency P99:    45.1ms
  Errors:         0 (0%)

Breakdown by query:
  getUser:   P95 12.3ms  (40% of requests)
  getPosts:  P95 28.4ms  (60% of requests)

Recommendations:
  ⚡ Consider adding index on tb_post(user_id)
  💡 Enable APQ for large queries
```

---

#### 6. `fraiseql dev`

**Enhanced development server:**
```bash
$ fraiseql dev --reload --debug

🚀 FraiseQL Development Server
📍 GraphQL endpoint: http://localhost:8000/graphql
📊 Playground:       http://localhost:8000/graphql
🔍 Debug mode:       enabled
♻️  Auto-reload:      enabled

[12:34:56] Watching for changes...
[12:34:57] Connected to PostgreSQL (10 connections)
[12:34:58] Loaded 15 types, 23 queries, 12 mutations
[12:34:59] Server started (127.0.0.1:8000)

[12:35:12] File changed: app/users.py
[12:35:13] Reloading...
[12:35:14] ✅ Reload complete (523ms)
```

---

### Implementation Priority

1. `fraiseql doctor` - Most valuable, implement first
2. `fraiseql generate` - High productivity gain
3. `fraiseql test` - Quality improvement
4. `fraiseql explain` - Performance optimization
5. `fraiseql benchmark` - Performance validation
6. `fraiseql dev` - Enhanced DX

---

## Task 3: Improved Error Messages

**Priority**: High
**Time**: 2-3 days
**Complexity**: Medium

### Before & After Examples

#### Example 1: Missing View

**Before:**
```
asyncpg.exceptions.UndefinedTableError: relation "v_user" does not exist
```

**After:**
```
❌ PostgreSQL View Not Found

View "v_user" does not exist in database "mydb".

This view is required by the @type decorator:
  File: app/users.py, line 12
  Code: @type(sql_source="v_user")

Common causes:
  1. View not created yet → Run migrations
  2. Wrong view name → Check spelling
  3. Wrong database → Check DATABASE_URL

How to fix:
  1. Create the view:
     psql -d mydb -c "CREATE VIEW v_user AS SELECT ..."

  2. Or run migrations:
     psql -d mydb -f migrations/001_create_views.sql

  3. Verify view exists:
     psql -d mydb -c "\d v_user"

Need help? https://fraiseql.dev/docs/troubleshooting#missing-view
```

---

#### Example 2: Type Mismatch

**Before:**
```
pydantic.error_wrappers.ValidationError: 1 validation error for User
id
  value is not a valid integer (type=type_error.integer)
```

**After:**
```
❌ GraphQL Type Mismatch

Field "id" on type "User" expects int but got str from database.

Location:
  File: app/users.py, line 15
  Type: User
  Field: id: int

PostgreSQL returns:
  Type: TEXT or VARCHAR
  Value: "abc123"

How to fix:

Option 1: Change Python type to match database
  class User:
      id: str  # ← Change from int to str

Option 2: Change database column to INTEGER
  ALTER TABLE tb_user ALTER COLUMN id TYPE INTEGER USING id::INTEGER;

Option 3: Convert in view
  CREATE VIEW v_user AS
  SELECT
      id::INTEGER as id,  -- ← Cast to integer
      ...

Type mapping reference:
  https://fraiseql.dev/docs/core/types-and-schema#type-mapping
```

---

#### Example 3: N+1 Query Warning

**Before:** (Silent performance problem)

**After:**
```
⚠️  Potential N+1 Query Detected

Query "posts" may cause N+1 queries:
  Fetching 100 posts
  → Triggers 100 separate queries for authors

Impact:
  Current: 101 queries, 450ms total
  Optimal: 1 query, 85ms total

Performance: 5.3x slower than necessary

How to fix:
  Move author relationship into JSONB view:

  CREATE VIEW v_post AS
  SELECT
      id,
      jsonb_build_object(
          'id', id,
          'title', title,
          'author', (
              SELECT jsonb_build_object('id', u.id, 'name', u.name)
              FROM tb_user u
              WHERE u.id = tb_post.user_id
          )
      ) as data
  FROM tb_post;

Learn more: https://fraiseql.dev/docs/performance#n-plus-one
```

---

### Implementation

**Error handler middleware:**
```python
from fraiseql.errors import FraiseQLError, enhance_error

@app.exception_handler(Exception)
async def enhanced_error_handler(request, exc):
    # Enhance error with context
    enhanced = enhance_error(exc, context={
        'file': get_source_file(exc),
        'line': get_source_line(exc),
        'code_snippet': get_code_snippet(exc)
    })

    # Format error message
    message = format_error_message(enhanced)

    # Log with context
    logger.error(message, extra={'exception': exc})

    return JSONResponse(
        status_code=500,
        content={'error': message}
    )
```

---

## Task 4: Comprehensive Testing Guide

**Priority**: Medium
**Time**: 3 days
**Complexity**: Low

### Content

**File**: `/home/lionel/code/fraiseql/docs/development/testing-guide.md`

```markdown
# Testing Guide

Complete guide to testing FraiseQL applications.

## Test Categories

### 1. Unit Tests (Fast, Isolated)

Test individual functions and classes:

\```python
import pytest
from app.users import User

def test_user_validation():
    user = User(id=1, name="Alice", email="alice@example.com")
    assert user.email == "alice@example.com"

def test_invalid_email():
    with pytest.raises(ValidationError):
        User(id=1, name="Alice", email="invalid")
\```

### 2. Integration Tests (Database)

Test with real database:

\```python
import pytest
from fraiseql.database import Database

@pytest.fixture
async def db():
    """Test database fixture."""
    db = await Database.create_pool(
        database_url="postgresql://test:test@localhost/test_db"
    )
    yield db
    await db.close()

@pytest.mark.asyncio
async def test_create_user(db):
    result = await db.call_function(
        "fn_create_user",
        email="test@example.com",
        name="Test User"
    )
    assert result['success'] == True
    assert result['user']['email'] == "test@example.com"
\```

### 3. GraphQL Tests

Test GraphQL queries:

\```python
from httpx import AsyncClient

@pytest.mark.asyncio
async def test_graphql_query():
    async with AsyncClient(app=app, base_url="http://test") as client:
        response = await client.post("/graphql", json={
            "query": "query { users { id name } }"
        })

    assert response.status_code == 200
    data = response.json()
    assert 'data' in data
    assert 'users' in data['data']
\```

### 4. End-to-End Tests

Test complete user flows:

\```python
@pytest.mark.e2e
async def test_user_registration_flow():
    # 1. Register user
    register_response = await client.post("/graphql", json={
        "query": """
            mutation {
                createUser(input: {
                    email: "newuser@example.com",
                    name: "New User"
                }) {
                    ... on UserCreated {
                        user { id email }
                    }
                }
            }
        """
    })
    user_id = register_response.json()['data']['createUser']['user']['id']

    # 2. Verify email sent
    assert email_was_sent("newuser@example.com")

    # 3. Confirm email
    await client.post("/confirm-email", json={"token": email_token})

    # 4. Login
    login_response = await client.post("/login", json={
        "email": "newuser@example.com",
        "password": "password123"
    })
    assert login_response.status_code == 200
\```

### 5. Performance Tests

Test under load:

\```python
import pytest
import asyncio

@pytest.mark.benchmark
async def test_query_performance():
    async def run_query():
        async with AsyncClient(app=app, base_url="http://test") as client:
            return await client.post("/graphql", json={
                "query": "query { users { id name } }"
            })

    # Run 100 concurrent requests
    tasks = [run_query() for _ in range(100)]
    start = time.time()
    results = await asyncio.gather(*tasks)
    duration = time.time() - start

    # Assertions
    assert all(r.status_code == 200 for r in results)
    assert duration < 5.0  # Should complete in under 5 seconds
    assert duration / 100 < 0.05  # P50 latency under 50ms
\```

---

## Testing Patterns

### Pattern 1: Test Fixtures

\```python
# conftest.py
import pytest
from testcontainers.postgres import PostgresContainer

@pytest.fixture(scope="session")
def postgres_container():
    """PostgreSQL container for tests."""
    with PostgresContainer("postgres:16") as postgres:
        yield postgres

@pytest.fixture
async def db(postgres_container):
    """Database connection for each test."""
    db = await Database.create_pool(
        database_url=postgres_container.get_connection_url()
    )

    # Run migrations
    await run_migrations(db)

    yield db

    # Cleanup
    await db.close()
\```

### Pattern 2: Factory Functions

\```python
# factories.py
from faker import Faker

fake = Faker()

async def create_user(db, **kwargs):
    """Factory to create test user."""
    defaults = {
        'email': fake.email(),
        'name': fake.name()
    }
    defaults.update(kwargs)

    result = await db.call_function("fn_create_user", **defaults)
    return result['user']

# Usage
user1 = await create_user(db)
user2 = await create_user(db, email="specific@example.com")
\```

### Pattern 3: Snapshot Testing

\```python
def test_graphql_schema_snapshot(snapshot):
    """Ensure schema doesn't change unexpectedly."""
    schema = generate_schema(types=[User, Post], queries=[users, posts])

    # Compare with saved snapshot
    assert schema == snapshot
\```

---

## Test Organization

\```
tests/
├── unit/
│   ├── test_types.py
│   ├── test_queries.py
│   └── test_mutations.py
├── integration/
│   ├── test_database.py
│   ├── test_graphql.py
│   └── test_authentication.py
├── e2e/
│   ├── test_user_flow.py
│   └── test_checkout_flow.py
├── performance/
│   ├── test_load.py
│   └── test_benchmarks.py
├── conftest.py          # Shared fixtures
├── factories.py         # Test data factories
└── __snapshots__/       # Snapshot files
\```

---

## Running Tests

\```bash
# All tests
pytest

# Specific category
pytest tests/unit/
pytest tests/integration/
pytest tests/e2e/

# With coverage
pytest --cov=app --cov-report=html

# Parallel execution
pytest -n auto

# Watch mode (auto-rerun on changes)
pytest-watch

# Performance tests only
pytest -m benchmark
\```

---

## CI/CD Integration

\```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:16
        env:
          POSTGRES_PASSWORD: test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v3

      - name: Set up Python
        uses: actions/setup-python@v4
        with:
          python-version: '3.10'

      - name: Install dependencies
        run: |
          pip install -e ".[test]"

      - name: Run tests
        run: |
          pytest --cov=app --cov-report=xml

      - name: Upload coverage
        uses: codecov/codecov-action@v3
        with:
          file: ./coverage.xml
\```

---

## Best Practices

1. **Write tests first** (TDD)
2. **Keep tests fast** (use mocks for external services)
3. **One assertion per test** (easier to debug)
4. **Clear test names** (`test_create_user_with_invalid_email`)
5. **Use factories** (don't repeat test data setup)
6. **Clean up after tests** (use fixtures)
7. **Test edge cases** (null, empty, max values)
8. **Test error paths** (not just happy path)

---

## Coverage Goals

- **Unit tests**: 90%+ coverage
- **Integration tests**: Critical paths covered
- **E2E tests**: Major user flows covered

\```bash
# Check coverage
pytest --cov=app --cov-report=term-missing

# Generate HTML report
pytest --cov=app --cov-report=html
open htmlcov/index.html
\```
```

---

## Task 5: Development Tools & Utilities

**Priority**: Medium
**Time**: 2-3 days
**Complexity**: Low-Medium

### Tools to Build

#### 1. Database Inspector

**CLI tool to explore database:**
```bash
$ fraiseql db inspect

Tables: 12
Views: 8
Functions: 15

Tables:
  ├─ tb_user (1,234 rows)
  ├─ tb_post (5,678 rows)
  └─ tb_comment (12,345 rows)

Views:
  ├─ v_user (JSONB view)
  ├─ v_post (JSONB view)
  └─ tv_user_profile (table view)

Functions:
  ├─ fn_create_user(email TEXT, name TEXT) → JSONB
  ├─ fn_update_post(id INT, title TEXT) → JSONB
  └─ fn_delete_comment(id INT) → JSONB
```

#### 2. Migration Generator

**Auto-generate migrations from schema changes:**
```bash
$ fraiseql db diff

Changes detected:
  + Add column: tb_user.avatar_url (TEXT)
  + Add index: idx_post_published_at
  - Remove column: tb_user.legacy_field

Generate migration? [y/N]: y

Created: migrations/20251024_add_user_avatar.sql
```

#### 3. Schema Validator

**Validate consistency:**
```bash
$ fraiseql validate

✅ All @type decorators have corresponding views
✅ All views return valid JSONB
✅ Python types match PostgreSQL types
❌ View v_post missing index on user_id
⚠️  Function fn_old_mutation not used in Python code
```

#### 4. Performance Profiler

**Profile query performance:**
```bash
$ fraiseql profile --query="users"

Query: users
Total time: 45.2ms
Breakdown:
  Database:  38.1ms (84%)
  Rust:      5.3ms (12%)
  Python:    1.8ms (4%)

Database breakdown:
  Sequential scan tb_user: 35.2ms
  JSONB build:            2.9ms

Recommendations:
  💡 Add index on tb_user(created_at) for sorting
```

---

## Task 6: Logging & Observability

**Priority**: Low
**Time**: 2 days
**Complexity**: Low

### Structured Logging

```python
import structlog

logger = structlog.get_logger()

logger.info(
    "graphql_query_executed",
    query_name="users",
    duration_ms=45.2,
    rows_returned=123,
    user_id="user-123",
    tenant_id="tenant-456"
)
```

### OpenTelemetry Integration

```python
from opentelemetry import trace
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor

# Instrument FastAPI
FastAPIInstrumentor.instrument_app(app)

# Custom spans
@query
async def users() -> list[User]:
    with trace.get_tracer(__name__).start_as_current_span("fetch_users"):
        result = await db.fetch_all("SELECT data FROM v_user")
        return [User(**r['data']) for r in result]
```

---

## Phase 5 Summary

**Total Time**: 2-3 weeks
**Complexity**: Medium-High
**Impact**: Dramatically improve developer experience

**Deliverables:**
- VS Code extension
- Enhanced CLI (6 new commands)
- Improved error messages
- Comprehensive testing guide
- Development tools
- Better logging

---

**Next**: Phase 6 (Community & Ecosystem)
