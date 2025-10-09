# 5-Minute Quickstart

Build a working GraphQL API from scratch. Copy-paste examples, minimal explanation.

## Prerequisites

```bash
python --version  # 3.11+
psql --version    # PostgreSQL client
pip install fraiseql fastapi uvicorn
```

## Step 1: Database Setup (1 minute)

```bash
createdb todo_app && psql -d todo_app << 'EOF'
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

## Step 2: Create API (2 minutes)

Save as `app.py`:

```python
from dataclasses import dataclass
from datetime import datetime
import fraiseql
from fraiseql import ID, FraiseQL
import os

app = FraiseQL(
    database_url=os.getenv("DATABASE_URL", "postgresql://localhost/todo_app")
)

@fraiseql.type
class Task:
    id: ID
    title: str
    description: str | None
    completed: bool
    created_at: datetime

@app.query
async def tasks(info, completed: bool | None = None) -> list[Task]:
    repo = info.context["repo"]
    where = {}
    if completed is not None:
        where["completed"] = completed
    results = await repo.find("v_task", where=where)
    return [Task(**result) for result in results]

@app.query
async def task(info, id: ID) -> Task | None:
    repo = info.context["repo"]
    result = await repo.find_one("v_task", where={"id": id})
    return Task(**result) if result else None
```

## Step 3: Test Queries (30 seconds)

```python
# Add to app.py
import asyncio

async def test_queries():
    from fraiseql.repository import FraiseQLRepository

    async with FraiseQLRepository(
        database_url=os.getenv("DATABASE_URL", "postgresql://localhost/todo_app")
    ) as repo:
        class Info:
            context = {"repo": repo}

        info = Info()
        all_tasks = await tasks(info)
        print(f"Found {len(all_tasks)} tasks")
        for task in all_tasks:
            print(f"  - {task.title} (completed: {task.completed})")

if __name__ == "__main__":
    asyncio.run(test_queries())
```

Run:
```bash
python app.py
# Output:
# Found 3 tasks
#   - Learn FraiseQL (completed: False)
#   - Build an API (completed: False)
#   - Deploy to production (completed: False)
```

## Step 4: Launch GraphQL Server (30 seconds)

Create `server.py`:

```python
from fastapi import FastAPI
from fraiseql.fastapi import GraphQLRouter
from app import app as fraiseql_app

api = FastAPI(title="Todo API")

api.include_router(
    GraphQLRouter(
        fraiseql_app,
        path="/graphql",
        enable_playground=True
    )
)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(api, host="0.0.0.0", port=8000)
```

Run:
```bash
python server.py
```

Open http://localhost:8000/graphql

## Step 5: Test in Playground (1 minute)

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

PostgreSQL functions:

```sql
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
```

Add to `app.py`:

```python
@fraiseql.input
class CreateTaskInput:
    title: str
    description: str | None = None

@app.mutation
async def create_task(info, input: CreateTaskInput) -> Task:
    repo = info.context["repo"]
    task_id = await repo.call_function(
        "fn_create_task",
        p_title=input.title,
        p_description=input.description
    )
    result = await repo.find_one("v_task", where={"id": task_id})
    return Task(**result)

@app.mutation
async def complete_task(info, id: ID) -> Task | None:
    repo = info.context["repo"]
    success = await repo.call_function("fn_complete_task", p_id=id)
    if success:
        result = await repo.find_one("v_task", where={"id": id})
        return Task(**result) if result else None
    return None
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
- PostgreSQL database with table and view
- GraphQL API with queries and mutations
- Interactive playground for testing

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
export DATABASE_URL="postgresql://username:password@localhost/todo_app"
```

**Module not found**:
```bash
pip install fraiseql
# Or: python3 -m pip install fraiseql
```

**PostgreSQL not found**:
- Mac: `brew install postgresql`
- Ubuntu: `sudo apt install postgresql`
- Windows: Download from postgresql.org

## Next Steps

- [Database API](./core/database-api.md) - Repository patterns and QueryOptions
- [Performance](./performance/index.md) - Rust transformation, APQ caching
- [Database Patterns](./advanced/database-patterns.md) - View design, N+1 prevention

## Key Concepts

**View Naming**:
- `v_` - Regular views (computed on query)
- `tv_` - Table views (materialized for performance)
- `fn_` - PostgreSQL functions for mutations

**Type Hints**:
- Required: Define your GraphQL schema
- `| None` - Optional fields
- `list[Type]` - Arrays

**Repository Pattern**:
- `repo.find()` - Query views
- `repo.find_one()` - Single record
- `repo.call_function()` - Execute PostgreSQL functions
