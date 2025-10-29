"""Database layer for blog API using FraiseQL CQRS."""

from typing import Any
from uuid import UUID

from fraiseql.cqrs import CQRSRepository as BaseCQRSRepository


def to_camel_case(snake_str: str) -> str:
    """Convert snake_case to camelCase."""
    components = snake_str.split("_")
    return components[0] + "".join(x.title() for x in components[1:])


class BlogRepository(BaseCQRSRepository):
    """Blog-specific repository extending FraiseQL CQRS base.

    This repository uses the generic select_from_json_view pattern
    for cleaner, more maintainable code.
    """

    # Query operations (using new generic pattern)

    async def get_user_by_id(self, user_id: UUID) -> dict[str, Any | None]:
        """Get user by ID."""
        return await self.get_by_id("v_users", user_id)

    async def get_user_by_email(self, email: str) -> dict[str, Any | None]:
        """Get user by email."""
        return await self.select_one_from_json_view("v_users", where={"email": email})

    async def get_post_by_id(self, post_id: UUID) -> dict[str, Any | None]:
        """Get post by ID with comments."""
        post_data = await self.get_by_id("v_posts", post_id)
        if post_data:
            comments_data = await self.get_comments_by_post(post_id)
            post_data["comments"] = comments_data
        return post_data

    async def get_posts(
        self,
        filters: dict[str, Any | None] = None,
        order_by: str | None = None,
        limit: int = 20,
        offset: int = 0,
    ) -> list[dict[str, Any]]:
        """Get posts with optional filtering and pagination.

        Accepts snake_case filter keys and converts to camelCase.
        Includes comments for each post.
        """
        # Convert snake_case filter keys to camelCase
        camel_filters = None
        if filters:
            camel_filters = {}
            for key, value in filters.items():
                camel_key = to_camel_case(key)
                camel_filters[camel_key] = value

        # Convert order_by from snake_case to camelCase
        camel_order_by = None
        if order_by:
            # Handle format like "created_at_desc"
            parts = order_by.split("_")
            if parts[-1].lower() in ("asc", "desc"):
                direction = parts[-1]
                field = "_".join(parts[:-1])
                camel_field = to_camel_case(field)
                camel_order_by = f"{camel_field}_{direction}"
            else:
                camel_order_by = to_camel_case(order_by)

        # Get posts with comments
        posts_data = await self.select_from_json_view(
            "v_posts",
            where=camel_filters,
            order_by=camel_order_by,
            limit=limit,
            offset=offset,
        )

        # For each post, fetch its comments
        for post_data in posts_data:
            post_id = post_data["id"]
            comments_data = await self.get_comments_by_post(UUID(post_id))
            post_data["comments"] = comments_data

        return posts_data

    async def get_comments_by_post(self, post_id: UUID) -> list[dict[str, Any]]:
        """Get comments for a post."""
        return await self.select_from_json_view(
            "v_comments",
            where={"postId": str(post_id)},
            order_by="createdAt_asc",
        )

    # Mutation operations (using base class methods)

    async def create_user(self, input_data: dict[str, Any]) -> dict[str, Any]:
        """Create a new user."""
        return await self.create("user", input_data)

    async def create_post(self, input_data: dict[str, Any]) -> dict[str, Any]:
        """Create a new post."""
        return await self.create("post", input_data)

    async def update_post(self, input_data: dict[str, Any]) -> dict[str, Any]:
        """Update a post."""
        return await self.update("post", input_data)

    async def create_comment(self, input_data: dict[str, Any]) -> dict[str, Any]:
        """Create a new comment."""
        return await self.create("comment", input_data)

    async def delete_post(self, post_id: UUID) -> dict[str, Any]:
        """Delete a post."""
        return await self.delete("post", post_id)

    async def increment_view_count(self, post_id: UUID) -> dict[str, Any]:
        """Increment post view count."""
        return await self.call_function(
            "fn_increment_view_count",
            {"post_id": str(post_id)},
        )
