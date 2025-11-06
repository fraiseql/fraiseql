"""GraphQL Type Definitions for Task Management API."""

from dataclasses import dataclass
from datetime import datetime

from fraiseql.types.scalars.id_scalar import ID


@dataclass
class User:
    """User type.

    Represents a user who can own projects and be assigned tasks.
    """

    id: int
    name: str
    email: str
    avatar_url: str | None
    created_at: datetime
    updated_at: datetime

    # Relationships (populated by nested resolvers)
    owned_projects: list["Project"] | None = None
    assigned_tasks: list["Task"] | None = None


@dataclass
class Project:
    """Project type.

    A project contains multiple tasks and has an owner.
    """

    id: int
    name: str
    description: str | None
    owner_id: ID
    status: str  # 'active', 'archived', 'completed'
    created_at: datetime
    updated_at: datetime

    # Computed fields from view
    owner_name: str | None = None
    task_count: int | None = None
    completed_count: int | None = None

    # Relationships (populated by nested resolvers)
    owner: User | None = None
    tasks: list["Task"] | None = None


@dataclass
class Task:
    """Task type.

    A task belongs to a project and can be assigned to a user.
    """

    id: int
    project_id: ID
    title: str
    description: str | None
    status: str  # 'todo', 'in_progress', 'completed', 'blocked'
    priority: str  # 'low', 'medium', 'high', 'urgent'
    assignee_id: ID | None
    due_date: datetime | None
    completed_at: datetime | None
    created_at: datetime
    updated_at: datetime

    # Computed fields from view
    project_name: str | None = None
    assignee_name: str | None = None

    # Relationships (populated by nested resolvers)
    project: Project | None = None
    assignee: User | None = None


# Input types for mutations


@dataclass
class CreateProjectInput:
    """Input for creating a new project."""

    name: str
    owner_id: ID
    description: str | None = None


@dataclass
class UpdateProjectInput:
    """Input for updating a project."""

    name: str | None = None
    description: str | None = None
    status: str | None = None


@dataclass
class CreateTaskInput:
    """Input for creating a new task."""

    project_id: ID
    title: str
    description: str | None = None
    priority: str = "medium"
    status: str = "todo"
    assignee_id: ID | None = None
    due_date: datetime | None = None


@dataclass
class UpdateTaskInput:
    """Input for updating a task."""

    title: str | None = None
    description: str | None = None
    status: str | None = None
    priority: str | None = None
    assignee_id: ID | None = None
    due_date: datetime | None = None
