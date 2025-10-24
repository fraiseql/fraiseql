"""GraphQL Type Definitions."""

from dataclasses import dataclass
from datetime import datetime


@dataclass
class User:
    """User type."""

    id: int
    name: str
    email: str
    created_at: datetime
    posts: list["Post"] | None = None


@dataclass
class Post:
    """Post type."""

    id: int
    user_id: int
    title: str
    content: str
    published: bool
    created_at: datetime
    author: User | None = None
