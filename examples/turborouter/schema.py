"""GraphQL Type Definitions."""

from dataclasses import dataclass
from datetime import datetime
from typing import Optional


@dataclass
class User:
    """User type."""

    id: int
    name: str
    email: str
    created_at: datetime
    posts: Optional[list["Post"]] = None


@dataclass
class Post:
    """Post type."""

    id: int
    user_id: int
    title: str
    content: str
    published: bool
    created_at: datetime
    author: Optional[User] = None
