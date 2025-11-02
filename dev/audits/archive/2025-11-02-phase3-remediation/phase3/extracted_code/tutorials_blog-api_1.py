# Extracted from: docs/tutorials/blog-api.md
# Block number: 1
from datetime import datetime
from uuid import UUID

from fraiseql import type


@type(sql_source="v_user")
class User:
    id: UUID
    email: str
    name: str
    bio: str | None
    avatar_url: str | None
    created_at: datetime


@type(sql_source="v_comment")
class Comment:
    id: UUID
    content: str
    created_at: datetime
    author: User
    post: "Post"
    replies: list["Comment"]


@type(sql_source="v_post")
class Post:
    id: UUID
    title: str
    slug: str
    content: str
    excerpt: str | None
    tags: list[str]
    is_published: bool
    published_at: datetime | None
    created_at: datetime
    author: User
    comments: list[Comment]
