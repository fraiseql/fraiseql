"""GraphQL Schema for Blog Example

Demonstrates FraiseQL's CQRS pattern:
- Queries read from tv_* tables (query side)
- Mutations write to tb_* tables and explicitly sync to tv_* (command side)
"""

from datetime import datetime

import strawberry

import fraiseql
from fraiseql import Info


@fraiseql.type(sql_source="tv_user", jsonb_column="data")
class User:
    """User type - read from tv_user (denormalized)."""

    id: str
    email: str
    username: str
    full_name: str = strawberry.field(name="fullName")
    bio: str | None
    published_post_count: int = strawberry.field(name="publishedPostCount")
    comment_count: int = strawberry.field(name="commentCount")
    created_at: datetime = strawberry.field(name="createdAt")
    updated_at: datetime = strawberry.field(name="updatedAt")


@strawberry.type
class Author:
    """Embedded author info in posts/comments."""

    id: str
    username: str
    full_name: str = strawberry.field(name="fullName")


@strawberry.type
class Comment:
    """Comment type - embedded in posts."""

    id: str
    content: str
    author: Author
    created_at: datetime = strawberry.field(name="createdAt")


@fraiseql.type(sql_source="tv_post", jsonb_column="data")
class Post:
    """Post type - read from tv_post (denormalized)."""

    id: str
    title: str
    content: str
    published: bool
    author: Author
    comment_count: int = strawberry.field(name="commentCount")
    comments: list[Comment]
    created_at: datetime = strawberry.field(name="createdAt")
    updated_at: datetime = strawberry.field(name="updatedAt")


@strawberry.type
class SyncMetrics:
    """Real-time sync performance metrics."""

    entity_type: str
    total_syncs_24h: int
    avg_duration_ms: float
    success_rate: float
    failures_24h: int


@strawberry.type
class Query:
    """GraphQL queries - all read from tv_* tables (query side)."""

    @strawberry.field
    async def users(self, info: Info, limit: int | None = 10) -> list[User]:
        """Get users with their post/comment counts."""
        db = info.context["db"]
        return await db.find("tv_user", "users", info, limit=limit, order_by="-createdAt")

    @strawberry.field
    async def user(self, info: Info, id: str) -> User | None:
        """Get a specific user by ID."""
        db = info.context["db"]
        return await db.find_one("tv_user", "user", info, id=id)

    @strawberry.field
    async def posts(
        self, info: Info, published_only: bool = True, limit: int | None = 10
    ) -> list[Post]:
        """Get posts with embedded author and comments."""
        db = info.context["db"]

        filters = {}
        if published_only:
            filters["published"] = True

        return await db.find(
            "tv_post", "posts", info, limit=limit, order_by="-createdAt", **filters
        )

    @strawberry.field
    async def post(self, info: Info, id: str) -> Post | None:
        """Get a specific post by ID."""
        db = info.context["db"]
        return await db.find_one("tv_post", "post", info, id=id)

    @strawberry.field
    async def sync_metrics(self, info, entity_type: str) -> SyncMetrics:
        """Get real-time sync metrics for monitoring."""
        pool = info.context["db_pool"]

        async with pool.acquire() as conn:
            stats = await conn.fetchrow(
                """
                SELECT
                    COUNT(*) as total_syncs,
                    AVG(duration_ms)::float as avg_duration,
                    (COUNT(*) FILTER (WHERE success) * 100.0 / NULLIF(COUNT(*), 0))::float as success_rate,
                    COUNT(*) FILTER (WHERE NOT success) as failures
                FROM sync_log
                WHERE entity_type = $1
                AND created_at > NOW() - INTERVAL '24 hours'
                """,
                entity_type,
            )

        return SyncMetrics(
            entity_type=entity_type,
            total_syncs_24h=stats["total_syncs"] or 0,
            avg_duration_ms=stats["avg_duration"] or 0.0,
            success_rate=stats["success_rate"] or 100.0,
            failures_24h=stats["failures"] or 0,
        )


@strawberry.type
class Mutation:
    """GraphQL mutations - write to tb_* then explicitly sync to tv_*."""

    @strawberry.mutation
    async def create_user(
        self, info, email: str, username: str, full_name: str, bio: str | None = None
    ) -> User:
        """Create a new user.

        EXPLICIT SYNC PATTERN:
        1. Insert into tb_user (command side)
        2. Explicitly sync to tv_user (query side)
        """
        from uuid import uuid4

        pool = info.context["db_pool"]
        sync = info.context["sync"]

        async with pool.acquire() as conn:
            # Step 1: Write to command side (tb_user)
            user_id = await conn.fetchval(
                """
                INSERT INTO tb_user (id, email, username, full_name, bio)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id
                """,
                uuid4(),
                email,
                username,
                full_name,
                bio,
            )

        # Step 2: EXPLICIT SYNC to query side (tv_user)
        # 👈 THIS IS VISIBLE IN YOUR CODE!
        await sync.sync_user([user_id], mode="incremental")

        # Step 3: Read from query side using FraiseQL repository
        db = info.context["db"]
        return await db.find_one("tv_user", "user", info, id=user_id)

    @strawberry.mutation
    async def create_post(
        self, info, title: str, content: str, author_id: str, published: bool = False
    ) -> Post:
        """Create a new post.

        EXPLICIT SYNC PATTERN:
        1. Insert into tb_post (command side)
        2. Explicitly sync to tv_post (query side)
        3. Also sync the author (post count changed)
        """
        from uuid import UUID, uuid4

        pool = info.context["db_pool"]
        sync = info.context["sync"]

        async with pool.acquire() as conn:
            # Step 1: Write to command side (tb_post)
            post_id = await conn.fetchval(
                """
                INSERT INTO tb_post (id, title, content, author_id, published)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id
                """,
                uuid4(),
                title,
                content,
                UUID(author_id),
                published,
            )

        # Step 2: EXPLICIT SYNC to query side
        await sync.sync_post([post_id], mode="incremental")

        # Step 3: Also sync author (post count changed)
        await sync.sync_user([UUID(author_id)], mode="incremental")

        # Step 4: Read from query side using FraiseQL repository
        db = info.context["db"]
        return await db.find_one("tv_post", "post", info, id=post_id)

    @strawberry.mutation
    async def create_comment(self, info, post_id: str, author_id: str, content: str) -> Comment:
        """Create a new comment.

        EXPLICIT SYNC PATTERN:
        1. Insert into tb_comment (command side)
        2. Explicitly sync post (comment count changed)
        3. Explicitly sync author (comment count changed)
        """
        from uuid import UUID, uuid4

        pool = info.context["db_pool"]
        sync = info.context["sync"]

        async with pool.acquire() as conn:
            # Step 1: Write to command side (tb_comment)
            comment_id = await conn.fetchval(
                """
                INSERT INTO tb_comment (id, post_id, author_id, content)
                VALUES ($1, $2, $3, $4)
                RETURNING id
                """,
                uuid4(),
                UUID(post_id),
                UUID(author_id),
                content,
            )

        # Step 2: EXPLICIT SYNC - update post (comment added)
        await sync.sync_post([UUID(post_id)], mode="incremental")

        # Step 3: EXPLICIT SYNC - update author (comment count changed)
        await sync.sync_user([UUID(author_id)], mode="incremental")

        # Step 4: Read from query side using FraiseQL repository
        db = info.context["db"]
        post = await db.find_one("tv_post", "post", info, id=UUID(post_id))

        # Find the new comment in the post's embedded comments
        new_comment = next(c for c in post.comments if c.id == str(comment_id))
        return new_comment

    @strawberry.mutation
    async def publish_post(self, info, post_id: str) -> Post:
        """Publish a post (set published=true).

        EXPLICIT SYNC PATTERN:
        1. Update tb_post (command side)
        2. Explicitly sync to tv_post (query side)
        """
        from uuid import UUID

        pool = info.context["db_pool"]
        sync = info.context["sync"]

        async with pool.acquire() as conn:
            # Step 1: Update command side
            await conn.execute("UPDATE tb_post SET published = true WHERE id = $1", UUID(post_id))

        # Step 2: EXPLICIT SYNC
        await sync.sync_post([UUID(post_id)], mode="incremental")

        # Step 3: Read from query side using FraiseQL repository
        db = info.context["db"]
        return await db.find_one("tv_post", "post", info, id=UUID(post_id))


# Create the GraphQL schema
schema = strawberry.Schema(query=Query, mutation=Mutation)
