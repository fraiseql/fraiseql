"""FastAPI Integration Example - Main Application.

This demonstrates a complete FastAPI + FraiseQL setup with:
- CQRS pattern (views for queries, functions for mutations)
- Connection pooling
- CORS configuration
- Context management
- GraphQL Playground
"""

from types import (
    CreateProjectInput,
    CreateTaskInput,
    Project,
    Task,
    UpdateProjectInput,
    UpdateTaskInput,
    User,
)

import uvicorn

from fraiseql import FraiseQL
from fraiseql.fastapi import create_app

# Initialize FraiseQL
app = FraiseQL(database_url="postgresql://localhost/fastapi_tasks_demo")

# Register types
app.register_type(User)
app.register_type(Project)
app.register_type(Task)

# Register input types for mutations
app.register_input_type(CreateProjectInput)
app.register_input_type(UpdateProjectInput)
app.register_input_type(CreateTaskInput)
app.register_input_type(UpdateTaskInput)

# Import and register queries
from queries import (
    Project_owner,
    Project_tasks,
    Task_assignee,
    Task_project,
    User_assigned_tasks,
    User_owned_projects,
    project,
    projects,
    task,
    tasks,
    user,
    users,
)

app.register_query(user)
app.register_query(users)
app.register_query(project)
app.register_query(projects)
app.register_query(task)
app.register_query(tasks)

# Register nested resolvers
app.register_field_resolver(User, "owned_projects", User_owned_projects)
app.register_field_resolver(User, "assigned_tasks", User_assigned_tasks)
app.register_field_resolver(Project, "owner", Project_owner)
app.register_field_resolver(Project, "tasks", Project_tasks)
app.register_field_resolver(Task, "project", Task_project)
app.register_field_resolver(Task, "assignee", Task_assignee)

# Import and register mutations
from mutations import (
    assign_task,
    create_project,
    create_task,
    delete_task,
    update_project,
    update_task,
)

app.register_mutation(create_project)
app.register_mutation(update_project)
app.register_mutation(create_task)
app.register_mutation(update_task)
app.register_mutation(assign_task)
app.register_mutation(delete_task)

# Create FastAPI app with configuration
fastapi_app = create_app(
    app,
    database_url="postgresql://localhost/fastapi_tasks_demo",
    enable_playground=True,
    cors_origins=["http://localhost:3000", "http://localhost:8080"],  # Add your frontend origins
    pool_size=20,  # Maximum number of database connections
    max_overflow=10,  # Additional connections when pool is full
)


# Optional: Add custom FastAPI routes
@fastapi_app.get("/")
async def root():
    """Root endpoint with API information."""
    return {
        "name": "FraiseQL Task Management API",
        "version": "1.0.0",
        "graphql": "/graphql",
        "playground": "/graphql",
        "docs": "/docs",
    }


@fastapi_app.get("/health")
async def health():
    """Health check endpoint."""
    return {"status": "healthy"}


if __name__ == "__main__":
    print("=" * 70)
    print("FastAPI + FraiseQL Task Management API")
    print("=" * 70)
    print()
    print("🚀 Features:")
    print("  ✅ CQRS Architecture (views + PostgreSQL functions)")
    print("  ✅ Automatic N+1 query prevention")
    print("  ✅ Type-safe GraphQL schema from Python dataclasses")
    print("  ✅ Connection pooling for high performance")
    print("  ✅ CORS enabled for frontend integration")
    print()
    print("📍 Endpoints:")
    print("  • GraphQL API:        http://localhost:8000/graphql")
    print("  • GraphQL Playground: http://localhost:8000/graphql")
    print("  • API Docs:           http://localhost:8000/docs")
    print("  • Health Check:       http://localhost:8000/health")
    print()
    print("💡 Example Queries:")
    print()
    print("  # Get all projects with tasks and assignees")
    print("  query {")
    print("    projects {")
    print("      id")
    print("      name")
    print("      taskCount")
    print("      tasks {")
    print("        title")
    print("        status")
    print("        assignee { name }")
    print("      }")
    print("    }")
    print("  }")
    print()
    print("  # Create a new task")
    print("  mutation {")
    print("    createTask(input: {")
    print("      projectId: 1")
    print('      title: "Implement feature"')
    print('      priority: "high"')
    print("    }) {")
    print("      id")
    print("      title")
    print("      createdAt")
    print("    }")
    print("  }")
    print()
    print("=" * 70)
    print()

    uvicorn.run(fastapi_app, host="0.0.0.0", port=8000)
