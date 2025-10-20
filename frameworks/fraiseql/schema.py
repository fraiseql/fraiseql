"""GraphQL Schema for Blog Example

Demonstrates FraiseQL's CQRS pattern:
- Queries read from tv_* tables (query side)
- Mutations write to tb_* tables and explicitly sync to tv_* (command side)
"""

from datetime import datetime
from typing import List, Optional

import strawberry


@strawberry.type
class User:
    """User type - read from tv_user (denormalized)."""

    id: str
    email: str
    username: str
    full_name: str = strawberry.field(name="fullName")
    bio: Optional[str]
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


@strawberry.type
class Post:
    """Post type - read from tv_post (denormalized)."""

    id: str
    title: str
    content: str
    published: bool
    author: Author
    comment_count: int = strawberry.field(name="commentCount")
    comments: List[Comment]
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
    async def users(self, info, limit: Optional[int] = 10) -> List[User]:
        """Get users with their post/comment counts."""
        pool = info.context["db_pool"]

        async with pool.acquire() as conn:
            rows = await conn.fetch(
                """
                SELECT data FROM tv_user
                ORDER BY (data->>'createdAt')::timestamptz DESC
                LIMIT $1
                """,
                limit,
            )

        return [User(**row["data"]) for row in rows]

    @strawberry.field
    async def user(self, info, id: str) -> Optional[User]:
        """Get a specific user by ID."""
        pool = info.context["db_pool"]

        async with pool.acquire() as conn:
            row = await conn.fetchrow("SELECT data FROM tv_user WHERE id = $1", id)

        return User(**row["data"]) if row else None

    @strawberry.field
    async def posts(
        self, info, published_only: bool = True, limit: Optional[int] = 10
    ) -> List[Post]:
        """Get posts with embedded author and comments."""
        pool = info.context["db_pool"]

        async with pool.acquire() as conn:
            if published_only:
                rows = await conn.fetch(
                    """
                    SELECT data FROM tv_post
                    WHERE (data->>'published')::boolean = true
                    ORDER BY (data->>'createdAt')::timestamptz DESC
                    LIMIT $1
                    """,
                    limit,
                )
            else:
                rows = await conn.fetch(
                    """
                    SELECT data FROM tv_post
                    ORDER BY (data->>'createdAt')::timestamptz DESC
                    LIMIT $1
                    """,
                    limit,
                )

        return [Post(**row["data"]) for row in rows]

    @strawberry.field
    async def post(self, info, id: str) -> Optional[Post]:
        """Get a specific post by ID."""
        pool = info.context["db_pool"]

        async with pool.acquire() as conn:
            row = await conn.fetchrow("SELECT data FROM tv_post WHERE id = $1", id)

        return Post(**row["data"]) if row else None

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
        self, info, email: str, username: str, full_name: str, bio: Optional[str] = None
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

        # Step 3: Read from query side
        async with pool.acquire() as conn:
            row = await conn.fetchrow("SELECT data FROM tv_user WHERE id = $1", user_id)

        return User(**row["data"])

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

        # Step 4: Read from query side
        async with pool.acquire() as conn:
            row = await conn.fetchrow("SELECT data FROM tv_post WHERE id = $1", post_id)

        return Post(**row["data"])

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

        # Step 4: Read from query side (embedded in post)
        async with pool.acquire() as conn:
            row = await conn.fetchrow(
                """
                SELECT data FROM tv_post WHERE id = $1
                """,
                UUID(post_id),
            )

        post_data = Post(**row["data"])
        # Find the new comment
        new_comment = next(c for c in post_data.comments if c.id == str(comment_id))
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

        # Step 3: Read from query side
        async with pool.acquire() as conn:
            row = await conn.fetchrow("SELECT data FROM tv_post WHERE id = $1", UUID(post_id))

        return Post(**row["data"])


# Create the GraphQL schema
schema = strawberry.Schema(query=Query, mutation=Mutation)
