# 5-Minute Quickstart

Build your first GraphQL API with FraiseQL in just 5 minutes! We'll create a simple task management API to demonstrate the core concepts.

## What We'll Build

A GraphQL API that can:
- Query tasks from a PostgreSQL view
- Create new tasks using a PostgreSQL function
- Mark tasks as complete

## Step 1: Database Setup

First, let's create our database and table:

```bash
# Create database
createdb todo_app

# Connect to it
psql -d todo_app
```

Create the tasks table:

```sql
-- Create tasks table
CREATE TABLE tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    description TEXT,
    completed BOOLEAN DEFAULT false,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Insert sample data
INSERT INTO tasks (title, description) VALUES 
    ('Learn FraiseQL', 'Complete the quickstart tutorial'),
    ('Build an API', 'Create my first GraphQL API'),
    ('Deploy to production', 'Ship it!');
```

## Step 2: Create a PostgreSQL View

FraiseQL reads from views that return JSONB data. Create a view for tasks:

```sql
-- Create a view with id column for filtering and data column with JSONB
CREATE VIEW v_task AS
SELECT 
    id,  -- Keep id as separate column for efficient filtering
    jsonb_build_object(
        'id', id,
        'title', title,
        'description', description,
        'completed', completed,
        'created_at', created_at
    ) AS data
FROM tasks
ORDER BY created_at DESC;
```

Test the view:
```sql
SELECT * FROM v_task LIMIT 1;
-- Returns: id column + data column with JSONB

-- You can filter efficiently:
SELECT data FROM v_task WHERE id = 'some-uuid';
```

## Step 3: Create Your FraiseQL App

Create a new file `app.py`:

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

# Define your GraphQL type
@fraiseql.type
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
    results = await repo.find("v_tasks", where=where)
    return [Task(**result) for result in results]

@app.query
async def task(info, id: ID) -> Task | None:
    """Get a single task by ID"""
    repo = info.context["repo"]
    # This efficiently uses WHERE id = ? on the view
    result = await repo.find_one("v_tasks", where={"id": id})
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

## Step 4: Run Your API

### Option A: Test Directly

```bash
python app.py
```

You should see:
```
Found 3 tasks:
  - Learn FraiseQL (completed: False)
  - Build an API (completed: False)
  - Deploy to production (completed: False)

3 incomplete tasks
```

### Option B: Run with FastAPI

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

## Step 5: Test with GraphQL Playground

Open http://localhost:8000/graphql in your browser and try these queries:

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

## Step 6: Add Mutations (Bonus)

Let's add the ability to create and update tasks. First, create PostgreSQL functions:

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
@fraiseql.input
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
    result = await repo.find_one("v_tasks", where={"id": task_id})
    return Task(**result)

@app.mutation
async def complete_task(info, id: ID) -> Task | None:
    """Mark a task as complete"""
    repo = info.context["repo"]
    
    # Call PostgreSQL function
    success = await repo.call_function("fn_complete_task", p_id=id)
    
    if success:
        # Fetch the updated task
        result = await repo.find_one("v_tasks", where={"id": id})
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
    FROM tasks;
    ```

## 🎉 Congratulations!

You've just built your first GraphQL API with FraiseQL! You've learned how to:

- ✅ Create PostgreSQL views with separate columns for filtering
- ✅ Define GraphQL types with Python dataclasses
- ✅ Write queries that efficiently filter using view columns
- ✅ Add mutations using PostgreSQL functions
- ✅ Test your API with GraphQL Playground

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
- **[Authentication](../guides/authentication.md)** - Add user authentication
- **[Deployment](../deployment/index.md)** - Deploy to production

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