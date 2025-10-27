"""Explicit Sync Module - CQRS Synchronization

This module demonstrates FraiseQL's explicit sync pattern:
- NO TRIGGERS (explicit function calls instead)
- Full visibility (sync is in your code)
- Easy testing (can mock sync functions)
- Industrial control (batch, defer, skip as needed)
"""

import time
from typing import Optional
from uuid import UUID

import asyncpg


class SyncError(Exception):
    """Raised when sync operation fails."""


class EntitySync:
    """Handles synchronization from command (tb_) to query (tv_) tables."""

    def __init__(self, pool: asyncpg.Pool):
        self.pool = pool

    async def _log_sync(
        self,
        conn: asyncpg.Connection,
        entity_type: str,
        entity_id: UUID,
        operation: str,
        duration_ms: int,
        success: bool = True,
        error_message: Optional[str] = None,
    ):
        """Log sync operation for monitoring."""
        await conn.execute(
            """
            INSERT INTO sync_log (entity_type, entity_id, operation, duration_ms, success, error_message)
            VALUES ($1, $2, $3, $4, $5, $6)
            """,
            entity_type,
            entity_id,
            operation,
            duration_ms,
            success,
            error_message,
        )

    async def sync_user(self, user_ids: list[UUID], mode: str = "incremental") -> None:
        """Sync users from tb_user to tv_user with denormalized post count.

        Args:
            user_ids: List of user IDs to sync
            mode: 'incremental' (default) or 'full'

        Example:
            await sync.sync_user([user_id], mode='incremental')
        """
        start_time = time.time()

        async with self.pool.acquire() as conn:
            for user_id in user_ids:
                try:
                    # Build denormalized user data
                    user_data = await conn.fetchrow(
                        """
                        SELECT
                            u.id,
                            u.email,
                            u.username,
                            u.full_name,
                            u.bio,
                            u.created_at,
                            u.updated_at,
                            COUNT(DISTINCT p.id) FILTER (WHERE p.published) as published_post_count,
                            COUNT(DISTINCT c.id) as comment_count
                        FROM tb_user u
                        LEFT JOIN tb_post p ON p.author_id = u.id
                        LEFT JOIN tb_comment c ON c.author_id = u.id
                        WHERE u.id = $1
                        GROUP BY u.id
                        """,
                        user_id,
                    )

                    if not user_data:
                        continue

                    # Convert to JSONB structure
                    jsonb_data = {
                        "id": str(user_data["id"]),
                        "email": user_data["email"],
                        "username": user_data["username"],
                        "fullName": user_data["full_name"],
                        "bio": user_data["bio"],
                        "publishedPostCount": user_data["published_post_count"],
                        "commentCount": user_data["comment_count"],
                        "createdAt": user_data["created_at"].isoformat(),
                        "updatedAt": user_data["updated_at"].isoformat(),
                    }

                    # Upsert to tv_user
                    await conn.execute(
                        """
                        INSERT INTO tv_user (id, data, updated_at)
                        VALUES ($1, $2, NOW())
                        ON CONFLICT (id) DO UPDATE
                        SET data = $2, updated_at = NOW()
                        """,
                        user_id,
                        jsonb_data,
                    )

                    # Log success
                    duration_ms = int((time.time() - start_time) * 1000)
                    await self._log_sync(conn, "user", user_id, mode, duration_ms, True)

                except Exception as e:
                    duration_ms = int((time.time() - start_time) * 1000)
                    await self._log_sync(conn, "user", user_id, mode, duration_ms, False, str(e))
                    raise SyncError(f"Failed to sync user {user_id}: {e}") from e

    async def sync_post(self, post_ids: list[UUID], mode: str = "incremental") -> None:
        """Sync posts from tb_post to tv_post with denormalized author and comments.

        Args:
            post_ids: List of post IDs to sync
            mode: 'incremental' (default) or 'full'

        Example:
            await sync.sync_post([post_id], mode='incremental')
        """
        start_time = time.time()

        async with self.pool.acquire() as conn:
            for post_id in post_ids:
                try:
                    # Build denormalized post data with author
                    post_data = await conn.fetchrow(
                        """
                        SELECT
                            p.id,
                            p.title,
                            p.content,
                            p.published,
                            p.created_at,
                            p.updated_at,
                            jsonb_build_object(
                                'id', u.id,
                                'username', u.username,
                                'fullName', u.full_name
                            ) as author,
                            COUNT(DISTINCT c.id) as comment_count
                        FROM tb_post p
                        JOIN tb_user u ON u.id = p.author_id
                        LEFT JOIN tb_comment c ON c.post_id = p.id
                        WHERE p.id = $1
                        GROUP BY p.id, u.id
                        """,
                        post_id,
                    )

                    if not post_data:
                        continue

                    # Get comments for this post
                    comments = await conn.fetch(
                        """
                        SELECT
                            c.id,
                            c.content,
                            c.created_at,
                            jsonb_build_object(
                                'id', u.id,
                                'username', u.username,
                                'fullName', u.full_name
                            ) as author
                        FROM tb_comment c
                        JOIN tb_user u ON u.id = c.author_id
                        WHERE c.post_id = $1
                        ORDER BY c.created_at DESC
                        """,
                        post_id,
                    )

                    # Convert to JSONB structure
                    jsonb_data = {
                        "id": str(post_data["id"]),
                        "title": post_data["title"],
                        "content": post_data["content"],
                        "published": post_data["published"],
                        "author": post_data["author"],
                        "commentCount": post_data["comment_count"],
                        "comments": [
                            {
                                "id": str(c["id"]),
                                "content": c["content"],
                                "author": c["author"],
                                "createdAt": c["created_at"].isoformat(),
                            }
                            for c in comments
                        ],
                        "createdAt": post_data["created_at"].isoformat(),
                        "updatedAt": post_data["updated_at"].isoformat(),
                    }

                    # Upsert to tv_post
                    await conn.execute(
                        """
                        INSERT INTO tv_post (id, data, updated_at)
                        VALUES ($1, $2, NOW())
                        ON CONFLICT (id) DO UPDATE
                        SET data = $2, updated_at = NOW()
                        """,
                        post_id,
                        jsonb_data,
                    )

                    # Log success
                    duration_ms = int((time.time() - start_time) * 1000)
                    await self._log_sync(conn, "post", post_id, mode, duration_ms, True)

                except Exception as e:
                    duration_ms = int((time.time() - start_time) * 1000)
                    await self._log_sync(conn, "post", post_id, mode, duration_ms, False, str(e))
                    raise SyncError(f"Failed to sync post {post_id}: {e}") from e

    async def sync_comment(self, comment_ids: list[UUID], mode: str = "incremental") -> None:
        """Sync comments from tb_comment to tv_comment with denormalized author.

        Args:
            comment_ids: List of comment IDs to sync
            mode: 'incremental' (default) or 'full'

        Example:
            await sync.sync_comment([comment_id], mode='incremental')
        """
        start_time = time.time()

        async with self.pool.acquire() as conn:
            for comment_id in comment_ids:
                try:
                    # Build denormalized comment data
                    comment_data = await conn.fetchrow(
                        """
                        SELECT
                            c.id,
                            c.post_id,
                            c.content,
                            c.created_at,
                            c.updated_at,
                            jsonb_build_object(
                                'id', u.id,
                                'username', u.username,
                                'fullName', u.full_name
                            ) as author
                        FROM tb_comment c
                        JOIN tb_user u ON u.id = c.author_id
                        WHERE c.id = $1
                        """,
                        comment_id,
                    )

                    if not comment_data:
                        continue

                    # Convert to JSONB structure
                    jsonb_data = {
                        "id": str(comment_data["id"]),
                        "postId": str(comment_data["post_id"]),
                        "content": comment_data["content"],
                        "author": comment_data["author"],
                        "createdAt": comment_data["created_at"].isoformat(),
                        "updatedAt": comment_data["updated_at"].isoformat(),
                    }

                    # Upsert to tv_comment
                    await conn.execute(
                        """
                        INSERT INTO tv_comment (id, data, updated_at)
                        VALUES ($1, $2, NOW())
                        ON CONFLICT (id) DO UPDATE
                        SET data = $2, updated_at = NOW()
                        """,
                        comment_id,
                        jsonb_data,
                    )

                    # Log success
                    duration_ms = int((time.time() - start_time) * 1000)
                    await self._log_sync(conn, "comment", comment_id, mode, duration_ms, True)

                except Exception as e:
                    duration_ms = int((time.time() - start_time) * 1000)
                    await self._log_sync(
                        conn, "comment", comment_id, mode, duration_ms, False, str(e)
                    )
                    raise SyncError(f"Failed to sync comment {comment_id}: {e}") from e

    async def sync_all_users(self) -> int:
        """Sync all users (full rebuild). Returns count of synced users."""
        async with self.pool.acquire() as conn:
            user_ids = await conn.fetch("SELECT id FROM tb_user")
            await self.sync_user([row["id"] for row in user_ids], mode="full")
            return len(user_ids)

    async def sync_all_posts(self) -> int:
        """Sync all posts (full rebuild). Returns count of synced posts."""
        async with self.pool.acquire() as conn:
            post_ids = await conn.fetch("SELECT id FROM tb_post")
            await self.sync_post([row["id"] for row in post_ids], mode="full")
            return len(post_ids)

    async def sync_all_comments(self) -> int:
        """Sync all comments (full rebuild). Returns count of synced comments."""
        async with self.pool.acquire() as conn:
            comment_ids = await conn.fetch("SELECT id FROM tb_comment")
            await self.sync_comment([row["id"] for row in comment_ids], mode="full")
            return len(comment_ids)
