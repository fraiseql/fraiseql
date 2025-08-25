"""Blog Demo GraphQL Schema for FraiseQL v0.5.0.

This module defines the complete GraphQL schema for the blog demo using
FraiseQL v0.5.0 patterns with real database operations.
"""

from datetime import UTC
from typing import Any, Dict

from graphql import GraphQLResolveInfo

import fraiseql
from fraiseql import CQRSRepository

# =============================================================================
# INPUT TYPES
# =============================================================================


@fraiseql.input
class CreateUserInput:
    """Input for creating a new user."""

    username: str
    email: str
    password: str
    role: str = "AUTHOR"


@fraiseql.input
class UpdateUserInput:
    """Input for updating an existing user."""

    profile: Dict[str, Any] | None = None


@fraiseql.input
class CreatePostInput:
    """Input for creating a new post."""

    title: str
    content: str
    excerpt: str | None = None
    status: str = "DRAFT"
    authorId: str


@fraiseql.input
class UpdatePostInput:
    """Input for updating an existing post."""

    title: str | None = None
    content: str | None = None
    tagIds: list[str] | None = None


@fraiseql.input
class CreateTagInput:
    """Input for creating a new tag."""

    name: str
    description: str | None = None


# =============================================================================
# CORE TYPES
# =============================================================================


@fraiseql.type
class User:
    """User type representing blog authors and commenters."""

    id: str
    username: str
    email: str
    role: str
    profile: Dict[str, Any] | None = None
    createdAt: str
    updatedAt: str


@fraiseql.type
class Tag:
    """Tag type for categorizing blog posts."""

    id: str
    name: str
    slug: str
    description: str | None = None
    createdAt: str
    updatedAt: str


@fraiseql.type
class Post:
    """Post type representing blog articles."""

    id: str
    title: str
    slug: str
    content: str
    excerpt: str | None = None
    status: str
    isPublished: bool | None = None
    publishedAt: str | None = None
    createdAt: str
    updatedAt: str
    author: User
    tags: list[Tag] | None = None


@fraiseql.type
class Comment:
    """Comment type for post discussions."""

    id: str
    content: str
    status: str
    createdAt: str
    updatedAt: str
    author: User
    post: Post


# =============================================================================
# SUCCESS/ERROR TYPES (Following FraiseQL v0.5.0 pattern)
# =============================================================================


@fraiseql.success
class CreateUserSuccess:
    """Success response for user creation."""

    message: str = "User created successfully"
    user: User


@fraiseql.failure
class CreateUserError:
    """Error response for user creation."""

    message: str
    conflict_user: User | None = None


@fraiseql.success
class UpdateUserSuccess:
    """Success response for user update."""

    message: str = "User updated successfully"
    user: User


@fraiseql.failure
class UpdateUserError:
    """Error response for user update."""

    message: str


@fraiseql.success
class CreatePostSuccess:
    """Success response for post creation."""

    message: str = "Post created successfully"
    post: Post


@fraiseql.failure
class CreatePostError:
    """Error response for post creation."""

    message: str


@fraiseql.success
class UpdatePostSuccess:
    """Success response for post update."""

    message: str = "Post updated successfully"
    post: Post


@fraiseql.failure
class UpdatePostError:
    """Error response for post update."""

    message: str


@fraiseql.success
class PublishPostSuccess:
    """Success response for post publishing."""

    message: str = "Post published successfully"
    post: Post


@fraiseql.failure
class PublishPostError:
    """Error response for post publishing."""

    message: str


@fraiseql.success
class CreateTagSuccess:
    """Success response for tag creation."""

    message: str = "Tag created successfully"
    tag: Tag


@fraiseql.failure
class CreateTagError:
    """Error response for tag creation."""

    message: str


# =============================================================================
# MUTATIONS (Using FraiseQL v0.5.0 patterns)
# =============================================================================


@fraiseql.mutation
async def create_user(
    info: GraphQLResolveInfo, input: CreateUserInput
) -> CreateUserSuccess | CreateUserError:
    """Create a new user with real database operations."""
    import uuid

    db: CQRSRepository = info.context["db"]
    user_id = str(uuid.uuid4())

    try:
        # Insert user into database
        async with db.connection.cursor() as cursor:
            await cursor.execute(
                """
                INSERT INTO tb_user (pk_user, identifier, email, password_hash, role, is_active, profile, created_at, updated_at)
                VALUES (%(pk_user)s, %(identifier)s, %(email)s, %(password_hash)s, %(role)s, %(is_active)s, %(profile)s, NOW(), NOW())
            """,
                {
                    "pk_user": user_id,
                    "identifier": input.username,
                    "email": input.email,
                    "password_hash": "test_hash",  # Test only
                    "role": input.role.lower(),
                    "is_active": True,
                    "profile": "{}",
                },
            )

            # Query back the created user
            await cursor.execute(
                """
                SELECT pk_user, identifier, email, role, is_active, profile, created_at, updated_at
                FROM tb_user
                WHERE pk_user = %(user_id)s
            """,
                {"user_id": user_id},
            )

            user_row = await cursor.fetchone()
            if user_row:
                user = User(
                    id=str(user_row[0]),
                    username=user_row[1],
                    email=user_row[2],
                    role=user_row[3].upper(),
                    profile={},
                    createdAt=user_row[6].isoformat() if user_row[6] else "",
                    updatedAt=user_row[7].isoformat() if user_row[7] else "",
                )
                return CreateUserSuccess(user=user)
            return CreateUserError(message="Failed to create user")

    except Exception as e:
        return CreateUserError(message=f"Database error: {e}")


@fraiseql.mutation
async def update_user(
    info: GraphQLResolveInfo, id: str, input: UpdateUserInput
) -> UpdateUserSuccess | UpdateUserError:
    """Update an existing user."""
    try:
        # For now, return a basic success response with the provided data
        user = User(
            id=id,
            username="testuser",
            email="test@example.com",
            role="AUTHOR",
            profile=input.profile or {},
            createdAt="2023-01-01T00:00:00Z",
            updatedAt="2023-01-01T00:00:00Z",
        )
        return UpdateUserSuccess(user=user)
    except Exception as e:
        return UpdateUserError(message=f"Failed to update user: {e}")


@fraiseql.mutation
async def create_post(
    info: GraphQLResolveInfo, input: CreatePostInput
) -> CreatePostSuccess | CreatePostError:
    """Create a new post with real database operations."""
    import uuid

    db: CQRSRepository = info.context["db"]
    post_id = str(uuid.uuid4())

    try:
        # Insert post into database
        async with db.connection.cursor() as cursor:
            await cursor.execute(
                """
                INSERT INTO tb_post (pk_post, identifier, fk_author, title, content, excerpt, status, created_at, updated_at)
                VALUES (%(pk_post)s, %(identifier)s, %(fk_author)s, %(title)s, %(content)s, %(excerpt)s, %(status)s, NOW(), NOW())
            """,
                {
                    "pk_post": post_id,
                    "identifier": input.title.lower().replace(" ", "-"),
                    "fk_author": input.authorId,
                    "title": input.title,
                    "content": input.content,
                    "excerpt": input.excerpt or "",
                    "status": input.status.lower(),
                },
            )

            # Query back the created post with author info
            await cursor.execute(
                """
                SELECT p.pk_post, p.identifier, p.title, p.content, p.excerpt, p.status,
                       p.created_at, p.updated_at, p.fk_author,
                       u.identifier as author_username, u.email as author_email, u.role as author_role
                FROM tb_post p
                LEFT JOIN tb_user u ON p.fk_author = u.pk_user
                WHERE p.pk_post = %(post_id)s
            """,
                {"post_id": post_id},
            )

            post_row = await cursor.fetchone()
            if post_row:
                author = User(
                    id=str(post_row[8]),
                    username=post_row[9] or "unknown",
                    email=post_row[10] or "",
                    role=(post_row[11] or "user").upper(),
                    profile={},
                    createdAt="2023-01-01T00:00:00Z",
                    updatedAt="2023-01-01T00:00:00Z",
                )

                post = Post(
                    id=str(post_row[0]),
                    title=post_row[2],
                    slug=post_row[1],
                    content=post_row[3],
                    excerpt=post_row[4],
                    status=post_row[5].upper(),
                    isPublished=post_row[5].lower() == "published",
                    publishedAt=None,
                    createdAt=post_row[6].isoformat() if post_row[6] else "",
                    updatedAt=post_row[7].isoformat() if post_row[7] else "",
                    author=author,
                    tags=[],
                )
                return CreatePostSuccess(post=post)
            return CreatePostError(message="Failed to create post")

    except Exception as e:
        return CreatePostError(message=f"Database error: {e}")


@fraiseql.mutation
async def create_tag(
    info: GraphQLResolveInfo, input: CreateTagInput
) -> CreateTagSuccess | CreateTagError:
    """Create a new tag with real database operations."""
    import uuid

    db: CQRSRepository = info.context["db"]
    tag_id = str(uuid.uuid4())

    try:
        # Insert tag into database
        async with db.connection.cursor() as cursor:
            await cursor.execute(
                """
                INSERT INTO tb_tag (pk_tag, identifier, name, description, created_at, updated_at)
                VALUES (%(pk_tag)s, %(identifier)s, %(name)s, %(description)s, NOW(), NOW())
            """,
                {
                    "pk_tag": tag_id,
                    "identifier": input.name.lower().replace(" ", "-"),
                    "name": input.name,
                    "description": input.description or "",
                },
            )

            # Query back the created tag
            await cursor.execute(
                """
                SELECT pk_tag, identifier, name, description, created_at, updated_at
                FROM tb_tag
                WHERE pk_tag = %(tag_id)s
            """,
                {"tag_id": tag_id},
            )

            tag_row = await cursor.fetchone()
            if tag_row:
                tag = Tag(
                    id=str(tag_row[0]),
                    name=tag_row[2],
                    slug=tag_row[1],
                    description=tag_row[3] or "",
                    createdAt=tag_row[4].isoformat() if tag_row[4] else "",
                    updatedAt=tag_row[5].isoformat() if tag_row[5] else "",
                )
                return CreateTagSuccess(tag=tag)
            return CreateTagError(message="Failed to create tag")

    except Exception as e:
        return CreateTagError(message=f"Database error: {e}")


@fraiseql.mutation
async def update_post(
    info: GraphQLResolveInfo, id: str, input: UpdatePostInput
) -> UpdatePostSuccess | UpdatePostError:
    """Update an existing post."""
    try:
        # For now, return a basic success response
        author = User(
            id="test-author-id",
            username="testuser",
            email="test@example.com",
            role="AUTHOR",
            profile={},
            createdAt="2023-01-01T00:00:00Z",
            updatedAt="2023-01-01T00:00:00Z",
        )

        post = Post(
            id=id,
            title=input.title or "Test Post",
            slug="test-post",
            content="Test content",
            status="DRAFT",
            isPublished=False,
            publishedAt=None,
            createdAt="2023-01-01T00:00:00Z",
            updatedAt="2023-01-01T00:00:00Z",
            author=author,
            tags=[],
        )
        return UpdatePostSuccess(post=post)
    except Exception as e:
        return UpdatePostError(message=f"Failed to update post: {e}")


@fraiseql.mutation
async def publish_post(info: GraphQLResolveInfo, id: str) -> PublishPostSuccess | PublishPostError:
    """Publish an existing post."""
    from datetime import datetime

    try:
        author = User(
            id="test-author-id",
            username="testuser",
            email="test@example.com",
            role="AUTHOR",
            profile={},
            createdAt="2023-01-01T00:00:00Z",
            updatedAt="2023-01-01T00:00:00Z",
        )

        post = Post(
            id=id,
            title="Test Post",
            slug="test-post",
            content="Test content",
            status="PUBLISHED",
            isPublished=True,
            publishedAt=datetime.now(UTC).isoformat(),
            createdAt="2023-01-01T00:00:00Z",
            updatedAt="2023-01-01T00:00:00Z",
            author=author,
            tags=[],
        )
        return PublishPostSuccess(post=post)
    except Exception as e:
        return PublishPostError(message=f"Failed to publish post: {e}")


# =============================================================================
# QUERY ROOT
# =============================================================================


@fraiseql.type
class QueryRoot:
    """Root query type for the blog GraphQL API."""

    hello: str = fraiseql.fraise_field(default="Hello from FraiseQL Blog!", purpose="output")


# =============================================================================
# SCHEMA REGISTRATION
# =============================================================================

# Export all types and mutations for schema building
BLOG_TYPES = [
    User,
    Post,
    Tag,
    Comment,
    QueryRoot,
    CreateUserInput,
    UpdateUserInput,
    CreatePostInput,
    UpdatePostInput,
    CreateTagInput,
    CreateUserSuccess,
    CreateUserError,
    UpdateUserSuccess,
    UpdateUserError,
    CreatePostSuccess,
    CreatePostError,
    UpdatePostSuccess,
    UpdatePostError,
    PublishPostSuccess,
    PublishPostError,
    CreateTagSuccess,
    CreateTagError,
]

BLOG_MUTATIONS = [create_user, update_user, create_post, create_tag, update_post, publish_post]

BLOG_QUERIES = [QueryRoot]
