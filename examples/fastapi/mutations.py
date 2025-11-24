"""GraphQL Mutation Resolvers.

These mutations use PostgreSQL functions to encapsulate business logic in the database.
This provides:
- ACID guarantees
- Centralized business rules
- Automatic timestamp management
- Better performance (fewer round-trips)
"""

from types import (
    CreateProjectInput,
    CreateTaskInput,
    Project,
    Task,
    UpdateProjectInput,
    UpdateTaskInput,
)


async def create_project(info, input: CreateProjectInput) -> Project:
    """Create a new project.

    Calls the `fn_create_project()` PostgreSQL function.

    Args:
        info: GraphQL resolve info with context
        input: CreateProjectInput with project details

    Returns:
        Newly created Project object

    Example:
        mutation {
            createProject(input: {
                name: "New Feature"
                description: "Exciting new functionality"
                ownerId: 1
            }) {
                id
                name
                createdAt
            }
        }
    """
    db = info.context["db"]

    result = await db.execute_function(
        "fn_create_project",
        p_name=input.name,
        p_description=input.description,
        p_owner_id=input.owner_id,
    )

    return result[0] if result else None


async def update_project(info, id: int, input: UpdateProjectInput) -> Project | None:
    """Update an existing project.

    Calls the `fn_update_project()` PostgreSQL function.

    Args:
        info: GraphQL resolve info with context
        id: Project ID to update
        input: UpdateProjectInput with fields to update

    Returns:
        Updated Project object or None if not found

    Example:
        mutation {
            updateProject(id: 1, input: {
                status: "completed"
            }) {
                id
                name
                status
                updatedAt
            }
        }
    """
    db = info.context["db"]

    result = await db.execute_function(
        "fn_update_project",
        p_id=id,
        p_name=input.name,
        p_description=input.description,
        p_status=input.status,
    )

    return result[0] if result else None


async def create_task(info, input: CreateTaskInput) -> Task:
    """Create a new task.

    Calls the `fn_create_task()` PostgreSQL function.

    Args:
        info: GraphQL resolve info with context
        input: CreateTaskInput with task details

    Returns:
        Newly created Task object

    Example:
        mutation {
            createTask(input: {
                projectId: 1
                title: "Implement feature X"
                description: "Add support for..."
                priority: "high"
                assigneeId: 2
                dueDate: "2024-12-31T23:59:59Z"
            }) {
                id
                title
                priority
                createdAt
            }
        }
    """
    db = info.context["db"]

    result = await db.execute_function(
        "fn_create_task",
        p_project_id=input.project_id,
        p_title=input.title,
        p_description=input.description,
        p_priority=input.priority,
        p_status=input.status,
        p_assignee_id=input.assignee_id,
        p_due_date=input.due_date,
    )

    return result[0] if result else None


async def update_task(info, id: int, input: UpdateTaskInput) -> Task | None:
    """Update an existing task.

    Calls the `fn_update_task()` PostgreSQL function.
    When status changes to 'completed', the function automatically sets completed_at.

    Args:
        info: GraphQL resolve info with context
        id: Task ID to update
        input: UpdateTaskInput with fields to update

    Returns:
        Updated Task object or None if not found

    Example:
        mutation {
            updateTask(id: 5, input: {
                status: "completed"
            }) {
                id
                title
                status
                completedAt
            }
        }
    """
    db = info.context["db"]

    result = await db.execute_function(
        "fn_update_task",
        p_id=id,
        p_title=input.title,
        p_description=input.description,
        p_status=input.status,
        p_priority=input.priority,
        p_assignee_id=input.assignee_id,
        p_due_date=input.due_date,
    )

    return result[0] if result else None


async def assign_task(info, task_id: int, user_id: int) -> Task | None:
    """Assign a task to a user.

    Calls the `fn_assign_task()` PostgreSQL function.

    Args:
        info: GraphQL resolve info with context
        task_id: Task ID to assign
        user_id: User ID to assign the task to

    Returns:
        Updated Task object with new assignee

    Example:
        mutation {
            assignTask(taskId: 3, userId: 2) {
                id
                title
                assignee {
                    id
                    name
                    email
                }
            }
        }
    """
    db = info.context["db"]

    result = await db.execute_function(
        "fn_assign_task", p_task_id=task_id, p_user_id=user_id
    )

    return result[0] if result else None


async def delete_task(info, id: int) -> bool:
    """Delete a task.

    Calls the `fn_delete_task()` PostgreSQL function.

    Args:
        info: GraphQL resolve info with context
        id: Task ID to delete

    Returns:
        True if task was deleted, False if not found

    Example:
        mutation {
            deleteTask(id: 10)
        }
    """
    db = info.context["db"]

    result = await db.execute_function("fn_delete_task", p_id=id)

    # PostgreSQL function returns BOOLEAN
    return result[0] if result else False
