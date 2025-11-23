"""GraphQL Query Resolvers."""

from types import Project, Task, User


async def user(info, id: int) -> User | None:
    """Get a single user by ID.

    Args:
        info: GraphQL resolve info with context
        id: User ID

    Returns:
        User object or None if not found
    """
    db = info.context["db"]
    return await db.find_one("v_users", id=id)


async def users(info, limit: int = 100, offset: int = 0) -> list[User]:
    """Get a list of users.

    Args:
        info: GraphQL resolve info with context
        limit: Maximum number of users to return
        offset: Number of users to skip

    Returns:
        List of User objects
    """
    db = info.context["db"]
    return await db.find("v_users", limit=limit, offset=offset, order_by="name")


async def project(info, id: int) -> Project | None:
    """Get a single project by ID.

    Args:
        info: GraphQL resolve info with context
        id: Project ID

    Returns:
        Project object or None if not found
    """
    db = info.context["db"]
    return await db.find_one("v_projects", id=id)


async def projects(
    info,
    limit: int = 100,
    offset: int = 0,
    status: str | None = None,
    owner_id: int | None = None,
) -> list[Project]:
    """Get a list of projects.

    Args:
        info: GraphQL resolve info with context
        limit: Maximum number of projects to return
        offset: Number of projects to skip
        status: Filter by project status
        owner_id: Filter by owner ID

    Returns:
        List of Project objects
    """
    db = info.context["db"]
    filters = {}
    if status is not None:
        filters["status"] = status
    if owner_id is not None:
        filters["owner_id"] = owner_id

    return await db.find(
        "v_projects", limit=limit, offset=offset, order_by="created_at DESC", **filters
    )


async def task(info, id: int) -> Task | None:
    """Get a single task by ID.

    Args:
        info: GraphQL resolve info with context
        id: Task ID

    Returns:
        Task object or None if not found
    """
    db = info.context["db"]
    return await db.find_one("v_tasks", id=id)


async def tasks(
    info,
    limit: int = 100,
    offset: int = 0,
    project_id: int | None = None,
    assignee_id: int | None = None,
    status: str | None = None,
    priority: str | None = None,
) -> list[Task]:
    """Get a list of tasks.

    Args:
        info: GraphQL resolve info with context
        limit: Maximum number of tasks to return
        offset: Number of tasks to skip
        project_id: Filter by project ID
        assignee_id: Filter by assignee ID
        status: Filter by task status
        priority: Filter by task priority

    Returns:
        List of Task objects
    """
    db = info.context["db"]
    filters = {}
    if project_id is not None:
        filters["project_id"] = project_id
    if assignee_id is not None:
        filters["assignee_id"] = assignee_id
    if status is not None:
        filters["status"] = status
    if priority is not None:
        filters["priority"] = priority

    return await db.find(
        "v_tasks", limit=limit, offset=offset, order_by="created_at DESC", **filters
    )


# Nested resolvers for relationships


async def User_owned_projects(user: User, info) -> list[Project]:
    """Get projects owned by a user.

    Args:
        user: Parent User object
        info: GraphQL resolve info with context

    Returns:
        List of Project objects owned by the user
    """
    db = info.context["db"]
    return await db.find("v_projects", owner_id=user.id, order_by="created_at DESC")


async def User_assigned_tasks(user: User, info) -> list[Task]:
    """Get tasks assigned to a user.

    Args:
        user: Parent User object
        info: GraphQL resolve info with context

    Returns:
        List of Task objects assigned to the user
    """
    db = info.context["db"]
    return await db.find("v_tasks", assignee_id=user.id, order_by="due_date ASC")


async def Project_owner(project: Project, info) -> User | None:
    """Get the owner of a project.

    Args:
        project: Parent Project object
        info: GraphQL resolve info with context

    Returns:
        User object who owns the project
    """
    db = info.context["db"]
    return await db.find_one("v_users", id=project.owner_id)


async def Project_tasks(project: Project, info) -> list[Task]:
    """Get tasks belonging to a project.

    Args:
        project: Parent Project object
        info: GraphQL resolve info with context

    Returns:
        List of Task objects in the project
    """
    db = info.context["db"]
    return await db.find("v_tasks", project_id=project.id, order_by="priority DESC, created_at DESC")


async def Task_project(task: Task, info) -> Project | None:
    """Get the project a task belongs to.

    Args:
        task: Parent Task object
        info: GraphQL resolve info with context

    Returns:
        Project object the task belongs to
    """
    db = info.context["db"]
    return await db.find_one("v_projects", id=task.project_id)


async def Task_assignee(task: Task, info) -> User | None:
    """Get the user assigned to a task.

    Args:
        task: Parent Task object
        info: GraphQL resolve info with context

    Returns:
        User object assigned to the task, or None if unassigned
    """
    if task.assignee_id is None:
        return None

    db = info.context["db"]
    return await db.find_one("v_users", id=task.assignee_id)
