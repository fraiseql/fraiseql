"""GraphQL Type Definitions for Task Management API."""

from dataclasses import dataclass
from datetime import datetime
from typing import Optional


@dataclass
class User:
    """User type.

    Represents a user who can own projects and be assigned tasks.
    """

    id: int
    name: str
    email: str
    avatar_url: Optional[str]
    created_at: datetime
    updated_at: datetime

    # Relationships (populated by nested resolvers)
    owned_projects: Optional[list["Project"]] = None
    assigned_tasks: Optional[list["Task"]] = None


@dataclass
class Project:
    """Project type.

    A project contains multiple tasks and has an owner.
    """

    id: int
    name: str
    description: Optional[str]
    owner_id: int
    status: str  # 'active', 'archived', 'completed'
    created_at: datetime
    updated_at: datetime

    # Computed fields from view
    owner_name: Optional[str] = None
    task_count: Optional[int] = None
    completed_count: Optional[int] = None

    # Relationships (populated by nested resolvers)
    owner: Optional[User] = None
    tasks: Optional[list["Task"]] = None


@dataclass
class Task:
    """Task type.

    A task belongs to a project and can be assigned to a user.
    """

    id: int
    project_id: int
    title: str
    description: Optional[str]
    status: str  # 'todo', 'in_progress', 'completed', 'blocked'
    priority: str  # 'low', 'medium', 'high', 'urgent'
    assignee_id: Optional[int]
    due_date: Optional[datetime]
    completed_at: Optional[datetime]
    created_at: datetime
    updated_at: datetime

    # Computed fields from view
    project_name: Optional[str] = None
    assignee_name: Optional[str] = None

    # Relationships (populated by nested resolvers)
    project: Optional[Project] = None
    assignee: Optional[User] = None


# Input types for mutations

@dataclass
class CreateProjectInput:
    """Input for creating a new project."""

    name: str
    description: Optional[str] = None
    owner_id: int = 1  # Default to first user


@dataclass
class UpdateProjectInput:
    """Input for updating a project."""

    name: Optional[str] = None
    description: Optional[str] = None
    status: Optional[str] = None


@dataclass
class CreateTaskInput:
    """Input for creating a new task."""

    project_id: int
    title: str
    description: Optional[str] = None
    priority: str = "medium"
    status: str = "todo"
    assignee_id: Optional[int] = None
    due_date: Optional[datetime] = None


@dataclass
class UpdateTaskInput:
    """Input for updating a task."""

    title: Optional[str] = None
    description: Optional[str] = None
    status: Optional[str] = None
    priority: Optional[str] = None
    assignee_id: Optional[int] = None
    due_date: Optional[datetime] = None
