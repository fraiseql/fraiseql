# Complete FraiseQL Example: Task Management API

This example demonstrates all major FraiseQL features in a real-world task management application.

## Project Structure

```
task-api/
├── app.py              # Main application
├── models.py           # Data models
├── queries.py          # Query resolvers
├── mutations.py        # Mutation definitions
├── auth.py            # Authentication setup
├── database.py        # Database repository
├── schema.sql         # PostgreSQL schema
└── .env              # Environment variables
```

## 1. Environment Setup (.env)

```bash
# Database
FRAISEQL_DATABASE_URL=postgresql://user:password@localhost:5432/taskdb

# Application
FRAISEQL_APP_NAME=Task Management API
FRAISEQL_APP_VERSION=1.0.0
FRAISEQL_ENVIRONMENT=development

# Development Auth
FRAISEQL_DEV_AUTH_USERNAME=admin
FRAISEQL_DEV_AUTH_PASSWORD=secret123

# Features
FRAISEQL_AUTO_CAMEL_CASE=true
FRAISEQL_ENABLE_PLAYGROUND=true
```

## 2. Database Schema (schema.sql)

```sql
-- Users table
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'user',
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Projects table
CREATE TABLE projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    owner_id UUID NOT NULL REFERENCES users(id),
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Tasks table
CREATE TABLE tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title VARCHAR(255) NOT NULL,
    description TEXT,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    priority VARCHAR(50) NOT NULL DEFAULT 'medium',
    project_id UUID NOT NULL REFERENCES projects(id),
    assignee_id UUID REFERENCES users(id),
    due_date DATE,
    tags TEXT[] DEFAULT '{}',
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes
CREATE INDEX idx_tasks_project_id ON tasks(project_id);
CREATE INDEX idx_tasks_assignee_id ON tasks(assignee_id);
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_projects_owner_id ON projects(owner_id);

-- Create update trigger
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_tasks_updated_at
    BEFORE UPDATE ON tasks
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();
```

## 3. Data Models (models.py)

```python
from datetime import date, datetime
from typing import Any, Optional
from uuid import UUID
from enum import Enum

from fraiseql import fraise_type, fraise_input, fraise_enum
from fraiseql.types import JSON


# Enums
@fraise_enum
class UserRole(Enum):
    ADMIN = "admin"
    USER = "user"
    GUEST = "guest"


@fraise_enum
class TaskStatus(Enum):
    PENDING = "pending"
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"
    CANCELLED = "cancelled"


@fraise_enum
class TaskPriority(Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    URGENT = "urgent"


# Output Types
@fraise_type
class User:
    id: UUID
    email: str
    name: str
    role: UserRole
    settings: JSON
    created_at: datetime

    # Computed fields can be added as methods
    def display_name(self) -> str:
        return f"{self.name} ({self.email})"


@fraise_type
class Project:
    id: UUID
    name: str
    description: Optional[str]
    owner_id: UUID
    metadata: dict[str, Any]  # JSON field
    created_at: datetime

    # Related data (resolved separately)
    owner: Optional[User] = None
    task_count: int = 0


@fraise_type
class Task:
    id: UUID
    title: str
    description: Optional[str]
    status: TaskStatus
    priority: TaskPriority
    project_id: UUID
    assignee_id: Optional[UUID]
    due_date: Optional[date]
    tags: list[str]
    metadata: JSON
    created_at: datetime
    updated_at: datetime

    # Related data
    project: Optional[Project] = None
    assignee: Optional[User] = None

    def is_overdue(self) -> bool:
        if not self.due_date:
            return False
        return date.today() > self.due_date


# Input Types
@fraise_input
class CreateUserInput:
    email: str
    name: str
    role: UserRole = UserRole.USER
    settings: Optional[JSON] = None


@fraise_input
class CreateProjectInput:
    name: str
    description: Optional[str] = None
    metadata: Optional[dict[str, Any]] = None


@fraise_input
class CreateTaskInput:
    title: str
    description: Optional[str] = None
    project_id: UUID
    assignee_id: Optional[UUID] = None
    priority: TaskPriority = TaskPriority.MEDIUM
    due_date: Optional[date] = None
    tags: list[str] = []
    metadata: Optional[JSON] = None


@fraise_input
class UpdateTaskInput:
    title: Optional[str] = None
    description: Optional[str] = None
    status: Optional[TaskStatus] = None
    priority: Optional[TaskPriority] = None
    assignee_id: Optional[UUID] = None
    due_date: Optional[date] = None
    tags: Optional[list[str]] = None
    metadata: Optional[JSON] = None


@fraise_input
class TaskFilters:
    status: Optional[TaskStatus] = None
    priority: Optional[TaskPriority] = None
    assignee_id: Optional[UUID] = None
    project_id: Optional[UUID] = None
    overdue: Optional[bool] = None
```

## 4. Database Repository (database.py)

```python
from typing import Optional, Any
from uuid import UUID
from datetime import date

from fraiseql.cqrs import CQRSRepository


class TaskRepository(CQRSRepository):
    """Repository for task management operations."""

    async def get_user_by_id(self, user_id: UUID) -> Optional[dict]:
        return await self.fetch_one(
            "SELECT * FROM users WHERE id = $1",
            user_id
        )

    async def get_user_by_email(self, email: str) -> Optional[dict]:
        return await self.fetch_one(
            "SELECT * FROM users WHERE email = $1",
            email
        )

    async def create_user(self, data: dict) -> dict:
        return await self.fetch_one(
            """
            INSERT INTO users (email, name, role, settings)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            """,
            data["email"],
            data["name"],
            data.get("role", "user"),
            data.get("settings", {})
        )

    async def get_projects_for_user(self, user_id: UUID) -> list[dict]:
        return await self.fetch_all(
            """
            SELECT p.*, COUNT(t.id) as task_count
            FROM projects p
            LEFT JOIN tasks t ON t.project_id = p.id
            WHERE p.owner_id = $1
            GROUP BY p.id
            ORDER BY p.created_at DESC
            """,
            user_id
        )

    async def create_project(self, owner_id: UUID, data: dict) -> dict:
        return await self.fetch_one(
            """
            INSERT INTO projects (name, description, owner_id, metadata)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            """,
            data["name"],
            data.get("description"),
            owner_id,
            data.get("metadata", {})
        )

    async def get_tasks(
        self,
        filters: dict[str, Any],
        limit: int = 20,
        offset: int = 0
    ) -> list[dict]:
        query = "SELECT * FROM tasks WHERE 1=1"
        params = []
        param_count = 0

        # Build dynamic filters
        if filters.get("status"):
            param_count += 1
            query += f" AND status = ${param_count}"
            params.append(filters["status"])

        if filters.get("priority"):
            param_count += 1
            query += f" AND priority = ${param_count}"
            params.append(filters["priority"])

        if filters.get("assignee_id"):
            param_count += 1
            query += f" AND assignee_id = ${param_count}"
            params.append(filters["assignee_id"])

        if filters.get("project_id"):
            param_count += 1
            query += f" AND project_id = ${param_count}"
            params.append(filters["project_id"])

        if filters.get("overdue"):
            query += " AND due_date < CURRENT_DATE"

        # Add ordering and pagination
        query += " ORDER BY created_at DESC"
        param_count += 1
        query += f" LIMIT ${param_count}"
        params.append(limit)
        param_count += 1
        query += f" OFFSET ${param_count}"
        params.append(offset)

        return await self.fetch_all(query, *params)

    async def create_task(self, data: dict) -> dict:
        return await self.fetch_one(
            """
            INSERT INTO tasks (
                title, description, project_id, assignee_id,
                priority, due_date, tags, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            """,
            data["title"],
            data.get("description"),
            data["project_id"],
            data.get("assignee_id"),
            data.get("priority", "medium"),
            data.get("due_date"),
            data.get("tags", []),
            data.get("metadata", {})
        )

    async def update_task(self, task_id: UUID, updates: dict) -> Optional[dict]:
        # Build dynamic update query
        set_parts = []
        params = []
        param_count = 0

        for field, value in updates.items():
            if value is not None:
                param_count += 1
                set_parts.append(f"{field} = ${param_count}")
                params.append(value)

        if not set_parts:
            return None

        param_count += 1
        params.append(task_id)

        query = f"""
            UPDATE tasks
            SET {', '.join(set_parts)}
            WHERE id = ${param_count}
            RETURNING *
        """

        return await self.fetch_one(query, *params)
```

## 5. Query Resolvers (queries.py)

```python
from typing import Optional, Any
from uuid import UUID
from datetime import date

from fraiseql import query, requires_auth
from models import User, Project, Task, TaskFilters
from database import TaskRepository


# Public queries
@query
async def health_check(info) -> dict[str, Any]:
    """Health check endpoint."""
    return {
        "status": "healthy",
        "timestamp": date.today().isoformat()
    }


# User queries
@query
async def get_user(info, id: UUID) -> Optional[User]:
    """Get a user by ID."""
    db: TaskRepository = info.context["db"]
    user_data = await db.get_user_by_id(id)
    return User(**user_data) if user_data else None


@query
@requires_auth
async def me(info) -> Optional[User]:
    """Get the current authenticated user."""
    db: TaskRepository = info.context["db"]
    user_context = info.context["user"]

    user_data = await db.get_user_by_email(user_context.email)
    return User(**user_data) if user_data else None


# Project queries
@query
@requires_auth
async def my_projects(info) -> list[Project]:
    """Get all projects owned by the current user."""
    db: TaskRepository = info.context["db"]
    user = await me(info)

    if not user:
        return []

    projects_data = await db.get_projects_for_user(user.id)
    return [Project(**data) for data in projects_data]


@query
async def project(info, id: UUID) -> Optional[Project]:
    """Get a project by ID."""
    db: TaskRepository = info.context["db"]
    project_data = await db.fetch_one(
        "SELECT * FROM projects WHERE id = $1",
        id
    )

    if not project_data:
        return None

    project = Project(**project_data)

    # Resolve owner
    owner_data = await db.get_user_by_id(project.owner_id)
    if owner_data:
        project.owner = User(**owner_data)

    return project


# Task queries
@query
async def tasks(
    info,
    filters: Optional[TaskFilters] = None,
    limit: int = 20,
    offset: int = 0
) -> list[Task]:
    """Get tasks with optional filtering."""
    db: TaskRepository = info.context["db"]

    # Convert filters to dict
    filter_dict = {}
    if filters:
        if filters.status:
            filter_dict["status"] = filters.status.value
        if filters.priority:
            filter_dict["priority"] = filters.priority.value
        if filters.assignee_id:
            filter_dict["assignee_id"] = filters.assignee_id
        if filters.project_id:
            filter_dict["project_id"] = filters.project_id
        if filters.overdue is not None:
            filter_dict["overdue"] = filters.overdue

    tasks_data = await db.get_tasks(filter_dict, limit, offset)
    return [Task(**data) for data in tasks_data]


@query
async def task(info, id: UUID) -> Optional[Task]:
    """Get a task by ID."""
    db: TaskRepository = info.context["db"]
    task_data = await db.fetch_one(
        "SELECT * FROM tasks WHERE id = $1",
        id
    )

    if not task_data:
        return None

    task = Task(**task_data)

    # Resolve related data
    if task.assignee_id:
        assignee_data = await db.get_user_by_id(task.assignee_id)
        if assignee_data:
            task.assignee = User(**assignee_data)

    project_data = await db.fetch_one(
        "SELECT * FROM projects WHERE id = $1",
        task.project_id
    )
    if project_data:
        task.project = Project(**project_data)

    return task


# Statistics queries
@query
@requires_auth
async def my_task_stats(info) -> dict[str, Any]:
    """Get task statistics for the current user."""
    db: TaskRepository = info.context["db"]
    user = await me(info)

    if not user:
        return {"total": 0}

    stats = await db.fetch_one(
        """
        SELECT
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE status = 'completed') as completed,
            COUNT(*) FILTER (WHERE status = 'in_progress') as in_progress,
            COUNT(*) FILTER (WHERE status = 'pending') as pending,
            COUNT(*) FILTER (WHERE due_date < CURRENT_DATE AND status != 'completed') as overdue
        FROM tasks
        WHERE assignee_id = $1
        """,
        user.id
    )

    return dict(stats)
```

## 6. Mutations (mutations.py)

```python
from uuid import UUID

from fraiseql import mutation, success, failure, fraise_type, requires_auth
from models import (
    CreateUserInput, CreateProjectInput, CreateTaskInput, UpdateTaskInput,
    User, Project, Task
)
from database import TaskRepository


# Success/Failure types
@success
@fraise_type
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"


@failure
@fraise_type
class CreateUserFailure:
    code: str
    message: str


@success
@fraise_type
class CreateProjectSuccess:
    project: Project
    message: str = "Project created successfully"


@failure
@fraise_type
class CreateProjectFailure:
    code: str
    message: str


@success
@fraise_type
class CreateTaskSuccess:
    task: Task
    message: str = "Task created successfully"


@failure
@fraise_type
class TaskOperationFailure:
    code: str
    message: str


@success
@fraise_type
class UpdateTaskSuccess:
    task: Task
    message: str = "Task updated successfully"


@success
@fraise_type
class DeleteSuccess:
    id: UUID
    message: str


# Mutations
@mutation
class CreateUser:
    input: CreateUserInput
    success: CreateUserSuccess
    failure: CreateUserFailure

    async def execute(self, db: TaskRepository, input_data: CreateUserInput):
        # Check if user exists
        existing = await db.get_user_by_email(input_data.email)
        if existing:
            return CreateUserFailure(
                code="USER_EXISTS",
                message=f"User with email {input_data.email} already exists"
            )

        # Create user
        user_data = await db.create_user({
            "email": input_data.email,
            "name": input_data.name,
            "role": input_data.role.value,
            "settings": input_data.settings or {}
        })

        user = User(**user_data)
        return CreateUserSuccess(user=user)


@mutation
class CreateProject:
    input: CreateProjectInput
    success: CreateProjectSuccess
    failure: CreateProjectFailure

    @requires_auth
    async def execute(
        self,
        db: TaskRepository,
        input_data: CreateProjectInput,
        user: User
    ):
        # Create project
        project_data = await db.create_project(
            owner_id=user.id,
            data={
                "name": input_data.name,
                "description": input_data.description,
                "metadata": input_data.metadata or {}
            }
        )

        project = Project(**project_data)
        project.owner = user

        return CreateProjectSuccess(project=project)


@mutation
class CreateTask:
    input: CreateTaskInput
    success: CreateTaskSuccess
    failure: TaskOperationFailure

    @requires_auth
    async def execute(
        self,
        db: TaskRepository,
        input_data: CreateTaskInput,
        user: User
    ):
        # Verify project exists and user has access
        project_data = await db.fetch_one(
            "SELECT * FROM projects WHERE id = $1",
            input_data.project_id
        )

        if not project_data:
            return TaskOperationFailure(
                code="PROJECT_NOT_FOUND",
                message="Project not found"
            )

        # In a real app, check if user has access to the project

        # Create task
        task_data = await db.create_task({
            "title": input_data.title,
            "description": input_data.description,
            "project_id": input_data.project_id,
            "assignee_id": input_data.assignee_id,
            "priority": input_data.priority.value,
            "due_date": input_data.due_date,
            "tags": input_data.tags,
            "metadata": input_data.metadata or {}
        })

        task = Task(**task_data)
        return CreateTaskSuccess(task=task)


@mutation
class UpdateTask:
    input: UpdateTaskInput
    success: UpdateTaskSuccess
    failure: TaskOperationFailure

    @requires_auth
    async def execute(
        self,
        db: TaskRepository,
        input_data: UpdateTaskInput,
        task_id: UUID,
        user: User
    ):
        # Build updates dict
        updates = {}
        if input_data.title is not None:
            updates["title"] = input_data.title
        if input_data.description is not None:
            updates["description"] = input_data.description
        if input_data.status is not None:
            updates["status"] = input_data.status.value
        if input_data.priority is not None:
            updates["priority"] = input_data.priority.value
        if input_data.assignee_id is not None:
            updates["assignee_id"] = input_data.assignee_id
        if input_data.due_date is not None:
            updates["due_date"] = input_data.due_date
        if input_data.tags is not None:
            updates["tags"] = input_data.tags
        if input_data.metadata is not None:
            updates["metadata"] = input_data.metadata

        # Update task
        task_data = await db.update_task(task_id, updates)

        if not task_data:
            return TaskOperationFailure(
                code="TASK_NOT_FOUND",
                message="Task not found"
            )

        task = Task(**task_data)
        return UpdateTaskSuccess(task=task)


@mutation
class DeleteTask:
    input: UUID  # Just the task ID
    success: DeleteSuccess
    failure: TaskOperationFailure

    @requires_auth
    async def execute(
        self,
        db: TaskRepository,
        task_id: UUID,
        user: User
    ):
        # In a real app, check permissions

        # Delete task
        deleted = await db.fetch_one(
            "DELETE FROM tasks WHERE id = $1 RETURNING id",
            task_id
        )

        if not deleted:
            return TaskOperationFailure(
                code="TASK_NOT_FOUND",
                message="Task not found"
            )

        return DeleteSuccess(
            id=task_id,
            message="Task deleted successfully"
        )
```

## 7. Authentication Setup (auth.py)

```python
from contextlib import asynccontextmanager
from typing import Optional

from fastapi import Request

from fraiseql.auth import AuthProvider, UserContext


class MockAuthProvider(AuthProvider):
    """Mock authentication provider for development."""

    async def get_user(self, token: str) -> Optional[UserContext]:
        """Mock user authentication."""
        # In production, validate JWT token
        if token == "valid-token-admin":
            return UserContext(
                user_id="123e4567-e89b-12d3-a456-426614174000",
                email="admin@example.com",
                permissions=["admin"],
                token=token
            )
        elif token == "valid-token-user":
            return UserContext(
                user_id="223e4567-e89b-12d3-a456-426614174001",
                email="user@example.com",
                permissions=["user"],
                token=token
            )
        return None


async def custom_context_getter(request: Request) -> dict:
    """Custom context with additional data."""
    # Get default context
    from fraiseql.fastapi.dependencies import build_graphql_context
    context = await build_graphql_context()

    # Add request info
    context["request"] = request
    context["ip_address"] = request.client.host

    # Add feature flags
    context["features"] = {
        "new_ui": True,
        "beta_features": request.headers.get("X-Beta") == "true"
    }

    return context
```

## 8. Main Application (app.py)

```python
import os
from contextlib import asynccontextmanager

from fastapi import FastAPI
from psycopg_pool import AsyncConnectionPool

from fraiseql import create_fraiseql_app
from models import *  # Import all models
from queries import *  # Import all queries (auto-registered via @query)
from mutations import *  # Import all mutations
from auth import MockAuthProvider, custom_context_getter
from database import TaskRepository


# Custom lifespan for additional resources
@asynccontextmanager
async def custom_lifespan(app: FastAPI):
    """Setup custom resources."""
    # Initialize cache connection
    app.state.cache = {}  # In production, use Redis

    # Initialize background task queue
    app.state.task_queue = []  # In production, use Celery/RQ

    print("🚀 Task Management API started")

    yield

    # Cleanup
    app.state.cache = None
    app.state.task_queue = None
    print("👋 Task Management API shutdown")


# Create the application
app = create_fraiseql_app(
    # Database (supports both URL and psycopg2 format)
    database_url=os.getenv(
        "FRAISEQL_DATABASE_URL",
        "dbname='taskdb' user='postgres' host='localhost'"
    ),

    # Types to register
    types=[
        User, Project, Task,
        UserRole, TaskStatus, TaskPriority,
        TaskFilters
    ],

    # Auth setup
    auth=MockAuthProvider() if os.getenv("FRAISEQL_ENVIRONMENT") == "development" else None,

    # Custom context
    context_getter=custom_context_getter,

    # Custom lifespan
    lifespan=custom_lifespan,

    # App metadata
    title="Task Management API",
    version="1.0.0",
    description="A complete task management system built with FraiseQL",

    # Production mode
    production=os.getenv("FRAISEQL_ENVIRONMENT") == "production"
)


# Add custom REST endpoints
@app.get("/")
async def root():
    """Root endpoint with API info."""
    return {
        "name": "Task Management API",
        "version": "1.0.0",
        "graphql": "/graphql",
        "playground": "/playground" if os.getenv("FRAISEQL_ENVIRONMENT") != "production" else None,
        "docs": "/docs"
    }


@app.get("/stats")
async def global_stats():
    """Get global statistics."""
    from fraiseql.fastapi.dependencies import get_db

    async with get_db() as db:
        stats = await db.fetch_one(
            """
            SELECT
                (SELECT COUNT(*) FROM users) as total_users,
                (SELECT COUNT(*) FROM projects) as total_projects,
                (SELECT COUNT(*) FROM tasks) as total_tasks,
                (SELECT COUNT(*) FROM tasks WHERE status = 'completed') as completed_tasks
            """
        )

    return dict(stats)


# Custom middleware
@app.middleware("http")
async def add_request_id(request, call_next):
    """Add request ID to all requests."""
    import uuid

    request_id = request.headers.get("X-Request-ID", str(uuid.uuid4()))
    request.state.request_id = request_id

    response = await call_next(request)
    response.headers["X-Request-ID"] = request_id

    return response


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        "app:app",
        host="0.0.0.0",
        port=8000,
        reload=os.getenv("FRAISEQL_ENVIRONMENT") != "production"
    )
```

## 9. Example GraphQL Queries

### Create a user
```graphql
mutation CreateUser {
  createUser(input: {
    email: "john@example.com"
    name: "John Doe"
    role: USER
    settings: { theme: "dark", notifications: true }
  }) {
    ... on CreateUserSuccess {
      user {
        id
        email
        name
        role
      }
      message
    }
    ... on CreateUserFailure {
      code
      message
    }
  }
}
```

### Get current user with projects
```graphql
query Me {
  me {
    id
    name
    email
    displayName
  }
  myProjects {
    id
    name
    taskCount
    owner {
      name
    }
  }
}
```

### Create a task
```graphql
mutation CreateTask($projectId: ID!) {
  createTask(input: {
    title: "Implement user authentication"
    description: "Add JWT-based auth to the API"
    projectId: $projectId
    priority: HIGH
    dueDate: "2024-12-31"
    tags: ["backend", "security"]
    metadata: { estimated_hours: 8 }
  }) {
    ... on CreateTaskSuccess {
      task {
        id
        title
        status
        priority
        isOverdue
      }
    }
    ... on TaskOperationFailure {
      code
      message
    }
  }
}
```

### Query tasks with filters
```graphql
query GetTasks {
  tasks(
    filters: {
      status: IN_PROGRESS
      priority: HIGH
      overdue: true
    }
    limit: 10
    offset: 0
  ) {
    id
    title
    status
    priority
    dueDate
    assignee {
      name
      email
    }
    project {
      name
    }
  }
}
```

### Get task statistics
```graphql
query MyStats {
  myTaskStats {
    total
    completed
    inProgress
    pending
    overdue
  }
}
```

## 10. Running the Application

1. **Setup Database:**
   ```bash
   createdb taskdb
   psql taskdb < schema.sql
   ```

2. **Install Dependencies:**
   ```bash
   pip install fraiseql[fastapi]
   ```

3. **Run Development Server:**
   ```bash
   python app.py
   ```

4. **Access GraphQL Playground:**
   ```
   http://localhost:8000/playground
   ```

5. **Test with curl:**
   ```bash
   curl -X POST http://localhost:8000/graphql \
     -H "Content-Type: application/json" \
     -H "Authorization: Bearer valid-token-user" \
     -d '{"query": "{ me { name email } }"}'
   ```

## Key Features Demonstrated

1. ✅ **All FraiseQL decorators**: `@query`, `@mutation`, `@fraise_type`, etc.
2. ✅ **JSON field support**: Using `dict[str, Any]` and `JSON` types
3. ✅ **Authentication**: With `@requires_auth` decorator
4. ✅ **Custom context**: Adding request data and feature flags
5. ✅ **Custom lifespan**: Managing additional resources
6. ✅ **Database URL normalization**: Supporting both formats
7. ✅ **Environment variables**: Using `FRAISEQL_` prefix
8. ✅ **CQRS pattern**: With `TaskRepository`
9. ✅ **Enums**: For type-safe status/priority/role
10. ✅ **Complex queries**: With filters and pagination
11. ✅ **Related data resolution**: Lazy loading of relationships
12. ✅ **Error handling**: Structured success/failure types
13. ✅ **REST endpoints**: Mixed GraphQL + REST API
14. ✅ **Middleware**: Request ID tracking
15. ✅ **Production readiness**: Environment-based configuration

This example demonstrates a production-ready FraiseQL application with all the features addressed in the user feedback!
