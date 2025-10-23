"""DataLoaders for GraphQL Benchmark to prevent N+1 queries."""

from typing import Any, Dict, List, Optional
from uuid import UUID

from fraiseql.optimization.dataloader import DataLoader

from .db import BenchmarkRepository


class UserDataLoader(DataLoader[UUID, Dict[str, Any]]):
    """DataLoader for batching user lookups by ID."""

    def __init__(self, db: BenchmarkRepository):
        super().__init__()
        self.db = db

    async def batch_load(self, user_ids: List[UUID]) -> List[Optional[Dict[str, Any]]]:
        """Batch load users by their IDs."""
        # Convert UUIDs to strings for the query
        user_id_strings = [str(user_id) for user_id in user_ids]

        # Batch fetch all users at once
        users_data = await self.db.get_users_by_ids(user_id_strings)

        # Create a lookup dict for O(1) access
        users_by_id = {UUID(user["id"]): user for user in users_data}

        # Return results in the same order as requested
        return [users_by_id.get(user_id) for user_id in user_ids]


class PostDataLoader(DataLoader[UUID, Dict[str, Any]]):
    """DataLoader for batching post lookups by ID."""

    def __init__(self, db: BenchmarkRepository):
        super().__init__()
        self.db = db

    async def batch_load(self, post_ids: List[UUID]) -> List[Optional[Dict[str, Any]]]:
        """Batch load posts by their IDs."""
        # Batch fetch all posts at once
        posts_data = await self.db.get_posts_by_ids([str(pid) for pid in post_ids])

        # Create a lookup dict for O(1) access
        posts_by_id = {UUID(post["id"]): post for post in posts_data}

        # Return results in the same order as requested
        return [posts_by_id.get(post_id) for post_id in post_ids]


class CommentDataLoader(DataLoader[UUID, List[Dict[str, Any]]]):
    """DataLoader for batching comment lookups by post ID."""

    def __init__(self, db: BenchmarkRepository):
        super().__init__()
        self.db = db

    async def batch_load(self, post_ids: List[UUID]) -> List[List[Dict[str, Any]]]:
        """Batch load comments by post IDs."""
        # Fetch all comments for all posts at once
        all_comments = await self.db.get_comments_by_post_ids([str(pid) for pid in post_ids])

        # Group comments by post_id
        comments_by_post: Dict[UUID, List[Dict[str, Any]]] = {}
        for comment in all_comments:
            post_id = UUID(comment["postId"])
            if post_id not in comments_by_post:
                comments_by_post[post_id] = []
            comments_by_post[post_id].append(comment)

        # Return results in the same order as requested
        return [comments_by_post.get(post_id, []) for post_id in post_ids]
