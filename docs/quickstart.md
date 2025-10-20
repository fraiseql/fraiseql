# 5-Minute Quickstart

🟢 **Beginner** - Build a working GraphQL API from scratch. One command setup, then test queries.

**📍 Navigation**: [← Getting Started](../GETTING_STARTED.md) • [Beginner Path →](tutorials/beginner-path.md) • [Examples →](../examples/)

## Prerequisites

- Python 3.13+
- PostgreSQL 13+
- FraiseQL installed

**[📖 Installation Guide](../INSTALLATION.md)** - Complete installation instructions for different use cases

## Step 1: Create Project (30 seconds)

**Option A: Use CLI (Recommended)**
```bash
fraiseql init todo-api
cd todo-api
```

**Option B: Manual Setup**
```bash
# Copy the working example
cp examples/todo_quickstart.py .
# Run it directly
python todo_quickstart.py
```

This creates a complete project structure:
```
todo-api/
├── src/
│   ├── main.py          # Your GraphQL app
│   ├── types/           # Type definitions
│   ├── queries/         # Custom query logic
│   └── mutations/       # Mutation handlers
├── migrations/          # Database migrations
├── tests/               # Test files
├── .env                 # Configuration
├── pyproject.toml       # Dependencies
└── README.md           # Project documentation
```

## Step 2: Database Setup (1 minute)

```bash
# Create database
createdb todo_app

# Set up tables and views
psql -d todo_app << 'EOF'
CREATE TABLE tb_task (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    completed BOOLEAN DEFAULT false,
    created_at TIMESTAMP DEFAULT NOW()
);

INSERT INTO tb_task (title, description) VALUES
    ('Learn FraiseQL', 'Complete quickstart tutorial'),
    ('Build an API', 'Create first GraphQL API'),
    ('Deploy to production', 'Ship it!');

CREATE VIEW v_task AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'title', title,
        'description', description,
        'completed', completed,
        'created_at', created_at
    ) AS data
FROM tb_task;

SELECT data FROM v_task LIMIT 1;
EOF
```

Update your `.env` file:
```bash
# Edit .env to point to your database
echo "FRAISEQL_DATABASE_URL=postgresql://localhost/todo_app" >> .env
```

### What You Just Created

You just set up **CQRS architecture**:
- **`tb_task`** (Command table): Where data is written
- **`v_task`** (Query view): Pre-packaged JSONB for fast reads

This demonstrates **database-first design** - the database structure comes first, then the API is built on top.

**Learn more**: [Core Concepts](../core/concepts-glossary.md)

## Step 3: Create API (2 minutes)

Replace `src/main.py` with our Task API:

```python
"""Todo API application."""

import os
from datetime import datetime
from typing import List

import fraiseql
from fraiseql import fraise_field
from fraiseql.types.scalars import UUID


@fraiseql.type
class Task:
    """A task in the todo system."""
    id: UUID = fraise_field(description="Task ID")
    title: str = fraise_field(description="Task title")
    description: str | None = fraise_field(description="Task description")
    completed: bool = fraise_field(description="Whether task is completed")
    created_at: datetime = fraise_field(description="When task was created")


@fraiseql.type
class QueryRoot:
    """Root query type."""
    tasks: List[Task] = fraise_field(description="List all tasks")
    task: Task | None = fraise_field(description="Get single task by ID")

    async def resolve_tasks(self, info, completed: bool | None = None):
        repo = info.context["repo"]
        where = {}
        if completed is not None:
            where["completed"] = completed
        results = await repo.find("v_task", where=where)
        return [Task(**result) for result in results]

    async def resolve_task(self, info, id: UUID):
        repo = info.context["repo"]
        result = await repo.find_one("v_task", where={"id": id})
        return Task(**result) if result else None


# Create the FastAPI app
app = fraiseql.create_fraiseql_app(
    queries=[QueryRoot],
    database_url=os.getenv("FRAISEQL_DATABASE_URL"),
)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
```

## Step 4: Test Queries (30 seconds)

Create a test script to verify your API:

```python
# Save as test_queries.py
import asyncio
import os
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

async def test_queries():
    from fraiseql.repository import FraiseQLRepository

    async with FraiseQLRepository(
        database_url=os.getenv("FRAISEQL_DATABASE_URL")
    ) as repo:
        # Test direct database queries
        results = await repo.find("v_task")
        print(f"Found {len(results)} tasks")
        for result in results:
            print(f"  - {result['title']} (completed: {result['completed']})")

if __name__ == "__main__":
    asyncio.run(test_queries())
```

Run:
```bash
python test_queries.py
# Output:
# Found 3 tasks
#   - Learn FraiseQL (completed: False)
#   - Build an API (completed: False)
#   - Deploy to production (completed: False)
```

## Step 5: Launch GraphQL Server (30 seconds)

Install dependencies and start the server:

```bash
# Install dependencies
pip install -e .

# Start the development server
python -m src.main
```

Or use the FraiseQL CLI:
```bash
fraiseql dev
```

Open http://localhost:8000/graphql

## Step 6: Test in Playground (1 minute)

### Query All Tasks
```graphql
query GetAllTasks {
  tasks {
    id
    title
    description
    completed
    createdAt
  }
}
```

### Query Incomplete Tasks
```graphql
query GetIncompleteTasks {
  tasks(completed: false) {
    id
    title
    completed
  }
}
```

### Query Single Task
```graphql
query GetTask($id: ID!) {
  task(id: $id) {
    id
    title
    description
    completed
    createdAt
  }
}
```

## Optional: Add Mutations (2 minutes)

First, add PostgreSQL functions:

```sql
psql -d todo_app << 'EOF'
CREATE OR REPLACE FUNCTION fn_create_task(
    p_title TEXT,
    p_description TEXT DEFAULT NULL
) RETURNS UUID AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO tb_task (title, description)
    VALUES (p_title, p_description)
    RETURNING id INTO v_id;
    RETURN v_id;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION fn_complete_task(p_id UUID)
RETURNS BOOLEAN AS $$
BEGIN
    UPDATE tb_task
    SET completed = true
    WHERE id = p_id;
    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
EOF
```

Add mutations to `src/main.py`:

```python
@fraiseql.input
class CreateTaskInput:
    """Input for creating a new task."""
    title: str = fraise_field(description="Task title")
    description: str | None = fraise_field(description="Task description")


@fraiseql.type
class MutationRoot:
    """Root mutation type."""
    create_task: Task = fraise_field(description="Create a new task")
    complete_task: Task | None = fraise_field(description="Mark task as completed")

    async def resolve_create_task(self, info, input: CreateTaskInput):
        repo = info.context["repo"]
        task_id = await repo.call_function(
            "fn_create_task",
            p_title=input.title,
            p_description=input.description
        )
        result = await repo.find_one("v_task", where={"id": task_id})
        return Task(**result)

    async def resolve_complete_task(self, info, id: UUID):
        repo = info.context["repo"]
        success = await repo.call_function("fn_complete_task", p_id=id)
        if success:
            result = await repo.find_one("v_task", where={"id": id})
            return Task(**result) if result else None
        return None


# Create the FastAPI app
app = fraiseql.create_fraiseql_app(
    queries=[QueryRoot],
    mutations=[MutationRoot],
    database_url=os.getenv("FRAISEQL_DATABASE_URL"),
)
```

Test mutations:

```graphql
mutation CreateNewTask {
  createTask(input: {
    title: "Finish quickstart"
    description: "Complete FraiseQL tutorial"
  }) {
    id
    title
    completed
  }
}

mutation MarkComplete($id: ID!) {
  completeTask(id: $id) {
    id
    title
    completed
  }
}
```

## Success

In 5 minutes you have:
- Complete project structure with proper organization
- PostgreSQL database with CQRS tables and views
- GraphQL API with queries and mutations
- Interactive playground for testing
- Ready for development and deployment

## Project Structure Explained

Your `todo-api/` project follows FraiseQL best practices:

```
src/
├── main.py          # GraphQL schema and resolvers
├── types/           # Reusable type definitions
├── queries/         # Complex query logic
└── mutations/       # Business logic for mutations

migrations/          # Database schema changes
tests/               # Test files
.env                 # Configuration (database URL, secrets)
```

## View Pattern Explanation

FraiseQL views include ID as separate column alongside JSONB data:

```sql
CREATE VIEW v_task AS
SELECT
    id,              -- Separate column for filtering (indexed)
    completed,       -- Optional: additional filter columns
    jsonb_build_object(...) AS data  -- Full object as JSONB
FROM tb_task;
```

**Benefits**:
- Efficient filtering: PostgreSQL uses index on id column
- Better query plans: Optimizer works with regular columns
- Flexibility: Add indexed columns for common filters

## Troubleshooting

**Database connection errors**:
```bash
# Check your .env file
cat .env
# Update if needed
echo "FRAISEQL_DATABASE_URL=postgresql://localhost/todo_app" > .env
```

**Module not found**:
```bash
pip install -e .
```

**PostgreSQL not found**:
- Mac: `brew install postgresql`
- Ubuntu: `sudo apt install postgresql`
- Windows: Download from postgresql.org

## Next Steps After Quickstart

### Evolve Your Project

**From Quickstart → Production**:
1. **Add proper migrations** - Move database setup to `migrations/`
2. **Split types** - Move Task to `src/types/task.py`
3. **Add tests** - Create `tests/test_task.py`
4. **Add authentication** - See [Native Auth Example](../../examples/native-auth-app/)
5. **Add caching** - See [APQ Multi-tenant Example](../../examples/apq_multi_tenant/)

### Learning Paths

**Beginner** (Recommended):
- [Beginner Learning Path](./tutorials/beginner-path.md) - Complete 2-3 hour journey

**Specific Topics**:
- [Blog API Tutorial](./tutorials/blog-api.md) - Complete CRUD application
- [Database Patterns](../../docs/advanced/database-patterns.md) - CQRS, views, N+1 prevention
- [Performance Guide](../../docs/performance/index.md) - Optimization techniques

### Project Templates

For larger applications, consider these templates:
- **Blog**: User posts, comments, authentication
- **E-commerce**: Products, orders, payments
- **Enterprise**: Multi-tenant, advanced patterns

```bash
# Try different templates
fraiseql init blog-api --template blog
fraiseql init shop --template ecommerce
```

## Key Concepts

**Project Structure**:
- `src/` - Application code (not root-level files)
- `migrations/` - Database schema evolution
- `.env` - Configuration (never commit)

**View Naming**:
- `v_` - Regular views (computed on query)
- `tv_` - Table views (materialized for performance)
- `fn_` - PostgreSQL functions for mutations

**Type System**:
- `@fraiseql.type` - GraphQL object types
- `@fraiseql.input` - Input types for mutations
- `frause_field()` - Field definitions with descriptions

**Repository Pattern**:
- `repo.find()` - Query views with filtering
- `repo.find_one()` - Single record by ID
- `repo.call_function()` - Execute PostgreSQL functions
