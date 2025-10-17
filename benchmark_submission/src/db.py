"""Database layer for GraphQL Benchmark using FraiseQL CQRS."""

from typing import Any, Dict, List, Optional
from uuid import UUID

from fraiseql.cqrs import CQRSRepository as BaseCQRSRepository


class BenchmarkRepository(BaseCQRSRepository):
    """Benchmark-specific repository extending FraiseQL CQRS base.

    Uses tv_* tables for denormalized reads and tb_* tables for writes.
    """

    # ==========================================================================
    # USER QUERIES
    # ==========================================================================

    async def get_users_with_posts(self, limit: int = 20, offset: int = 0) -> List[Dict[str, Any]]:
        """Get users with their posts denormalized."""
        # Use a custom query to join users with their posts
        query = """
        SELECT
            u.data as user_data,
            COALESCE(jsonb_agg(p.data) FILTER (WHERE p.data IS NOT NULL), '[]'::jsonb) as posts
        FROM tv_user u
        LEFT JOIN tv_post p ON (p.data->>'authorId')::uuid = (u.data->>'id')::uuid
        GROUP BY u.id, u.data
        ORDER BY u.data->>'name'
        LIMIT $1 OFFSET $2
        """
        rows = await self.fetch_rows(query, limit, offset)

        result = []
        for row in rows:
            user_data = row["user_data"]
            posts_data = row["posts"]
            result.append({"user": user_data, "posts": posts_data})
        return result

    async def get_user_with_posts(self, user_id: UUID) -> Optional[Dict[str, Any]]:
        """Get a single user with their posts."""
        query = """
        SELECT
            u.data as user_data,
            COALESCE(jsonb_agg(p.data) FILTER (WHERE p.data IS NOT NULL), '[]'::jsonb) as posts
        FROM tv_user u
        LEFT JOIN tv_post p ON (p.data->>'authorId')::uuid = (u.data->>'id')::uuid
        WHERE (u.data->>'id')::uuid = $1
        GROUP BY u.id, u.data
        """
        row = await self.fetch_row(query, str(user_id))
        if not row:
            return None

        return {"user": row["user_data"], "posts": row["posts"]}

    async def get_users_filtered(
        self, filters: Dict[str, Any], order_by: Optional[str] = None, limit: int = 20
    ) -> List[Dict[str, Any]]:
        """Get users with complex filtering."""
        where_clauses = []
        params = []

        if "age_gt" in filters:
            where_clauses.append("(u.data->>'age')::int > $1")
            params.append(filters["age_gt"])

        if "age_lt" in filters:
            where_clauses.append("(u.data->>'age')::int < $1")
            params.append(filters["age_lt"])

        if "city" in filters:
            where_clauses.append("u.data->>'city' = $1")
            params.append(filters["city"])

        if "name_contains" in filters:
            where_clauses.append("u.data->>'name' ILIKE $1")
            params.append(f"%{filters['name_contains']}%")

        where_sql = " AND ".join(where_clauses) if where_clauses else "TRUE"
        order_sql = f"ORDER BY {order_by}" if order_by else "ORDER BY u.data->>'name'"

        query = f"""
        SELECT u.data
        FROM tv_user u
        WHERE {where_sql}
        {order_sql}
        LIMIT ${len(params) + 1}
        """
        params.append(limit)

        rows = await self.fetch_rows(query, *params)
        return [row["data"] for row in rows]

    # ==========================================================================
    # POST QUERIES
    # ==========================================================================

    async def get_posts_with_authors_and_comments(
        self, limit: int = 20, offset: int = 0
    ) -> List[Dict[str, Any]]:
        """Get posts with authors and comments denormalized."""
        query = """
        SELECT
            p.data as post_data,
            u.data as author_data,
            COALESCE(jsonb_agg(c.data) FILTER (WHERE c.data IS NOT NULL), '[]'::jsonb) as comments
        FROM tv_post p
        JOIN tv_user u ON (u.data->>'id')::uuid = (p.data->>'authorId')::uuid
        LEFT JOIN tv_comment c ON (c.data->>'postId')::uuid = (p.data->>'id')::uuid
        GROUP BY p.id, p.data, u.data
        ORDER BY p.data->>'createdAt' DESC
        LIMIT $1 OFFSET $2
        """
        rows = await self.fetch_rows(query, limit, offset)

        result = []
        for row in rows:
            result.append(
                {
                    "post": row["post_data"],
                    "author": row["author_data"],
                    "comments": row["comments"],
                }
            )
        return result

    async def get_post_with_author_and_comments(self, post_id: UUID) -> Optional[Dict[str, Any]]:
        """Get a single post with author and comments."""
        query = """
        SELECT
            p.data as post_data,
            u.data as author_data,
            COALESCE(jsonb_agg(c.data) FILTER (WHERE c.data IS NOT NULL), '[]'::jsonb) as comments
        FROM tv_post p
        JOIN tv_user u ON (u.data->>'id')::uuid = (p.data->>'authorId')::uuid
        LEFT JOIN tv_comment c ON (c.data->>'postId')::uuid = (p.data->>'id')::uuid
        WHERE (p.data->>'id')::uuid = $1
        GROUP BY p.id, p.data, u.data
        """
        row = await self.fetch_row(query, str(post_id))
        if not row:
            return None

        return {"post": row["post_data"], "author": row["author_data"], "comments": row["comments"]}

    async def get_posts_by_author(self, author_id: UUID) -> List[Dict[str, Any]]:
        """Get posts by author ID."""
        query = """
        SELECT p.data
        FROM tv_post p
        WHERE (p.data->>'authorId')::uuid = $1
        ORDER BY p.data->>'createdAt' DESC
        """
        rows = await self.fetch_rows(query, str(author_id))
        return [row["data"] for row in rows]

    # ==========================================================================
    # COMMENT QUERIES
    # ==========================================================================

    async def get_comments_by_post(self, post_id: UUID) -> List[Dict[str, Any]]:
        """Get comments for a post."""
        query = """
        SELECT c.data
        FROM tv_comment c
        WHERE (c.data->>'postId')::uuid = $1
        ORDER BY c.data->>'createdAt'
        """
        rows = await self.fetch_rows(query, str(post_id))
        return [row["data"] for row in rows]

    # ==========================================================================
    # BATCH METHODS FOR DATALOADER SUPPORT
    # ==========================================================================

    async def get_users_by_ids(self, user_ids: List[str]) -> List[Dict[str, Any]]:
        """Get multiple users by their IDs for DataLoader."""
        if not user_ids:
            return []

        placeholders = ", ".join(f"${i + 1}" for i in range(len(user_ids)))
        query = f"""
        SELECT data
        FROM tv_user
        WHERE (data->>'id')::uuid IN ({placeholders})
        """
        rows = await self.fetch_rows(query, *user_ids)
        return [row["data"] for row in rows]

    async def get_posts_by_ids(self, post_ids: List[str]) -> List[Dict[str, Any]]:
        """Get multiple posts by their IDs for DataLoader."""
        if not post_ids:
            return []

        placeholders = ", ".join(f"${i + 1}" for i in range(len(post_ids)))
        query = f"""
        SELECT data
        FROM tv_post
        WHERE (data->>'id')::uuid IN ({placeholders})
        """
        rows = await self.fetch_rows(query, *post_ids)
        return [row["data"] for row in rows]

    async def get_comments_by_post_ids(self, post_ids: List[str]) -> List[Dict[str, Any]]:
        """Get all comments for multiple posts for DataLoader."""
        if not post_ids:
            return []

        placeholders = ", ".join(f"${i + 1}" for i in range(len(post_ids)))
        query = f"""
        SELECT data
        FROM tv_comment
        WHERE (data->>'postId')::uuid IN ({placeholders})
        ORDER BY data->>'createdAt'
        """
        rows = await self.fetch_rows(query, *post_ids)
        return [row["data"] for row in rows]

    # ==========================================================================
    # MUTATION METHODS
    # ==========================================================================

    async def create_user(
        self, name: str, email: str, age: Optional[int] = None, city: Optional[str] = None
    ) -> Dict[str, Any]:
        """Create a new user."""
        # Insert into write-side table
        user_data = {"name": name, "email": email, "age": age, "city": city}

        # Use the CQRS insert method
        result = await self.insert(
            "tb_user",
            {"identifier": f"user_{email}", "name": name, "email": email, "age": age, "city": city},
        )

        # Return the created user data (will be synced to tv_user automatically)
        return await self.get_by_id("tv_user", result["id"])

    async def create_post(
        self, title: str, content: Optional[str], author_id: UUID, published: bool = False
    ) -> Dict[str, Any]:
        """Create a new post."""
        # Get author pk from read-side
        author_data = await self.get_by_id("tv_user", author_id)
        if not author_data:
            raise ValueError(f"Author with ID {author_id} not found")

        # Insert into write-side table
        result = await self.insert(
            "tb_post",
            {
                "identifier": f"post_{title.replace(' ', '_')}",
                "title": title,
                "content": content,
                "published": published,
                "fk_author": author_data["pk_user"],  # Use the primary key
            },
        )

        return await self.get_by_id("tv_post", result["id"])

    async def create_comment(self, content: str, post_id: UUID, author_id: UUID) -> Dict[str, Any]:
        """Create a new comment."""
        # Get post and author pks from read-side
        post_data = await self.get_by_id("tv_post", post_id)
        author_data = await self.get_by_id("tv_user", author_id)

        if not post_data:
            raise ValueError(f"Post with ID {post_id} not found")
        if not author_data:
            raise ValueError(f"Author with ID {author_id} not found")

        # Insert into write-side table
        result = await self.insert(
            "tb_comment",
            {
                "content": content,
                "fk_post": post_data["pk_post"],
                "fk_author": author_data["pk_author"],
            },
        )

        return await self.get_by_id("tv_comment", result["id"])
