---
← [Home](../index.md) | [Getting Started](index.md) | [Next: GraphQL Playground](graphql-playground.md) →
---

# 5-Minute Quickstart

> **In this section:** Build a working GraphQL API in 5 minutes with copy-paste examples
> **Prerequisites:** Python 3.10+, PostgreSQL installed
> **Time to complete:** 5 minutes

Get a working GraphQL API in 5 minutes! No complex setup, just copy-paste and run.

## Prerequisites ✅

```bash
# Check you have these installed:
python --version  # 3.10 or higher
psql --version    # PostgreSQL client
pip --version     # Python package manager

# Install FraiseQL (30 seconds):
pip install fraiseql fastapi uvicorn
```

## Step 1: Quick Database Setup (1 minute)

Copy and paste this entire block into your terminal:

```bash
# Create database and add sample data in one go
createdb todo_app && psql -d todo_app << 'EOF'
-- Create table with sample data
CREATE TABLE tb_task (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    completed BOOLEAN DEFAULT false,
    created_at TIMESTAMP DEFAULT NOW()
);

INSERT INTO tb_task (title, description) VALUES
    ('Learn FraiseQL', 'Complete the quickstart tutorial'),
    ('Build an API', 'Create my first GraphQL API'),
    ('Deploy to production', 'Ship it!');

-- Create the view that FraiseQL will read
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

-- Verify it works
SELECT data FROM v_task LIMIT 1;
EOF
```

## Step 2: Create Your API (2 minutes)

Save this as `app.py`:

```python
from dataclasses import dataclass
from datetime import datetime
import fraiseql
from fraiseql import ID, FraiseQL
import asyncio
import os

# Initialize FraiseQL with your database
app = FraiseQL(
    database_url=os.getenv("DATABASE_URL", "postgresql://localhost/todo_app")
)

# Define your GraphQL type (both syntaxes work)
@fraiseql.type  # or @fraiseql.fraise_type
class Task:
    id: ID
    title: str
    description: str | None
    completed: bool
    created_at: datetime

# Define a query to fetch tasks
@app.query
async def tasks(info, completed: bool | None = None) -> list[Task]:
    """Get all tasks, optionally filtered by completion status"""
    repo = info.context["repo"]

    # Build WHERE clause if filter provided
    where = {}
    if completed is not None:
        where["completed"] = completed

    # Fetch from our view - FraiseQL uses the separate columns for filtering
    results = await repo.find("v_task", where=where)
    return [Task(**result) for result in results]

@app.query
async def task(info, id: ID) -> Task | None:
    """Get a single task by ID"""
    repo = info.context["repo"]
    # This efficiently uses WHERE id = ? on the view
    result = await repo.find_one("v_task", where={"id": id})
    return Task(**result) if result else None

# For testing without a web server
async def test_queries():
    """Test our queries directly"""
    from fraiseql.repository import FraiseQLRepository

    async with FraiseQLRepository(
        database_url=os.getenv("DATABASE_URL", "postgresql://localhost/todo_app")
    ) as repo:
        # Create a mock info object
        class Info:
            context = {"repo": repo}

        info = Info()

        # Test fetching all tasks
        all_tasks = await tasks(info)
        print(f"Found {len(all_tasks)} tasks:")
        for task in all_tasks:
            print(f"  - {task.title} (completed: {task.completed})")

        # Test fetching incomplete tasks
        incomplete = await tasks(info, completed=False)
        print(f"\n{len(incomplete)} incomplete tasks")

if __name__ == "__main__":
    # Run test queries
    asyncio.run(test_queries())
```

## Step 3: Run Your API (1 minute)

### Quick Test First:

```bash
python app.py
```

Expected output:
```
Found 3 tasks:
  - Learn FraiseQL (completed: False)
  - Build an API (completed: False)
  - Deploy to production (completed: False)

3 incomplete tasks
```

## Step 4: Launch GraphQL Server (30 seconds)

Create `server.py`:

```python
from fastapi import FastAPI
from fraiseql.fastapi import GraphQLRouter
from app import app as fraiseql_app

# Create FastAPI app
api = FastAPI(title="Todo API")

# Add GraphQL endpoint
api.include_router(
    GraphQLRouter(
        fraiseql_app,
        path="/graphql",
        enable_playground=True  # Enable GraphQL playground
    )
)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(api, host="0.0.0.0", port=8000)
```

Run the server:
```bash
pip install fastapi uvicorn
python server.py
```

## Step 5: Test with GraphQL Playground (30 seconds)

Open http://localhost:8000/graphql in your browser and try this query:

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

## Next: Add Mutations (Optional - 2 minutes)

Want to create and update tasks? Add these PostgreSQL functions:

```sql
-- Function to create a task
CREATE OR REPLACE FUNCTION fn_create_task(
    p_title TEXT,
    p_description TEXT DEFAULT NULL
) RETURNS UUID AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO tasks (title, description)
    VALUES (p_title, p_description)
    RETURNING id INTO v_id;

    RETURN v_id;
END;
$$ LANGUAGE plpgsql;

-- Function to mark task complete
CREATE OR REPLACE FUNCTION fn_complete_task(p_id UUID)
RETURNS BOOLEAN AS $$
BEGIN
    UPDATE tasks
    SET completed = true
    WHERE id = p_id;

    RETURN FOUND;  -- Returns true if row was updated
END;
$$ LANGUAGE plpgsql;
```

Add mutations to your `app.py`:

```python
# Add these imports at the top
@fraiseql.input  # or @fraiseql.fraise_input
class CreateTaskInput:
    title: str
    description: str | None = None

# Add these mutations after your queries
@app.mutation
async def create_task(info, input: CreateTaskInput) -> Task:
    """Create a new task"""
    repo = info.context["repo"]

    # Call PostgreSQL function
    task_id = await repo.call_function(
        "fn_create_task",
        p_title=input.title,
        p_description=input.description
    )

    # Fetch the created task
    result = await repo.find_one("v_task", where={"id": task_id})
    return Task(**result)

@app.mutation
async def complete_task(info, id: ID) -> Task | None:
    """Mark a task as complete"""
    repo = info.context["repo"]

    # Call PostgreSQL function
    success = await repo.call_function("fn_complete_task", p_id=id)

    if success:
        # Fetch the updated task
        result = await repo.find_one("v_task", where={"id": id})
        return Task(**result) if result else None
    return None
```

Test the mutations in GraphQL Playground:

```graphql
mutation CreateNewTask {
  createTask(input: {
    title: "Finish quickstart"
    description: "Complete the FraiseQL tutorial"
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

## Understanding the View Pattern

!!! info "Why separate ID column?"
    FraiseQL views typically include the ID as a separate column alongside the JSONB data:

    - **Efficient filtering**: PostgreSQL can use indexes on the `id` column
    - **Better query plans**: The optimizer can work with regular columns
    - **Flexibility**: Can add other indexed columns for common filters

    Example with multiple filter columns:
    ```sql
    CREATE VIEW v_task AS
    SELECT
        id,
        completed,  -- Another column for filtering
        user_id,    -- And another
        jsonb_build_object(...) AS data
    FROM tb_task;
    ```

## 🎉 Success! You Have a Working API!

In just 5 minutes, you've:
- ✅ Set up a PostgreSQL database with the `tb_task` table
- ✅ Created a `v_task` view for FraiseQL to read
- ✅ Built a complete GraphQL API with queries
- ✅ Tested it in GraphQL Playground

## Troubleshooting Common Issues

<details>
<summary>🔧 "psql: command not found"</summary>

Install PostgreSQL:
- Mac: `brew install postgresql`
- Ubuntu/Debian: `sudo apt install postgresql`
- Windows: Download from postgresql.org
</details>

<details>
<summary>🔧 "createdb: command not found"</summary>

PostgreSQL tools aren't in your PATH. Find them:
```bash
# Mac/Linux
find / -name createdb 2>/dev/null
# Add the directory to PATH
export PATH="/usr/local/pgsql/bin:$PATH"
```
</details>

<details>
<summary>🔧 "ModuleNotFoundError: No module named 'fraiseql'"</summary>

```bash
pip install fraiseql
# Or if you have multiple Python versions:
python3 -m pip install fraiseql
```
</details>

<details>
<summary>🔧 Database connection errors</summary>

Check your connection string:
```bash
# Default assumes local PostgreSQL with your username
export DATABASE_URL="postgresql://username:password@localhost/todo_app"
# Or modify in app.py directly
```
</details>

## What's Next?

### Immediate Next Steps

1. **[GraphQL Playground Guide](graphql-playground.md)** - Learn advanced playground features
2. **[Build Your First Real API](first-api.md)** - Create a more complex API
3. **[Core Concepts](../core-concepts/index.md)** - Understand FraiseQL's architecture

### Key Concepts to Explore

- **[Database Views](../core-concepts/database-views.md)** - Learn view patterns and optimization
- **[Type System](../core-concepts/type-system.md)** - Advanced typing features
- **[CQRS Pattern](../core-concepts/architecture.md)** - Understand the architecture

### Build Something Real

- **[Blog API Tutorial](../tutorials/blog-api.md)** - Complete production-ready example
- **[Authentication](../advanced/authentication.md)** - Add user authentication
- **[Docker Deployment](../advanced/docker.md)** - Deploy with Docker

## Tips for Success

!!! tip "Best Practices"
    1. **Include filter columns in views** - Keep commonly filtered fields as separate columns
    2. **Use functions for mutations** - Keep business logic in the database
    3. **Return JSONB in data column** - FraiseQL expects a `data` column with JSONB
    4. **Use type hints** - They're not optional, they define your schema
    5. **Test in playground first** - Before writing client code

!!! warning "Common Pitfalls"
    - Forgetting to include ID as a separate column (impacts performance)
    - Missing type hints on function parameters
    - Not handling NULL values (use `| None` syntax)
    - Forgetting to handle empty arrays in JSONB aggregations

## See Also

### Related Concepts
- [**Core Concepts**](../core-concepts/index.md) - Understand FraiseQL's philosophy
- [**Type System**](../core-concepts/type-system.md) - Deep dive into GraphQL types
- [**Database Views**](../core-concepts/database-views.md) - View patterns and optimization
- [**Query Translation**](../core-concepts/query-translation.md) - How queries become SQL

### Next Steps
- [**GraphQL Playground**](graphql-playground.md) - Master the interactive testing tool
- [**Your First API**](first-api.md) - Build a more complex application
- [**Blog Tutorial**](../tutorials/blog-api.md) - Complete production example

### Reference
- [**API Documentation**](../api-reference/index.md) - Complete API reference
- [**Decorators Reference**](../api-reference/decorators.md) - All available decorators
- [**Error Codes**](../errors/error-types.md) - Troubleshooting guide

### Advanced Topics
- [**Mutations Guide**](../mutations/index.md) - Advanced mutation patterns
- [**Performance Tuning**](../advanced/performance.md) - Optimization techniques
- [**Authentication**](../advanced/authentication.md) - Add user authentication
